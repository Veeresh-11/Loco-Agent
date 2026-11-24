# Secure Workspace Read Acquisition Design

**Status:** Design checkpoint; no filesystem acquisition implementation selected  
**Date:** 2026-11-24  
**Depends on:** PR #96 workspace-read-request-v1 and the workspace snapshot/platform-boundary work

## 1. Scope and non-goals

This design defines the security properties required before Rust may implement a bounded, read-only workspace file capability. It does not add file access, authorize reads, change the live extension path, or advertise a new sidecar capability.

The v1 request in PR #96 is only a shape/lexical contract. A valid workspaceId is not yet resolved to a trusted root handle, and a valid relativePath is not proof of containment. The existing workspace_fs.rs path-validation-then-open helper is explicitly a TOCTOU-vulnerable prototype and must not be promoted or wired into the sidecar.

Writes, patch application, shell execution, Git commands, recursive RAG indexing, and approval authentication are out of scope.

## 2. Required security invariant

The only accepted acquisition sequence is:

1. The trusted extension host registers a workspace root with the Rust authority. The registration binds an opaque host-issued ID to a retained OS directory handle and an adapter-derived root identity. A provider or request must never supply an arbitrary root path.
2. Rust validates the strict request DTO, version, ID syntax, portable relative-path grammar, and requested byte cap.
3. Rust resolves the opaque ID only in its own live registry. Unknown, stale, replaced, or unregistered IDs fail closed.
4. Rust traverses from the retained root handle using native handle-relative operations. It must not concatenate an absolute path and reopen from the process working directory.
5. Every intermediate component is opened without following symlinks/reparse points. Every opened object is checked by handle for expected type and the adapter's root-volume/mount policy.
6. The final object is opened as a regular file, and its identity and size are obtained from the opened handle, not a pathname lookup.
7. Rust reads at most maxBytes + 1 bytes from that same handle, rejects growth beyond the cap, and derives any digest/identity from the acquired handle and bytes.
8. The adapter returns bytes and bounded metadata only after all checks succeed. Errors do not fall back to TypeScript path-based reads.

A path check followed by a separate open, canonicalize-then-open, prefix comparison, or final-path string check is not an acceptable substitute.

## 3. Workspace registration and lifetime

- The host issues an unpredictable opaque ID and registers it through a future authenticated parent/child protocol operation.
- Rust owns the ID-to-root-handle mapping. The request cannot choose or replace the root.
- Registration opens the root once, verifies it is a directory, captures the platform identity, and retains the handle for the registration lifetime.
- Re-registering the same ID with a different root identity invalidates the old registration; requests racing replacement fail closed.
- Workspace removal, extension shutdown, sidecar restart, and authority replacement invalidate registrations. No serialized absolute path is a reusable capability.
- Root handles must be opened with only the access rights needed for enumeration/read. Inheritance into unrelated children is prohibited.
- A later protocol revision must explicitly negotiate a new capability such as workspace_read. The current sidecar continues to advertise only proposal_evaluation.

## 4. Portable path grammar

The PR #96 portable grammar is a first lexical filter, not the security boundary. The adapter must additionally:

- Parse only /-separated components; reject empty, dot, dot-dot, rooted, drive-qualified, UNC, alternate-stream, NUL/control, and platform-ambiguous components.
- Reject reserved Windows device names and trailing dots/spaces consistently on every OS so requests have portable meaning.
- Never normalize an escaping path into an accepted path.
- Treat each component as an opaque name passed to a relative native open. Do not resolve it through a separate pathname API.
- Reject a final directory, device, FIFO, socket, symlink, reparse point, or other non-regular object.
- Apply the same grammar at every trust boundary (TypeScript, JSON Schema where expressible, and Rust); Rust is authoritative.

## 5. Platform adapter requirements

### 5.1 Linux

Candidate implementation:

- Retain an O_PATH | O_DIRECTORY | O_CLOEXEC root descriptor (or an equivalently reviewed directory descriptor).
- Prefer openat2(2) with a root dirfd and RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS | RESOLVE_NO_MAGICLINKS; evaluate RESOLVE_NO_XDEV for the explicit no-cross-mount policy.
- If the supported kernel baseline cannot guarantee openat2, use component-by-component openat(2) with O_NOFOLLOW | O_DIRECTORY | O_CLOEXEC for each intermediate component and a no-follow final open. Do not fall back to ordinary path-based open(2).
- Use fstat/descriptor metadata for type, size, device, and identity. Where mount boundaries matter, use a reviewed mount-ID source (for example statx(2) mount ID when supported) rather than assuming st_dev detects same-device bind mounts.
- Retain all directory descriptors until traversal completes; close them deterministically on success, error, and cancellation.

Unresolved before implementation: supported kernel minimum, openat2 availability policy, mount-ID fallback behavior, overlay/network filesystem policy, and exact rustix/libc API coverage. If a required primitive is unavailable, deny rather than downgrade to pathname traversal.

### 5.2 macOS

Candidate implementation:

- Retain a root directory descriptor and traverse with openat(2) component by component, using O_NOFOLLOW and directory-only opens for intermediate components.
- Obtain metadata from opened descriptors using fstat/fstatfs; never use pathname metadata as the authority.
- Compare descriptor-derived f_fsid values with the retained root volume identity at every intermediate and final object, in addition to the applicable device checks. The existing PR #77 work demonstrates that APFS System/Data volume identity can differ in f_fsid while st_dev is equal.
- Reject a zero/unusable root volume identity and fail closed when the platform cannot establish the required identity.
- Do not claim this detects every same-volume firmlink. The documented native metadata probe returned SF_NOUNLINK, not SF_FIRMLINK, on the tested macOS host. Same-volume firmlink traversal remains unresolved in issue #74.

Unresolved before implementation: a documented and testable native mechanism for same-volume firmlink detection or a narrower explicit policy that makes this limitation acceptable. Do not treat SF_NOUNLINK, generic sunlnk, pathname canonicalization, or st_dev equality as proof of firmlink safety.

### 5.3 Windows

Candidate implementation:

- Retain a root directory handle opened with directory semantics and the minimum required access/share flags.
- Traverse relative to the retained parent handle using a reviewed handle-relative native open mechanism (candidate: NtCreateFile with OBJECT_ATTRIBUTES.RootDirectory and a single-component UNICODE_STRING). Do not construct and reopen a full path for each component.
- Open each component with reparse-point following disabled (candidate flags/options include FILE_OPEN_REPARSE_POINT / FILE_FLAG_OPEN_REPARSE_POINT as applicable), then inspect the opened handle's attributes and reparse tag. Reject all reparse points unless a specific tag and resolution policy has been reviewed.
- Obtain final file identity and volume identity from the handle (candidate: GetFileInformationByHandleEx(FileIdInfo)), and verify the final object is a regular disk file with the expected volume identity.
- Use explicit sharing semantics and retain handles through the read. Revalidate identity and size from the same handle before returning bytes.

Unresolved before implementation: the exact supported Windows API sequence for race-resistant relative opens across supported Windows versions; NtCreateFile stability/ABI and dependency policy; reparse-tag handling; volume mount-point and per-directory case-sensitivity behavior; and safe handling of cloud placeholder/offline files. A CreateFileW call on a concatenated absolute path plus a final-path check is not an acceptable substitute.

## 6. Bounds, identity, and read semantics

- Enforce the request maximum (1 byte through 10 MiB) and an implementation-owned cap no larger than the contract maximum.
- Obtain initial size from the final opened handle. Reject a known size greater than the cap before allocating.
- Allocate no more than the permitted bound. Read at most cap+1 bytes to detect growth; never read the complete file into an unbounded buffer.
- Treat short reads, file mutation during read, I/O errors, cancellation, and identity/metadata inconsistency as explicit failure outcomes. Do not return partial bytes as success.
- Capture stable adapter-specific identity from the opened handle: volume/mount identity plus file ID/inode tuple as supported. Opaque identity is not portable and must not be decoded by shared contract code.
- Where the filesystem exposes a reliable change indicator, capture and compare it around the read. Content digest describes the bytes actually returned; it does not alone prove that the target remained unchanged before/after the read.
- A later write/approval flow must independently re-open and revalidate the target immediately before mutation. This read capability is not a write grant.

## 7. Adversarial test matrix

A platform adapter is not eligible for integration until its CI exercises the following cases on real supported filesystems where feasible:

| Case | Expected result |
|---|---|
| Normal file within root and byte cap | Read exact bytes |
| Absolute, drive, UNC, dot-dot, repeated separator, reserved-name path | Reject before acquisition |
| Symlink in each intermediate position | Reject |
| Symlink as final component | Reject |
| Rename/swap intermediate directory during traversal | Never escape retained root; deterministic denial or safe in-root result |
| Replace final path after open | Read only from the already-opened handle; identity reflects that handle |
| Hard link to an outside-created inode inside root | Apply explicit hard-link policy; do not assume pathname containment implies inode provenance |
| Final directory, device, FIFO, socket, cloud placeholder | Reject unless a narrowly reviewed regular-file semantic applies |
| Oversized file before open/read | Reject without unbounded allocation |
| File grows beyond cap during read | Reject; return no partial success |
| File truncates or mutates during read | Explicit failure or documented consistent-handle semantics |
| Same-device bind mount / nested mount (Linux) | Reject under no-cross-mount policy |
| APFS System/Data volume boundary and firmlink mappings (macOS) | Reject known volume boundary; separately test and document unresolved same-volume firmlink behavior |
| NTFS volume mount point, junction, symlink, other reparse tags (Windows) | Reject |
| Cancellation, descriptor exhaustion, permission loss, I/O error | Close handles and fail closed |
| Stale/replaced workspace registration | Reject |
| Unknown protocol capability/version/fields | Reject; no fallback to legacy file APIs |

Tests that require mount privileges or platform-specific setup must run in dedicated CI fixtures or a documented privileged test environment. A skipped fixture is not evidence that the boundary is safe.

## 8. Integration gates

1. Resolve PR #96 exact-head TypeScript lint and preserve shared Rust/TS/Python fixture parity.
2. Complete the transitive filesystem/process inventory and add a static regression gate for new direct privileged APIs.
3. Resolve the platform-specific open primitive and filesystem policy questions above. Do not begin with a cross-platform path-based abstraction that hides platform differences.
4. Implement one platform adapter at a time behind a Rust-owned registry of retained root handles.
5. Add native adversarial fixtures and run on Windows, macOS, and Linux before enabling that adapter.
6. Add a separately negotiated sidecar capability only after all platform adapters and protocol tests pass.
7. Integrate behind an explicit experimental opt-in. Missing sidecar, unsupported filesystem, protocol mismatch, timeout, or denial must not fall back to TypeScript direct reads.
8. Keep writes, shell execution, and approval-dependent operations unavailable until their separate security gates are met.

## 9. Current status

- PR #96 defines request shape and lexical validation only.
- The current sidecar does not implement or advertise workspace reads.
- workspace_fs.rs remains a non-production prototype.
- Linux, macOS, and Windows native acquisition decisions remain to be proven by implementation and adversarial tests.
- macOS same-volume firmlink handling remains an open issue (#74).
- No live task path or default behavior changes in this design checkpoint.
