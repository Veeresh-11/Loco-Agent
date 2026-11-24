//! SQLite-backed single-use approval challenge store (prototype).
//!
//! Consumption uses a conditional UPDATE so a challenge can only transition
//! from unconsumed to consumed once. This store does not authenticate approval
//! identity; authenticity remains a separate requirement.

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::path::Path;
use std::time::Duration;

use crate::approval_binding::{ApprovalChallenge, ApprovalError, verify_approval_binding};
use crate::ProposedAction;

#[derive(Debug)]
pub enum StoreError {
    Sql(rusqlite::Error),
    InvalidChallenge,
    Binding(ApprovalError),
    AlreadyConsumedOrExpired,
    NotFound,
}
impl From<rusqlite::Error> for StoreError {
    fn from(value: rusqlite::Error) -> Self { Self::Sql(value) }
}

pub struct ApprovalStore { conn: Connection }

impl ApprovalStore {
    /// Open or create a local store. Secure the database with platform ACLs.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        Self::initialize(Connection::open(path)?)
    }
    pub fn in_memory() -> Result<Self, StoreError> {
        Self::initialize(Connection::open_in_memory()?)
    }
    fn initialize(conn: Connection) -> Result<Self, StoreError> {
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
        Ok(Self { conn })
    }
    /// Insert a challenge. IDs cannot be overwritten.
    pub fn issue(&mut self, challenge: &ApprovalChallenge) -> Result<(), StoreError> {
        if challenge.challenge_id.trim().is_empty()
            || challenge.action_digest.len() != 64
            || !challenge.action_digest.is_ascii()
            || !challenge.action_digest.bytes().all(|byte| byte.is_ascii_hexdigit())
            || challenge.expires_at_unix > i64::MAX as u64
        {
            return Err(StoreError::InvalidChallenge);
        }
        self.conn.execute(
            "INSERT INTO approval_challenges(challenge_id, action_digest, expires_at, consumed_at)
             VALUES (?1, ?2, ?3, NULL)",
            params![challenge.challenge_id, challenge.action_digest, challenge.expires_at_unix as i64],
        )?;
        Ok(())
    }
    /// Atomically consume an existing, unexpired, unused challenge for the exact
    /// action. The timestamp must come from a trusted clock.
    pub fn consume(&mut self, challenge_id: &str, action: &ProposedAction, now_unix: u64) -> Result<(), StoreError> {
        if challenge_id.trim().is_empty() || now_unix > i64::MAX as u64 {
            return Err(StoreError::InvalidChallenge);
        }
        // Acquire the write reservation before reading the challenge. With multiple
        // processes/connections, a deferred read-then-write transaction can race
        // while upgrading its lock. IMMEDIATE serializes consumers before the read.
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
    use crate::approval_binding::create_challenge;

    fn sample_action() -> ProposedAction {
        ProposedAction::RunCommand {
            executable: "echo".into(), args: vec!["hello".into()],
            working_directory: Some(".".into()), timeout_ms: Some(1000),
        }
    }
    #[test]
    fn consumes_once() {
        let mut store = ApprovalStore::in_memory().unwrap();
        let a = sample_action();
        let c = create_challenge("single-use".into(), &a, 200).unwrap();
        store.issue(&c).unwrap();
        store.consume("single-use", &a, 199).unwrap();
        assert!(store.is_consumed("single-use").unwrap());
        assert!(matches!(store.consume("single-use", &a, 199), Err(StoreError::AlreadyConsumedOrExpired)));
    }
    #[test]
    fn expired_challenge_rejected() {
        let mut store = ApprovalStore::in_memory().unwrap();
        let a = sample_action();
        let c = create_challenge("expired".into(), &a, 100).unwrap();
        store.issue(&c).unwrap();
        assert!(matches!(store.consume("expired", &a, 100), Err(StoreError::AlreadyConsumedOrExpired)));
    }
    #[test]
    fn action_mismatch_does_not_consume() {
        let mut store = ApprovalStore::in_memory().unwrap();
        let a = sample_action();
        let c = create_challenge("mismatch".into(), &a, 200).unwrap();
        store.issue(&c).unwrap();
        let modified = ProposedAction::RunCommand {
            executable: "echo".into(), args: vec!["changed".into()],
            working_directory: Some(".".into()), timeout_ms: Some(1000),
        };
        assert!(matches!(store.consume("mismatch", &modified, 199), Err(StoreError::Binding(ApprovalError::DigestMismatch))));
        assert!(!store.is_consumed("mismatch").unwrap());
    }
    #[test]
    fn duplicate_id_rejected() {
        let mut store = ApprovalStore::in_memory().unwrap();
        let a = sample_action();
        let c = create_challenge("duplicate".into(), &a, 200).unwrap();
        store.issue(&c).unwrap();
        assert!(store.issue(&c).is_err());
    }
    #[test]
    fn unknown_challenge_rejected() {
        let mut store = ApprovalStore::in_memory().unwrap();
        assert!(matches!(store.consume("missing", &sample_action(), 100), Err(StoreError::NotFound)));
    }

    #[test]
    fn issue_rejects_non_hex_action_digest() {
        let mut store = ApprovalStore::in_memory().unwrap();
        let mut challenge = create_challenge("bad-digest".into(), &sample_action(), 200).unwrap();
        challenge.action_digest = "z".repeat(64);
        assert!(matches!(store.issue(&challenge), Err(StoreError::InvalidChallenge)));
    }

    #[test]
    fn issue_rejects_non_ascii_action_digest() {
        let mut store = ApprovalStore::in_memory().unwrap();
        let mut challenge = create_challenge("unicode-digest".into(), &sample_action(), 200).unwrap();
        challenge.action_digest = "🦀".repeat(16);
        assert!(matches!(store.issue(&challenge), Err(StoreError::InvalidChallenge)));
    }

    #[test]
    fn issue_rejects_expiry_outside_sqlite_integer_range() {
        let mut store = ApprovalStore::in_memory().unwrap();
        let challenge = create_challenge("overflow-expiry".into(), &sample_action(), i64::MAX as u64 + 1).unwrap();
        assert!(matches!(store.issue(&challenge), Err(StoreError::InvalidChallenge)));
    }


    #[test]
    fn concurrent_connections_can_consume_a_challenge_only_once() {
        use std::sync::{Arc, Barrier};

        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("approvals.sqlite");
        let action = sample_action();
        let challenge = create_challenge("concurrent".into(), &action, 500).unwrap();

        let mut issuer = ApprovalStore::open(&database).unwrap();
        issuer.issue(&challenge).unwrap();
        let first = ApprovalStore::open(&database).unwrap();
        let second = ApprovalStore::open(&database).unwrap();
        let barrier = Arc::new(Barrier::new(3));

        let first_barrier = Arc::clone(&barrier);
        let first_action = action.clone();
        let first_thread = std::thread::spawn(move || {
            let mut store = first;
            first_barrier.wait();
            match store.consume("concurrent", &first_action, 499) {
                Ok(()) => true,
                Err(StoreError::AlreadyConsumedOrExpired) => false,
                Err(error) => panic!("unexpected first consumer error: {error:?}"),
            }
        });

        let second_barrier = Arc::clone(&barrier);
        let second_thread = std::thread::spawn(move || {
            let mut store = second;
            second_barrier.wait();
            match store.consume("concurrent", &action, 499) {
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

    #[test]
    fn consume_rejects_time_outside_sqlite_integer_range() {
        let mut store = ApprovalStore::in_memory().unwrap();
        let action = sample_action();
        let challenge = create_challenge("overflow-now".into(), &action, 200).unwrap();
        store.issue(&challenge).unwrap();
        assert!(matches!(
            store.consume("overflow-now", &action, i64::MAX as u64 + 1),
            Err(StoreError::InvalidChallenge)
        ));
        assert!(!store.is_consumed("overflow-now").unwrap());
    }
}
