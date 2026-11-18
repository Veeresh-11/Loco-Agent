# Controlled Workspace Write Design

Status: design-only proposal. No filesystem mutation implementation is included.

## Goal

Define the security requirements and reviewable interface for a future Rust-controlled workspace write path. AI-generated content remains untrusted. The write path must never rely on Python or TypeScript as the enforcement authority.

Invariant: **Python proposes. Rust authorizes. TypeScript presents. The user approves. Rust enforces.**

## Scope and non-goals

This design covers replacing or creating a single workspace file from an approved patch proposal. It does not implement patch application, file writes, command execution, or approvals. It does not claim race-free behavior until platform-specific primitives and adversarial tests demonstrate it.

Initial implementation should remain deliberately narrow:
- one target file per operation;
- bounded patch and target sizes;
- no binary files;
- no symlink targets or symlink traversal;
- no special files, directories, devices, sockets, or pipes;
- reject unsupported path forms rather than normalize them permissively;
- explicit create-vs-replace semantics;
- no implicit overwrite.

## Threat model

Assume a hostile or compromised AI provider, malicious proposal payload, concurrent workspace process, untrusted repository contents, and a user who may approve an action without noticing changed target state.

Protect against:
- path traversal and alternate path spellings;
- symlink/reparse-point escape;
- target changes between proposal, approval, and write (TOCTOU);
- replacing an unexpected file type;
- stale approvals or replayed approvals;
- patch/path/digest mismatch;
- partial writes and crash interruption;
- oversized input and resource exhaustion;
- misleading audit records.

Out of scope for this prototype: kernel compromise, administrator/root compromise, malicious filesystem driver, or guarantees on filesystems/platforms that do not provide the required primitives. Such cases must be documented as degraded/unsupported, not silently treated as safe.

## Required action binding

A future write authorization must bind at least:
- stable action/challenge identifier;
- workspace identity and root identity;
- relative target path in canonical contract form;
- operation kind (create or replace);
- patch bytes digest;
- expected current target digest (absent for create);
- resulting content digest, if deterministically computed;
- policy version and decision;
- expiry and replay-store identity.

Approval must be one-shot. Consumption and write authorization should be ordered so a replay cannot authorize another operation. Failed or interrupted writes must not make an approval reusable.

## Path and workspace handling

- Resolve and retain a trusted workspace root identity when the workspace is established.
- Reject absolute paths, traversal components, empty components, alternate separators, drive/UNC/device prefixes, and platform-specific ambiguous forms.
- Do not trust a path merely because string validation or canonicalization passed earlier.
- Avoid following symlinks/reparse points at every relevant component.
- Use directory-handle-relative operations and platform-specific no-follow/open semantics where available.
- Revalidate the target identity and state at the point of mutation.
- Where a platform/filesystem cannot meet required guarantees, fail closed or explicitly downgrade with user-visible explanation.

## Mutation protocol (proposed)

1. Receive a bounded action proposal and parse it using the versioned contract.
2. Validate workspace, target path, operation type, patch envelope, and target digest.
3. Obtain a fresh target snapshot through the security core.
4. Compute a canonical action digest from the exact action and snapshot.
5. Present the complete target, operation, change summary, and digest-bound approval to the user.
6. After approval, consume the one-shot approval/challenge.
7. Reopen/revalidate target via secure handle-relative/platform primitives; confirm target identity and digest still match.
8. Apply patch to an in-memory bounded buffer (no write yet); validate the resulting content and expected digest.
9. Write to a securely created temporary file in the same directory, preserving only explicitly permitted metadata.
10. Flush file data as required, then perform a platform-supported atomic replace/create operation with race checks.
11. Flush directory metadata where supported and required by durability policy.
12. Record outcome in append-only audit. Distinguish approved, rejected, failed, committed, and uncertain/interrupted outcomes.

The above is a design sequence, not a claim that all platforms provide identical atomicity or durability semantics.

## Platform strategy questions (must be resolved with evidence)

### Linux
Evaluate directory file descriptors, openat/openat2 availability and resolution flags, O_NOFOLLOW, file identity checks, renameat2 semantics, and filesystem support. Document fallback behavior.

### Windows
Evaluate handle-based path traversal, reparse-point rejection, volume/file identity, CreateFile flags, replace/create semantics, sharing modes, and atomicity/durability behavior. Do not rely only on string path normalization.

### macOS
Evaluate directory-relative operations, no-follow flags, file identity checks, rename/replace behavior, and filesystem-specific limitations. Do not assume Linux primitives are portable.

## Error and interruption behavior

Return typed outcomes rather than generic success/failure:
- RejectedBeforeApproval
- ApprovalExpired
- ApprovalReplay
- TargetChanged
- PathUnsafe
- UnsupportedFilesystemGuarantee
- PatchInvalid
- WriteFailedBeforeCommit
- CommitSucceeded
- CommitOutcomeUncertain

Never report a commit as successful without confirmation. If the outcome is uncertain, force a fresh target read and reconcile; never retry automatically using the same approval.

## Audit requirements

Audit events should correlate the action/challenge, workspace, target, operation, proposal digest, expected/current digest, policy version, approval state, and final outcome. Audit must not store secrets or raw file contents by default. Record explicit error codes and whether the filesystem outcome is confirmed or uncertain.

## Implementation slices

1. **Platform capability research and threat-model review** (no writes).
2. **Secure target snapshot API** using platform-specific handles, identity, and no-follow behavior.
3. **Patch application to bounded memory** with independent parser tests and expected-result digest.
4. **Approval consume-and-revalidate gate** that binds exact target state and patch.
5. **Atomic write adapter per OS** with capability checks and fail-closed behavior.
6. **Adversarial race/reparse/symlink tests** on Windows, Linux, and macOS.
7. **Integration behind an explicit feature flag**, default off until parity and review.
8. **End-to-end audit and recovery tests** including interrupted/uncertain commit states.

## Acceptance criteria for implementation

- No mutation can occur without a Rust security decision and a consumed, action-bound approval.
- The exact patch, path, workspace, target snapshot, and operation type are bound.
- Target state is revalidated immediately before commit.
- Symlink/reparse and path traversal attacks are blocked by platform-level tests.
- Create and replace are separate, explicit operations.
- No automatic retry reuses an approval.
- Bounded memory and disk usage are tested.
- CI includes OS-specific adversarial tests; passing unit tests alone is insufficient.
- Feature remains disabled if platform guarantees are unsupported or tests fail.

## Decision

Do not implement a production write path until platform research resolves the primitive choices and CI tests demonstrate the intended guarantees. Keep this design reviewable as the baseline for future code changes.
