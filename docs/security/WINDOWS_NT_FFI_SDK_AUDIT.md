# Windows x64 native FFI and SDK contract audit

**Date:** 2026-11-24  
**Scope:** Source-level comparison of the Windows x64 declarations in `core/loco-core/src/windows_root_handle.rs` against Microsoft's published Win32/WDK definitions, plus the exact hosted Windows test evidence available to this branch.

**Disposition:** The asserted x64 layouts and function parameter shapes match the published definitions reviewed below. This is not an independent security review, a complete audit of every Windows SDK release, or proof of runtime behavior beyond the exact hosted runner tested. One documentation-contract mismatch around `OBJ_DONT_REPARSE` remains an explicit blocker for broad claims.

## ABI declaration comparison

| Rust declaration | Microsoft definition | x64 layout asserted in Rust | Static comparison |
|---|---|---|---|
| `UnicodeString` | `UNICODE_STRING { USHORT Length; USHORT MaximumLength; PWSTR Buffer; }` | size 16, align 8; offsets 0, 2, 8 | Matches |
| `ObjectAttributes` | `OBJECT_ATTRIBUTES { ULONG Length; HANDLE RootDirectory; PUNICODE_STRING ObjectName; ULONG Attributes; PVOID SecurityDescriptor; PVOID SecurityQualityOfService; }` | size 48, align 8; offsets 0, 8, 16, 24, 32, 40 | Matches |
| `IoStatusValue` + `IoStatusBlock` | `IO_STATUS_BLOCK` union of `NTSTATUS` / `PVOID`, followed by `ULONG_PTR Information` | union size/alignment 8; block size 16, align 8; information offset 8 | Matches |
| `FileTime` | `FILETIME { DWORD low; DWORD high; }` | size 8, align 4; offsets 0, 4 | Matches |
| `ByHandleFileInformation` | `BY_HANDLE_FILE_INFORMATION` with three `FILETIME` fields and the documented DWORD fields | size 52, align 4; asserted offsets 0 through 48 | Matches |
| `FileIdInfo` | `FILE_ID_INFO { ULONGLONG VolumeSerialNumber; FILE_ID_128 FileId; }` | size 24, align 8; offsets 0, 8 | Matches |

This is a type/field-layout comparison, not a build-time inclusion of Microsoft's SDK headers. The assertions detect drift in the Rust declarations but cannot automatically detect a future SDK contract change. The module deliberately compile-errors for Windows targets other than x86_64.

## Function signature and parameter audit

### `NtCreateFile`

The Rust declaration has the same 11-parameter order and widths as Microsoft's user-mode `winternl.h` signature:

1. `PHANDLE` output
2. `ACCESS_MASK` desired access
3. `POBJECT_ATTRIBUTES`
4. `PIO_STATUS_BLOCK`
5. `PLARGE_INTEGER` allocation size
6. `ULONG` file attributes
7. `ULONG` share access
8. `ULONG` create disposition
9. `ULONG` create options
10. `PVOID` EA buffer
11. `ULONG` EA length

The Rust pointer to `i64` represents the 8-byte `LARGE_INTEGER` storage used for the optional allocation size. `i32` is the 32-bit `NTSTATUS` return representation; the other scalar parameters are 32-bit unsigned values. `extern "system"` is used, and the module restricts the declaration to x64.

The selected flags align with the documented parameter semantics:
- Directory opens request `FILE_LIST_DIRECTORY | FILE_TRAVERSE | FILE_READ_ATTRIBUTES | SYNCHRONIZE`; they do not request `FILE_READ_DATA` on a directory.
- File opens request `FILE_READ_DATA | FILE_READ_ATTRIBUTES | SYNCHRONIZE`.
- Both use `FILE_OPEN` (open existing, do not create) and `FILE_SYNCHRONOUS_IO_NONALERT`; Microsoft documents that synchronous I/O options require `SYNCHRONIZE`, which is present.
- The directory and file share modes intentionally omit delete and/or write sharing to pin handles and fail closed on conflicts.
- The code checks the returned `NTSTATUS` and treats negative statuses as failure. The supplied `IO_STATUS_BLOCK` is not used as a success substitute.

### Win32 information functions

The declarations for `GetFileInformationByHandle`, `GetFileInformationByHandleEx`, and `GetVolumeInformationW` use pointer, `DWORD`/enum-sized, and BOOL-compatible integer representations consistent with their Win32 signatures. `FileIdInfo` is class value 18 (`0x12`), matching Microsoft's `FILE_INFO_BY_HANDLE_CLASS` enumeration. The adapter does not fall back to the narrower `BY_HANDLE_FILE_INFORMATION` identity if `FileIdInfo` is unavailable; it fails closed.

Microsoft documents `FILE_ID_INFO` as a 64-bit volume serial plus a 128-bit file identifier, with the pair identifying a file on a single computer. The protocol documentation also notes that a filesystem that does not support 128-bit IDs may return zero for the ID; the current adapter treats a successful zero ID as data rather than independently rejecting it. Whether zero identifiers should be rejected for the declared NTFS-only admission policy is a separate policy/test question, not an ABI-layout mismatch.

## Important unresolved contract mismatch: `OBJ_DONT_REPARSE`

The current adapter sets `OBJ_CASE_INSENSITIVE | OBJ_DONT_REPARSE` on the `OBJECT_ATTRIBUTES` passed to user-mode `NtCreateFile`.

Microsoft's general `OBJECT_ATTRIBUTES` reference defines `OBJ_DONT_REPARSE` as preventing reparse-point traversal during name parsing and says an encountered reparse returns `STATUS_REPARSE_POINT_ENCOUNTERED`.

However, Microsoft's user-mode `NtCreateFile` reference describes the `OBJECT_ATTRIBUTES.Attributes` value more narrowly as zero or `OBJ_CASE_INSENSITIVE`. The two pages do not explicitly reconcile whether the user-mode `NtCreateFile` contract accepts `OBJ_DONT_REPARSE` for the exact `RootDirectory` + single-component relative-open pattern used here.

This discrepancy must remain visible:
- The general flag definition supports the intended semantics.
- The user-mode function page does not clearly list this flag among the accepted values.
- The current hosted Windows Server 2025 runner empirically accepts the flag for the tested NTFS reparse scenarios, but one runner build does not resolve the documentation gap across Windows versions.
- The code also checks the returned handle's reparse attribute. That is defense in depth, but a post-open check alone must not be described as proof that no reparse traversal occurred before the handle was returned.

**Required before broad support claims:** run the reparse denial probes on at least two explicit Windows Server runner generations, retain the raw NTSTATUS values for ordinary directory, symlink, and junction controls, and obtain authoritative clarification or a separately documented supported contract for the user-mode call. Until then, keep the Windows path fail closed and do not weaken or remove the flag as a workaround.

## Native CI evidence currently available

Rust Core Tests [run #45353310385](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45353310385) passed all six jobs. The Windows job reported Microsoft Windows Server 2025 Datacenter, version `10.0.26100`, build `26100`, architecture `X64`, and NTFS for the runner temporary volume.

The Windows job passed 121 unit tests (0 failed, 1 ignored), then separately ran the ignored real-secondary-NTFS-volume fixture and passed it (1 passed). The logs show these named reparse tests passed:
- `rejects_directory_symlink_when_creation_is_permitted`
- `rejects_ntfs_junction_mount_point_reparse_tag`
- `snapshot_adapter_rejects_final_file_symlink_reparse_tag`
- `snapshot_adapter_rejects_intermediate_directory_symlink_reparse_tag`
- `snapshot_adapter_rejects_intermediate_ntfs_junction`
- `intermediate_junction_replacement_race_never_captures_outside_bytes`

This is evidence for that hosted Windows Server 2025 build and its NTFS fixture only. The Windows Server 2022 matrix added alongside this audit is intended to broaden the OS-build evidence; until it passes, it is pending.

## Sources

- Microsoft [`UNICODE_STRING`](https://learn.microsoft.com/en-us/windows/win32/api/ntdef/ns-ntdef-_unicode_string) — field types and byte-count semantics.
- Microsoft [`OBJECT_ATTRIBUTES`](https://learn.microsoft.com/en-us/windows/win32/api/ntdef/ns-ntdef-_object_attributes) — field types, RootDirectory-relative naming, and `OBJ_DONT_REPARSE` definition.
- Microsoft [user-mode `NtCreateFile`](https://learn.microsoft.com/en-us/windows/win32/api/winternl/nf-winternl-ntcreatefile) — function signature, access masks, synchronous I/O requirements, share modes, disposition/options, and the narrower Attributes wording.
- Microsoft [`IO_STATUS_BLOCK`](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/ns-wdm-_io_status_block) — union and pointer-sized Information field.
- Microsoft [`BY_HANDLE_FILE_INFORMATION`](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/ns-fileapi-by_handle_file_information) — field order and identity limitations.
- Microsoft [`FILE_ID_INFO`](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_id_info) and [`FILE_ID_128`](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-file_id_128) — 64-bit volume serial and 128-bit identifier.
- Microsoft [`GetFileInformationByHandleEx`](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getfileinformationbyhandleex) — `FileIdInfo` class value 18.

## Review disposition

The static x64 layout comparison found no field-size or offset mismatch in the declarations listed above. This is not independent approval. Keep the Windows FFI and workspace snapshot PR stack draft until the `OBJ_DONT_REPARSE` user-mode contract discrepancy, expanded Windows version matrix, and independent native security review are resolved.
