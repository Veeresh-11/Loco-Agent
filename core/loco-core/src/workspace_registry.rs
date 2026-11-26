//! Process-local registry for Rust-owned workspace snapshot adapters.
//!
//! Handles are opaque lookup identifiers, not OS capabilities or user
//! authentication. The registry owns each adapter (and therefore its retained
//! root handle) until explicit revocation or registry drop. No path is returned
//! from this module and no operation other than bounded snapshot capture exists.

use std::collections::HashMap;
use std::path::Path;

use crate::snapshot_payload::{
    CapturedWorkspaceSnapshot, SnapshotCaptureRequest, SnapshotPayloadError,
    WorkspaceSnapshotAdapter,
};

/// Hard cap for concurrently registered workspace roots in one authority process.
pub const MAX_REGISTERED_WORKSPACES: usize = 16;
const HANDLE_BYTES: usize = 32;
const HANDLE_GENERATION_ATTEMPTS: usize = 4;

/// Opaque, process-local reference to an adapter owned by WorkspaceRegistry.
///
/// The string form is intended only for the sidecar protocol. It must not be
/// persisted or treated as proof of user approval or as a filesystem path.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WorkspaceHandle(String);

impl WorkspaceHandle {
    /// Opaque wire representation. Callers cannot construct a valid handle
    /// from a string; only registry registration can mint one.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug)]
pub enum WorkspaceRegistryError {
    EntropyUnavailable,
    CapacityExceeded,
    HandleGenerationFailed,
    UnknownOrRevokedHandle,
    Capture(SnapshotPayloadError),
}

impl From<SnapshotPayloadError> for WorkspaceRegistryError {
    fn from(value: SnapshotPayloadError) -> Self {
        Self::Capture(value)
    }
}

/// Owns workspace adapters for the lifetime of the authority process.
///
/// This type intentionally has no internal locking: the current stdio
/// authority handles one request at a time. If request concurrency is added,
/// synchronization and per-workspace capture serialization must be designed
/// explicitly rather than silently sharing adapters.
#[derive(Default)]
pub struct WorkspaceRegistry {
    adapters: HashMap<WorkspaceHandle, Box<dyn WorkspaceSnapshotAdapter>>,
}

#[derive(Debug)]
pub enum WorkspaceRegistrationError {
    Factory(crate::workspace_factory::WorkspaceFactoryError),
    Registry(WorkspaceRegistryError),
}

impl WorkspaceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct and register an adapter from an explicitly selected workspace
    /// root. The root is untrusted input and is never inferred from a proposal.
    pub fn register_selected_workspace(
        &mut self,
        selected_root: &Path,
        workspace_id: impl Into<String>,
    ) -> Result<WorkspaceHandle, WorkspaceRegistrationError> {
        let adapter = crate::workspace_factory::create_workspace_adapter(
            selected_root,
            workspace_id,
        )
        .map_err(WorkspaceRegistrationError::Factory)?;
        self.register(adapter)
            .map_err(WorkspaceRegistrationError::Registry)
    }

    /// Register an already-constructed native adapter and retain ownership.
    ///
    /// The caller must construct the adapter from the explicit workspace
    /// selection/trust flow. This method does not authenticate that caller or
    /// validate a root path itself.
    pub fn register(
        &mut self,
        adapter: Box<dyn WorkspaceSnapshotAdapter>,
    ) -> Result<WorkspaceHandle, WorkspaceRegistryError> {
        if self.adapters.len() >= MAX_REGISTERED_WORKSPACES {
            return Err(WorkspaceRegistryError::CapacityExceeded);
        }

        for _ in 0..HANDLE_GENERATION_ATTEMPTS {
            let handle = WorkspaceHandle(generate_handle_id()?);
            if !self.adapters.contains_key(&handle) {
                self.adapters.insert(handle.clone(), adapter);
                return Ok(handle);
            }
        }

        Err(WorkspaceRegistryError::HandleGenerationFailed)
    }

    /// Capture through the adapter retained for this process-local handle.
    /// Caller-supplied snapshot metadata is not accepted by this API.
    pub fn capture(
        &self,
        handle: &WorkspaceHandle,
        request: &SnapshotCaptureRequest,
    ) -> Result<CapturedWorkspaceSnapshot, WorkspaceRegistryError> {
        let adapter = self.adapters.get(handle)
            .ok_or(WorkspaceRegistryError::UnknownOrRevokedHandle)?;
        adapter.capture(request).map_err(WorkspaceRegistryError::Capture)
    }

    /// Capture using the opaque wire handle without allowing callers to
    /// construct a WorkspaceHandle. The lookup is process-local and exact.
    pub fn capture_by_wire_handle(
        &self,
        wire_handle: &str,
        request: &SnapshotCaptureRequest,
    ) -> Result<CapturedWorkspaceSnapshot, WorkspaceRegistryError> {
        let (handle, adapter) = self.adapters.iter()
            .find(|(handle, _)| handle.as_str() == wire_handle)
            .ok_or(WorkspaceRegistryError::UnknownOrRevokedHandle)?;
        let _ = handle;
        adapter.capture(request).map_err(WorkspaceRegistryError::Capture)
    }

    /// Revoke using an opaque wire handle. Unknown and already-revoked values
    /// are intentionally indistinguishable to protocol callers.
    pub fn revoke_by_wire_handle(&mut self, wire_handle: &str) -> bool {
        let handle = self.adapters.keys()
            .find(|handle| handle.as_str() == wire_handle)
            .cloned();
        handle.is_some_and(|handle| self.adapters.remove(&handle).is_some())
    }

    /// Revoke a handle and drop its retained adapter/root handle.
    pub fn revoke(&mut self, handle: &WorkspaceHandle) -> bool {
        self.adapters.remove(handle).is_some()
    }

    pub fn registered_count(&self) -> usize {
        self.adapters.len()
    }
}

fn generate_handle_id() -> Result<String, WorkspaceRegistryError> {
    let mut random = [0u8; HANDLE_BYTES];
    getrandom::getrandom(&mut random)
        .map_err(|_| WorkspaceRegistryError::EntropyUnavailable)?;

    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(HANDLE_BYTES * 2);
    for byte in random {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    Ok(encoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot_payload::{build_snapshot_contract, SnapshotCaptureRequest};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TestAdapter {
        captures: AtomicUsize,
    }

    impl WorkspaceSnapshotAdapter for TestAdapter {
        fn capture(
            &self,
            request: &SnapshotCaptureRequest,
        ) -> Result<CapturedWorkspaceSnapshot, SnapshotPayloadError> {
            self.captures.fetch_add(1, Ordering::SeqCst);
            let bytes = b"registry-test-payload".to_vec();
            let contract = build_snapshot_contract(
                "registry-test-workspace",
                request.relative_path(),
                "test-device:inode:1",
                &bytes,
            )?;
            CapturedWorkspaceSnapshot::new(contract, bytes)
        }
    }

    fn adapter() -> Box<dyn WorkspaceSnapshotAdapter> {
        Box::new(TestAdapter { captures: AtomicUsize::new(0) })
    }

    #[test]
    fn registration_mints_opaque_random_handle_and_owns_adapter() {
        let mut registry = WorkspaceRegistry::new();
        let handle = registry.register(adapter()).unwrap();

        assert_eq!(handle.as_str().len(), 64);
        assert!(handle.as_str().bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_eq!(registry.registered_count(), 1);

        let request = SnapshotCaptureRequest::new("src/main.rs", 1024).unwrap();
        let captured = registry.capture(&handle, &request).unwrap();
        assert_eq!(captured.bytes(), b"registry-test-payload");
    }

    #[test]
    fn fabricated_handle_is_rejected() {
        let registry = WorkspaceRegistry::new();
        let fabricated = WorkspaceHandle("a".repeat(64));
        let request = SnapshotCaptureRequest::new("README.md", 1024).unwrap();

        assert!(matches!(
            registry.capture(&fabricated, &request),
            Err(WorkspaceRegistryError::UnknownOrRevokedHandle)
        ));
    }

    #[test]
    fn revoked_handle_cannot_capture_again() {
        let mut registry = WorkspaceRegistry::new();
        let handle = registry.register(adapter()).unwrap();
        assert!(registry.revoke(&handle));
        assert!(!registry.revoke(&handle));

        let request = SnapshotCaptureRequest::new("README.md", 1024).unwrap();
        assert!(matches!(
            registry.capture(&handle, &request),
            Err(WorkspaceRegistryError::UnknownOrRevokedHandle)
        ));
        assert_eq!(registry.registered_count(), 0);
    }

    #[test]
    fn registry_enforces_registration_capacity() {
        let mut registry = WorkspaceRegistry::new();
        for _ in 0..MAX_REGISTERED_WORKSPACES {
            registry.register(adapter()).unwrap();
        }

        assert!(matches!(
            registry.register(adapter()),
            Err(WorkspaceRegistryError::CapacityExceeded)
        ));
        assert_eq!(registry.registered_count(), MAX_REGISTERED_WORKSPACES);
    }

    #[test]
    fn handles_are_distinct_for_separate_registrations() {
        let mut registry = WorkspaceRegistry::new();
        let first = registry.register(adapter()).unwrap();
        let second = registry.register(adapter()).unwrap();
        assert_ne!(first, second);
    }

    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    #[test]
    fn selected_workspace_is_constructed_then_owned_by_registry() {
        let parent = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
        let selected = parent.path().join("selected");
        std::fs::create_dir(&selected).unwrap();
        std::fs::write(selected.join("probe.txt"), b"registered-selection").unwrap();

        let mut registry = WorkspaceRegistry::new();
        let handle = registry
            .register_selected_workspace(&selected, "registered-selection")
            .unwrap();
        let request = SnapshotCaptureRequest::new("probe.txt", 1024).unwrap();
        let captured = registry.capture(&handle, &request).unwrap();
        assert_eq!(captured.bytes(), b"registered-selection");
        assert_eq!(registry.registered_count(), 1);
    }
}
