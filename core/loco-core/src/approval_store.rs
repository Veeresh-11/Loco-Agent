//! SQLite-backed single-use approval challenge store (prototype).
//!
//! The store owns challenge expiry and consumption time. This is still not an
//! authenticated approval mechanism; platform-authenticator evidence is a
//! separate, unresolved requirement.

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use crate::approval_binding::{action_digest, verify_approval_binding, ApprovalChallenge, ApprovalError};
use crate::approval_clock::{ApprovalClock, ApprovalClockError, SystemApprovalClock};
use crate::ProposedAction;

pub const MAX_APPROVAL_TTL_SECONDS: u64 = 300;

#[derive(Debug)]
pub enum StoreError {
    Sql(rusqlite::Error),
    Entropy(getrandom::Error),
    RuntimeEpochUnavailable,
    StaleRuntime,
    InvalidChallenge,
    Binding(ApprovalError),
    Clock(ApprovalClockError),
    AlreadyConsumedOrExpired,
    NotFound,
}
impl From<rusqlite::Error> for StoreError {
    fn from(value: rusqlite::Error) -> Self { Self::Sql(value) }
}

fn generate_challenge_id() -> Result<String, getrandom::Error> {
    let mut nonce = [0u8; 32];
    getrandom::getrandom(&mut nonce)?;
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(64);
    for byte in nonce {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    Ok(encoded)
}

fn is_valid_challenge_id(challenge_id: &str) -> bool {
    challenge_id.len() == 64
        && challenge_id.is_ascii()
        && challenge_id.bytes().all(|byte| byte.is_ascii_hexdigit())
}

static PROCESS_EPOCH: OnceLock<Result<String, String>> = OnceLock::new();

fn process_epoch() -> Result<&'static str, StoreError> {
    match PROCESS_EPOCH.get_or_init(|| generate_challenge_id().map_err(|error| error.to_string())) {
        Ok(epoch) => Ok(epoch.as_str()),
        Err(_) => Err(StoreError::RuntimeEpochUnavailable),
    }
}

pub struct ApprovalStore {
    conn: Connection,
    clock: Arc<dyn ApprovalClock>,
    process_epoch: String,
    monotonic_deadlines: HashMap<String, Duration>,
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
        Self::initialize_with_epoch(conn, clock, process_epoch()?.to_string())
    }

    fn initialize_with_epoch(
        conn: Connection,
        clock: Arc<dyn ApprovalClock>,
        process_epoch: String,
    ) -> Result<Self, StoreError> {
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS approval_challenges (
                challenge_id TEXT PRIMARY KEY,
                action_digest TEXT NOT NULL,
                expires_at INTEGER NOT NULL,
                consumed_at INTEGER NULL,
                process_epoch TEXT NOT NULL DEFAULT ''
             );
             CREATE INDEX IF NOT EXISTS approval_challenges_expiry
                ON approval_challenges(expires_at);
             CREATE TABLE IF NOT EXISTS approval_runtime (
                singleton_id INTEGER PRIMARY KEY CHECK(singleton_id = 1),
                process_epoch TEXT NOT NULL
             );"
        )?;

        // Migrate stores created before process epochs were introduced. Existing
        // rows receive an empty epoch and therefore become unusable (fail closed).
        let has_epoch: bool = {
            let mut statement = conn.prepare("PRAGMA table_info(approval_challenges)")?;
            let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
            let found = columns.filter_map(Result::ok).any(|name| name == "process_epoch");
            found
        };
        if !has_epoch {
            conn.execute(
                "ALTER TABLE approval_challenges ADD COLUMN process_epoch TEXT NOT NULL DEFAULT ''",
                [],
            )?;
        }

        // A newly started authority rotates the database's active epoch. Any
        // challenge issued by a previous process is now invalid, even if the
        // wall clock was rolled backward while the application was stopped.
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO approval_runtime(singleton_id, process_epoch) VALUES (1, ?1)
             ON CONFLICT(singleton_id) DO UPDATE SET process_epoch=excluded.process_epoch",
            params![process_epoch],
        )?;
        tx.commit()?;
        Ok(Self { conn, clock, process_epoch, monotonic_deadlines: HashMap::new() })
    }

    fn ensure_active_epoch(tx: &rusqlite::Transaction<'_>, expected_epoch: &str) -> Result<(), StoreError> {
        let active: String = tx.query_row(
            "SELECT process_epoch FROM approval_runtime WHERE singleton_id=1",
            [],
            |row| row.get(0),
        )?;
        if active != expected_epoch {
            return Err(StoreError::StaleRuntime);
        }
        Ok(())
    }

    /// Issue a challenge with a cryptographically random 256-bit identifier.
    /// Expiry is calculated by the Rust-owned clock.
    pub fn issue(
        &mut self,
        action: &ProposedAction,
        ttl_seconds: u64,
    ) -> Result<ApprovalChallenge, StoreError> {
        if !(1..=MAX_APPROVAL_TTL_SECONDS).contains(&ttl_seconds) {
            return Err(StoreError::InvalidChallenge);
        }
        let challenge_id = generate_challenge_id().map_err(StoreError::Entropy)?;
        let now = self.clock.now_unix_seconds().map_err(StoreError::Clock)?;
        let monotonic_now = self.clock.monotonic_now().map_err(StoreError::Clock)?;
        let monotonic_deadline = monotonic_now.checked_add(Duration::from_secs(ttl_seconds)).ok_or(StoreError::InvalidChallenge)?;
        let expires_at_unix = now.checked_add(ttl_seconds).ok_or(StoreError::InvalidChallenge)?;
        if expires_at_unix > i64::MAX as u64 {
            return Err(StoreError::InvalidChallenge);
        }
        let digest = action_digest(action).map_err(|_| StoreError::InvalidChallenge)?;
        let challenge = ApprovalChallenge {
            challenge_id,
            action_digest: digest,
            expires_at_unix,
        };
        let tx = self.conn.unchecked_transaction()?;
        Self::ensure_active_epoch(&tx, &self.process_epoch)?;
        tx.execute(
            "INSERT INTO approval_challenges(challenge_id, action_digest, expires_at, consumed_at, process_epoch)
             VALUES (?1, ?2, ?3, NULL, ?4)",
            params![challenge.challenge_id, challenge.action_digest, challenge.expires_at_unix as i64, self.process_epoch],
        )?;
        tx.commit()?;
        Ok(challenge)
    }

    /// Consume an existing, fresh challenge for the exact action. Current time
    /// is obtained internally; callers cannot supply or backdate the timestamp.
    pub fn consume(
        &mut self,
        challenge_id: &str,
        action: &ProposedAction,
    ) -> Result<(), StoreError> {
        if !is_valid_challenge_id(challenge_id) {
            return Err(StoreError::InvalidChallenge);
        }
        let now_unix = self.clock.now_unix_seconds().map_err(StoreError::Clock)?;
        let monotonic_now = self.clock.monotonic_now().map_err(StoreError::Clock)?;
        let monotonic_deadline = self.monotonic_deadlines.get(challenge_id).copied().ok_or(StoreError::AlreadyConsumedOrExpired)?;
        if monotonic_now >= monotonic_deadline {
            return Err(StoreError::AlreadyConsumedOrExpired);
        }
        if now_unix > i64::MAX as u64 {
            return Err(StoreError::InvalidChallenge);
        }

        // Acquire the write reservation before reading challenge state. This
        // serializes concurrent consumers across independent SQLite connections.
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        Self::ensure_active_epoch(&tx, &self.process_epoch)?;
        let record: Option<(String, u64, Option<u64>, String)> = tx.query_row(
            "SELECT action_digest, expires_at, consumed_at, process_epoch FROM approval_challenges WHERE challenge_id=?1",
            params![challenge_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        ).optional()?;
        let (digest, expires_at, consumed_at, challenge_epoch) = record.ok_or(StoreError::NotFound)?;
        if challenge_epoch != self.process_epoch { return Err(StoreError::AlreadyConsumedOrExpired); }
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
        monotonic_millis: AtomicU64,
        fail: Mutex<Option<ApprovalClockError>>,
    }
    impl TestClock {
        fn new(now: u64) -> Self {
            Self { now: AtomicU64::new(now), monotonic_millis: AtomicU64::new(0), fail: Mutex::new(None) }
        }
        fn set(&self, now: u64) { self.now.store(now, Ordering::SeqCst); }
        fn advance_monotonic(&self, millis: u64) { self.monotonic_millis.fetch_add(millis, Ordering::SeqCst); }
        fn fail_with(&self, error: ApprovalClockError) {
            *self.fail.lock().unwrap() = Some(error);
        }
    }
    impl ApprovalClock for TestClock {
        fn now_unix_seconds(&self) -> Result<u64, ApprovalClockError> {
            if let Some(error) = self.fail.lock().unwrap().clone() { return Err(error); }
            Ok(self.now.load(Ordering::SeqCst))
        }
        fn monotonic_now(&self) -> Result<Duration, ApprovalClockError> {
            Ok(Duration::from_millis(self.monotonic_millis.load(Ordering::SeqCst)))
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
        let challenge = store.issue(&action, 30).unwrap();
        assert_eq!(challenge.expires_at_unix, 130);
        clock.set(129);
        clock.advance_monotonic(29_000);
        store.consume(&challenge.challenge_id, &action).unwrap();
        assert!(store.is_consumed(&challenge.challenge_id).unwrap());
        assert!(matches!(
            store.consume(&challenge.challenge_id, &action),
            Err(StoreError::AlreadyConsumedOrExpired)
        ));
    }

    #[test]
    fn expired_challenge_rejected_using_internal_time() {
        let (mut store, clock) = test_store_at(100);
        let action = sample_action();
        let challenge = store.issue(&action, 10).unwrap();
        clock.set(110);
        clock.advance_monotonic(10_000);
        assert!(matches!(
            store.consume(&challenge.challenge_id, &action),
            Err(StoreError::AlreadyConsumedOrExpired)
        ));
    }

    #[test]
    fn action_mismatch_does_not_consume() {
        let (mut store, _) = test_store_at(100);
        let action = sample_action();
        let challenge = store.issue(&action, 30).unwrap();
        let modified = ProposedAction::RunCommand {
            executable: "echo".into(), args: vec!["changed".into()],
            working_directory: Some(".".into()), timeout_ms: Some(1000),
        };
        assert!(matches!(
            store.consume(&challenge.challenge_id, &modified),
            Err(StoreError::Binding(ApprovalError::DigestMismatch))
        ));
        assert!(!store.is_consumed(&challenge.challenge_id).unwrap());
    }

    #[test]
    fn issued_challenges_have_distinct_random_ids() {
        let (mut store, _) = test_store_at(100);
        let action = sample_action();
        let first = store.issue(&action, 30).unwrap();
        let second = store.issue(&action, 30).unwrap();
        assert_ne!(first.challenge_id, second.challenge_id);
    }

    #[test]
    fn unknown_challenge_rejected() {
        let (mut store, _) = test_store_at(100);
        assert!(matches!(
            store.consume(&"0".repeat(64), &sample_action()),
            Err(StoreError::NotFound)
        ));
    }

    #[test]
    fn issue_rejects_invalid_ttl() {
        let (mut store, _) = test_store_at(100);
        let action = sample_action();
        assert!(matches!(store.issue(&action, 0), Err(StoreError::InvalidChallenge)));
        assert!(matches!(
            store.issue(&action, MAX_APPROVAL_TTL_SECONDS + 1),
            Err(StoreError::InvalidChallenge)
        ));
    }

    #[test]
    fn challenge_ids_are_256_bit_hex_values() {
        let (mut store, _) = test_store_at(100);
        let challenge = store.issue(&sample_action(), 30).unwrap();
        assert!(is_valid_challenge_id(&challenge.challenge_id));
        assert_eq!(challenge.challenge_id.len(), 64);
    }

    #[test]
    fn issue_rejects_expiry_outside_sqlite_integer_range() {
        let (mut store, _) = test_store_at(i64::MAX as u64);
        assert!(matches!(
            store.issue(&sample_action(), 1),
            Err(StoreError::InvalidChallenge)
        ));
    }

    #[test]
    fn clock_failure_fails_closed_without_consuming() {
        let (mut store, clock) = test_store_at(100);
        let action = sample_action();
        let challenge = store.issue(&action, 30).unwrap();
        clock.fail_with(ApprovalClockError::ClockMovedBackward);
        assert!(matches!(
            store.consume(&challenge.challenge_id, &action),
            Err(StoreError::Clock(ApprovalClockError::ClockMovedBackward))
        ));
        assert!(!store.is_consumed(&challenge.challenge_id).unwrap());
    }

    #[test]
    fn reopening_store_invalidates_prior_process_challenges_and_stale_handles() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("restart.sqlite");
        let action = sample_action();
        let first_clock: Arc<dyn ApprovalClock> = Arc::new(TestClock::new(1_000));
        let mut first = ApprovalStore::initialize_with_epoch(
            Connection::open(&database).unwrap(), first_clock, "process-one".into()
        ).unwrap();
        let challenge = first.issue(&action, 60).unwrap();

        let second_clock: Arc<dyn ApprovalClock> = Arc::new(TestClock::new(1_001));
        let mut second = ApprovalStore::initialize_with_epoch(
            Connection::open(&database).unwrap(), second_clock, "process-two".into()
        ).unwrap();

        assert!(matches!(
            first.consume(&challenge.challenge_id, &action),
            Err(StoreError::StaleRuntime)
        ));
        assert!(matches!(
            second.consume(&challenge.challenge_id, &action),
            Err(StoreError::AlreadyConsumedOrExpired)
        ));
    }

    #[test]
    fn concurrent_connections_can_consume_a_challenge_only_once() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("approvals.sqlite");
        let action = sample_action();
        let clock: Arc<dyn ApprovalClock> = Arc::new(TestClock::new(499));
        let mut issuer = ApprovalStore::initialize(Connection::open(&database).unwrap(), clock.clone()).unwrap();
        let challenge = issuer.issue(&action, 1).unwrap();
        let first = ApprovalStore::initialize(Connection::open(&database).unwrap(), clock.clone()).unwrap();
        let second = ApprovalStore::initialize(Connection::open(&database).unwrap(), clock).unwrap();
        let barrier = Arc::new(Barrier::new(3));

        let first_barrier = Arc::clone(&barrier);
        let first_challenge_id = challenge.challenge_id.clone();
        let second_challenge_id = challenge.challenge_id.clone();
        let first_action = action.clone();
        let first_thread = std::thread::spawn(move || {
            let mut store = first;
            first_barrier.wait();
            match store.consume(&first_challenge_id, &first_action) {
                Ok(()) => true,
                Err(StoreError::AlreadyConsumedOrExpired) => false,
                Err(error) => panic!("unexpected first consumer error: {error:?}"),
            }
        });

        let second_barrier = Arc::clone(&barrier);
        let second_thread = std::thread::spawn(move || {
            let mut store = second;
            second_barrier.wait();
            match store.consume(&second_challenge_id, &action) {
                Ok(()) => true,
                Err(StoreError::AlreadyConsumedOrExpired) => false,
                Err(error) => panic!("unexpected second consumer error: {error:?}"),
            }
        });

        barrier.wait();
        let first_succeeded = first_thread.join().unwrap();
        let second_succeeded = second_thread.join().unwrap();
        assert_ne!(first_succeeded, second_succeeded, "exactly one consumer must succeed");
        assert!(issuer.is_consumed(&challenge.challenge_id).unwrap());
    }
}
