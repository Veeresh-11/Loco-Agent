# Secure Workspace Snapshot Adapter Research

Status: research/design only. No filesystem adapter or file-writing code is added.

## Objective

Implement a bounded, workspace-scoped read/snapshot capability in Rust that can produce a trustworthy snapshot for approval binding. It must use filesystem operations that are resistant to path traversal, symlink/reparse-point escapes, and check/use races. This document does not declare any dependency or platform approach approved for production.

## Security boundary

The Rust core must accept a trusted workspace directory capability established by the host and resolve user/agent-supplied relative paths only through that capability. A canonicalized path string alone is not a security boundary. The opened handle is authoritative for type, size, identity, and bytes.

Required snapshot fields:
- versioned workspace identity
- strict relative target path
- regular-file type established from opened handle
- bounded byte length
- SHA-256 computed over the bytes actually read
- stable, platform-derived file identity when available
- explicit outcome when the platform/filesystem cannot meet requirements

Read limits: default 10 MiB (align with current workspace-read prototype); allow lower caller limits but never higher than the global maximum. Reads must detect growth beyond the cap and return a limit error.

## Candidate library evaluation: cap-std

Candidate: `cap-std` capability-oriented filesystem API. Its `Dir` operations are scoped to an open directory handle rather than ambient path authority, and its documented behavior rejects paths that escape the directory through absolute paths or traversal. It supports Linux, macOS, Windows and several other platforms.

However, capability APIs do not eliminate the need for a security review. Do not adopt cap-std until its current release, dependency tree, security advisories, fallback path-resolution implementation, platform behavior, and required operations have been reviewed. A recently surfaced advisory alleges a symlink/trailing-slash issue affecting fallback/manual resolution paths; verify advisory status and affected/fixed versions directly before any dependency decision. This finding is a research blocker, not a claim about all versions or all usage.

## Alternatives to compare

- `cap-std`: portable capability-style `Dir` abstraction. Evaluate documented platform-specific resolution and fallback behavior, opened-handle metadata, Windows race handling, and whether its API supports required identity and no-follow semantics.
- `rustix`: lower-level Unix/Linux/Apple primitives, with platform-specific APIs. It is not a Windows adapter; use only for explicitly scoped platform implementations. Evaluate kernel support and safe fallback behavior.
- Direct platform adapter: Linux `openat2` resolution flags; Windows handle-relative `CreateFile`/NT APIs and reparse-point semantics; macOS descriptor-relative/no-follow operations. Higher maintenance and review burden, but may expose controls not abstracted by a portable crate.
- `std::fs`: not acceptable as sole security boundary because ordinary path checks and operations can race (TOCTOU); keep only for non-security-sensitive utilities or test scaffolding.

## API shape (proposal)

`WorkspaceRoot` is created only by the host after the user selects a workspace and securely opens its root directory. It owns a directory handle/capability and an opaque workspace identifier.

`snapshot_file(relative_path, max_bytes)`:
1. Enforce lexical path constraints before filesystem access.
2. Resolve/open target relative to the retained workspace root capability, using the chosen adapter's confinement/no-follow policy.
3. Reject symlink/reparse points and non-regular files based on opened-handle metadata.
4. Verify file size and caller/global limits.
5. Read no more than max_bytes + 1 and reject if over limit.
6. Compute digest over captured bytes, not a separate earlier read.
7. Capture stable platform identity from handle metadata, if supported.
8. Return bytes plus `WorkspaceSnapshot`; ensure snapshot length/digest match returned bytes.
9. Never convert a snapshot into authorization; policy/approval remains a separate gate.

## Required adapter tests

- absolute path, traversal, empty and ambiguous path components
- symlink/reparse escape at every path component
- in-workspace symlink policy (reject by default)
- target swapped to symlink between checks/open
- parent directory replaced or renamed concurrently
- file replaced after open (handle identity remains tied to opened object)
- special files, directories, sockets, FIFOs/devices
- file growth beyond limit while reading
- truncation/modification during read; document whether detected, and where guarantees are unavailable
- file identity and digest consistency
- platform-specific fallback behavior
- permission denied, unsupported primitive, filesystem limitations

## Initial decision

No implementation dependency is selected yet. A portable capability abstraction is attractive only if its current code and security advisories are independently reviewed and its documented guarantees match our threat model. For the first implementation, prefer a small read-only adapter API and no mutation methods. If a platform cannot meet the required confinement, return a typed unsupported/security error rather than falling back to plain `std::fs` path operations.

## Implementation gate

Before merging any adapter:
- pin/review dependency version and advisory status;
- inspect the actual platform code paths used by the supported OS/kernel/filesystems;
- prove the opened handle is confined to workspace root;
- capture type, size, identity and digest from the same opened handle/bytes;
- run adversarial tests in Windows, Linux and macOS CI;
- document any guarantees that are platform-dependent;
- keep all file writes disabled.
