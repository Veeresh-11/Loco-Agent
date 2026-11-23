# Windows Trusted Workspace Root Acquisition Gate

Status: implementation prerequisite / design refinement. This document adds a concrete root-acquisition boundary to the Windows snapshot design. It does not implement an adapter or claim Windows security parity.

## Why this gate is separate

A retained directory handle only protects later operations if the handle was acquired from the intended workspace without traversing an attacker-controlled reparse point. A later component-wise `NtCreateFile` walk cannot repair an unsafe initial open. The Windows adapter must establish and retain a trusted volume-root handle first, then derive the selected workspace root by walking components relative to pinned handles.

The selected workspace path is user-controlled input. It is never itself an authority token.

## Initial supported scope

The first native prototype should deliberately support only:
- Local fixed volumes addressed through a canonical volume GUID path (`\\?\Volume{GUID}\`).
- A trusted volume-root directory handle opened directly from that volume GUID path.
- Workspace paths represented as validated individual components relative to that volume root.
- Read-only directory traversal and regular-file capture.

The first prototype should reject UNC/network shares, mapped network drives, device namespace paths, volume mount-point crossings, alternate data streams, and any path form it cannot unambiguously map to a single local volume. Support for removable media and non-NTFS filesystems remains disabled until separately tested.

The host may resolve a user-selected path to a volume GUID plus component list, but Rust must independently validate the representation and must not trust a host-supplied canonical path string as proof of containment. The adapter must verify the final opened directory handle and retain it for its lifetime.

## Root acquisition sequence

1. Parse the selected workspace path into a volume GUID root and individual components before opening anything. Reject drive-relative forms (for example `C:folder`), UNC paths, NT object-manager paths, device paths, empty components, dot components, dot-dot components, ADS syntax, trailing dots/spaces, control characters, and non-portable names.
2. Require a syntactically valid volume GUID root. Do not use the selected workspace path as the initial `CreateFileW` target.
3. Open only the volume GUID root with `CreateFileW`, requesting directory/list/traverse/read-attributes rights needed by the prototype, using `FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT`, and explicit share flags. Keep the returned handle open.
4. Query the opened volume-root handle for `FileAttributeTagInfo`, `FileIdInfo`, and `FileStandardInfo`. Require a directory and reject any reparse-point attribute/tag. Record volume identity from the handle.
5. Walk the workspace components one at a time using native `NtCreateFile` with `OBJECT_ATTRIBUTES.RootDirectory` set to the currently pinned parent handle. Pass exactly one validated component per call; never pass a multi-component user string. Use directory-only options for intermediate components and open-reparse-point semantics. Apply `OBJ_DONT_REPARSE` where supported, then inspect every returned handle and reject reparse points regardless of the flag.
6. After each successful component open, verify from that handle that it is a directory, not a reparse point, and on the expected volume. Retain the new handle before releasing the previous parent handle. Do not reopen a component by path.
7. The final retained handle is the workspace root capability. Its volume/file identity is recorded from the handle. Diagnostic path strings may be retained for display only and must never be used to authorize later access.

If any required native operation, information class, flag, filesystem behavior, or identity check is unsupported or ambiguous, initialization fails closed. There is no fallback to `std::fs`, `canonicalize`, ordinary full-path `CreateFileW`, or a weaker path-based open.

## File capture after root acquisition

The eventual adapter should use the same component-wise handle-relative walk from the retained workspace root:
- Validate the portable relative path and split it into components.
- Open each intermediate component relative to its pinned parent handle; require directory and no reparse point.
- Open the final component read-only with no-follow/reparse-point controls; require a regular data file, not a directory, device, pipe, or other special object.
- Query final-handle volume/file ID, size, link count (where supported), and timestamps.
- Enforce the request cap and global 10 MiB ceiling before and during reading.
- Read only from the final handle, hash the captured bytes, and compare handle metadata before/after the read.
- Build and validate the existing `WorkspaceSnapshot` contract from the opened-handle identity and captured bytes.

Share modes must be explicit. The initial prototype should prefer denying concurrent write/delete sharing while the read handle is held, but this must be tested for compatibility and deadlock behavior. Sharing violations fail closed. Metadata comparison is mutation detection, not a guarantee of an atomic snapshot.

## Required native API proof

Before adding a production adapter module, record and test the exact supported API surface:
- `CreateFileW` volume-GUID-root open flags, access mask, and share-mode behavior.
- `NtCreateFile` and `OBJECT_ATTRIBUTES.RootDirectory` layout and calling convention for supported Windows targets.
- `OBJ_DONT_REPARSE` and `FILE_OPEN_REPARSE_POINT` behavior for intermediate and final components.
- `FileAttributeTagInfo`, `FileIdInfo`, `FileStandardInfo`, and `FileBasicInfo` structure layouts and availability.
- Volume identity consistency between the retained root and every descendant handle.
- File identity and link-count behavior on NTFS; other filesystems remain unsupported until validated.
- Error mapping for reparse encounters, unsupported information classes, access denial, sharing violations, and rename races.

Native FFI must be isolated in one Windows-only module, with documented safety invariants and minimal unsafe code. No unsafe handle ownership conversion is allowed without a single, explicit owner and close path.

## Adversarial acceptance matrix

| Case | Required outcome |
|---|---|
| Valid local volume GUID root and ordinary workspace | Open and retain the selected directory handle |
| Workspace path includes a symlink, junction, mount point, or other reparse point | Reject during component traversal |
| Intermediate directory is renamed/replaced during traversal | Continue only through the already-opened parent handle; never escape to replacement path |
| Final target is replaced after open | Read from the opened file handle; identity describes that handle |
| UNC, mapped network drive, device namespace, drive-relative path | Reject before filesystem access |
| ADS or ambiguous separator/component form | Reject before filesystem access |
| Directory, device, pipe, socket-like or other non-regular target | Reject based on opened-handle metadata |
| File exceeds the request cap or grows beyond it | Reject without returning a snapshot |
| Concurrent write/delete sharing cannot be denied | Fail closed for the initial prototype |
| Required information class or no-reparse behavior is unavailable | Return an explicit acquisition failure; no fallback |
| Unsupported filesystem or volume identity mismatch | Reject initialization/capture |

Tests must run on Windows CI and on a supported NTFS test environment. CI success alone is not evidence for behavior on ReFS, SMB, FAT/exFAT, removable media, or third-party filesystem filters.

## Exit criteria

This gate is complete only when a Windows-only prototype demonstrates:
1. Trusted volume-root handle acquisition.
2. Component-wise relative traversal with pinned parent handles.
3. Reparse-point rejection both during parsing and by post-open handle inspection.
4. Stable volume/file identity from opened handles.
5. Bounded read and digest/length consistency using the shared snapshot contract.
6. Adversarial Windows tests for root and nested reparse points, path replacement, sharing violations, and unsupported filesystems.
7. No path-based fallback and no write or command execution capability.

Until all exit criteria pass, Windows snapshot capture remains unavailable. Linux support does not imply Windows support.

## References

- Microsoft Learn: [CreateFileW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew)
- Microsoft Learn: [NtCreateFile](https://learn.microsoft.com/en-us/windows/win32/api/winternl/nf-winternl-ntcreatefile)
- Microsoft Learn: [OBJECT_ATTRIBUTES](https://learn.microsoft.com/en-us/windows/win32/api/ntdef/ns-ntdef-_object_attributes)
- Microsoft Learn: [GetFileInformationByHandleEx](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getfileinformationbyhandleex)
- Microsoft Learn: [FILE_ATTRIBUTE_TAG_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_attribute_tag_info)
- Microsoft Learn: [FILE_ID_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_id_info)

This document is a security gate and implementation specification, not a claim that the native sequence has already been implemented or independently verified.
