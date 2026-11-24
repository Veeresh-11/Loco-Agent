//! SQLite-backed single-use approval challenge store (prototype).
//!
//! The store owns challenge expiry and consumption time. This is still not an
//! authenticated approval mechanism; platform-authenticator evidence is a
//! separate, unresolved requirement.

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use crate::approval_binding::{action_digest, verify_approval_binding, ApprovalChallenge, ApprovalError};
use crate::approval_clock::{ApprovalClock, ApprovalClockError, SystemApprovalClock};
use crate::ProposedAction;

pub const MAX_APPROVAL_TTL_SECONDS: u64 = 300;

#[derive(Debug)]
pub enum StoreError {
    Sql(rusqlite::Error),
    InvalidChallenge,
    Binding(ApprovalError),
    Clock(ApprovalClockError),
    AlreadyConsumedOrExpired,
    NotFound,
}
impl From<rusqlite::Error> for StoreError {
    fn from(value: rusqlite::Error) -> Self { Self::Sql(value) }
}

pub struct ApprovalStore {
    conn: Connection,
    clock: Arc<dyn ApprovalClock>,
}

impl ApprovalStore {
    /// Open or create a local store. Secure the database with platform ACLs.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        Self::initialize(Connection::open(path)?, Arc::new(SystemApprovalClock))
    }
    pub fn in_memory() -> Result<Self, StoreError> {
        Self::initialize(Connection::open_in_memory()?, Arc::new(SystemApprovalClock))
    }
    fn initialize(
        conn: Connection,
        clock: Arc<dyn ApprovalClock>,
    ) -> Result<Self, StoreError> {
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS approval_challenges (
                challenge_id TEXT PRIMARY KEY,
                action_digest TEXT NOT NULL,
                expires_at INTEGER NOT NULL,
                consumed_at INTEGER NULL
             );
             CREATE INDEX IF NOT EXISTS approval_challenges_expiry
                ON approval_challenges(expires_at);"
        )?;
        Ok(Self { conn, clock })
    }

    /// Issue a challenge whose expiry is calculated by the Rust-owned clock.
    /// IDs are supplied by the caller for now; unpredictable nonce generation
    /// and authenticated platform assertions remain separate security gates.
    pub fn issue(
        &mut self,
        challenge_id: &str,
        action: &ProposedAction,
        ttl_seconds: u64,
    ) -> Result<ApprovalChallenge, StoreError> {
        if challenge_id.trim().is_empty()
            || challenge_id.len() > 128
            || !(1..=MAX_APPROVAL_TTL_SECONDS).contains(&ttl_seconds)
        {
            return Err(StoreError::InvalidChallenge);
        }
        let now = self.clock.now_unix_seconds().map_err(StoreError::Clock)?;
        let expires_at_unix = now.checked_add(ttl_seconds).ok_or(StoreError::InvalidChallenge)?;
        if expires_at_unix > i64::MAX as u64 {
            return Err(StoreError::InvalidChallenge);
        }
        let digest = action_digest(action).map_err(|_| StoreError::InvalidChallenge)?;
        let challenge = ApprovalChallenge {
            challenge_id: challenge_id.to_string(),
            action_digest: digest,
            expires_at_unix,
        };
        self.conn.execute(
            "INSERT INTO approval_challenges(challenge_id, action_digest, expires_at, consumed_at)
             VALUES (?1, ?2, ?3, NULL)",
            params![challenge.challenge_id, challenge.action_digest, challenge.expires_at_unix as i64],
        )?;
        Ok(challenge)
    }

    /// Consume an existing, fresh challenge for the exact action. Current time
    /// is obtained internally; callers cannot supply or backdate the timestamp.
    pub fn consume(
        &mut self,
        challenge_id: &str,
        action: &ProposedAction,
    ) -> Result<(), StoreError> {
        if challenge_id.trim().is_empty() || challenge_id.len() > 128 {
            return Err(StoreError::InvalidChallenge);
        }
        let now_unix = self.clock.now_unix_seconds().map_err(StoreError::Clock)?;
        if now_unix > i64::MAX as u64 {
            return Err(StoreError::InvalidChallenge);
        }

        // Acquire the write reservation before reading challenge state. This
        // serializes concurrent consumers across independent SQLite connections.
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let record: Option<(String, u64, Option<u64>)> = tx.query_row(
            "SELECT action_digest, expires_at, consumed_at FROM approval_challenges WHERE challenge_id=?1",
            params![challenge_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).optional()?;
        let (digest, expires_at, consumed_at) = record.ok_or(StoreError::NotFound)?;
        if consumed_at.is_some() || now_unix >= expires_at {
            return Err(StoreError::AlreadyConsumedOrExpired);
        }
        let challenge = ApprovalChallenge {
            challenge_id: challenge_id.to_string(),
            action_digest: digest,
            expires_at_unix: expires_at,
        };
        verify_approval_binding(&challenge, action, now_unix).map_err(StoreError::Binding)?;
        let changed = tx.execute(
            "UPDATE approval_challenges SET consumed_at=?2
             WHERE challenge_id=?1 AND consumed_at IS NULL AND expires_at>?2",
            params![challenge_id, now_unix as i64],
        )?;
        if changed != 1 { return Err(StoreError::AlreadyConsumedOrExpired); }
        tx.commit()?;
        Ok(())
    }

    /// Status helper only; consume is the authoritative atomic operation.
    pub fn is_consumed(&self, challenge_id: &str) -> Result<bool, StoreError> {
        let value: Option<Option<i64>> = self.conn.query_row(
            "SELECT consumed_at FROM approval_challenges WHERE challenge_id=?1",
            params![challenge_id],
            |row| row.get(0),
        ).optional()?;
        match value {
            Some(Some(_)) => Ok(true),
            Some(None) => Ok(false),
            None => Err(StoreError::NotFound),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Barrier, Mutex};

    struct TestClock {
        now: AtomicU64,
        fail: Mutex<Option<ApprovalClockError>>,
    }
    impl TestClock {
        fn new(now: u64) -> Self {
            Self { now: AtomicU64::new(now), fail: Mutex::new(None) }
        }
        fn set(&self, now: u64) { self.now.store(now, Ordering::SeqCst); }
        fn fail_with(&self, error: ApprovalClockError) {
            *self.fail.lock().unwrap() = Some(error);
        }
    }
    impl ApprovalClock for TestClock {
        fn now_unix_seconds(&self) -> Result<u64, ApprovalClockError> {
            if let Some(error) = self.fail.lock().unwrap().clone() { return Err(error); }
            Ok(self.now.load(Ordering::SeqCst))
        }
    }

    fn sample_action() -> ProposedAction {
        ProposedAction::RunCommand {
            executable: "echo".into(), args: vec!["hello".into()],
            working_directory: Some(".".into()), timeout_ms: Some(1000),
        }
    }
    fn test_store_at(now: u64) -> (ApprovalStore, Arc<TestClock>) {
        let clock = Arc::new(TestClock::new(now));
        let store = ApprovalStore::initialize(
            Connection::open_in_memory().unwrap(),
            clock.clone(),
        ).unwrap();
        (store, clock)
    }

    #[test]
    fn store_issues_and_consumes_using_its_own_clock() {
        let (mut store, clock) = test_store_at(100);
        let action = sample_action();
        let challenge = store.issue("single-use", &action, 30).unwrap();
        assert_eq!(challenge.expires_at_unix, 130);
        clock.set(129);
        store.consume("single-use", &action).unwrap();
        assert!(store.is_consumed("single-use").unwrap());
        assert!(matches!(
            store.consume("single-use", &action),
            Err(StoreError::AlreadyConsumedOrExpired)
        ));
    }

    #[test]
    fn expired_challenge_rejected_using_internal_time() {
        let (mut store, clock) = test_store_at(100);
        let action = sample_action();
        store.issue("expired", &action, 10).unwrap();
        clock.set(110);
        assert!(matches!(
            store.consume("expired", &action),
            Err(StoreError::AlreadyConsumedOrExpired)
        ));
    }

    #[test]
    fn action_mismatch_does_not_consume() {
        let (mut store, _) = test_store_at(100);
        let action = sample_action();
        store.issue("mismatch", &action, 30).unwrap();
        let modified = ProposedAction::RunCommand {
            executable: "echo".into(), args: vec!["changed".into()],
            working_directory: Some(".".into()), timeout_ms: Some(1000),
        };
        assert!(matches!(
            store.consume("mismatch", &modified),
            Err(StoreError::Binding(ApprovalError::DigestMismatch))
        ));
        assert!(!store.is_consumed("mismatch").unwrap());
    }

    #[test]
    fn duplicate_id_rejected() {
        let (mut store, _) = test_store_at(100);
        let action = sample_action();
        store.issue("duplicate", &action, 30).unwrap();
        assert!(matches!(store.issue("duplicate", &action, 30), Err(StoreError::Sql(_))));
    }

    #[test]
    fn unknown_challenge_rejected() {
        let (mut store, _) = test_store_at(100);
        assert!(matches!(
            store.consume("missing", &sample_action()),
            Err(StoreError::NotFound)
        ));
    }

    #[test]
    fn issue_rejects_invalid_id_and_ttl() {
        let (mut store, _) = test_store_at(100);
        let action = sample_action();
        assert!(matches!(store.issue(" ", &action, 10), Err(StoreError::InvalidChallenge)));
        assert!(matches!(store.issue("x", &action, 0), Err(StoreError::InvalidChallenge)));
        assert!(matches!(
            store.issue("x", &action, MAX_APPROVAL_TTL_SECONDS + 1),
            Err(StoreError::InvalidChallenge)
        ));
        assert!(matches!(store.issue(&"x".repeat(129), &action, 10), Err(StoreError::InvalidChallenge)));
    }

    #[test]
    fn issue_rejects_expiry_outside_sqlite_integer_range() {
        let (mut store, _) = test_store_at(i64::MAX as u64);
        assert!(matches!(
            store.issue("overflow", &sample_action(), 1),
            Err(StoreError::InvalidChallenge)
        ));
    }

    #[test]
    fn clock_failure_fails_closed_without_consuming() {
        let (mut store, clock) = test_store_at(100);
        let action = sample_action();
        store.issue("clock-failure", &action, 30).unwrap();
        clock.fail_with(ApprovalClockError::ClockMovedBackward);
        assert!(matches!(
            store.consume("clock-failure", &action),
            Err(StoreError::Clock(ApprovalClockError::ClockMovedBackward))
        ));
        assert!(!store.is_consumed("clock-failure").unwrap());
    }

    #[test]
    fn concurrent_connections_can_consume_a_challenge_only_once() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("approvals.sqlite");
        let action = sample_action();
        let clock: Arc<dyn ApprovalClock> = Arc::new(TestClock::new(499));
        let mut issuer = ApprovalStore::initialize(Connection::open(&database).unwrap(), clock.clone()).unwrap();
        issuer.issue("concurrent", &action, 1).unwrap();
        let first = ApprovalStore::initialize(Connection::open(&database).unwrap(), clock.clone()).unwrap();
        let second = ApprovalStore::initialize(Connection::open(&database).unwrap(), clock).unwrap();
        let barrier = Arc::new(Barrier::new(3));

        let first_barrier = Arc::clone(&barrier);
        let first_action = action.clone();
        let first_thread = std::thread::spawn(move || {
            let mut store = first;
            first_barrier.wait();
            match store.consume("concurrent", &first_action) {
                Ok(()) => true,
                Err(StoreError::AlreadyConsumedOrExpired) => false,
                Err(error) => panic!("unexpected first consumer error: {error:?}"),
            }
        });

        let second_barrier = Arc::clone(&barrier);
        let second_thread = std::thread::spawn(move || {
            let mut store = second;
            second_barrier.wait();
            match store.consume("concurrent", &action) {
                Ok(()) => true,
                Err(StoreError::AlreadyConsumedOrExpired) => false,
                Err(error) => panic!("unexpected second consumer error: {error:?}"),
            }
        });

        barrier.wait();
        let first_succeeded = first_thread.join().unwrap();
        let second_succeeded = second_thread.join().unwrap();
        assert_ne!(first_succeeded, second_succeeded, "exactly one consumer must succeed");
        assert!(issuer.is_consumed("concurrent").unwrap());
    }
}
