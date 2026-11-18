# Windows Workspace Snapshot Acquisition — Security Design

Status: design proposal; implementation is not yet enabled.

## Goal
Create a Windows-only, fail-closed adapter for the read-only WorkspaceSnapshotAdapter contract. It must remain workspace-root bound and reject reparse points during traversal.

## Threats
- Traversal, drive/UNC/NT namespace path confusion
- Symlinks, junctions, mount points and other reparse points
- Component replacement / TOCTOU races
- Mutation while reading, special targets, size abuse, hard-link aliases
- Case/normalization, reserved names, alternate data streams

## Proposed approach
1. Apply portable lexical path validation: reject absolute, drive-qualified, UNC, ADS colon, empty, dot/dot-dot, trailing dot/space, control characters, and reserved device stems.
2. Pin root with a directory handle from CreateFileW + FILE_FLAG_BACKUP_SEMANTICS; keep it for adapter lifetime. Canonical path is diagnostic only.
3. Avoid opening the entire relative path in one ordinary CreateFileW call. Prototype handle-relative traversal using native NtCreateFile and OBJECT_ATTRIBUTES.RootDirectory, one component at a time.
4. Open components without following reparse points (FILE_OPEN_REPARSE_POINT / FILE_FLAG_OPEN_REPARSE_POINT semantics), inspect opened handle attributes/tag, reject FILE_ATTRIBUTE_REPARSE_POINT; intermediate components must be directories.
5. Define explicit share modes; prototype read-only sharing and deny write/delete sharing to reduce mutation while held. Fail closed on sharing violations and review deadlock implications.
6. Final target must be a regular file; reject directory/device/pipe-like objects. Consider hard-link rejection with handle-based information; document filesystem limitations.
7. Enforce byte cap before and during bounded reads; read only from final handle.
8. Compare handle metadata before/after read (file ID, volume identity, size, link count where available, timestamps). Reject mismatch. This is best-effort detection, not atomic snapshot guarantee.
9. SHA-256 captured bytes and length remain authoritative; validate existing snapshot contract.
10. No path-based fallback or weaker adapter fallback. Unsupported native behavior returns acquisition failure.

## Implementation gates
- Validate native NtCreateFile interface/structures for supported Windows targets and keep FFI minimal.
- Review FileAttributeTagInfo, FileIdInfo and compatibility fallback options.
- Validate sharing behavior and link-count policy.
- Test on Windows CI and NTFS; treat other filesystems separately until verified.

## Adversarial tests
Root/nested directories; empty and cap-boundary files; absolute/drive/UNC/ADS/traversal/reserved names; symlink/junction/mount-point reparse in every path component; rename/replacement races; concurrent mutation; directory and special-object attempts; hard links; case collisions; file ID and metadata stability; request parser fuzz/property tests.

## Open questions
- Native handle-relative traversal compatibility and FFI review
- Reliable handle metadata classes across supported Windows versions
- Fail-closed sharing violation behavior
- Hard-link policy and filesystem support
- CI matrix and unsupported filesystem behavior

## References (Microsoft Learn)
- CreateFileW: https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew
- GetFinalPathNameByHandleW: https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getfinalpathnamebyhandlew
- Creating and Opening Files: https://learn.microsoft.com/en-us/windows/desktop/FileIO/creating-and-opening-files

This document is a design gate, not an implementation claim or proof of Windows security parity.

## Additional native API review (2026-11-18)

### Handle-relative component opens
Microsoft documents that `NtCreateFile` can receive an `OBJECT_ATTRIBUTES.RootDirectory` handle and a relative `ObjectName`, so a traversal prototype should open one component at a time from a trusted parent handle. Use a single component per operation; do not pass user-supplied multi-component strings to the native call.

### Reparse-point gate
`FILE_OPEN_REPARSE_POINT` opens a reparse point itself rather than following normal reparse processing for the final target. The `OBJ_DONT_REPARSE` object attribute is documented to fail parsing when reparse points are encountered. These controls must be tested and combined with post-open handle inspection; a flag alone is not a security proof.

### Critical root-handle acquisition caveat
The relative-open API documents how a preexisting directory handle anchors subsequent opens. It does not, by itself, prove that the initial workspace root was reached without following an attacker-controlled reparse point. Root acquisition must therefore be separately specified and tested. If root acquisition cannot establish the intended boundary, the adapter must reject initialization rather than proceed.

### Prototype acceptance criteria
- Trusted root handle acquisition documented for supported Windows versions/filesystems.
- Component-wise relative opens from each pinned parent handle.
- Reparse-point rejection during parse and post-open inspection.
- Intermediate handle must be verified as a directory.
- Final handle must be verified as a regular file.
- Byte cap enforced; metadata before/after read compared.
- No fallback to ordinary path-based opening.
- Adversarial tests for root and nested reparse paths, rename races, and unsupported filesystems.

### Microsoft sources
- `NtCreateFile`: https://learn.microsoft.com/windows/win32/api/winternl/nf-winternl-ntcreatefile
- `OBJECT_ATTRIBUTES`: https://learn.microsoft.com/windows/win32/api/ntdef/ns-ntdef-_object_attributes
- `OBJ_DONT_REPARSE` is documented as a reparse parsing guard in `OBJECT_ATTRIBUTES`; validate actual behavior on the supported user-mode target matrix before relying on it.
