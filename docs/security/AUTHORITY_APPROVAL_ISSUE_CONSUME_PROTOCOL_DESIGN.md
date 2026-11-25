# Authority Approval Issue/Consume — Protocol Design

**Status:** Proposed wire-contract design; implementation not yet authorized  
**Date:** 2026-11-25  
**Base:** Protocol v2 registration/capture chain (#132–#138)  
**Scope:** Rust-owned, single-use action + snapshot binding challenge lifecycle  
**Security posture:** Fail closed; no authenticated human approval claim; no writes, patch application, or command execution

## 1. Current verified implementation boundary

The protocol v2 draft stack already implements:

- Strict v2 DTOs and lexical validation (#133).
- A process-local session state machine with one handshake, unique request IDs, bounded request count and terminal shutdown (#135).
- Rust-owned workspace registration, capture acknowledgement and revocation through native adapters and opaque process-local handles (#135).
- A TypeScript v2 client (#136).
- Direct adversarial handler tests (#137) and TypeScript portable-path parity (#138).

Exact-head CI observed for this stack on 2026-11-25:

- #133 Rust Core Tests #45561353483 and Contract Validation #45561353697 passed.
- #135 Rust Core Tests #45563628930 and Contract Validation #45563628849 passed.
- #136 TypeScript CI #45564926678 passed.
- #137 Contract Validation #45568074629 and Rust Core Tests #45568074945 passed.
- #138 TypeScript CI #45568062826 passed.

All are still open draft PRs in a stacked dependency chain. Passing CI does not merge or independently approve the design.

The current v2 capture handler discards the captured bytes and metadata after returning only `captured` and `byteLength`. This is intentional. The TypeScript client cannot supply a serialized snapshot as trusted input.

The Rust approval prototype already has:
- `ApprovalStore::issue(action, captured_snapshot, ttl)`
- `ApprovalStore::consume(challenge_id, action, fresh_captured_snapshot)`
- Action and canonical snapshot binding digests
- Rust-owned TTL and monotonic freshness checks
- Single-use consumption, process-epoch invalidation and bounded storage
- File-scoped action matching; command actions are unsupported

However, it is not called from the v2 handler. It is also explicitly not authenticated human approval: no trusted platform-authenticator assertion is currently verified by the Rust authority.

## 2. Design decision: separate binding from human approval

Do not call a successful binding check an approval grant.

There are three distinct events:

1. **Issue binding challenge:** Rust captures the requested file through the registered adapter and stores a challenge bound to the exact action and snapshot.
2. **Authenticate user intent:** A separately designed platform-authenticator ceremony proves user presence/verification and binds the authenticator assertion to the challenge. This is not implemented by this milestone.
3. **Consume binding challenge:** Rust recaptures the same file and verifies action, snapshot identity/content, freshness, process epoch, expiry and one-time use. This confirms the binding is still valid; it does not itself authenticate the user or authorize an operation.

Until step 2 is implemented and independently reviewed, the protocol may expose only a clearly named *binding challenge* lifecycle. It must not expose `approved`, `authorized`, `grant`, or an execution token.

## 3. Proposed wire messages

Keep protocol v2.0 unchanged for existing clients. Introduce a separately versioned additive protocol contract **2.1** for approval-binding messages. The server must advertise supported protocol versions/capabilities explicitly; it must not silently reinterpret a 2.0 message. A client that only supports 2.0 continues to use registration/capture/revocation only.

The 2.1 handshake must declare the exact protocol version and required capability set. If the server does not advertise `approval_binding_challenge`, the client must not attempt these messages. Unknown fields remain rejected.

### 3.1 Issue binding challenge

Request fields:

- `protocolVersion: "2.1"`
- `messageType: "issueBindingChallenge"`
- `requestId`: unique process-lifetime request ID
- `workspaceHandle`: opaque process-local handle issued by Rust
- `action`: complete versioned `ProposedAction` DTO; no lossy field conversion
- `ttlSeconds`: bounded integer (1–300)

Rules:

- Only file-scoped `ReadFile` and `ProposePatch` actions are accepted in this milestone.
- `RunCommand` is rejected as unsupported; it has no file snapshot binding and remains disabled.
- Rust resolves the workspace handle, derives the capture path from the action itself, constructs a bounded capture request and captures through the retained adapter.
- Rust does not accept snapshot digest, file identity, byte length, capture timestamp, expiry, or action digest from the caller.
- `ProposePatch` must include an expected target digest equal to the captured target digest.
- Rust passes the exact action and internally captured snapshot to `ApprovalStore::issue`.
- A successful response contains only `challengeId`, `actionDigest`, `snapshotDigest`, and `expiresAtUnix` for display/correlation. These values are opaque and never accepted back as proof.
- Do not return captured bytes or trusted file identity to TypeScript.

Response type: `bindingChallengeIssued`. It means only that Rust stored a short-lived binding challenge.

### 3.2 Consume binding challenge

Request fields:

- `protocolVersion: "2.1"`
- `messageType: "consumeBindingChallenge"`
- `requestId`
- `workspaceHandle`
- `challengeId`
- `action`: complete action again; Rust recomputes its canonical digest

Rules:

- Rust resolves the live process-local workspace handle.
- Rust derives the path and capture limit from the action, then performs a new capture through the native adapter immediately before calling `ApprovalStore::consume`.
- The store verifies challenge expiry, process epoch, exact action digest, canonical snapshot binding, capture freshness and single-use semantics.
- A failed mismatch must not consume the challenge unless the store's documented security policy explicitly invalidates it. Errors must not reveal whether a challenge ID exists to an untrusted caller.
- Only one of two concurrent consume attempts may succeed.
- A successful response is `bindingVerified: true`, never `approved: true` or `authorized: true`.
- This response grants no permission to read/write/patch/execute. It only confirms the prototype binding check.

### 3.3 Revoke and shutdown

- Workspace revocation invalidates access to future captures. All outstanding challenges associated with that workspace registration must also become unusable.
- Shutdown, EOF, process restart, or process-epoch rotation invalidates all outstanding challenges.
- Re-registration creates a new opaque handle and must not revive challenges from a prior registration.

## 4. Required state and binding model

Every issued challenge must be bound to:

- Complete canonical action digest, including all action fields.
- Canonical snapshot binding digest (workspace identity, relative path, file kind, byte length, content digest and file identity as defined by the existing snapshot contract).
- Workspace registration identity / opaque handle.
- Authority process epoch.
- Rust-owned issue time, monotonic freshness point and expiry.
- Single-use state.

If the current `ApprovalStore` does not persist workspace registration identity, extend its internal binding model before wiring the protocol. Do not attempt to bind only the opaque handle string if handles can be reused; handle generation must remain cryptographically random and process-local, and the process epoch must be checked independently.

The store must remain Rust-owned. The protocol handler must not deserialize caller-supplied `CapturedWorkspaceSnapshot` or construct one from caller metadata.

## 5. Error contract

Use stable typed reason codes with no sensitive path or storage detail:

- `UNSUPPORTED_ACTION`
- `WORKSPACE_NOT_FOUND`
- `CAPTURE_DENIED`
- `INVALID_TTL`
- `BINDING_CHALLENGE_INVALID`
- `BINDING_CHALLENGE_EXPIRED_OR_CONSUMED`
- `BINDING_MISMATCH`
- `BINDING_STORE_UNAVAILABLE`
- `CAPACITY_EXCEEDED`
- `UNSUPPORTED_PROTOCOL_VERSION`

Do not distinguish unknown challenge from expired/consumed to the protocol caller. Internal logs may use non-sensitive reason codes only; never log challenge IDs, handles, file contents, absolute paths or digests unless a separate reviewed diagnostic policy permits it.

## 6. Required tests before implementation is accepted

### DTO and protocol
- Exact 2.1 version required for new messages; 2.0 clients remain unaffected.
- Unknown fields, missing fields, malformed IDs, duplicate request IDs, invalid TTL and oversized frames fail closed.
- No caller-supplied digest, identity, capture time, expiry or snapshot DTO is accepted.
- Full action DTO round-trips without field loss across Rust and TypeScript fixtures.
- Capability negotiation cannot enable a capability the server does not implement.

### Issue
- Registered handle + matching file action produces a challenge.
- Unregistered/revoked handle, invalid path, unsupported platform, unsupported action, stale capture and invalid patch target deny.
- TypeScript-supplied digest or snapshot metadata is rejected.
- Command action is rejected.
- Challenge TTL bounds and storage capacity are enforced.

### Consume
- Exact action and unchanged fresh snapshot succeeds once.
- Changed action, workspace, path, file kind, byte length, content digest, file identity or process epoch fails.
- Expired challenge, restart, workspace revoke, stale capture and storage/clock failure fail closed.
- Concurrent double-consume yields exactly one success.
- A failed mismatch does not accidentally consume a valid challenge unless explicitly specified and tested.

### Lifecycle
- EOF and shutdown drop registrations and invalidate challenges.
- Duplicate registration/revocation and unknown handle behavior are deterministic.
- Sidecar crash/restart cannot revive challenges.
- No write, patch application, command, file-content return, or execution-capability token occurs on any path.

## 7. Explicit non-goals and blockers

- No platform authenticator integration in this milestone.
- No claim of human presence, user verification, or authenticated consent.
- No workspace writes, patch application, command execution or privileged operation.
- No production activation integration until executable provenance and runtime integrity gates are satisfied.
- No TypeScript fallback read after a capture acknowledgement.
- No use of policy `allow` or `bindingVerified` as an execution capability.
- Independent security review remains required before this is described as approval.

## 8. Implementation sequence

1. Accept this contract and decide exact 2.1 negotiation semantics.
2. Extend the internal store binding to include registration identity and invalidate on workspace revocation.
3. Add strict Rust 2.1 DTOs and shared Rust/TypeScript fixtures.
4. Add handler issue/consume branches using Rust-owned adapter capture and `ApprovalStore`.
5. Add TypeScript client methods that expose only opaque challenge display data and a boolean binding result.
6. Add adversarial issue/consume, replay, concurrency, revocation and restart tests.
7. Keep all privileged operations disabled; obtain independent review.

**Acceptance criterion:** A protocol 2.1 client can request and consume a short-lived, single-use binding challenge tied to an exact action and a fresh Rust-owned capture; revocation/restart invalidates it; no human-approval claim or privileged operation is enabled.
