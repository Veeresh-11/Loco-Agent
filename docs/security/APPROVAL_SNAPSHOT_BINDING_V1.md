# Approval Snapshot Binding v1 — Design

**Status:** Proposed prototype contract; not production authorization  
**Date:** 2026-11-24  
**Depends on:** approval-store prototype through PR #90; workspace snapshot contract v1 and OS-specific adapters

## Problem

The current approval challenge binds only the canonical `ProposedAction` digest and challenge expiry. It does not bind the exact workspace snapshot from which the proposal was derived. A path can still identify a different file object or different bytes by the time approval is consumed. The existing `WorkspaceSnapshot` already carries workspace ID, relative path, file kind, byte length, content SHA-256, and opaque adapter-derived file identity, but that binding is not included in the challenge.

This proposal adds a second, independent digest to the approval challenge. It does not make the challenge authenticated human approval; the platform-authenticator decision in [Approval Trust Boundary](APPROVAL_TRUST_BOUNDARY.md) remains unresolved.

## Required binding

A file-scoped approval must bind both:

1. **Action digest:** canonical Action Digest v1 for the complete action.
2. **Snapshot binding digest:** a domain-separated digest over the complete validated snapshot contract:
   - snapshot contract version;
   - workspace ID;
   - normalized relative path;
   - file kind;
   - byte length;
   - content SHA-256;
   - adapter-derived file identity.

The snapshot digest therefore changes if the workspace, target path, file kind, byte length, content, or opened-object identity changes. The opaque file identity is included as data; it must not be interpreted by another platform or reconstructed from a pathname.

## Canonical encoding proposal

Use a new, separately versioned encoding, not JSON serialization:

- Prefix: `LOCO-SNAPSHOT-BINDING-V1\0`.
- Strings: UTF-8 bytes prefixed by an unsigned 64-bit big-endian byte length.
- Byte length: unsigned 64-bit big-endian.
- File kind: one fixed one-byte enum tag; v1 accepts only regular files.
- Optional values: one-byte presence tag followed by the value when present.
- Hash the complete encoded byte sequence with SHA-256.
- Reject invalid snapshot contracts before encoding; missing content digest or file identity is a hard failure.

Publish fixed test vectors before treating this format as a cross-language contract. Rust is the sole verifier; TypeScript must relay the opaque digest and must not construct trusted snapshot metadata.

## Freshness and lifecycle

- The trusted capture boundary stamps a monotonic capture instant inside the Rust process. No timestamp supplied by TypeScript, Python, a provider, or a serialized snapshot is authoritative.
- Initial prototype maximum snapshot age: **30 seconds** at challenge issuance and again at challenge consumption. The challenge TTL remains independently bounded by the approval store (currently at most 300 seconds).
- The capture instant is process-local and is never persisted or compared across restarts. Existing process-epoch invalidation continues to revoke all outstanding challenges after restart.
- If monotonic time is unavailable, moves backward, or cannot be compared to the capture instant, fail closed.
- Challenge issuance stores the action digest and snapshot binding digest together. Consumption recomputes both from the current Rust-held capture and requires constant-time digest equality.
- A new capture may satisfy the challenge only if it is fresh and its full binding digest is identical. Any changed file identity, content digest, length, path, or workspace denies consumption.
- Immediately before a future write or command, Rust must acquire a fresh target/workspace snapshot again and compare it with the approved binding. Approval never freezes filesystem state.

The 30-second snapshot-age value is a conservative prototype default, not a UX or production policy decision. It should be validated against the actual review flow before release.

## Action-to-snapshot scope

V1 is deliberately limited to file-scoped actions:

- `ReadFile`: action path must exactly equal the snapshot relative path.
- `ProposePatch`: action path must exactly equal the snapshot relative path, and `expected_target_sha256` must be present and equal the snapshot content digest.
- `RunCommand`: **deny as unsupported by this file-only binding**. A single file snapshot does not represent the command's executable identity, arguments, environment, working directory, or complete input state. Command approval requires a separately versioned workspace/command-context snapshot contract.

No missing snapshot, identity-less snapshot, stale capture, mismatched path, absent patch precondition, or unsupported action may fall back to action-only approval.

## Storage and compatibility

- Add a non-null `snapshot_digest` column to the challenge record.
- Existing rows from the previous schema are invalidated by the existing process-epoch rotation; migration must not make old action-only challenges consumable.
- Keep the database row and returned challenge DTO aligned. Do not accept a caller-supplied snapshot digest as proof: Rust derives it from the captured snapshot.
- Preserve immediate SQLite transactions, one-time consumption, process-epoch checks, monotonic challenge deadlines, retention limits, and database/WAL bounds.
- Do not add a production executor or change the meaning of an `allow` policy result in this work.

## Required tests before implementation can be accepted

1. Fixed canonical snapshot-binding test vectors.
2. Changing each individual snapshot field (workspace, path, kind, length, content digest, file identity, contract version) changes the digest or fails validation.
3. Matching action and fresh matching snapshot can issue and consume once.
4. Same action with changed content digest, file identity, path, or workspace is rejected.
5. Patch action with absent or mismatched `expected_target_sha256` is rejected.
6. Read action whose path differs from snapshot path is rejected.
7. Run-command action is rejected by the file-only binding.
8. Snapshot older than 30 seconds at issue or consume is rejected; a missing/backward monotonic reading fails closed.
9. A fresh recapture with identical full binding may satisfy the challenge; a changed recapture cannot.
10. Existing replay, concurrent-consumption, process-restart, TTL, storage-capacity, and SQLite failure tests continue to pass.

## Explicit non-goals and gates

- This is not a signature, MAC, WebAuthn assertion, OS-authenticator result, or proof of human intent.
- A digest stored in local SQLite is not protected against same-user tampering. Authenticator and database ACL decisions remain separate gates.
- No generic software key, UI click, or unsigned DTO may authorize privileged operations.
- The platform-authenticator research issue #82 remains open. Same-user malware, enrollment/recovery/revocation, app/RP identity, IPC peer identity, and OS-owned user-presence semantics require independent decisions.
- No production write or command execution is enabled. Independent security review remains mandatory.


## Implementation checkpoint — 2026-11-24

PR #125 implements this prototype contract in Rust:

- `canonical_snapshot_digest.rs` provides the domain-separated, length-prefixed Snapshot Binding Digest v1.
- `CapturedWorkspaceSnapshot` records a private process-monotonic capture instant; its constructors are crate-private so callers outside the Rust crate cannot manufacture a trusted capture or set its timestamp.
- `ApprovalStore::issue` and `consume` require a captured snapshot and enforce the 30-second maximum age using the same Rust-owned monotonic clock source used by the approval store.
- The challenge DTO and SQLite row both carry the snapshot digest. Consumption verifies action and snapshot digests in constant time.
- `ReadFile` requires an exact path match. `ProposePatch` requires an exact path and a present target digest equal to the captured content digest. `RunCommand` is rejected as unsupported by this file-only contract.
- Existing action-only rows migrate with an empty snapshot digest and remain unusable; process-epoch invalidation remains in force.

Rust Core Tests [run #45367727222](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45367727222) passed Ubuntu, Windows, and macOS. Contract Validation [run #45367727547](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45367727547) passed.

This is still a prototype. The store does not authenticate a human, protect SQLite against same-user tampering, or invoke a platform authenticator. A future privileged operation must reacquire and compare the target/workspace snapshot immediately before use. No production write or command path is enabled.
