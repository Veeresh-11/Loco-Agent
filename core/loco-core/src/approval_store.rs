//! SQLite-backed single-use approval challenge store (prototype).
//!
//! The store owns challenge expiry and consumption time. This is still not an
//! authenticated approval mechanism; platform-authenticator evidence is a
//! separate, unresolved requirement.

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use sha2::{Digest, Sha256};

use crate::approval_binding::{action_digest, verify_approval_binding, ApprovalChallenge, ApprovalError};
use crate::approval_clock::{ApprovalClock, ApprovalClockError, SystemApprovalClock};
use crate::ProposedAction;
use crate::canonical_snapshot_digest::canonical_snapshot_binding_digest;
use crate::snapshot_payload::CapturedWorkspaceSnapshot;

pub const MAX_APPROVAL_TTL_SECONDS: u64 = 300;
pub const MAX_APPROVAL_SNAPSHOT_AGE: Duration = Duration::from_secs(30);
pub const MAX_OUTSTANDING_APPROVAL_CHALLENGES: usize = 1024;
pub const MAX_PERSISTED_APPROVAL_CHALLENGES: u64 = 4096;
pub const APPROVAL_RETENTION_BATCH_SIZE: u64 = 256;
pub const MAX_APPROVAL_DATABASE_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_APPROVAL_WAL_BYTES: u64 = 1024 * 1024;
pub const APPROVAL_WAL_AUTOCHECKPOINT_PAGES: u32 = 128;

#[derive(Debug)]
pub enum StoreError {
    Sql(rusqlite::Error),
    Entropy(getrandom::Error),
    RuntimeEpochUnavailable,
    StaleRuntime,
    InvalidChallenge,
    InvalidRegistration,
    CapacityExceeded,
    PersistentCapacityExceeded,
    StorageLimitExceeded,
    Binding(ApprovalError),
    Clock(ApprovalClockError),
    AlreadyConsumedOrExpired,
    NotFound,
}
impl From<rusqlite::Error> for StoreError {
    fn from(value: rusqlite::Error) -> Self { Self::Sql(value) }
}

fn digest_workspace_registration(workspace_handle: &str) -> Result<String, StoreError> {
    if workspace_handle.len() != 64
        || !workspace_handle.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(StoreError::InvalidRegistration);
    }
    let digest = Sha256::digest(workspace_handle.as_bytes());
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut encoded, "{byte:02x}").map_err(|_| StoreError::InvalidRegistration)?;
    }
    Ok(encoded)
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
static MONOTONIC_DEADLINES: OnceLock<Mutex<HashMap<String, Duration>>> = OnceLock::new();

fn process_epoch() -> Result<&'static str, StoreError> {
    match PROCESS_EPOCH.get_or_init(|| generate_challenge_id().map_err(|error| error.to_string())) {
        Ok(epoch) => Ok(epoch.as_str()),
        Err(_) => Err(StoreError::RuntimeEpochUnavailable),
    }
}

fn ensure_snapshot_fresh(snapshot: &CapturedWorkspaceSnapshot, now: Duration) -> Result<(), StoreError> {
    let captured_at = snapshot.captured_at_monotonic();
    let age = now.checked_sub(captured_at).ok_or(StoreError::Binding(ApprovalError::SnapshotStale))?;
    if age > MAX_APPROVAL_SNAPSHOT_AGE {
        return Err(StoreError::Binding(ApprovalError::SnapshotStale));
    }
    Ok(())
}

fn prune_and_check_capacity(
    deadlines: &mut HashMap<String, Duration>,
    now: Duration,
) -> Result<(), StoreError> {
    deadlines.retain(|_, deadline| *deadline > now);
    if deadlines.len() >= MAX_OUTSTANDING_APPROVAL_CHALLENGES {
        return Err(StoreError::CapacityExceeded);
    }
    Ok(())
}

fn prune_persistent_challenges(
    tx: &rusqlite::Transaction<'_>,
    now_unix: u64,
    active_epoch: &str,
) -> Result<usize, StoreError> {
    let deleted = tx.execute(
        "DELETE FROM approval_challenges WHERE rowid IN (
            SELECT rowid FROM approval_challenges
            WHERE consumed_at IS NOT NULL OR expires_at <= ?1 OR process_epoch != ?2
            ORDER BY expires_at ASC LIMIT ?3
         )",
        params![now_unix as i64, active_epoch, APPROVAL_RETENTION_BATCH_SIZE],
    )?;
    Ok(deleted)
}

fn persisted_challenge_count(tx: &rusqlite::Transaction<'_>) -> Result<u64, StoreError> {
    Ok(tx.query_row("SELECT COUNT(*) FROM approval_challenges", [], |row| row.get(0))?)
}

fn persistent_capacity_reached(tx: &rusqlite::Transaction<'_>) -> Result<bool, StoreError> {
    Ok(persisted_challenge_count(tx)? >= MAX_PERSISTED_APPROVAL_CHALLENGES)
}

/// Apply SQLite page and journal bounds to one connection. The database
/// page ceiling is hard; the WAL journal limit is enforced when SQLite can
/// reset/truncate the WAL after checkpointing (long-lived readers can delay it).
fn configure_storage_limits(conn: &Connection) -> Result<(), StoreError> {
    let page_size: u64 = conn.pragma_query_value(None, "page_size", |row| row.get(0))?;
    if page_size == 0 {
        return Err(StoreError::StorageLimitExceeded);
    }
    let max_pages = (MAX_APPROVAL_DATABASE_BYTES / page_size).max(1);
    conn.pragma_update(None, "max_page_count", max_pages)?;
    conn.pragma_update(None, "journal_size_limit", MAX_APPROVAL_WAL_BYTES)?;
    conn.pragma_update(None, "wal_autocheckpoint", APPROVAL_WAL_AUTOCHECKPOINT_PAGES)?;

    let page_count: u64 = conn.pragma_query_value(None, "page_count", |row| row.get(0))?;
    let configured_max: u64 = conn.pragma_query_value(None, "max_page_count", |row| row.get(0))?;
    if page_count > max_pages || configured_max > max_pages {
        return Err(StoreError::StorageLimitExceeded);
    }
    Ok(())
}

pub struct ApprovalStore {
    conn: Connection,
    clock: Arc<dyn ApprovalClock>,
    process_epoch: String,
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
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
        configure_storage_limits(&conn)?;
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS approval_challenges (
                challenge_id TEXT PRIMARY KEY,
                action_digest TEXT NOT NULL,
                snapshot_digest TEXT NOT NULL DEFAULT '',
                expires_at INTEGER NOT NULL,
                consumed_at INTEGER NULL,
                process_epoch TEXT NOT NULL DEFAULT '',
                registration_digest TEXT NOT NULL DEFAULT ''
             );
             CREATE INDEX IF NOT EXISTS approval_challenges_expiry
                ON approval_challenges(expires_at);
             CREATE TABLE IF NOT EXISTS approval_runtime (
                singleton_id INTEGER PRIMARY KEY CHECK(singleton_id = 1),
                process_epoch TEXT NOT NULL
             );"
        )?;

        // Old action-only rows receive an empty snapshot digest and are unusable.
        // The process epoch also rotates on startup, invalidating all old challenges.
        let has_snapshot_digest: bool = {
            let mut statement = conn.prepare("PRAGMA table_info(approval_challenges)")?;
            let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
            let found = columns.filter_map(Result::ok).any(|name| name == "snapshot_digest");
            found
        };
        if !has_snapshot_digest {
            conn.execute(
                "ALTER TABLE approval_challenges ADD COLUMN snapshot_digest TEXT NOT NULL DEFAULT ''",
                [],
            )?;
        }

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

        // Challenges are additionally scoped to one process-local workspace registration.
        // Legacy rows receive an empty digest and remain usable only through the legacy
        // unscoped prototype API; registration-bound consume rejects them.
        let has_registration_digest: bool = {
            let mut statement = conn.prepare("PRAGMA table_info(approval_challenges)")?;
            let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
            columns.filter_map(Result::ok).any(|name| name == "registration_digest")
        };
        if !has_registration_digest {
            conn.execute(
                "ALTER TABLE approval_challenges ADD COLUMN registration_digest TEXT NOT NULL DEFAULT ''",
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
        Ok(Self { conn, clock, process_epoch })
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
        snapshot: &CapturedWorkspaceSnapshot,
        ttl_seconds: u64,
    ) -> Result<ApprovalChallenge, StoreError> {
        if !(1..=MAX_APPROVAL_TTL_SECONDS).contains(&ttl_seconds) {
            return Err(StoreError::InvalidChallenge);
        }
        let challenge_id = generate_challenge_id().map_err(StoreError::Entropy)?;
        let now = self.clock.now_unix_seconds().map_err(StoreError::Clock)?;
        let monotonic_now = self.clock.monotonic_now().map_err(StoreError::Clock)?;
        ensure_snapshot_fresh(snapshot, monotonic_now)?;
        crate::approval_binding::validate_action_snapshot_binding(action, snapshot).map_err(StoreError::Binding)?;
        let monotonic_deadline = monotonic_now.checked_add(Duration::from_secs(ttl_seconds)).ok_or(StoreError::InvalidChallenge)?;
        let expires_at_unix = now.checked_add(ttl_seconds).ok_or(StoreError::InvalidChallenge)?;
        if expires_at_unix > i64::MAX as u64 {
            return Err(StoreError::InvalidChallenge);
        }
        let digest = action_digest(action).map_err(|_| StoreError::InvalidChallenge)?;
        let snapshot_digest = canonical_snapshot_binding_digest(snapshot.snapshot());
        let challenge = ApprovalChallenge {
            challenge_id,
            action_digest: digest,
            snapshot_digest,
            expires_at_unix,
        };
        // Always acquire SQLite's write reservation before the shared deadline
        // mutex. consume follows the same lock order, preventing lock inversion.
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        Self::ensure_active_epoch(&tx, &self.process_epoch)?;
        prune_persistent_challenges(&tx, now, &self.process_epoch)?;
        if persistent_capacity_reached(&tx)? {
            // Commit bounded cleanup even when admission is denied, allowing
            // subsequent requests to make progress through old rows.
            tx.commit()?;
            return Err(StoreError::PersistentCapacityExceeded);
        }
        let mut deadlines = MONOTONIC_DEADLINES.get_or_init(|| Mutex::new(HashMap::new()))
            .lock().map_err(|_| StoreError::Clock(ApprovalClockError::ClockStatePoisoned))?;
        prune_and_check_capacity(&mut deadlines, monotonic_now)?;
        tx.execute(
            "INSERT INTO approval_challenges(challenge_id, action_digest, snapshot_digest, expires_at, consumed_at, process_epoch)
             VALUES (?1, ?2, ?3, ?4, NULL, ?5)",
            params![challenge.challenge_id, challenge.action_digest, challenge.snapshot_digest, challenge.expires_at_unix as i64, self.process_epoch],
        )?;
        deadlines.insert(challenge.challenge_id.clone(), monotonic_deadline);
        if let Err(error) = tx.commit() {
            deadlines.remove(&challenge.challenge_id);
            return Err(StoreError::Sql(error));
        }
        Ok(challenge)
    }

    /// Issue a challenge bound to a specific process-local workspace handle.
    /// Only a digest of the opaque handle is persisted. The challenge is not a
    /// human approval and grants no authority by itself.
    pub fn issue_for_registration(
        &mut self,
        action: &ProposedAction,
        snapshot: &CapturedWorkspaceSnapshot,
        workspace_handle: &str,
        ttl_seconds: u64,
    ) -> Result<ApprovalChallenge, StoreError> {
        let registration_digest = digest_workspace_registration(workspace_handle)?;
        let challenge = self.issue(action, snapshot, ttl_seconds)?;
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        Self::ensure_active_epoch(&tx, &self.process_epoch)?;
        let changed = tx.execute(
            "UPDATE approval_challenges SET registration_digest=?2
             WHERE challenge_id=?1 AND process_epoch=?3 AND registration_digest=''",
            params![challenge.challenge_id, registration_digest, self.process_epoch],
        )?;
        if changed != 1 {
            return Err(StoreError::InvalidChallenge);
        }
        tx.commit()?;
        Ok(challenge)
    }

    /// Revoke every outstanding challenge issued under this workspace registration.
    /// Consumed and unconsumed rows are removed; their in-memory deadlines are cleared.
    pub fn revoke_registration(&mut self, workspace_handle: &str) -> Result<(), StoreError> {
        let registration_digest = digest_workspace_registration(workspace_handle)?;
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        Self::ensure_active_epoch(&tx, &self.process_epoch)?;
        let challenge_ids = {
            let mut statement = tx.prepare(
                "SELECT challenge_id FROM approval_challenges WHERE registration_digest=?1"
            )?;
            let rows = statement.query_map(params![registration_digest], |row| row.get::<_, String>(0))?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        tx.execute(
            "DELETE FROM approval_challenges WHERE registration_digest=?1",
            params![registration_digest],
        )?;
        tx.commit()?;
        let mut deadlines = MONOTONIC_DEADLINES.get_or_init(|| Mutex::new(HashMap::new()))
            .lock().map_err(|_| StoreError::Clock(ApprovalClockError::ClockStatePoisoned))?;
        for challenge_id in challenge_ids {
            deadlines.remove(&challenge_id);
        }
        Ok(())
    }

    /// Consume an existing, fresh challenge for the exact action. Current time
    /// is obtained internally; callers cannot supply or backdate the timestamp.
    pub fn consume(
        &mut self,
        challenge_id: &str,
        action: &ProposedAction,
        snapshot: &CapturedWorkspaceSnapshot,
    ) -> Result<(), StoreError> {
        self.consume_inner(challenge_id, action, snapshot, None)
    }

    /// Consume only when the challenge belongs to the supplied live workspace registration.
    pub fn consume_for_registration(
        &mut self,
        challenge_id: &str,
        action: &ProposedAction,
        snapshot: &CapturedWorkspaceSnapshot,
        workspace_handle: &str,
    ) -> Result<(), StoreError> {
        let registration_digest = digest_workspace_registration(workspace_handle)?;
        self.consume_inner(challenge_id, action, snapshot, Some(&registration_digest))
    }

    fn consume_inner(
        &mut self,
        challenge_id: &str,
        action: &ProposedAction,
        snapshot: &CapturedWorkspaceSnapshot,
        expected_registration_digest: Option<&str>,
    ) -> Result<(), StoreError> {
        if !is_valid_challenge_id(challenge_id) {
            return Err(StoreError::InvalidChallenge);
        }
        let now_unix = self.clock.now_unix_seconds().map_err(StoreError::Clock)?;
        let monotonic_now = self.clock.monotonic_now().map_err(StoreError::Clock)?;
        ensure_snapshot_fresh(snapshot, monotonic_now)?;
        if now_unix > i64::MAX as u64 {
            return Err(StoreError::InvalidChallenge);
        }

        // Acquire the write reservation before reading challenge state. This
        // serializes concurrent consumers across independent SQLite connections.
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        Self::ensure_active_epoch(&tx, &self.process_epoch)?;
        let record: Option<(String, String, u64, Option<u64>, String, String)> = tx.query_row(
            "SELECT action_digest, snapshot_digest, expires_at, consumed_at, process_epoch, registration_digest FROM approval_challenges WHERE challenge_id=?1",
            params![challenge_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
        ).optional()?;
        let (digest, snapshot_digest, expires_at, consumed_at, challenge_epoch, registration_digest) =
            record.ok_or(StoreError::NotFound)?;
        if challenge_epoch != self.process_epoch { return Err(StoreError::AlreadyConsumedOrExpired); }
        match expected_registration_digest {
            Some(expected) if registration_digest != expected => return Err(StoreError::AlreadyConsumedOrExpired),
            None if !registration_digest.is_empty() => return Err(StoreError::AlreadyConsumedOrExpired),
            _ => {}
        }
        let monotonic_deadline = MONOTONIC_DEADLINES.get_or_init(|| Mutex::new(HashMap::new()))
            .lock().map_err(|_| StoreError::Clock(ApprovalClockError::ClockStatePoisoned))?
            .get(challenge_id).copied().ok_or(StoreError::AlreadyConsumedOrExpired)?;
        if monotonic_now >= monotonic_deadline {
            return Err(StoreError::AlreadyConsumedOrExpired);
        }
        if consumed_at.is_some() || now_unix >= expires_at {
            return Err(StoreError::AlreadyConsumedOrExpired);
        }
        let challenge = ApprovalChallenge {
            challenge_id: challenge_id.to_string(),
            action_digest: digest,
            snapshot_digest,
            expires_at_unix: expires_at,
        };
        verify_approval_binding(&challenge, action, snapshot, now_unix).map_err(StoreError::Binding)?;
        let changed = tx.execute(
            "UPDATE approval_challenges SET consumed_at=?2
             WHERE challenge_id=?1 AND consumed_at IS NULL AND expires_at>?2",
            params![challenge_id, now_unix as i64],
        )?;
        if changed != 1 { return Err(StoreError::AlreadyConsumedOrExpired); }
        tx.commit()?;
        MONOTONIC_DEADLINES.get_or_init(|| Mutex::new(HashMap::new()))
            .lock().map_err(|_| StoreError::Clock(ApprovalClockError::ClockStatePoisoned))?
            .remove(challenge_id);
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
        ProposedAction::ReadFile { path: "src/lib.rs".into(), max_bytes: Some(4096) }
    }

    fn sample_snapshot_at(captured_at: Duration) -> CapturedWorkspaceSnapshot {
        let bytes = b"captured";
        let contract = crate::snapshot_payload::build_snapshot_contract(
            "workspace-test", "src/lib.rs", "opaque-file-id", bytes,
        ).unwrap();
        CapturedWorkspaceSnapshot::new_at(contract, bytes.to_vec(), captured_at).unwrap()
    }

    fn issue(store: &mut ApprovalStore, action: &ProposedAction, ttl: u64) -> Result<ApprovalChallenge, StoreError> {
        let captured_at = store.clock.monotonic_now().unwrap();
        let snapshot = sample_snapshot_at(captured_at);
        store.issue(action, &snapshot, ttl)
    }

    fn consume(store: &mut ApprovalStore, challenge_id: &str, action: &ProposedAction) -> Result<(), StoreError> {
        let captured_at = store.clock.monotonic_now().unwrap();
        let snapshot = sample_snapshot_at(captured_at);
        store.consume(challenge_id, action, &snapshot)
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
    fn registration_bound_challenge_requires_same_workspace_handle() {
        let (mut store, clock) = test_store_at(100);
        let action = sample_action();
        let snapshot = sample_snapshot_at(Duration::ZERO);
        let handle = "a".repeat(64);
        let challenge = store.issue_for_registration(&action, &snapshot, &handle, 30).unwrap();

        let fresh = sample_snapshot_at(clock.monotonic_now().unwrap());
        assert!(matches!(
            store.consume_for_registration(&challenge.challenge_id, &action, &fresh, &"b".repeat(64)),
            Err(StoreError::AlreadyConsumedOrExpired)
        ));
        assert!(!store.is_consumed(&challenge.challenge_id).unwrap());

        store.consume_for_registration(&challenge.challenge_id, &action, &fresh, &handle).unwrap();
        assert!(store.is_consumed(&challenge.challenge_id).unwrap());
    }

    #[test]
    fn legacy_consume_cannot_consume_registration_bound_challenge() {
        let (mut store, _) = test_store_at(100);
        let action = sample_action();
        let snapshot = sample_snapshot_at(Duration::ZERO);
        let challenge = store.issue_for_registration(&action, &snapshot, &"c".repeat(64), 30).unwrap();
        let fresh = sample_snapshot_at(Duration::ZERO);
        assert!(matches!(
            store.consume(&challenge.challenge_id, &action, &fresh),
            Err(StoreError::AlreadyConsumedOrExpired)
        ));
        assert!(!store.is_consumed(&challenge.challenge_id).unwrap());
    }

    #[test]
    fn revoking_workspace_registration_invalidates_its_outstanding_challenges() {
        let (mut store, _) = test_store_at(100);
        let action = sample_action();
        let snapshot = sample_snapshot_at(Duration::ZERO);
        let handle = "d".repeat(64);
        let challenge = store.issue_for_registration(&action, &snapshot, &handle, 30).unwrap();

        store.revoke_registration(&handle).unwrap();
        let fresh = sample_snapshot_at(Duration::ZERO);
        assert!(matches!(
            store.consume_for_registration(&challenge.challenge_id, &action, &fresh, &handle),
            Err(StoreError::NotFound)
        ));
    }

    #[test]
    fn registration_binding_rejects_malformed_workspace_handles() {
        let (mut store, _) = test_store_at(100);
        let action = sample_action();
        let snapshot = sample_snapshot_at(Duration::ZERO);
        assert!(matches!(
            store.issue_for_registration(&action, &snapshot, "not-a-handle", 30),
            Err(StoreError::InvalidRegistration)
        ));
        assert!(matches!(
            store.revoke_registration("not-a-handle"),
            Err(StoreError::InvalidRegistration)
        ));
    }

    #[test]
    fn store_issues_and_consumes_using_its_own_clock() {
        let (mut store, clock) = test_store_at(100);
        let action = sample_action();
        let challenge = issue(&mut store, &action, 30).unwrap();
        assert_eq!(challenge.expires_at_unix, 130);
        clock.set(129);
        clock.advance_monotonic(29_000);
        consume(&mut store, &challenge.challenge_id, &action).unwrap();
        assert!(store.is_consumed(&challenge.challenge_id).unwrap());
        assert!(matches!(
            consume(&mut store,&challenge.challenge_id, &action),
            Err(StoreError::AlreadyConsumedOrExpired)
        ));
    }

    #[test]
    fn expired_challenge_rejected_using_internal_time() {
        let (mut store, clock) = test_store_at(100);
        let action = sample_action();
        let challenge = issue(&mut store, &action, 10).unwrap();
        clock.set(110);
        clock.advance_monotonic(10_000);
        assert!(matches!(
            consume(&mut store,&challenge.challenge_id, &action),
            Err(StoreError::AlreadyConsumedOrExpired)
        ));
    }

    #[test]
    fn file_store_configures_database_and_wal_limits() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("bounded.sqlite");
        let store = ApprovalStore::open(&database).unwrap();
        let page_size: u64 = store.conn.pragma_query_value(None, "page_size", |row| row.get(0)).unwrap();
        let max_pages: u64 = store.conn.pragma_query_value(None, "max_page_count", |row| row.get(0)).unwrap();
        let journal_limit: u64 = store.conn.pragma_query_value(None, "journal_size_limit", |row| row.get(0)).unwrap();
        let autocheckpoint: u32 = store.conn.pragma_query_value(None, "wal_autocheckpoint", |row| row.get(0)).unwrap();
        assert!(max_pages * page_size <= MAX_APPROVAL_DATABASE_BYTES);
        assert!(max_pages * page_size + page_size > MAX_APPROVAL_DATABASE_BYTES);
        assert_eq!(journal_limit, MAX_APPROVAL_WAL_BYTES);
        assert_eq!(autocheckpoint, APPROVAL_WAL_AUTOCHECKPOINT_PAGES);
    }

    #[test]
    fn oversized_existing_database_fails_initialization_closed() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("oversized.sqlite");
        {
            let conn = Connection::open(&database).unwrap();
            conn.execute("CREATE TABLE oversized(payload BLOB NOT NULL)", []).unwrap();
            let payload = vec![0x33u8; (MAX_APPROVAL_DATABASE_BYTES + 1024 * 1024) as usize];
            conn.execute("INSERT INTO oversized(payload) VALUES (?1)", params![payload]).unwrap();
        }
        let result = ApprovalStore::open(&database);
        assert!(matches!(result, Err(StoreError::StorageLimitExceeded)));
    }

    #[test]
    fn database_page_limit_rejects_growth_past_configured_ceiling() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("page-limit.sqlite");
        let store = ApprovalStore::open(&database).unwrap();
        let result = store.conn.execute(
            "CREATE TABLE storage_pressure(payload BLOB NOT NULL)", [],
        );
        assert!(result.is_ok());
        let payload = vec![0x5au8; (MAX_APPROVAL_DATABASE_BYTES + 1024 * 1024) as usize];
        let insert = store.conn.execute(
            "INSERT INTO storage_pressure(payload) VALUES (?1)", params![payload],
        );
        assert!(insert.is_err(), "SQLite must reject writes beyond max_page_count");
    }

    #[test]
    fn persistent_retention_deletes_only_one_bounded_batch() {
        let (mut store, _) = test_store_at(100);
        for index in 0..(APPROVAL_RETENTION_BATCH_SIZE + 44) {
            store.conn.execute(
                "INSERT INTO approval_challenges(challenge_id, action_digest, expires_at, consumed_at, process_epoch)
                 VALUES (?1, ?2, 1, NULL, 'old-epoch')",
                params![format!("{index:064x}"), "0".repeat(64)],
            ).unwrap();
        }
        let tx = store.conn.transaction_with_behavior(TransactionBehavior::Immediate).unwrap();
        let deleted = prune_persistent_challenges(&tx, 100, &store.process_epoch).unwrap();
        assert_eq!(deleted as u64, APPROVAL_RETENTION_BATCH_SIZE);
        assert_eq!(persisted_challenge_count(&tx).unwrap(), 44);
        tx.rollback().unwrap();
    }

    #[test]
    fn persistent_challenge_count_is_hard_capped() {
        let (mut store, _) = test_store_at(100);
        for index in 0..MAX_PERSISTED_APPROVAL_CHALLENGES {
            store.conn.execute(
                "INSERT INTO approval_challenges(challenge_id, action_digest, expires_at, consumed_at, process_epoch)
                 VALUES (?1, ?2, 1000, NULL, ?3)",
                params![format!("{index:064x}"), "0".repeat(64), store.process_epoch],
            ).unwrap();
        }
        let tx = store.conn.transaction_with_behavior(TransactionBehavior::Immediate).unwrap();
        assert!(persistent_capacity_reached(&tx).unwrap());
        tx.rollback().unwrap();
    }

    #[test]
    fn outstanding_challenge_cache_is_bounded_and_prunes_expired_entries() {
        let mut deadlines = HashMap::new();
        for index in 0..MAX_OUTSTANDING_APPROVAL_CHALLENGES {
            deadlines.insert(index.to_string(), Duration::from_secs(30));
        }
        assert!(matches!(
            prune_and_check_capacity(&mut deadlines, Duration::from_secs(0)),
            Err(StoreError::CapacityExceeded)
        ));
        assert_eq!(deadlines.len(), MAX_OUTSTANDING_APPROVAL_CHALLENGES);
        assert!(prune_and_check_capacity(&mut deadlines, Duration::from_secs(30)).is_ok());
        assert!(deadlines.is_empty());
    }

    #[test]
    fn monotonic_deadline_expires_even_when_wall_clock_does_not_advance() {
        let (mut store, clock) = test_store_at(100);
        let action = sample_action();
        let challenge = issue(&mut store, &action, 5).unwrap();
        clock.advance_monotonic(5_000);
        assert!(matches!(
            consume(&mut store,&challenge.challenge_id, &action),
            Err(StoreError::AlreadyConsumedOrExpired)
        ));
        assert!(!store.is_consumed(&challenge.challenge_id).unwrap());
    }

    #[test]
    fn legacy_action_only_rows_migrate_with_empty_snapshot_digest_and_fail_closed() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE approval_challenges (
                challenge_id TEXT PRIMARY KEY,
                action_digest TEXT NOT NULL,
                expires_at INTEGER NOT NULL,
                consumed_at INTEGER NULL
             );"
        ).unwrap();
        let action = sample_action();
        let challenge_id = "a".repeat(64);
        conn.execute(
            "INSERT INTO approval_challenges(challenge_id, action_digest, expires_at, consumed_at)
             VALUES (?1, ?2, 200, NULL)",
            params![challenge_id, action_digest(&action).unwrap()],
        ).unwrap();

        let clock = Arc::new(TestClock::new(100));
        let mut store = ApprovalStore::initialize_with_epoch(
            conn, clock, "new-process-epoch".into(),
        ).unwrap();
        let migrated_digest: String = store.conn.query_row(
            "SELECT snapshot_digest FROM approval_challenges WHERE challenge_id=?1",
            params![challenge_id],
            |row| row.get(0),
        ).unwrap();
        assert!(migrated_digest.is_empty(), "legacy action-only rows must not gain a snapshot binding");

        let snapshot = sample_snapshot_at(Duration::from_millis(0));
        assert!(matches!(
            store.consume(&challenge_id, &action, &snapshot),
            Err(StoreError::AlreadyConsumedOrExpired)
        ));
    }

    #[test]
    fn stale_snapshot_is_rejected_at_issue_and_consume() {
        let (mut store, clock) = test_store_at(100);
        let action = sample_action();
        let stale = sample_snapshot_at(Duration::from_millis(0));
        clock.advance_monotonic(MAX_APPROVAL_SNAPSHOT_AGE.as_millis() as u64 + 1);
        assert!(matches!(
            store.issue(&action, &stale, 30),
            Err(StoreError::Binding(ApprovalError::SnapshotStale))
        ));

        let (mut store, clock) = test_store_at(100);
        let fresh = sample_snapshot_at(Duration::from_millis(0));
        let challenge = store.issue(&action, &fresh, 30).unwrap();
        clock.advance_monotonic(MAX_APPROVAL_SNAPSHOT_AGE.as_millis() as u64 + 1);
        assert!(matches!(
            store.consume(&challenge.challenge_id, &action, &fresh),
            Err(StoreError::Binding(ApprovalError::SnapshotStale))
        ));
        assert!(!store.is_consumed(&challenge.challenge_id).unwrap());
    }

    #[test]
    fn changed_snapshot_identity_is_rejected_without_consuming() {
        let (mut store, _) = test_store_at(100);
        let action = sample_action();
        let original = sample_snapshot_at(Duration::from_millis(0));
        let challenge = store.issue(&action, &original, 30).unwrap();
        let bytes = b"captured";
        let changed_contract = crate::snapshot_payload::build_snapshot_contract(
            "workspace-test", "src/lib.rs", "different-file-id", bytes,
        ).unwrap();
        let changed = CapturedWorkspaceSnapshot::new_at(
            changed_contract, bytes.to_vec(), Duration::from_millis(0),
        ).unwrap();
        assert!(matches!(
            store.consume(&challenge.challenge_id, &action, &changed),
            Err(StoreError::Binding(ApprovalError::SnapshotDigestMismatch))
        ));
        assert!(!store.is_consumed(&challenge.challenge_id).unwrap());
    }

    #[test]
    fn action_mismatch_does_not_consume() {
        let (mut store, _) = test_store_at(100);
        let action = sample_action();
        let challenge = issue(&mut store,&action, 30).unwrap();
        let modified = ProposedAction::ReadFile { path: "src/lib.rs".into(), max_bytes: Some(2048) };
        assert!(matches!(
            consume(&mut store, &challenge.challenge_id, &modified),
            Err(StoreError::Binding(ApprovalError::DigestMismatch))
        ));
        assert!(!store.is_consumed(&challenge.challenge_id).unwrap());
    }

    #[test]
    fn issued_challenges_have_distinct_random_ids() {
        let (mut store, _) = test_store_at(100);
        let action = sample_action();
        let first = issue(&mut store,&action, 30).unwrap();
        let second = issue(&mut store,&action, 30).unwrap();
        assert_ne!(first.challenge_id, second.challenge_id);
    }

    #[test]
    fn unknown_challenge_rejected() {
        let (mut store, _) = test_store_at(100);
        assert!(matches!(
            consume(&mut store, &"0".repeat(64), &sample_action()),
            Err(StoreError::NotFound)
        ));
    }

    #[test]
    fn issue_rejects_invalid_ttl() {
        let (mut store, _) = test_store_at(100);
        let action = sample_action();
        assert!(matches!(issue(&mut store, &action, 0), Err(StoreError::InvalidChallenge)));
        assert!(matches!(
            issue(&mut store, &action, MAX_APPROVAL_TTL_SECONDS + 1),
            Err(StoreError::InvalidChallenge)
        ));
    }

    #[test]
    fn challenge_ids_are_256_bit_hex_values() {
        let (mut store, _) = test_store_at(100);
        let challenge = issue(&mut store, &sample_action(), 30).unwrap();
        assert!(is_valid_challenge_id(&challenge.challenge_id));
        assert_eq!(challenge.challenge_id.len(), 64);
    }

    #[test]
    fn issue_rejects_expiry_outside_sqlite_integer_range() {
        let (mut store, _) = test_store_at(i64::MAX as u64);
        assert!(matches!(
            issue(&mut store, &sample_action(), 1),
            Err(StoreError::InvalidChallenge)
        ));
    }

    #[test]
    fn clock_failure_fails_closed_without_consuming() {
        let (mut store, clock) = test_store_at(100);
        let action = sample_action();
        let challenge = issue(&mut store,&action, 30).unwrap();
        clock.fail_with(ApprovalClockError::ClockMovedBackward);
        assert!(matches!(
            consume(&mut store,&challenge.challenge_id, &action),
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
        let challenge = issue(&mut first, &action, 60).unwrap();

        let second_clock: Arc<dyn ApprovalClock> = Arc::new(TestClock::new(1_001));
        let mut second = ApprovalStore::initialize_with_epoch(
            Connection::open(&database).unwrap(), second_clock, "process-two".into()
        ).unwrap();

        assert!(matches!(
            consume(&mut first, &challenge.challenge_id, &action),
            Err(StoreError::StaleRuntime)
        ));
        assert!(matches!(
            consume(&mut second, &challenge.challenge_id, &action),
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
        let challenge = issue(&mut issuer, &action, 1).unwrap();
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
            match consume(&mut store, &first_challenge_id, &first_action) {
                Ok(()) => true,
                Err(StoreError::AlreadyConsumedOrExpired) => false,
                Err(error) => panic!("unexpected first consumer error: {error:?}"),
            }
        });

        let second_barrier = Arc::clone(&barrier);
        let second_thread = std::thread::spawn(move || {
            let mut store = second;
            second_barrier.wait();
            match consume(&mut store, &second_challenge_id, &action) {
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
