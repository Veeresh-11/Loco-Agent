//! SQLite-backed single-use approval challenge store (prototype).
//!
//! Consumption uses a conditional UPDATE so a challenge can only transition
//! from unconsumed to consumed once. This store does not authenticate approval
//! identity; authenticity remains a separate requirement.

use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

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
        if challenge.challenge_id.trim().is_empty() || challenge.action_digest.len() != 64 {
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
        if challenge_id.trim().is_empty() { return Err(StoreError::InvalidChallenge); }
        let tx = self.conn.transaction()?;
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
}
