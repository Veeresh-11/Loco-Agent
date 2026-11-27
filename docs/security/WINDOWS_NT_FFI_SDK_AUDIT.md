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

Microsoft documents `FILE_ID_INFO` as a 64-bit volume serial plus a 128-bit file identifier, with the pair identifying a file on a single computer. The protocol documentation also notes that a filesystem that does not support 128-bit IDs may return zero for the ID. Follow-up review found that the adapter previously accepted this all-zero sentinel as a valid identity; commit `8db3b9cc14b972f0010fa3d9c053ede7dfb082ba` now rejects it as `InvalidFileIdentity`, with positive/negative helper tests. The NTFS-only admission policy remains in force; this check prevents an unsupported identity from being used if the filesystem/provider fails to supply a 128-bit ID.

## Important unresolved contract mismatch: `OBJ_DONT_REPARSE`

The current adapter sets `OBJ_CASE_INSENSITIVE | OBJ_DONT_REPARSE` on the `OBJECT_ATTRIBUTES` passed to user-mode `NtCreateFile`.

Microsoft's general `OBJECT_ATTRIBUTES` reference defines `OBJ_DONT_REPARSE` as preventing reparse-point traversal during name parsing and says an encountered reparse returns `STATUS_REPARSE_POINT_ENCOUNTERED`.

However, Microsoft's user-mode `NtCreateFile` reference describes the `OBJECT_ATTRIBUTES.Attributes` value more narrowly as zero or `OBJ_CASE_INSENSITIVE`. The two pages do not explicitly reconcile whether the user-mode `NtCreateFile` contract accepts `OBJ_DONT_REPARSE` for the exact `RootDirectory` + single-component relative-open pattern used here.

This discrepancy must remain visible:
- The general flag definition supports the intended semantics.
- The user-mode function page does not clearly list this flag among the accepted values.
- The hosted Windows Server 2022 (build 20348) and Windows Server 2025 (build 26100) runners both empirically passed the tested NTFS reparse scenarios with this flag. This broadens the runtime evidence to two Windows Server generations but does not reconcile the published documentation gap or establish behavior on every Windows release/filesystem.
- The code also checks the returned handle's reparse attribute. That is defense in depth, but a post-open check alone must not be described as proof that no reparse traversal occurred before the handle was returned.

**Additional Microsoft Q&A evidence (reviewed 2026-11-24):**
- A Microsoft External Staff moderator reported testing user-mode `NtCreateFile` with a filesystem `RootDirectory` plus one relative `ObjectName` on Windows 11 build 26200 / local NTFS. The reported tests rejected both final and intermediate junction traversal with `STATUS_REPARSE_POINT_ENCOUNTERED`, while a plain directory succeeded. The answer also explicitly distinguished measured behavior from the published documentation and noted that the function-specific page's narrower wording was still present: [Microsoft Q&A, September 14, 2026](https://learn.microsoft.com/en-us/answers/questions/6001857/is-obj-dont-reparse-supported-by-user-mode-ntcreat).
- A separate Microsoft External Staff moderator advised not to treat the same combination as a fully documented, version-stable user-mode security contract, because the `NtCreateFile` page does not expressly list `OBJ_DONT_REPARSE`; a November 6 follow-up reiterated that no published documentation confirms this exact combination across Windows versions, filesystems, and filter drivers: [Microsoft Q&A, November 2–6, 2026](https://learn.microsoft.com/en-us/answers/questions/6056645/does-user-mode-ntcreatefile-support-obj-dont-repar).
- These are useful, attributed Q&A observations and interpretations, not a revised Microsoft API specification or a formal support statement. The two answers agree that the general flag definition exists and that the function page is narrower; they differ in how confidently the general definition can be applied to this user-mode call. The repo's Windows Server 2022/2025 NTFS tests add local evidence on those exact builds but do not resolve the contractual scope.
- The same November answer mentions `CreateDirectory2W(DIRECTORY_FLAGS_DISALLOW_PATH_REDIRECTS)` for path-based directory creation on Windows 11 24H2 / Server 2025. That is not a drop-in replacement for the retained-parent-handle, file-opening operation in this adapter, so this implementation does not switch to it.

**Required before broad support claims:** the two explicit Windows Server generations and their raw reparse tests are now complete. Still obtain authoritative clarification or a separately documented supported contract for the user-mode call, and test any additional Windows versions, filesystems, or filter-driver environments before claiming them. Until then, keep the Windows path fail closed and do not weaken or remove the flag as a workaround.

## Native CI evidence currently available

Rust Core Tests [run #45354176701](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45354176701) passed all seven jobs (Ubuntu, Windows Server 2022, Windows Server 2025, macOS 15 arm64/Intel, and macOS 26 arm64/Intel).

- Windows Server 2022 Datacenter: version `10.0.20348`, build `20348`, x64, PowerShell `7.6.10`, NTFS temporary volume.
- Windows Server 2025 Datacenter: version `10.0.26100`, build `26100`, x64, PowerShell `7.6.10`, NTFS temporary volume.

Both Windows jobs passed 121 unit tests (0 failed, 1 ignored), then separately ran the ignored real-secondary-NTFS-volume fixture and passed it (1 passed each). The logs show these named reparse tests passed on both builds:
- `rejects_directory_symlink_when_creation_is_permitted`
- `rejects_ntfs_junction_mount_point_reparse_tag`
- `snapshot_adapter_rejects_final_file_symlink_reparse_tag`
- `snapshot_adapter_rejects_intermediate_directory_symlink_reparse_tag`
- `snapshot_adapter_rejects_intermediate_ntfs_junction`
- `intermediate_junction_replacement_race_never_captures_outside_bytes`

This is evidence for the two listed hosted Windows Server builds and their NTFS fixtures only. It does not resolve the `OBJ_DONT_REPARSE` documentation mismatch, and it does not establish compatibility for other Windows builds, architectures, or filesystems.

## Native Windows SDK compile-time ABI verification (2026-11-24)

The source-level layout audit is now backed by a C11 compile-time probe, `scripts/windows-ffi-abi-probe.c`, which includes the installed SDK's actual `windows.h` and `winternl.h` declarations. The workflow compiles it with the x64 MSVC toolchain on both explicit Windows runner generations.

The probe uses `_Static_assert` for:
- x64 sizes, alignments, and field offsets of `UNICODE_STRING`, `OBJECT_ATTRIBUTES`, `IO_STATUS_BLOCK`, `FILETIME`, `BY_HANDLE_FILE_INFORMATION`, and `FILE_ID_INFO`;
- `OBJ_DONT_REPARSE == 0x1000` and its inclusion in `OBJ_VALID_ATTRIBUTES`;
- `FileIdInfo == 18`; and
- assignment of the SDK's `NtCreateFile` declaration to the full expected `NTAPI` function-pointer signature.

Rust Core Tests [run #45358148067](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45358148067) passed all nine matrix jobs. The native ABI compile step passed on:
- Windows Server 2022 runner: Windows SDK `10.0.26100.0`, MSVC `19.44.35230.0` (VS 2022).
- Windows Server 2025 runner: Windows SDK `10.0.26100.0`, MSVC `19.52.37029.0` (VS 18).

The full Rust tests also passed on both Windows jobs. This is compile-time evidence for these two installed x64 SDK/toolchain combinations; it is not a formal Microsoft guarantee for all Windows versions or proof of runtime behavior for every filesystem/filter driver. In particular, it does not resolve the separate documentation-scope caveat for using `OBJ_DONT_REPARSE` with user-mode, handle-relative `NtCreateFile`.

## Native `OBJ_DONT_REPARSE` runtime status probe (2026-11-24)

A test-only direct `NtCreateFile` probe now records the raw NTSTATUS for handle-relative opens with and without `OBJ_DONT_REPARSE`. The fixture uses a real NTFS junction and verifies its target is traversable through the ordinary Win32 path API before interpreting the native statuses.

Rust Core Tests [run #45362986206](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45362986206) passed all nine matrix jobs; Contract Validation [run #45362986234](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45362986234) passed. The raw probe output was identical on both Windows builds:

| Probe | Windows Server 2022 | Windows Server 2025 |
|---|---:|---:|
| Ordinary directory with `OBJ_DONT_REPARSE` | `0x00000000` (success) | `0x00000000` (success) |
| Ordinary file with `OBJ_DONT_REPARSE` | `0x00000000` (success) | `0x00000000` (success) |
| Final junction, no flag | `0x00000000` (success) | `0x00000000` (success) |
| Final junction, `OBJ_DONT_REPARSE` | `0xC000050B` | `0xC000050B` |
| Intermediate junction, no flag | `0x00000000` (success) | `0x00000000` (success) |
| Intermediate junction, `OBJ_DONT_REPARSE` | `0xC000050B` | `0xC000050B` |

The flagged denial value `0xC000050B` equals `STATUS_REPARSE_POINT_ENCOUNTERED`. The Win32 path control independently read the outside sentinel through the junction, confirming that the junction fixture itself was active. The raw native baseline also succeeded for both final and intermediate junction paths without the flag.

This is direct runtime evidence for the exact user-mode, handle-relative `NtCreateFile` call shape on Windows Server 2022 build 20348 and Windows Server 2025 build 26100, both x64/NTFS. It materially narrows the earlier uncertainty about whether the flag is honored by this call shape on these builds. It still does not create a universal or version-stable Microsoft API contract, nor does it cover other Windows releases, architectures, filesystems, or filter drivers. Retain the documentation caveat and fail-closed production behavior pending broader authoritative clarification and independent security review.


## Documentation-contract follow-up (2026-11-26)

A further review of Microsoft's current documentation and two October 2026 Microsoft Q&A moderator responses narrows the question but does **not** close the support-contract gate.

### What the sources establish

- The general Microsoft `OBJECT_ATTRIBUTES` reference explicitly defines `OBJ_DONT_REPARSE`: name parsing must not follow reparse points, and encountering one returns `STATUS_REPARSE_POINT_ENCOUNTERED`. It also permits a filesystem directory handle as `RootDirectory` and a relative `ObjectName`: [Microsoft OBJECT_ATTRIBUTES](https://learn.microsoft.com/en-us/windows/win32/api/ntdef/ns-ntdef-_object_attributes). [Source](https://learn.microsoft.com/en-us/windows/win32/api/ntdef/ns-ntdef-_object_attributes)
- Microsoft's user-mode `NtCreateFile` reference documents a relative name through `RootDirectory`, but its function-specific `Attributes` description still says zero or `OBJ_CASE_INSENSITIVE`; it does not expressly list `OBJ_DONT_REPARSE`: [Microsoft NtCreateFile](https://learn.microsoft.com/en-us/windows/win32/api/winternl/nf-winternl-ntcreatefile). [Source](https://learn.microsoft.com/en-us/windows/win32/api/winternl/nf-winternl-ntcreatefile)
- A Microsoft External Staff moderator's October 19, 2026 answer reports that Windows SDK 10.0.26100.0 `winternl.h` lists `OBJ_DONT_REPARSE` under valid `OBJECT_ATTRIBUTES.Attributes` values and includes it in `OBJ_VALID_ATTRIBUTES`. The answer also notes that the user-mode `NtCreateFile` page remains narrower and does not expressly reconcile this exact call shape: [Microsoft Q&A answer](https://learn.microsoft.com/en-us/answers/a/13008163).
- A separate Microsoft External Staff moderator's November 2–6, 2026 answer advises not to treat user-mode `NtCreateFile` + filesystem `RootDirectory` + `OBJ_DONT_REPARSE` as a fully documented, version-stable security contract, because the function-specific page does not expressly list the flag: [Microsoft Q&A thread](https://learn.microsoft.com/en-us/answers/questions/6056645/does-user-mode-ntcreatefile-support-obj-dont-repar).

### Security interpretation and disposition

The SDK-header evidence strengthens the case that the flag is a valid object-attributes bit in the reviewed SDK. The general structure documentation defines its intended no-reparse behavior. The two Microsoft Q&A responses are moderator guidance, not a replacement specification; the more cautious response explicitly distinguishes implementation observations from a version-stable guarantee. The function-specific page has not been reconciled in the evidence reviewed here.

Together with the repository's raw runtime probes on Windows Server 2022 build 20348 and Server 2025 build 26100 (both x64/NTFS), this supports a bounded empirical statement: the tested user-mode, handle-relative call rejected final and intermediate NTFS junctions with `STATUS_REPARSE_POINT_ENCOUNTERED` on those two builds. It does **not** establish the same behavior for other Windows releases, architectures, filesystems, redirectors, filter drivers, or third-party reparse tags.

**Decision unchanged:** retain `OBJ_DONT_REPARSE`, retain the post-open reparse-attribute and file-identity checks, and fail closed on every unexpected status. Do not remove the flag, weaken the post-open checks, or claim universal Windows support to work around the documentation gap. The remaining gate is authoritative clarification for the exact user-mode combination (or a separately documented supported construction), a deliberately bounded support matrix, and independent native security review. No production authorization or write behavior is enabled by this documentation update.

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
