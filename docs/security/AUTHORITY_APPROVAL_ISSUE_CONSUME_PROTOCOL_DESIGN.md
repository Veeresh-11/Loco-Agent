# Authority Approval Issue/Consume — Protocol Design

**Status:** Proposed wire contract; implementation gate only  
**Date:** 2026-11-25  
**Base:** Authority registration/capture protocol v2 stack (#132–#138)

## 1. Current state

The versioned registration/capture protocol is already implemented in the open draft chain: #132 design, #133 strict DTOs, #135 Rust session and handlers, #136 TypeScript client, #137 adversarial handler tests, and #138 path parity. Exact-head CI was green for the DTO, Rust handler/session, TypeScript client, adversarial Rust/contract tests, and TypeScript parity commits. These PRs remain unmerged and require dependency-order review.

The Rust `ApprovalStore` already exposes `issue(action, captured_snapshot, ttl)` and `consume(challenge_id, action, fresh_snapshot)`. It binds canonical action and snapshot digests, checks freshness/expiry, enforces single-use consumption and process-epoch invalidation. It is not wired into the v2 handler.

The v2 capture handler intentionally discards captured bytes and trusted metadata, returning only a capture acknowledgement and byte length. TypeScript must not provide snapshot metadata back to Rust.

## 2. Critical distinction: binding is not human approval

This milestone must not claim authenticated user approval. There is no platform-authenticator assertion verification in the current authority.

Keep three separate events:

1. **Issue binding challenge:** Rust captures the file via the registered native adapter and stores a challenge bound to the exact action and snapshot.
2. **Authenticate user intent:** a future, separately reviewed platform-authenticator ceremony binds user presence/verification to the challenge.
3. **Consume binding challenge:** Rust recaptures and verifies exact action, snapshot identity/content, freshness, expiry, process epoch and single-use semantics.

Until step 2 exists, success means only `bindingVerified`, never `approved`, `authorized`, `grant`, or an execution token.

## 3. Versioning decision to validate

Keep existing protocol 2.0 unchanged for current clients. Add a strictly versioned **2.1** contract for the new binding-challenge messages. The 2.1 handshake must explicitly negotiate/advertise supported versions and the `approval_binding_challenge` capability. A 2.0 client continues to use registration/capture/revoke only; no silent reinterpretation or downgrade.

### Issue request

`issueBindingChallenge` fields:
- `protocolVersion: "2.1"`
- `requestId` unique for process lifetime
- `workspaceHandle` minted by Rust
- complete `action` DTO (no lossy conversion)
- `ttlSeconds` bounded to 1–300

Rust resolves the handle, accepts only file-scoped ReadFile/ProposePatch, derives the capture path/limit from the action, captures through the retained native adapter, and calls `ApprovalStore::issue`. It rejects RunCommand. It accepts no caller-supplied digest, file identity, byte length, timestamp, expiry, or serialized snapshot.

Response `bindingChallengeIssued` returns opaque `challengeId`, `actionDigest`, `snapshotDigest`, and `expiresAtUnix` for display/correlation only. No file bytes or trusted identity leave Rust.

### Consume request

`consumeBindingChallenge` fields:
- `protocolVersion: "2.1"`
- `requestId`
- `workspaceHandle`
- `challengeId`
- complete `action` DTO again

Rust resolves the live handle, recaptures the exact action path through the native adapter immediately before `ApprovalStore::consume`, and lets the store check expiry, epoch, action digest, snapshot binding, freshness and single use. Unknown/expired/consumed challenge IDs must not be distinguishable to the caller.

Response `bindingVerified: true` confirms only the binding check. It grants no file read, write, patch, command or execution authority.

## 4. State binding and revocation

The challenge must be bound to:
- complete canonical action digest;
- canonical snapshot binding (workspace identity, relative path, file kind, byte length, content digest and file identity);
- workspace registration identity / opaque process-local handle;
- authority process epoch;
- Rust-owned issue time, monotonic freshness and expiry;
- single-use state.

If the current store does not persist registration identity, extend its internal binding before protocol wiring. Workspace revocation must invalidate all challenges for that registration. Shutdown, EOF, process restart and epoch rotation invalidate all outstanding challenges. Re-registration must not revive prior challenges.

## 5. Typed failure outcomes

Use stable non-sensitive codes: `UNSUPPORTED_ACTION`, `WORKSPACE_NOT_FOUND`, `CAPTURE_DENIED`, `INVALID_TTL`, `BINDING_CHALLENGE_INVALID`, `BINDING_CHALLENGE_EXPIRED_OR_CONSUMED`, `BINDING_MISMATCH`, `BINDING_STORE_UNAVAILABLE`, `CAPACITY_EXCEEDED`, `UNSUPPORTED_PROTOCOL_VERSION`.

Never log paths, contents, challenge IDs, handles or raw storage errors. Do not reveal whether an unknown challenge exists.

## 6. Required tests

- Strict 2.1 DTOs; 2.0 compatibility; unknown fields/version rejection; request-ID uniqueness; bounded frames.
- No caller-supplied snapshot metadata accepted.
- Full action DTO preserved across Rust/TypeScript fixtures.
- Valid issue for registered handle; deny revoked/unregistered handle, unsupported platform/action, stale capture, invalid path/TTL, bad patch target, and capacity exhaustion.
- Exact fresh consume succeeds once; changed action/workspace/path/kind/length/digest/file identity/epoch fails.
- Expiry, restart, revocation, stale capture, clock/storage failure fail closed.
- Concurrent double consume yields exactly one success.
- Failed mismatch does not accidentally consume a valid challenge unless explicitly specified.
- EOF/shutdown releases registrations and invalidates challenges.
- No file content returned; no writes, patching, commands, approval grants or execution tokens on any path.

## 7. Non-goals and release gates

- No platform authenticator in this milestone and no claim of human presence, user verification or authenticated consent.
- No workspace writes, patch application, command execution or production activation integration.
- No TypeScript fallback read after capture acknowledgement.
- No use of policy `allow` or `bindingVerified` as an execution capability.
- Independent security review remains mandatory before this can be described as approval.

## 8. Implementation order

1. Review this contract and confirm 2.1 negotiation semantics.
2. Extend internal store binding with workspace registration identity and revocation invalidation.
3. Add strict Rust 2.1 DTOs and shared Rust/TypeScript fixtures.
4. Add Rust handler issue/consume branches using Rust-owned capture and `ApprovalStore`.
5. Add TypeScript client methods exposing only opaque challenge display fields and a binding result.
6. Add replay, concurrency, revocation, restart and failure-path tests.
7. Keep privileged operations disabled and obtain independent security review.

**Acceptance criterion:** a 2.1 client can issue and consume a short-lived, single-use binding challenge tied to an exact action and fresh Rust-owned capture; revocation/restart invalidates it; no human-approval claim or privileged operation is enabled.
