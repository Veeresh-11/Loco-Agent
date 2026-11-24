# Snapshot Capture Trust-Boundary Audit

**Status:** Audit findings; no production capability is accepted by this document  
**Date:** 2026-11-24  
**Parent:** PR #125, snapshot-bound approval store prototype

## Purpose

This audit distinguishes three properties that must not be conflated:

1. **Payload consistency:** the metadata's byte length and SHA-256 agree with the byte buffer.
2. **Capture freshness:** the capture was stamped using a monotonic clock owned by the Rust process and is still within the prototype freshness window.
3. **Trusted acquisition:** the bytes and identity were obtained from the intended workspace through a platform-specific, handle-relative acquisition path that enforces the supported filesystem boundary.

The first two properties do not establish the third.

## Evidence from the current implementation

The PR #125 implementation adds a private `captured_at_monotonic: Duration` to `CapturedWorkspaceSnapshot`. Its constructors are crate-private, and `new_at` calls `validate_snapshot_payload` before creating the object. The approval store applies the 30-second freshness check at issue and consume.

However, `snapshot_payload.rs` states explicitly that:

- `build_snapshot_contract` constructs metadata from bytes already acquired by the caller;
- the helper does not prove that bytes came from a safely opened file;
- the caller must supply the workspace ID and adapter-derived identity;
- `validate_snapshot_payload` checks contract validity, length and SHA-256 consistency;
- the `WorkspaceSnapshotAdapter` trait describes the intended boundary but says the trait alone does not make an implementation secure.

The PR #125 feature-branch tree contains concrete Linux, macOS and Windows adapter modules. The Linux and macOS adapters implement `WorkspaceSnapshotAdapter` and call `CapturedWorkspaceSnapshot::new` only after their bounded read and metadata/path checks; the Windows adapter similarly constructs the capture after its handle-relative read checks. PR #125 itself changes the generic payload and approval modules, not those adapter files. Crucially, the inspected production authority/sidecar path still does not call the adapters or `ApprovalStore::issue/consume`: the approval-store calls found in `approval_store.rs` are test helpers/tests, and the sidecar protocol/authority remains proposal-evaluation only. The protocol explicitly says an `allow` decision is not an execution capability. Thus the acquisition modules exist, but the end-to-end trust path from IPC request → adapter capture → snapshot-bound approval has not been integrated.

## Findings

### F-1 — Rust timestamp is not equivalent to trusted file acquisition

**Severity:** High for any future authorization use; prototype-only code remains non-production.

A caller within the Rust crate can invoke crate-private constructors. The timestamp is currently assigned by the concrete adapters at their final `CapturedWorkspaceSnapshot::new(...)` call, after their acquisition checks. The issue is API authority: `new` is `pub(crate)`, so any future module in the crate can construct a capture from caller-supplied bytes/metadata using the public metadata builder. The capture type itself does not encode provenance proving that a platform adapter created it.

**Required control:** Keep constructors private to the acquisition module or use a sealed/private capability type created only by the concrete adapter. Ensure the timestamp is assigned after the adapter has completed its bounded read and post-read identity/mutation checks. Audit every constructor call site.

### F-2 — Freshness uses two monotonic-clock interfaces

The capture timestamp uses `process_monotonic_now()` based on a process-wide `Instant` origin. `ApprovalStore` obtains its current monotonic reading from the injected `ApprovalClock` trait.

The production `SystemApprovalClock` delegates to the same process-wide source, but test or future custom implementations may not share that origin. The existing tests intentionally supply timestamps from a fake clock. This is workable for controlled tests, but the API contract must make the shared-origin requirement explicit and must not allow an injected clock with a different epoch in production.

**Required control:** Prefer one private monotonic clock authority/token for both stamping and freshness comparison, or inject a single clock source into the capture authority and approval store. Keep any fake clock injection test-only. Reject incomparable epochs rather than interpreting them as durations from the same origin.

### F-3 — Approval binding proves metadata equality, not a live filesystem revalidation

The snapshot digest binds workspace ID, relative path, file kind, byte length, content digest and opaque file identity. The store then checks the supplied captured object against that digest. This proves that the presented object matches the stored values, not that the file still has that identity/content in the filesystem at the instant a future operation acts.

**Required control:** Before every future privileged read/write/replace or command decision, the Rust authority must perform a fresh operation-specific acquisition and revalidate the exact handle, identity, bytes/precondition and filesystem boundary immediately before use. The current store is not a write/command authorization capability.

### F-4 — Snapshot object cloning and reuse must remain safe

`CapturedWorkspaceSnapshot` derives `Clone`, and the capture's monotonic timestamp is copied when cloned. Reusing a clone does not refresh the timestamp, which correctly preserves age, but callers must not mistake a clone for a new acquisition.

**Required control:** Keep clone semantics documented as same-capture duplication only. A recapture must call the concrete adapter and obtain a newly stamped capture. Add tests that cloning does not extend freshness and that a fresh identical recapture is distinguishable by a new capture instant while retaining the same snapshot digest.

## Acceptance work before treating PR #125 as integrated

1. Restrict `CapturedWorkspaceSnapshot` construction to a private acquisition module or sealed adapter-issued capability; keep test constructors behind test-only visibility.
2. Integrate the concrete platform adapter into the Rust authority request path and make `ApprovalStore::issue/consume` reachable only through that trusted capture path. Today, the inspected `.issue()` / `.consume()` references are test-only; the production sidecar remains proposal-evaluation only.
3. Ensure no caller-supplied DTO, path-only helper, TypeScript, or Python layer can bypass capture.
3. Unify the monotonic clock origin used for capture stamps and store comparisons; make fake-clock injection test-only or share a single injected clock source.
4. Add tests for clone/reuse not refreshing age, fresh recapture, clock-origin mismatch, and capture completion time.
5. Demonstrate the concrete platform adapter returns this exact type only after bounded handle-relative acquisition, stable identity checks, and post-read mutation checks.
6. Maintain the current fail-closed behavior for unsupported actions and do not enable writes or command execution.
7. Resolve the existing native-platform release gates (including the macOS firmlink signal discrepancy and Windows `OBJ_DONT_REPARSE` contract caveat), then obtain independent security review of the complete integrated call path, native acquisition matrix and platform API assumptions.

## Merge and release decision

Do not merge PR #124/#125 into the default branch or promote the approval store as a production authorization boundary based only on the existing green unit/contract CI. The current PR #125 validation supports the digest/store prototype on its tested CI jobs; it does not replace source-level call-site evidence, end-to-end acquisition integration, a platform authenticator, same-user malware analysis, SQLite ACL protection, or independent review.

No production writes or command execution should be enabled.
