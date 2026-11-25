//! Per-process protocol v2 lifecycle state.
//!
//! This module enforces handshake ordering, process-lifetime request-ID
//! uniqueness, a bounded request budget, and terminal shutdown. It deliberately
//! grants no filesystem or execution authority; handlers must separately
//! validate DTOs and resolve workspace handles through WorkspaceRegistry.

use std::collections::HashSet;

use crate::authority_protocol_v2::valid_request_id;

pub const MAX_PROTOCOL_V2_REQUESTS_PER_PROCESS: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolV2SessionError {
    InvalidRequestId,
    HandshakeRequired,
    UnexpectedHandshake,
    DuplicateRequestId,
    RequestLimitExceeded,
    SessionTerminated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionPhase {
    AwaitingHello,
    Active,
    Terminated,
}

/// Sequential state for one stdio authority process.
///
/// The current sidecar permits one outstanding request at a time. This type is
/// intentionally not synchronized; introducing concurrent handlers requires a
/// separate design for request ordering and adapter serialization.
pub struct AuthorityProtocolV2Session {
    phase: SessionPhase,
    seen_request_ids: HashSet<String>,
    accepted_requests: usize,
}

impl Default for AuthorityProtocolV2Session {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthorityProtocolV2Session {
    pub fn new() -> Self {
        Self {
            phase: SessionPhase::AwaitingHello,
            seen_request_ids: HashSet::new(),
            accepted_requests: 0,
        }
    }

    /// Accept the one and only initial hello request.
    pub fn accept_hello(&mut self, request_id: &str) -> Result<(), ProtocolV2SessionError> {
        match self.phase {
            SessionPhase::AwaitingHello => {}
            SessionPhase::Active => return Err(ProtocolV2SessionError::UnexpectedHandshake),
            SessionPhase::Terminated => return Err(ProtocolV2SessionError::SessionTerminated),
        }
        self.accept_unique_id(request_id)?;
        self.phase = SessionPhase::Active;
        Ok(())
    }

    /// Accept a non-hello operation after handshake.
    pub fn accept_operation(&mut self, request_id: &str) -> Result<(), ProtocolV2SessionError> {
        match self.phase {
            SessionPhase::AwaitingHello => return Err(ProtocolV2SessionError::HandshakeRequired),
            SessionPhase::Active => {}
            SessionPhase::Terminated => return Err(ProtocolV2SessionError::SessionTerminated),
        }
        self.accept_unique_id(request_id)
    }

    /// Accept shutdown and permanently close the protocol session.
    pub fn accept_shutdown(&mut self, request_id: &str) -> Result<(), ProtocolV2SessionError> {
        self.accept_operation(request_id)?;
        self.phase = SessionPhase::Terminated;
        Ok(())
    }

    pub fn is_active(&self) -> bool {
        self.phase == SessionPhase::Active
    }

    pub fn is_terminated(&self) -> bool {
        self.phase == SessionPhase::Terminated
    }

    pub fn accepted_request_count(&self) -> usize {
        self.accepted_requests
    }

    fn accept_unique_id(&mut self, request_id: &str) -> Result<(), ProtocolV2SessionError> {
        if !valid_request_id(request_id) {
            return Err(ProtocolV2SessionError::InvalidRequestId);
        }
        if self.seen_request_ids.contains(request_id) {
            return Err(ProtocolV2SessionError::DuplicateRequestId);
        }
        if self.accepted_requests >= MAX_PROTOCOL_V2_REQUESTS_PER_PROCESS {
            return Err(ProtocolV2SessionError::RequestLimitExceeded);
        }
        self.seen_request_ids.insert(request_id.to_owned());
        self.accepted_requests += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_one_hello_before_operations() {
        let mut session = AuthorityProtocolV2Session::new();
        assert_eq!(
            session.accept_operation("op-1"),
            Err(ProtocolV2SessionError::HandshakeRequired)
        );
        assert_eq!(session.accepted_request_count(), 0);
        session.accept_hello("hello-1").unwrap();
        assert!(session.is_active());
        session.accept_operation("op-1").unwrap();
        assert_eq!(session.accepted_request_count(), 2);
    }

    #[test]
    fn rejects_duplicate_and_invalid_request_ids_for_process_lifetime() {
        let mut session = AuthorityProtocolV2Session::new();
        session.accept_hello("hello-1").unwrap();
        assert_eq!(
            session.accept_operation("hello-1"),
            Err(ProtocolV2SessionError::DuplicateRequestId)
        );
        assert_eq!(
            session.accept_operation("contains whitespace"),
            Err(ProtocolV2SessionError::InvalidRequestId)
        );
        session.accept_operation("unique-2").unwrap();
        assert_eq!(session.accepted_request_count(), 2);
    }

    #[test]
    fn rejects_second_handshake_without_changing_active_state() {
        let mut session = AuthorityProtocolV2Session::new();
        session.accept_hello("hello-1").unwrap();
        assert_eq!(
            session.accept_hello("hello-2"),
            Err(ProtocolV2SessionError::UnexpectedHandshake)
        );
        assert!(session.is_active());
        session.accept_operation("op-1").unwrap();
    }

    #[test]
    fn shutdown_is_terminal_and_uses_a_unique_request_id() {
        let mut session = AuthorityProtocolV2Session::new();
        session.accept_hello("hello-1").unwrap();
        session.accept_shutdown("shutdown-1").unwrap();
        assert!(session.is_terminated());
        assert_eq!(
            session.accept_operation("op-2"),
            Err(ProtocolV2SessionError::SessionTerminated)
        );
        assert_eq!(
            session.accept_hello("hello-2"),
            Err(ProtocolV2SessionError::SessionTerminated)
        );
    }

    #[test]
    fn enforces_bounded_process_request_budget() {
        let mut session = AuthorityProtocolV2Session::new();
        session.accept_hello("hello-0").unwrap();
        for index in 1..MAX_PROTOCOL_V2_REQUESTS_PER_PROCESS {
            session.accept_operation(&format!("op-{index}")).unwrap();
        }
        assert_eq!(session.accepted_request_count(), MAX_PROTOCOL_V2_REQUESTS_PER_PROCESS);
        assert_eq!(
            session.accept_operation("one-too-many"),
            Err(ProtocolV2SessionError::RequestLimitExceeded)
        );
        assert_eq!(session.accepted_request_count(), MAX_PROTOCOL_V2_REQUESTS_PER_PROCESS);
    }

    #[test]
    fn failed_requests_do_not_consume_request_budget_or_reserve_ids() {
        let mut session = AuthorityProtocolV2Session::new();
        session.accept_hello("hello-1").unwrap();
        assert_eq!(
            session.accept_operation("bad id"),
            Err(ProtocolV2SessionError::InvalidRequestId)
        );
        session.accept_operation("bad-id").unwrap();
        assert_eq!(session.accepted_request_count(), 2);
    }
}
