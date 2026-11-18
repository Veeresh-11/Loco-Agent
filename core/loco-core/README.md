# loco-core — Rust security-core prototype

This is the initial Rust policy-evaluation prototype for LOCO Agent.

## Scope (prototype only)

- Deserialize typed action proposals.
- Reject unsupported contract versions and missing IDs.
- Apply a basic relative-path syntax guard.
- Allow basic read proposals only when the prototype read policy is enabled.
- Require approval for patch and command proposals.
- Reject invalid limits and empty executable strings.

## Explicitly not implemented

- No filesystem reads or writes.
- No command/process execution.
- No canonical workspace containment.
- No symlink or TOCTOU protection.
- No cryptographic approval binding or replay prevention.
- No persistent policy, capabilities, or audit ledger.
- No IPC/sidecar.
- The read `allow` decision is only a policy prototype result and must not be wired to actual file I/O until filesystem enforcement is implemented.

## Tests

Unit tests cover the prototype decision logic and basic path syntax. Run locally from this crate:

```sh
cargo test
```

Do not treat these tests as a security certification. This is groundwork for the next milestone: filesystem-aware workspace boundary validation and approval binding.


## Bounded workspace read prototype

`workspace_fs::read_workspace_file` provides a bounded read helper with path validation, regular-file checks, metadata size precheck, and a capped read to detect growth. It is not wired into proposal execution or authorization. Path validation/open remain separate and can race; production use requires handle-relative/race-resistant OS primitives, action authorization, and approval enforcement. No write or command executor is implemented in this PR.

## Action approval binding prototype

The approval binding module hashes a Rust-serialized action and checks that a supplied challenge digest matches the action currently being evaluated, plus an expiry timestamp. Digest comparison uses constant-time equality.

Important limitations:
- Challenge IDs are caller-supplied and not guaranteed unique.
- No signature/MAC authenticates challenge or approval data.
- No replay/one-time consumption store exists.
- Caller-provided time is trusted input; a trusted clock must be provided.
- The Rust serde JSON representation is not a cross-language canonical JSON format. Python/TypeScript must not independently compute this digest until canonical serialization is standardized.
- This is not yet connected to action execution.

This provides a binding-validation prototype, not a complete approval system.


## Audit event DTO

`audit_event` provides typed v1.0 audit metadata and bounded scalar `details` validation (32 keys, 128-byte keys, 2,000-byte string values). Call `AuditEvent::validate()` before serialization/storage. It is not a redaction engine and scalar fields can still contain secrets; sensitive-value filtering is a separate required layer.


## Patch proposal validation prototype

`patch_validation::validate_patch_proposal` checks a bounded unified-diff envelope, rejects binary patches, binds the patch headers to the requested relative target, and requires the expected SHA-256 of the current target bytes. It does **not** apply the patch, write files, authorize the operation, or provide race-resistant filesystem protection. It is validation groundwork only; real patch application must use secure handle-relative/platform primitives and an authorization/approval gate.


## Strict patch envelope follow-up

The follow-up validator prototype deliberately supports one file and one hunk only, and checks unified-diff hunk line counts. Multi-file and multi-hunk patches are rejected. This remains envelope validation, not a patch parser/application engine; tests must confirm behavior before use.
