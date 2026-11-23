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

### F-07 — Windows root admission uses an explicit NTFS allowlist [Implementation added; broader matrix remains open]

The Windows root gate now queries the filesystem name with `GetVolumeInformationW` after confirming the volume is fixed and the root path is a canonical volume-GUID path. Only `NTFS` is admitted. ReFS, FAT/exFAT, unknown filesystems, and query failures are rejected; there is no compatibility fallback. The allowlist is intentionally narrow because this adapter's handle-relative traversal, reparse, sharing, file-ID, and hard-link behavior has not yet been validated across other filesystems.

A Windows-native test checks that the CI temporary workspace volume reports NTFS and is admitted. This establishes coverage only for the Windows CI runner's NTFS volume. It does not validate ReFS or other filesystems. Before expanding the allowlist, add dedicated native tests for each filesystem's file IDs, reparse semantics, hard links, sharing behavior, rename/mount semantics, and metadata behavior.

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
6. F-07 now admits NTFS only; add native tests before considering ReFS or other filesystems.
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


## Follow-up finding — macOS filesystem admission (2026-11-23)

The macOS adapter now queries the filesystem type from the retained workspace-root descriptor with `fstatfs`. Only the exact filesystem name `apfs` is admitted. A failed query, invalid filesystem-name encoding, or any other filesystem name fails closed. This avoids treating the existing `st_dev` descendant check as proof that every filesystem has been reviewed.

The native macOS test asserts that the hosted CI temporary workspace reports APFS and that the adapter admits that root. The result is limited to the filesystem actually used by that runner. It does not validate HFS+, network or removable filesystems, APFS firmlinks, or mount arrangements not exercised by the test. Descendant directory and final-file device checks remain the mount-boundary control; native tests are still required to establish their behavior for supported APFS mount configurations.

**Disposition:** APFS is the only admitted macOS filesystem baseline in this prototype. Do not expand the allowlist without filesystem-specific native tests and review. This change remains a read-only snapshot prototype and does not provide atomic file-content snapshots.


## Follow-up test — nested APFS volume boundary (2026-11-23)

PR #69 adds a macOS CI fixture that creates a sparse APFS disk image and mounts it beneath a temporary workspace root. The dedicated native test confirms the workspace and nested volume both report `apfs`, confirms their `st_dev` values differ, creates a file on the mounted volume, and requires capture through the workspace-relative path to fail with `AcquisitionFailed`.

This directly exercises the adapter's existing same-device descendant rule against a distinct mounted APFS volume. The test is explicitly invoked by the macOS workflow and is not part of the normal test run unless the fixture is prepared. The job must fail if fixture setup or the dedicated assertion fails.

Scope remains narrow: this does not validate APFS firmlinks, HFS+, network/removable volumes, or every APFS mount arrangement. It also does not establish atomic file-content snapshots. Record the exact-head macOS job result before treating this test as executed evidence.


## Follow-up test — real Windows secondary NTFS volume mountpoint (2026-11-23)

PR #70 adds a Windows CI fixture that creates and attaches a VHDX, formats its partition as NTFS, and assigns it to a directory mounted beneath the workspace. The dedicated test verifies that the workspace and mounted directory resolve to distinct volume-GUID roots, confirms the mounted volume is admitted as NTFS on its own, and requires the workspace-relative directory traversal to fail closed with a reparse, relative-open, or volume-identity error.

This is distinct from the existing junction test: it uses a real secondary volume mountpoint rather than a junction to another directory on the same volume. The workflow explicitly invokes the ignored test and detaches the VHDX in an always-run cleanup step. Treat this as executed native evidence only after the exact-head Windows CI job passes.

The test covers the hosted runner's NTFS behavior only. It does not expand the allowlist or validate ReFS, FAT/exFAT, removable/network filesystems, or all Windows storage/filter-driver combinations.


## Follow-up native mount evidence (2026-11-23)

- **Windows secondary NTFS volume:** PR #70 implementation head `f07175b81f7e6c0526f08c829adb18b6db22dc43`; Rust matrix run `45186946850` and Contract Validation `45186946764` passed. The Windows runner created a separate NTFS VHDX mounted beneath the workspace; the capture traversal test and cleanup passed.
- **Linux same-device bind mount:** PR #71 implementation head `dc5f51419261011d4971076693457bcec7562e17`; Rust matrix run `45187459253` and Contract Validation `45187459247` passed. Ubuntu created a real bind mount whose device identity matched the workspace; the explicit `RESOLVE_NO_XDEV` rejection test passed.
- **Linux distinct ext4 mount:** PR #72 implementation head `b6d17385bedc767dd940f5a4347b62151a29f7b9`; Rust matrix run `45187940996` and Contract Validation `45187941018` passed. Ubuntu created and mounted an ext4 loop image with a distinct device identity; capture failed closed as required.
- **macOS nested APFS volume:** rerun in PR #72's matrix; the APFS sparse-image fixture and dedicated mount-boundary test passed.

These are exact hosted CI environment results, not universal platform guarantees. Linux's bind-mount test proves a case that `st_dev` alone cannot distinguish; `RESOLVE_NO_XDEV` is the enforcement tested. The ext4 loop mount tests a different-device crossing. The Windows result is limited to NTFS and the macOS result to APFS. Firmlinks, HFS+, ReFS, FAT/exFAT, network/removable filesystems, and broader native OS/storage combinations remain outside this evidence.


## Follow-up test — macOS /Users firmlink boundary (2026-11-23)

PR #73 adds a native macOS test that uses the hosted runner's `/Users` firmlink into the APFS Data volume. The first run showed `/` and `$HOME` share the same `st_dev`, demonstrating that device identity alone does not detect this firmlink. The test now checks the opened `/Users` handle's `st_flags` for Apple's `SF_FIRMLINK` bit, then requires descriptor-relative capture through `Users/...` to fail closed.

The test constructs an adapter directly from the root handle only inside the test module; this is not a production constructor change, and normal workspace admission continues to reject `/`. The exact-head macOS job must confirm that the opened handle retains the `SF_FIRMLINK` bit and that the adapter rejects it. Apple's XNU header defines `SF_FIRMLINK` as `0x00800000`; Apple's FileFlags documentation describes firmlinks as transparent links between the read-only System and writable Data volumes. This does not cover all firmlink paths, OS releases, or APFS layouts.
