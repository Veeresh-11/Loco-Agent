# Windows Native Root Handle Prototype

Status: prototype / Windows CI validation required. This is not a Windows WorkspaceSnapshotAdapter and does not establish production security parity.

## Implemented boundary

The Windows-only Rust module `windows_root_handle` currently provides:

- Strict parsing of a canonical volume GUID root (`\\?\Volume{GUID}\`).
- A fixed-volume-only admission check.
- Initial root open through `CreateFileW` with directory semantics and open-reparse-point behavior.
- Root metadata inspection through GetFileInformationByHandle on the opened handle; require a directory, reject reparse-point attributes, and capture volume serial + file index.
- One-component-at-a-time directory traversal through `NtCreateFile` using the retained parent handle as `OBJECT_ATTRIBUTES.RootDirectory`.
- `OBJ_DONT_REPARSE` on each relative open, followed by handle metadata inspection and volume-identity comparison.
- Share read/write but deny delete sharing while each directory handle is retained, so the prototype fails closed on concurrent rename/delete attempts.
- Windows tests for current-temp-directory traversal, retained-handle rename denial, invalid path components, and directory-symlink rejection where the runner permits symlink creation.

The implementation intentionally exposes no file read, snapshot construction, patch, write, process, or authorization operation.

## Important compatibility and security caveats

1. The initial root is opened directly by a volume GUID root path. The selected workspace path is not passed to `CreateFileW`; it is decomposed and traversed from the retained volume-root handle.
2. The relative-open implementation uses native `NtCreateFile` and a manually declared minimal FFI surface. Its structure layout, calling convention, access mask, and handle ownership must be reviewed for every supported Windows target.
3. Microsoft documentation describes `RootDirectory` relative resolution and `OBJ_DONT_REPARSE`, but the user-mode `NtCreateFile` page has narrower wording for object attributes. The prototype therefore treats every non-success status as a hard failure and also checks returned-handle metadata. Handle identity is read from the stable BY_HANDLE_FILE_INFORMATION structure rather than nightly-only Rust MetadataExt identity accessors. A Windows CI pass is measured compatibility evidence for that runner, not a universal contract across Windows versions, filesystems, or filter drivers.
4. The prototype allows ordinary sharing on the fixed volume-root handle, but denies delete sharing on traversed workspace directory handles. This is intentionally conservative and can cause sharing violations or compatibility issues. Those outcomes are errors, not reasons to fall back.
5. Only local fixed volumes are admitted. Network, removable, and non-NTFS behavior is not claimed.
6. The prototype has not yet opened a final regular file, enforced byte limits, hashed captured bytes, or compared file metadata before/after a read.

## Validation required

The PR CI must compile and run the Windows-only tests on `windows-latest`, as well as retain the existing Linux/macOS test matrix. Before this module can be used for snapshots, follow-up work must add:

- A documented NTSTATUS-to-error mapping and native FFI review.
- Adversarial tests for symlinks, junctions, mount points, component replacement, and share violations on NTFS.
- Final-file opening with explicit non-directory semantics and opened-handle type/reparse checks.
- Bounded reads and identity/digest construction using the shared snapshot contract.
- A documented supported Windows version and filesystem matrix.

No path-based fallback is permitted. If a required operation or guarantee is unavailable, the Windows adapter must remain unavailable.
