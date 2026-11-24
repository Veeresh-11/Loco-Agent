//! Authority-owned wall-clock access for approval freshness.
//!
//! The process-wide guard detects backward wall-clock movement while this
//! process is alive. It cannot detect rollback across process or machine restart.

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalClockError {
    BeforeUnixEpoch,
    TimeOutOfRange,
    ClockMovedBackward,
    ClockStatePoisoned,
}

pub trait ApprovalClock: Send + Sync {
    fn now_unix_seconds(&self) -> Result<u64, ApprovalClockError>;
    /// Monotonic elapsed time from an arbitrary process-local origin.
    fn monotonic_now(&self) -> Result<Duration, ApprovalClockError>;
}

#[derive(Debug, Default)]
pub struct SystemApprovalClock;

static LAST_SEEN_UNIX: OnceLock<Mutex<Option<u64>>> = OnceLock::new();
static MONOTONIC_ORIGIN: OnceLock<Instant> = OnceLock::new();

pub(crate) fn process_monotonic_now() -> Duration {
    MONOTONIC_ORIGIN.get_or_init(Instant::now).elapsed()
}

fn enforce_non_decreasing(
    last_seen: &mut Option<u64>,
    now: u64,
) -> Result<u64, ApprovalClockError> {
    if last_seen.is_some_and(|previous| now < previous) {
        return Err(ApprovalClockError::ClockMovedBackward);
    }
    *last_seen = Some(now);
    Ok(now)
}

impl ApprovalClock for SystemApprovalClock {
    fn now_unix_seconds(&self) -> Result<u64, ApprovalClockError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ApprovalClockError::BeforeUnixEpoch)?
            .as_secs();
        let state = LAST_SEEN_UNIX.get_or_init(|| Mutex::new(None));
        let mut last_seen = state.lock().map_err(|_| ApprovalClockError::ClockStatePoisoned)?;
        enforce_non_decreasing(&mut last_seen, now)
    }

    fn monotonic_now(&self) -> Result<Duration, ApprovalClockError> {
        Ok(process_monotonic_now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_rejects_backward_wall_time() {
        let mut last_seen = Some(100);
        assert_eq!(enforce_non_decreasing(&mut last_seen, 99), Err(ApprovalClockError::ClockMovedBackward));
        assert_eq!(last_seen, Some(100));
    }

    #[test]
    fn clock_accepts_equal_or_forward_wall_time() {
        let mut last_seen = Some(100);
        assert_eq!(enforce_non_decreasing(&mut last_seen, 100), Ok(100));
        assert_eq!(enforce_non_decreasing(&mut last_seen, 101), Ok(101));
        assert_eq!(last_seen, Some(101));
    }
}
