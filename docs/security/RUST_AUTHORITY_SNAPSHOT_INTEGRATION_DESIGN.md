# Rust Authority Snapshot Integration — Design and Gate

**Status:** Design checkpoint; implementation is not authorized by this document  
**Base:** PR #128 native-platform + snapshot-approval integration  
**Scope:** Read-only, proposal-evaluation sidecar boundary  
**Security posture:** Fail closed; no workspace writes or command execution

## 1. Current implementation facts

The current `loco-authority` stdio process accepts a `hello`, `evaluate`, and `shutdown` message. The evaluate request contains an `ActionProposal`; the handler calls the pure `evaluate()` policy function and returns a decision. It does not open a workspace root, invoke a snapshot adapter, or call `ApprovalStore`.

The native snapshot adapters are separate Rust APIs. They acquire a retained root handle and expose `capture()` through `WorkspaceSnapshotAdapter`. The captured value carries private snapshot bytes and a Rust-process monotonic capture instant. The approval store accepts `ProposedAction` plus `CapturedWorkspaceSnapshot`, not an untrusted serialized snapshot DTO.

The current TypeScript sidecar client sends proposal-evaluation messages only. Its documentation correctly says an `allow` result is not an execution capability.

Consequently, the approval prototype is not yet in the production request path. This is an intentional gap, not a partially completed authorization path.

## 2. Trust-boundary gaps to close before wiring

1. **No workspace-root registration protocol exists.** A proposal path is not a workspace root and must never be used to infer one.
2. **No Rust-owned workspace registry exists.** The authority process needs to own the adapter and retained root handle for the lifetime of the registration.
3. **No capture-to-approval call path exists.** The authority handler currently does not call an adapter or `ApprovalStore::issue/consume`.
4. **Wire and internal action types differ by layer.** The request contains `ActionProposal`; approval binds its `ProposedAction`. The conversion must preserve the exact action and must not drop fields.
5. **The approval wire DTO is not yet aligned with snapshot binding.** The current `ApprovalChallengeContract` exposes action digest and expiry, but the snapshot-bound challenge also carries a snapshot digest. Any future DTO change needs a versioned contract decision; the TypeScript side must treat digests as opaque.
6. **A policy decision is not authority to perform I/O.** Existing `allow` for a basic read is a policy result only. It must not trigger a read, patch, or command in the extension or sidecar.
7. **Capture is not an atomic filesystem snapshot.** Adapter metadata checks and content hashing detect important changes but do not freeze a file against concurrent mutation. A future operation must reacquire immediately before use and compare the full binding.

## 3. Proposed staged architecture

### Stage A — Rust-owned workspace registration (design and tests first)

- Registration must be an explicit, separate protocol operation; never derive a root from `ProposedAction.path`.
- The caller supplies only a candidate root selected through the host's explicit workspace-selection/trust flow. That path is untrusted input, not proof of identity or authority.
- Rust validates the candidate and creates the platform-specific adapter, retaining its opened root handle in an in-process registry.
- Rust generates an unpredictable, process-local workspace handle identifier. Subsequent capture requests refer to that identifier, not a caller-supplied root path.
- Unknown, expired, revoked, or process-restarted handles fail closed. Handles are never serialized as filesystem paths or persisted as reusable capabilities.
- Registration must enforce an explicit maximum number of roots, identifier length limits, and deterministic cleanup on shutdown.
- A successful registration means only that Rust retained a root handle. It does not mean the caller is authenticated or that a proposal is approved.

**Important limitation:** This process-local handle is an object reference inside the sidecar protocol, not a defense against arbitrary same-user malware or a substitute for platform-authenticator evidence. The existing platform-authenticator research gate remains separate.

### Stage B — Rust-owned capture operation

- Capture requests contain a registered workspace handle, normalized relative path, and bounded byte limit.
- Rust resolves the handle from its registry and invokes the platform adapter; it never accepts caller-provided file identity, file kind, content digest, capture time, or serialized snapshot as trusted data.
- The adapter-created `CapturedWorkspaceSnapshot` remains inside Rust. Only bounded, explicitly necessary display data may be returned to the parent; raw snapshot metadata must not be accepted back as authority.
- The capture API is file-only for v1. Command actions remain unsupported.
- Any platform without a validated adapter or required primitive returns a hard denial. No portable pathname fallback is permitted.

### Stage C — Approval challenge lifecycle

- Convert `ActionProposal.action` to the exact internal `ProposedAction` without changing any field.
- For file-scoped actions, require a fresh adapter capture and call `ApprovalStore::issue(action, capture, ttl)`.
- The challenge binds the canonical action digest and snapshot-binding digest. The store owns freshness, expiry, one-time consumption, and process-epoch checks.
- Any user-confirmation/authenticator ceremony must be designed as a separate authenticated protocol. A UI click or a returned challenge is not human approval.
- At consumption, Rust must obtain a fresh capture from the registered root and require the complete snapshot binding to match. A caller-supplied digest is never evidence.
- A changed path, workspace, file kind, byte length, content digest, file identity, stale capture, missing identity, or unsupported action denies.
- Challenge consumption does not authorize execution. It only records successful verification of this prototype binding.

### Stage D — Policy and protocol semantics

- Keep policy evaluation pure and separate from acquisition and approval verification.
- Preserve the existing `decision` semantics. In particular, `allow` remains a policy result and must not imply that a read was performed or that an executor may run.
- Introduce explicit response fields or message types for capture and approval lifecycle only after the versioned protocol contract is reviewed.
- If the wire approval DTO changes, publish a new contract version or a documented backward-compatible extension. Include `snapshotDigest` only as opaque display/transport data; never accept it back as proof.
- Keep stdout exclusively for bounded protocol frames and stderr for diagnostics. Continue rejecting unknown fields, duplicate request IDs, oversized frames, and invalid state transitions.

## 4. Required negative and lifecycle tests

### Registration
- Reject empty, relative, symlinked, unsupported, or otherwise invalid roots.
- Reject unregistered and fabricated workspace handles.
- Verify handles are process-local, bounded, revoked on shutdown, and invalid after restart.
- Verify the root handle remains bound to the originally opened object if the display pathname is renamed or replaced.
- Verify platform-specific fail-closed behavior when required kernel primitives are unavailable.

### Capture
- Reject traversal, absolute paths, symlinks/reparse points, hard links where the platform policy forbids them, directories, special files, oversized files, and mount/volume boundary escapes.
- Reject a capture whose content length or digest does not match the captured bytes.
- Reject stale captures and monotonic-clock failures.
- Verify no TypeScript-supplied metadata can enter the trusted capture object.

### Approval
- Issue and consume with the same action and full snapshot binding exactly once.
- Reject changed workspace ID, relative path, file kind, byte length, content digest, file identity, and contract version.
- Reject read path mismatch and patch target digest absence/mismatch.
- Reject `RunCommand` under the file-only contract.
- Reject replay, concurrent double-consumption, expired challenges, restart-epoch mismatch, storage errors, and capacity exhaustion.
- Verify a fresh identical recapture can match only when all binding fields are identical; changed recapture denies.

### Protocol / authority
- Reject capture or approval messages before handshake, after shutdown, with duplicate request IDs, unknown fields, invalid handles, and oversized frames.
- Verify a policy `allow` never causes a filesystem read, write, patch, or command.
- Verify no response or client-side API exposes an execution-capability token.
- Verify unsupported platforms and missing adapters deny without fallback.

## 5. Explicit non-goals and release gates

- No workspace write, patch application, process launch, or command execution.
- No claim of authenticated human approval.
- No claim that the parent extension's self-reported name/version authenticates its process.
- No claim that local SQLite is protected against same-user tampering.
- No generic software key, UI click, or policy `allow` may authorize privileged operations.
- No cross-platform fallback from native handle-relative acquisition to pathname-based access.
- Independent security review and the platform-authenticator decision remain mandatory before any privileged executor is considered.

## 6. Implementation order

1. Review and accept this trust-boundary design.
2. Add Rust-only registry types and unit tests without changing the stdio protocol.
3. Add a narrowly scoped registration/capture protocol proposal and contract tests.
4. Integrate adapters into the authority process behind explicit protocol states; keep the registry and captures Rust-owned.
5. Integrate approval issuance/consumption using fresh adapter captures; do not return execution authority.
6. Add cross-platform CI and adversarial lifecycle tests.
7. Obtain independent security review before considering any executor.

**Acceptance criterion for this milestone:** the sidecar can register a selected workspace root, retain it in Rust, capture a file through the platform adapter, and bind an approval challenge to that capture — while still performing no file read on behalf of a proposal, no writes, and no command execution.