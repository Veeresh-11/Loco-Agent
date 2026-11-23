# Native Filesystem API Review — Snapshot Adapters

**Review date:** 2026-11-23  
**Reviewed branch:** `security/native-filesystem-api-review`  
**Review base:** PR #60, commit `92d9d75f3cb886da4849f3213f8465292fbab6cf`  
**Scope:** Static review of the current Linux, macOS, and Windows read-only snapshot adapters and their native API assumptions.

This is a source-level review, not a certification. No native VM/filesystem testing was performed as part of this document. Findings below are review conclusions and follow-up requirements; do not interpret them as proof of runtime behavior on every supported OS/filesystem.

## Executive summary

The adapters use the right general direction: retain a root directory handle, resolve each descendant relative to a held parent, reject symlinks/reparse points, bound reads, and validate a digest over the returned bytes. The current prototypes still have platform-policy differences and unresolved native compatibility requirements.

**Merge gate: not ready.** Keep the snapshot stack draft until the high-priority decisions and tests below are completed.

## Findings

### F-01 — Linux descendant mount crossings are rejected [Implementation added; native test gate remains]

Linux descendant capture now sets `RESOLVE_NO_XDEV` in addition to `BENEATH | NO_SYMLINKS | NO_MAGICLINKS`. The policy is intentionally strict: **all descendant mount crossings are denied, including bind mounts**, even when the mounted subtree resides on the same underlying filesystem. This aligns the Linux adapter with the macOS device-boundary and Windows volume-boundary intent and avoids silently treating a mounted subtree as ordinary workspace content.

The workspace root itself is acquired without `NO_XDEV`, so a workspace whose root directory resides on a separate mounted filesystem can still be admitted. The restriction applies to resolution below the retained root handle.

Compatibility cost: workspaces that intentionally include nested mounts or bind-mounted dependency/source directories will receive fail-closed acquisition errors for paths beneath those mounts. Users must choose a workspace root that contains the desired files on one mount, or a future separately reviewed trusted-mount admission design must replace this strict policy. No path-based fallback is permitted.

A Linux test now attempts to create a real bind mount beneath the workspace and verifies capture fails closed. It runs the assertion when the runner has mount privileges; hosted unprivileged runners report a permission-based skip. **A privileged Linux VM test remains required before claiming the real bind-mount behavior has been exercised.** A real distinct-filesystem mount test is also still required.

The openat2 API remains Linux 5.6+; unsupported syscall/flag behavior continues to fail closed. Do not add a fallback.

### F-02 — Linux captured byte count must equal the initial file size [Implemented by PR #62]

The Linux adapter now requires the captured byte count to equal the initial opened-handle metadata length before constructing the snapshot. Deterministic helper tests cover exact, short, and over-length results. This is consistency validation only: metadata comparison remains best-effort and does not create an atomic content snapshot.

### F-03 — Windows native FFI declarations have an x64 ABI/layout gate [Partially implemented]

The manual `UNICODE_STRING`, `OBJECT_ATTRIBUTES`, `FILETIME`, `BY_HANDLE_FILE_INFORMATION`, `IO_STATUS_BLOCK`, and `NtCreateFile` declarations now have compile-time x64 size, alignment, and field-offset assertions. The module explicitly emits a compile error for non-x86_64 Windows targets. The declared support scope is therefore **Windows x64 only**; x86 and ARM64 are unsupported until their ABI/calling-convention declarations and native tests are separately added.

These assertions reduce silent layout drift but do not replace an independent comparison against the exact Windows SDK headers or runtime validation of desired-access/share/create flags and NTSTATUS handling. Prefer SDK-backed definitions for public structures where practical. The FFI gate remains open for independent review before approval integration.

### F-04 — `OBJ_DONT_REPARSE` is documented, but the adapter must retain post-open verification [High]

Microsoft's `OBJECT_ATTRIBUTES` documentation defines `OBJ_DONT_REPARSE` as preventing reparse traversal and specifies `STATUS_REPARSE_POINT_ENCOUNTERED` when reparses are encountered. The adapter also checks the opened handle's reparse attribute, which is the correct defense-in-depth pattern.

Keep both checks. Do not treat the flag alone as a complete security proof. Preserve a native Windows test for intermediate junctions, directory symlinks, final-file symlinks, and real volume mount points. PR #60 does not yet create a real secondary-volume mount point.

### F-05 — Windows sharing modes intentionally trade compatibility for pinning [Partially implemented]

Directory opens omit delete sharing and final-file opens share read only. This prevents conflicting writes/renames while the snapshot handle is held; an already-open incompatible handle must make acquisition fail closed. The adapter does not retry with weaker sharing or reopen by pathname.

PR #65 adds a native Windows test that holds a pre-existing writer handle with no sharing and verifies snapshot capture returns `AcquisitionFailed`. Existing tests cover the inverse direction: an acquired snapshot handle blocks subsequent writes and renames. The test still needs exact-head Windows CI execution. Compatibility failures from editors, indexers, antivirus/filter drivers, sync clients, and other handles remain expected behavior and should be surfaced as acquisition failures.

### F-06 — Windows uses handle-derived 128-bit file identity [Implementation added; approval semantics remain open]

The adapter now calls `GetFileInformationByHandleEx` with `FileIdInfo` (class 18) for every opened root/directory/final-file handle. Its identity value contains the 64-bit volume serial and full 128-bit file identifier. The manually declared `FILE_ID_INFO` layout has compile-time x64 size/alignment/offset assertions. There is **no fallback** to the legacy 32-bit volume serial + 64-bit file index: if the filesystem or OS cannot provide `FileIdInfo`, acquisition fails closed. Microsoft documents the `FileIdInfo` information class as unsupported before Windows 8 / Windows Server 2012, so older Windows versions are outside the adapter's supported baseline.

A Windows-native test checks that repeated opens of the same file return the same identity and distinct files on the same volume have distinct 128-bit IDs. The exact-head Windows job must execute this test.

This improves per-computer handle identity but does **not** make an approval identifier globally unique, permanent, or safe across volume reformat, restore, clone, file-ID reuse, or filesystem migration. Approval records must bind a fresh handle-derived identity to workspace/policy/action and content digest, and enforcement must reacquire the target and compare identity plus digest immediately before the operation. Never trust a serialized old file ID as authorization by itself.

### F-07 — Windows volume type is not the same as a supported-filesystem allowlist [Medium]

The root gate admits `DRIVE_FIXED` volumes. That is not, by itself, a declared filesystem support matrix. The adapter must query and record the actual filesystem type, then explicitly allow only filesystems whose reparse, hard-link, sharing, rename, and metadata semantics have been tested.

Until that allowlist and native tests exist, unsupported or unknown filesystems must fail closed. Keep network/removable/unverified volume types out of scope as already documented.

### F-08 — macOS descriptor-relative traversal is appropriately component-wise, but native filesystem validation remains [Medium]

The adapter opens each directory component relative to a retained descriptor with `O_DIRECTORY | O_NOFOLLOW`, then opens the final file with `O_NOFOLLOW | O_NONBLOCK`. Apple's `open(2)` documentation defines `O_NOFOLLOW` as refusing to follow a final-component symlink; opening one component per `openat` call is therefore important and must be preserved.

The adapter checks `st_dev` for descendants, which rejects different-device mount crossings. Validate this behavior on the supported APFS/HFS+ configurations and explicitly document APFS firmlinks, network filesystems, removable volumes, and other filesystems as supported or unsupported based on native tests—not assumptions.

### F-09 — Snapshot metadata comparisons are not atomic snapshots [Informational, applies to all]

The adapters compare identity/size/link/timestamps before and after bounded reads. This can detect many concurrent modifications but does not freeze content or prove that every intermediate byte came from one atomic version under all filesystem semantics.

Continue to treat the digest as binding the returned bytes to the returned snapshot record only. Approval enforcement must reacquire/revalidate the target immediately before writing or execution; it must not rely on an old snapshot as a lock.

## Platform API notes

| Platform | Current primitive | Required support baseline / caveat |
|---|---|---|
| Linux | `openat2` with `BENEATH`, `NO_SYMLINKS`, `NO_MAGICLINKS`, `NO_XDEV` for descendants | Linux 5.6+; no fallback; descendant mount and bind-mount crossings are rejected |
| macOS | component-wise `openat` with `O_NOFOLLOW` | native APFS/HFS+ and mount behavior still require testing |
| Windows | `CreateFileW` for volume root; `NtCreateFile` with `RootDirectory` and `OBJ_DONT_REPARSE` for descendants | manually declared FFI requires ABI gates; fixed-volume admission is not yet a filesystem allowlist |

## Required implementation/review order

1. Run PR #63 bind-mount and distinct-filesystem mount tests on a privileged Linux VM; hosted CI may lack mount privileges.
2. F-02 exact Linux byte-count validation is implemented by PR #62.
3. F-03 Windows ABI/layout assertions and the x64-only target gate are implemented by PR #64; independent SDK-header review remains.
4. Complete F-04/F-05 native Windows reparse and sharing tests, including a real second-volume mount point; PR #65 adds pre-existing sharing-conflict coverage.
5. F-06 now uses `FILE_ID_INFO`; define approval freshness/digest semantics and test supported filesystems.
6. Define and test F-07 filesystem allowlist.
7. Complete F-08 native macOS filesystem/mount matrix.
8. Re-run the complete CI matrix and update the roadmap with exact run IDs and platforms actually tested.

## References

- Linux `openat2(2)`: https://man7.org/linux/man-pages/man2/openat2.2.html
- Linux `RESOLVE_NO_XDEV` semantics and bind-mount caveat: https://manpages.ubuntu.com/manpages/noble/man2/openat2.2.html
- Microsoft `OBJECT_ATTRIBUTES` and `OBJ_DONT_REPARSE`: https://learn.microsoft.com/en-us/windows/win32/api/ntdef/ns-ntdef-_object_attributes
- Microsoft `CreateFileW`, reparse-point opening: https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew
- Microsoft file sharing semantics: https://learn.microsoft.com/en-us/windows/desktop/FileIO/creating-and-opening-files
- Apple `open(2)`, `O_NOFOLLOW`: https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/open.2.html
- Apple `stat(2)`, device and inode metadata: https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/stat.2.html

## Review disposition

**Do not approve the snapshot stack for merge yet.** The review found actionable hardening work and policy decisions, but no claim is made here that the adapters are exploitable or that the native APIs fail. Resolve the findings, add the native tests, and obtain an independent review of the Windows FFI and cross-platform mount policy before changing PR readiness.
