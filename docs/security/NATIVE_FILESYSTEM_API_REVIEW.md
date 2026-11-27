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

### F-01 — Linux mount crossing is currently permitted [High: policy decision required]

The Linux root acquisition uses `NO_SYMLINKS | NO_MAGICLINKS`; descendant capture uses `BENEATH | NO_SYMLINKS | NO_MAGICLINKS`. Neither call sets `RESOLVE_NO_XDEV`. Therefore the current code does not reject mount-point or bind-mount crossings during resolution.

The macOS adapter rejects descendant objects whose `st_dev` differs from the workspace root; the Windows adapter checks the volume serial against the retained volume root. This makes the current cross-platform policy inconsistent.

**Required decision:** explicitly choose one of:
1. Reject all descendant mount crossings on Linux by adding `RESOLVE_NO_XDEV`, with tests and documented bind-mount compatibility impact; or
2. Permit selected mounted subtrees and define a trusted mount-admission policy that is equivalent across platforms.

Do not add `NO_XDEV` blindly: Linux documentation notes it rejects bind mounts too, which are common in containerized and developer environments. Add tests for a real mount and bind mount on a privileged Linux runner/VM before claiming coverage.

The `openat2` API was introduced in Linux 5.6. The current adapter intentionally has no path-based fallback; keep that fail-closed behavior and document the minimum kernel requirement clearly.

### F-02 — Linux captured byte count is not explicitly compared with the initial file size [Medium]

The Linux adapter bounds the read and compares handle metadata before and after, but unlike the macOS and Windows adapters it does not explicitly require `bytes.len() == metadata.len()` after reading.

Add the exact-length check before constructing the snapshot. This is a low-risk consistency hardening change and should have tests for short reads and concurrent truncation. Metadata comparison remains best-effort and must not be described as an atomic content snapshot.

### F-03 — Windows native FFI declarations need an ABI/layout gate [High]

The Windows adapter manually declares `UNICODE_STRING`, `OBJECT_ATTRIBUTES`, `IO_STATUS_BLOCK`, `BY_HANDLE_FILE_INFORMATION`, and `NtCreateFile`. The declarations use `repr(C)` and `extern "system"`, which are directionally appropriate, but current CI evidence is the hosted Windows runner's target architecture only.

Before relying on these declarations:
- Prefer SDK-backed definitions where practical; otherwise add compile-time size/alignment/offset assertions for every supported target architecture.
- Add a Windows x64 and any intended x86/ARM64 compile/test matrix. If an architecture is not supported, explicitly exclude it from the support matrix.
- Keep the exact native return status and raw NTSTATUS in diagnostic/test context; continue to fail closed for unknown failures.
- Verify desired-access, share-access, create-options, and synchronous-I/O flags against the SDK headers used by the supported Windows SDK.

### F-04 — `OBJ_DONT_REPARSE` is documented, but the adapter must retain post-open verification [High]

Microsoft's `OBJECT_ATTRIBUTES` documentation defines `OBJ_DONT_REPARSE` as preventing reparse traversal and specifies `STATUS_REPARSE_POINT_ENCOUNTERED` when reparses are encountered. The adapter also checks the opened handle's reparse attribute, which is the correct defense-in-depth pattern.

Keep both checks. Do not treat the flag alone as a complete security proof. Preserve a native Windows test for intermediate junctions, directory symlinks, final-file symlinks, and real volume mount points. PR #60 does not yet create a real secondary-volume mount point.

### F-05 — Windows sharing modes intentionally trade compatibility for pinning [Medium]

The directory opens omit delete sharing, and final-file opens share read only. Windows sharing rules make subsequent conflicting write/delete/rename opens fail while the handle remains open; an existing incompatible handle can also make acquisition fail.

This is a defensible fail-closed strategy, but document that editors, indexers, antivirus/filter drivers, sync clients, and other open handles can cause acquisition failures. Add native tests for pre-existing incompatible handles and record the supported failure behavior. Do not add a fallback that reopens by pathname.

### F-06 — Windows file/volume identity should not be treated as globally unique [Medium]

The current adapter builds identity from the 32-bit volume serial and 64-bit file index exposed by `BY_HANDLE_FILE_INFORMATION`. Treat this only as an identity scoped to the currently held volume/handle context, not as a globally unique or durable identifier for approval records.

Before approval integration, evaluate `GetFileInformationByHandleEx(FileIdInfo)` and its volume/file identifier fields, and define how identity is invalidated across volume reformat, restore, clone, or filesystem changes. Bind approvals to a fresh handle-derived identity and content digest at enforcement time.

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
| Linux | `openat2` with `BENEATH`, `NO_SYMLINKS`, `NO_MAGICLINKS` | Linux 5.6+; no fallback; mount-crossing policy is unresolved |
| macOS | component-wise `openat` with `O_NOFOLLOW` | native APFS/HFS+ and mount behavior still require testing |
| Windows | `CreateFileW` for volume root; `NtCreateFile` with `RootDirectory` and `OBJ_DONT_REPARSE` for descendants | manually declared FFI requires ABI gates; fixed-volume admission is not yet a filesystem allowlist |

## Required implementation/review order

1. Resolve F-01 mount policy and implement real mount/bind-mount tests on a privileged Linux VM.
2. Add F-02 exact Linux byte-count validation.
3. Add F-03 Windows ABI/layout assertions and define supported architectures.
4. Complete F-04/F-05 native Windows reparse and sharing tests, including a real second-volume mount point.
5. Decide F-06 identity format before binding snapshots to approvals.
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


## Review addendum — 2026-11-27: PR #104 bulk-record residuals and PR #106 evidence

This addendum records a focused follow-up against the current branch files, not a recertification.

### PR #106 — native evidence probe
- Current reviewed head: `cc7758783760ca5c940fe5d438cae445322a585d`.
- Rust Core Tests #45984257580 passed on Ubuntu, Windows and macOS. The macOS job executed the firmlink metadata probe and nested APFS checks.
- No Contract Validation run was associated with this exact head during this refresh.
- No submitted GitHub review or inline review thread was returned. Review is first-pass only; independent native security review remains outstanding.

### PR #104 — bulk parser follow-up
- Current PR head at review: `5b78582e99230302ee2936f87c60b5ff7a0d1988`.
- The implementation now checks each attrreference's own structure boundary, checked payload bounds, nonnegative offsets, NUL termination and absolute path. These are accepted as partial remediation of earlier offset findings.
- Residual test gap: no synthetic fixture seam exercises the full bulk record parser across malformed header/record lengths, required and omitted returned-attribute bits, optional `ATTR_CMN_ERROR` placement and nonzero per-entry errors, and cross-payload overlap between name and no-firmlink path. The older `parse_nofirmlink_path_record` malformed fixture does not exercise the bulk layout.
- Residual evidence gap: the parser confirms `ATTR_CMN_FLAGS` was requested/returned, then skips its four-byte value. Decode and preserve the value in diagnostics/probe output. This does not itself settle the security contract: observed hosted-runner values continue to conflict with documented `SF_FIRMLINK` semantics; no path/flag heuristic may be promoted into an authorization grant without authoritative reconciliation.
- Historical exact-head CI for #104 SHA `5b78582...`: Rust Core Tests #45344512869 and Contract Validation #45344512880 both passed. No later #104 source change is asserted by this addendum.
- Keep the adapter fail-closed and preserve descriptor-relative opens, no-follow behavior, post-open path validation and filesystem identity checks.

### Repository branch review status
A refreshed branch-name census yielded 175 branches (100 + 75). **All branches reviewed: NO.** A branch listing only enumerates names; each branch still requires a comparison against its base/main, unique commit/file inspection, associated PR and review-thread checks, and CI/reachability classification. The previous 21/21 first-pass metric applied only to its enumerated PR scope and is not a repository-wide review-completion figure.


## Review addendum — 2026-11-27: PR #107 bulk parser follow-up

- Reviewed PR #107's one-file implementation and synthetic fixtures at initial head `677e9bf2859102df37141d3aca4e734ef51a62d1`. The extracted helper and tests cover the previously missing optional `ATTR_CMN_ERROR` handling, returned attribute masks, malformed lengths, offset/alignment checks, NUL termination, path validation, and name/path/fixed-field overlap. Raw `ATTR_CMN_FLAGS` is preserved as diagnostic data only; no authorization inference is made from it.
- Found and corrected the remaining filename bound: `ATTR_CMN_NAME` had a 256-byte cap, while XNU documents UTF-8 name payloads up to `NAME_MAX * 3 + 1` bytes. The branch now uses 766 bytes (Darwin NAME_MAX 255) and includes maximum-bound and over-bound fixtures. Reference: https://github.com/apple-oss-distributions/xnu/blob/main/bsd/man/man2/getattrlist.2#L2939-L2958.
- Fix commits: `242ca636a78ad51dbd3cc86db59c10299dd04dd5` and `0af5c5f8eb231254f0d82ea2b01c00f75d497a90`.
- Current exact head: `0af5c5f8eb231254f0d82ea2b01c00f75d497a90`. No Actions runs or combined status checks were returned for this head at the time of this addendum. Prior green runs #45346723420/#45346723355 belong to `677e9bf...` only.
- Branch comparison against its declared base: 15 ahead / 4 behind; merge base `8c4a2d0bced52c76fa528a3aab2ecc13328a3cde`. The declared base is now at `8c4a2d0bced52c76fa528a3aab2ecc13328a3cde`; the feature branch must be reconciled before integration.
- PR #107 remains draft/open. The connector refused a formal REQUEST_CHANGES review because the repository owner authored the PR; a detailed PR conversation comment was posted instead: https://github.com/Veeresh-11/Loco-Agent/pull/107#issuecomment-7218001141.
- Parser correctness does not resolve the separate native firmlink flag discrepancy. Keep issue #74 and independent native security review open; no production confinement/authorization claim is made.


### CI status refresh — PR #107 corrected head
- Contract Validation #45996920864 passed on `0af5c5f8eb231254f0d82ea2b01c00f75d497a90`.
- Rust Core Tests #45996920804 was still running at last poll; Ubuntu, Windows and macOS jobs were in progress. Await all three before marking exact-head validation complete.
- The PR branch remains 4 commits behind its declared base and requires reconciliation.


## Review addendum — 2026-11-27: PR #106 diagnostic probe follow-up

- Reviewed the complete PR #106 patch and the current native source `scripts/macos-firmlink-record-probe.c` at branch head `cc7758783760ca5c940fe5d438cae445322a585d`.
- The prior hardening correctly added lower-bound checks for attrreference payloads, absolute/exactly terminated no-firmlink paths, and name/path non-overlap. The probe remains diagnostic and does not convert flags or paths into an authorization signal.
- Found two additional parser robustness issues:
  1. The name payload lower-bound calculation unconditionally reserved a flags word and no-firmlink-path reference even though the parser separately supports records where those attributes are not returned. This could reject a valid record with an omitted optional field.
  2. The name parser checked only the final NUL and did not reject an embedded NUL, unlike the no-firmlink path parser. A truncated C-string interpretation could mislabel a record.
- Corrected both in commit `b9361b2e1c4303e6b9f8bf9edfac98e4dca0776f`: compute trailing fixed-field space from the actual returned attribute bitmap, and require exact name termination with no embedded NUL.
- The exact-head workflow has not yet been returned for this new commit. PR #106 is still draft/open and reports mergeable=false pending branch state; do not treat prior run #45984257580 as validation of this new head.
- PR #107 now compares against the updated #106 base at 5 commits behind / 15 ahead (merge base remains `8c4a2d0bced52c76fa528a3aab2ecc13328a3cde`). Reconcile #107 after #106's exact-head CI settles.
- The firmlink flag discrepancy remains unresolved. Issue #74 and independent native security review remain mandatory gates; no production confinement claim is supported.


### PR #106 exact-head validation refresh — 2026-11-27

Rust Core Tests run [#45997726375](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45997726375) was created for head `ba088e03e610e5c835f6b131b90a22ac75334877`, which includes parser fix `b9361b2e1c4303e6b9f8bf9edfac98e4dca0776f`. The run was pending with no jobs published at the last poll. Earlier run #45984257580 applies only to the prior head. Keep the probe under review until this exact-head run completes on the supported OS matrix.


## PR #104 parent parser review — 2026-11-27

Reviewed the parent implementation at head `5b78582e99230302ee2936f87c60b5ff7a0d1988`, source `core/loco-core/src/macos_snapshot_adapter.rs`.

Findings:
- `MAX_ENTRY_NAME_BYTES` was declared as 766 (Darwin `NAME_MAX * 3 + 1`) but the call site still passed a 256-byte maximum, rejecting valid longer UTF-8 names.
- `attr_reference_range` did not enforce 4-byte alignment for the attrreference location and payload start.
- The name payload bound did not require the payload to begin after the flags/path-reference fixed fields and did not establish disjoint name/path payloads.
- Absolute-path validation did not reject `.` or `..` components.

The descendant PR #107 parser correction addresses these items with the 766-byte bound, alignment checks, fixed-field and payload non-overlap, absolute path/dot-component validation, exact NUL termination and malformed-record tests. A detailed PR #104 conversation review was posted (comment #7218166118). Parent PR #104 must remain stacked and unmerged until the corrected descendant implementation is reconciled and exact-head validated.

## PR #106 / #107 dependency refresh — 2026-11-27

- PR #106 updated head `ba088e03e610e5c835f6b131b90a22ac75334877` passed Rust Core Tests run [#45997726375](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45997726375) on Ubuntu, Windows and macOS.
- PR #107 was reconciled onto that exact #106 head in merge commit `e013d8e0fa5be7e14e0a4ed38a50bfe5cdeda5bb`, with the parser file as the only #107 change relative to #106. Compare is 16 ahead / 0 behind.
- Reconciled-head Contract Validation [#45998186236](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45998186236) was in progress; Rust Core Tests [#45998186291](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45998186291) was queued at the last poll. Previous green PR #107 runs do not validate the reconciled SHA.
- No PR was merged. Issue #74 and independent native security review remain open.


### PR #107 final-head validation — 2026-11-27

PR #107 is reconciled onto the latest PR #106 branch head `c3f35dae2c628ff795869f7c62f26fb38913de2f`. Current PR #107 head: `2d2a153c8775f075f69bd79a162be550bea11472`; compare is 19 ahead / 0 behind and only the parser source file differs. Exact-head Rust Core Tests [#45998450842](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45998450842) was pending and Contract Validation [#45998450978](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45998450978) was in progress at the last poll. Runs against superseded merge commit `e013d8e...` are not current-head evidence. Keep the PR draft until both current-head runs pass and independent review is obtained.

## Review addendum — nested-firmlink branch — 2026-11-27

Reviewed branch `test/macos-nested-firmlink-preflight` (no associated PR found) against its feature base and the current parser-hardening branch. It is 25 commits ahead / 0 behind `feat/macos-bulk-firmlink-preflight`, but diverges from PR #107 by 9 ahead / 15 behind. Its unique diff spans the macOS snapshot adapter, `scripts/macos-firmlink-record-probe.c`, and `docs/security/MACOS_SNAPSHOT_ADAPTER.md`.

Findings:
- The Rust bulk-record parser in this branch still calls `attr_reference_range(..., 2, 256)`, rejecting valid longer UTF-8 names despite XNU's documented `NAME_MAX * 3 + 1` payload bound.
- Its `attr_reference_range` helper does not enforce 4-byte alignment for reference and payload offsets. The nested C probe similarly omits explicit alignment checks and caps the name to `NAME_MAX + 1` bytes rather than the UTF-8 payload maximum.
- The top-level C `dump_record` parser checks only the final name NUL, not embedded NUL bytes, so diagnostic output can truncate/mislabel malformed names.
- The adapter document describes a `getattrlistat` preflight, while this branch's adapter uses `getattrlistbulk`. It also claims the probe asserts all six top-level mapped/ordinary controls; the top-level record dump prints those observations, while the explicit expected-path assertion is in the separate nested-mapping probe.
- The branch does contain nested `/System/Library` mapping checks, but these are not sufficient to establish the unresolved general firmlink security contract or replace independent review.

Do not merge this stale, unsubmitted branch as-is. Any useful nested-firmlink tests/evidence should be transferred selectively onto the current #106/#107 stack, with parser fixes retained and documentation claims matched to actual assertions. The branch's raw flags/path output remains diagnostic only.

## PR #107 current exact-head validation — 2026-11-27

Current head `2d2a153c8775f075f69bd79a162be550bea11472` passed Rust Core Tests [#45998450842](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45998450842) on Ubuntu, Windows and macOS and Contract Validation [#45998450978](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45998450978). Compare to the current #106 base is 19 ahead / 0 behind, with the parser source as the only changed file. The branch is CI-green but remains draft pending independent review and issue #74 resolution.


## PR #110 review — openat resolve-beneath diagnostic and descriptor identity — 2026-11-27

Reviewed PR #110's adapter regression, native `O_RESOLVE_BENEATH` probe, workflow integration, XNU audit, and support-matrix claims.

The opened-handle regression deliberately bypasses bulk preflight, opens OS-provided root and nested System/Data firmlinks relative to retained parent descriptors, and asserts that parent handles identify the System volume while opened target handles identify the Data volume via `f_fsid`. It also checks the runner's same-`st_dev` case. This supports the narrow tested System/Data boundary only; it does not establish other OS releases, architectures, or filesystems.

Probe review findings:
- Ordinary controls `/System` and `/usr` previously used the same permissive result handling as mapped entries, so they could be reported blocked while the probe still exited successfully.
- Any failed flagged open was treated as a beneath-policy block, instead of requiring `ENOTCAPABLE`; other errors could be misattributed.
- `F_GETPATH_NOFIRMLINK` failures were printed but did not fail the probe, despite path evidence being part of the experiment's stated evidence.
- Some C output strings used escaped backslash-n rather than newline characters, reducing log readability and machine parsing.

Corrected in commit `33549ebfddf06c4025f3b2a900eab161c7375c8c`:
- ordinary controls must open successfully under `O_RESOLVE_BENEATH`;
- mapped entries are classified as blocked only on `ENOTCAPABLE`; other errors are inconclusive/fail;
- baseline and successful flagged opens must return `F_GETPATH_NOFIRMLINK` path evidence;
- output records use actual newlines.

The fix changes the diagnostic probe only, not production authorization. Exact-head Rust Core Tests [#45999159794](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45999159794) and Contract Validation [#45999159812](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45999159812) were in progress at the last poll. The earlier successful runs #45982960193/#45982960286 were for superseded head `a50177e...`; do not use them as validation for the new commit. Keep PR #110 draft until exact-head CI and independent review complete.


### PR #110 current-head validation status — 2026-11-27

Current PR #110 head is `5f6aed9fea3ac37c88ca281fa16ea6850805db7d`, 16 ahead / 0 behind PR #109 base. Exact-head Rust Core Tests [#45999357228](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45999357228) and Contract Validation [#45999357627](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45999357627) are pending. The Rust run on intermediate code head `33549eb...` was cancelled after subsequent commits; its Contract Validation success is not current-head evidence. Keep PR #110 draft until current-head workflows pass and the macOS probe logs are inspected.


### PR #110 exact-head result — 2026-11-27

Current head `5f6aed9fea3ac37c88ca281fa16ea6850805db7d` passed Rust Core Tests [#45999357228](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45999357228) on Ubuntu, Windows and macOS and Contract Validation [#45999357627](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45999357627). Inspected macOS job #140506854666: the lexical parent-escape control was blocked with errno 107; ordinary `/System` and `/usr` opened; all four root and seven nested mappings opened with both baseline and `O_RESOLVE_BENEATH`; mapped no-firmlink paths pointed into `/System/Volumes/Data`. The probe hardening therefore passed on the exact head and confirmed that `O_RESOLVE_BENEATH` does not reject these firmlinks. PR #110 remains draft pending independent review and support-matrix decision.


## PR #112 / cross-platform native filesystem matrix — scoped source review — 2026-11-27

Reviewed PR #112 current head `da6b484b908fc56364e43bbfa01aad0b3de85739`, including Linux and Windows adapter implementations, explicit runner matrix, Windows SDK ABI probe/audit, macOS native probes, and support matrices.

**Linux adapter review:** workspace-root acquisition uses `openat2` with `NO_SYMLINKS|NO_MAGICLINKS`; retained-handle metadata is compared to canonical-path metadata by device/inode. Descendant capture uses `BENEATH|NO_SYMLINKS|NO_MAGICLINKS|NO_XDEV`, then checks regular-file type, single hard-link provenance, bounded complete read, and before/after handle metadata. This is a conservative handle-anchored design for the tested Linux scope; the metadata comparison is correctly documented as best-effort mutation detection, not an atomic snapshot.

**Windows adapter review:** admission requires a canonical volume-GUID root, fixed drive and NTFS; descendants are opened as individual components relative to retained handles with `NtCreateFile`, `OBJ_DONT_REPARSE`, and restrictive share modes. Returned handles are checked for reparse attributes, directory/file type and volume identity; file IDs are obtained via `FILE_ID_INFO` with no narrower fallback, and the all-zero unsupported sentinel is rejected. The Windows tests cover final/intermediate junctions, handle pinning, file identity stability, sharing conflicts and a real secondary NTFS mount. The independent SDK-header C probe verifies x64 structure layouts, `NtCreateFile` signature and `OBJ_DONT_REPARSE` valid-mask membership.

**macOS matrix review:** the branch adds explicit macOS 15 arm64/Intel jobs and records exact builds 15.8.2 (24H232) and 15.7.9 (24G830), in addition to macOS 26. Support docs appropriately limit claims to those hosted runner builds and retain unsupported/unverified OS, filesystem and configuration rows.

**Disposition:** no new source defect was confirmed in this scoped review. The Windows user-mode `NtCreateFile` + `OBJ_DONT_REPARSE` contract remains unresolved because the function-specific Microsoft documentation is narrower than the general `OBJECT_ATTRIBUTES` flag documentation. SDK header acceptance and empirical Server 2022/2025 NTFS tests are useful but do not settle version-stable documented semantics. Do not remove the flag or claim universal Windows support; retain fail-closed behavior and the explicit blocker.

Exact current-head validation: Rust Core Tests [#45949106596](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45949106596) and Contract Validation [#45949106692](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45949106692) both passed. This is a scoped current-diff review, not a complete review of all inherited commits/branch history. PR #112 remains draft pending independent native security review and contract resolution.


## PR #111 / macOS 26 Intel architecture matrix — scoped review — 2026-11-27

Reviewed PR #111 head `1e143c564937b59c7b5cac301de1f840c43a911b` against base `c3ddc2d10c50e10520ca8d454fd8a40aa42f88cf`. The unique diff is two commits and two paths: the Rust Core Tests workflow matrix and macOS support matrix documentation. No adapter implementation or production authorization code changes are included.

The workflow replaces the moving `macos-latest` label with explicit `macos-26` and `macos-26-intel` legs, and captures both `sw_vers` and `uname -m` before building/running the native probes. Exact-head Rust Core Tests [#45351453924](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45351453924) passed all four jobs. Inspected macOS job logs confirm:
- arm64: macOS 26.7.2, build 25G332, architecture `arm64`; 130 passed, 0 failed, 1 ignored.
- Intel: macOS 26.6.1, build 25G76, architecture `x86_64`; 130 passed, 0 failed, 1 ignored.
- Both ran the native firmlink metadata and `O_RESOLVE_BENEATH` probes, logged root/nested mapping observations, and passed `opened_system_data_firmlinks_are_rejected_by_handle_volume_identity`.

**Review finding:** the support matrix had stale statements claiming Intel was still unvalidated despite the exact-head Intel job passing. Corrected the feature-branch support matrix to record the exact macOS build, architecture, test run, and bounded scope. The update does not claim support for other macOS releases, APFS layouts, or filesystems.

**Disposition:** no code defect found in this two-file CI/documentation diff. PR #111 remains draft/open pending independent security review and the wider firmlink acceptance gate. This is a scoped review of the unique diff, not a full branch-history review.


## Stale ancestor branch review — feat/macos-bulk-firmlink-preflight-rebase — 2026-11-27

Compared stale branch head `bf2d918a761f8b2ef5f93316674f293a63046e80` with active feature branch head `5b78582e99230302ee2936f87c60b5ff7a0d1988`: 0 ahead / 10 behind, no unique diff; the stale head is the merge base. No PR is associated with the stale rebase branch.

Inspected the historical commit `fix(macos): use libc-supported bulk attributes`. It removes `ATTR_CMN_ERROR` from the requested common attributes and removes parsing of the per-entry error value. That historical branch would no longer explicitly reject a target record with a nonzero per-entry attribute error if the other requested fields were present.

The active descendant has superseded that change: it defines the Darwin `ATTR_CMN_ERROR` bit, requests it, conditionally parses the returned field, and uses bounded `attr_reference_range` checks for the variable-length payloads. The stale branch therefore has no unique active implementation to fix. Do not revive the historical reduced-error path.

**Disposition:** ancestry-only/stale branch review recorded; no code changed. The active `feat/macos-bulk-firmlink-preflight` branch and complete inherited history still require review. This does not count as a full branch review.


## PR #104 active bulk-preflight parser review refresh — 2026-11-27

Refreshed PR #104 at head `5b78582e99230302ee2936f87c60b5ff7a0d1988` against its declared base `15e9a8ce6d7d1a974fc04e369e8fffb899119916`: 13 commits ahead / 0 behind, two changed paths. PR remains draft/open.

Exact-head Rust Core Tests [#45344512869](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45344512869) and Contract Validation [#45344512880](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45344512880) passed. The macOS job log #138275985709 reports 121 passed, 0 failed, 1 ignored; the nested APFS mount-boundary fixture passed, while the same-volume nested-firmlink fixture remains unavailable/ignored.

The build log independently confirms `MAX_ENTRY_NAME_BYTES` is unused and the production bulk parser still calls `attr_reference_range(..., 1, 256)`. The declared 766-byte UTF-8 name limit is therefore not active. The same build reports the old `FSOPT_RESOLVE_BENEATH` constant unused after migration to `getattrlistbulk`.

The inline `bulk_entry_path_no_firmlink` parser also does not enforce 4-byte alignment for attrreference locations/payload starts and does not prove name/path payload disjointness from later fixed fields. It assumes the serialized order of optional `ATTR_CMN_ERROR`, name, flags and extended path fields; there are no isolated synthetic bulk-record fixtures covering truncated headers/records, missing returned bits, optional error-field placement/nonzero errors, overlapping payloads, or maximum-length names. The helper tests cover range arithmetic but do not establish the full production record parser contract.

PR #107 contains a separate hardened parser implementation and fixtures for the 766-byte bound, alignment, fixed-field/payload separation, dot-component rejection, exact NUL termination and malformed records. Those improvements are not yet wired into PR #104's live inline bulk parser. Integrate/reuse the tested parser in the production path and add focused synthetic bulk-record coverage before treating these gaps as resolved.

The production parser advances over `ATTR_CMN_FLAGS` without decoding or recording the value. Keep the raw flag discrepancy (`SF_FIRMLINK` versus hosted-runner `SF_NOUNLINK`) unresolved and diagnostic only. Do not infer firmlink authorization from these flags, `st_dev`, path strings, or `O_RESOLVE_BENEATH`; retain descriptor-relative opens, post-open descriptor checks and opened-handle filesystem identity as the tested System/Data boundary.

**Disposition:** scoped review of PR #104's complete unique diff refreshed; no code change made in this review task. CI green does not clear parser correctness gaps. Keep PR #104 draft/open and unmerged pending parser integration, same-volume nested-firmlink evidence, issue #74 authoritative semantics reconciliation, and independent native security review. This is not a full review of all inherited branch history and does not change **All branches reviewed: NO**.


## PR #104 parser integration and nested-firmlink branch review — 2026-11-27

### Parser integration on PR #104
Commit `a52d5b64929c10c3596b45db38ca902ac8f88ecc` ports PR #107's pure `parse_bulk_entry_record` helper and synthetic fixtures into the active `feat/macos-bulk-firmlink-preflight` production path. The bulk enumerator now delegates each length-delimited record to the helper rather than parsing inline. The helper applies exact record-length validation, a 64 KiB cap, required attribute checks, optional `ATTR_CMN_ERROR` handling, the 766-byte UTF-8 name bound, 4-byte reference/payload alignment, fixed-field and name/path overlap checks, exact NUL termination, absolute-path and dot-component checks. Raw flags remain diagnostic only.

The branch file was fetched after the commit and verified to contain the helper, active 766-byte bound, alignment checks and synthetic malformed-record fixtures. Exact-head Contract Validation [#46001917511](https://github.com/Veeresh-11/Loco-Agent/actions/runs/46001917511) and Rust Core Tests [#46001917390](https://github.com/Veeresh-11/Loco-Agent/actions/runs/46001917390) passed. Rust Core Tests passed on Ubuntu, Windows and macOS. Inspected macOS job #140515454612: 129 passed, 0 failed, 1 ignored; all new synthetic bulk-record parser tests passed. The ignored test is the unavailable nested APFS mount fixture; the workflow's real nested APFS mount-boundary step passed. This validates the parser integration commit, but does not resolve issue #74 or replace independent security review. Keep PR #104 draft/open.

### `test/macos-nested-firmlink-preflight` — scoped review
Compared against updated PR #104: 25 ahead / 1 behind; merge base remains old PR #104 head `5b78582e99230302ee2936f87c60b5ff7a0d1988`. The branch has not inherited parser integration commit `a52d5b6...` and has no associated PR.

Findings:
- Its Rust bulk parser still calls `attr_reference_range(..., 2, 256)` and its helper has no 4-byte alignment enforcement. Do not transfer that parser as-is.
- The nested C probe caps names at `NAME_MAX + 1`, rather than the UTF-8 attribute payload maximum, and does not enforce 4-byte alignment of attrreference locations or payload starts. It does include record-bound checks, exact final-NUL/embedded-NUL checks, and name/path separation from later fixed fields.
- The native probe enumerates OS-provided mappings under `/System/Library` and checks expected no-firmlink paths. This is useful bounded diagnostic evidence for those enumerated mappings, not a general authorization proof. Raw flags remain diagnostic only.
- The branch's adapter documentation says a same-volume nested-firmlink fixture is not the right criterion because Apple firmlinks are volume-group mappings. PR #104's description and current tracker acceptance notes still request a same-volume nested-firmlink fixture. This is an acceptance-contract inconsistency that must be resolved against authoritative Apple/XNU semantics and the threat model; do not silently remove the fixture requirement or claim these nested mappings prove same-volume behavior.

**Disposition:** scoped source/documentation review only; no branch changes made. Do not merge or transfer the nested branch as-is. First obtain exact-head CI for PR #104 commit `a52d5b6...`; then resolve the fixture-contract discrepancy and selectively port any valid nested mapping assertions with hardened Rust and C parsing. This is not a full inherited-history review and does not change **All branches reviewed: NO**.


## Firmlink fixture acceptance-contract reconciliation — 2026-11-27

Reviewed the nested-firmlink branch's Rust adapter, native C probe and adapter documentation, then checked the disputed fixture requirement against Apple's primary materials.

### Authoritative semantics and decision

- Apple's WWDC19 *What's New in Apple File Systems* session describes firmlinks as traversal points from a directory on the System volume to a directory on the Data volume, with one-to-one source/target mappings, and says a firmlink cannot cross a volume-group boundary. It also describes the installer as creating the mappings ([session, 7:29–8:22](https://developer.apple.com/videos/play/wwdc2019/710/)).
- Apple's Developer Documentation describes firmlinks as transparent links between the read-only System volume and writable Data volume, using '/Applications' as an example, and identifies the corresponding C flag as 'SF_FIRMLINK' ([FileFlags.firmlink](https://developer.apple.com/documentation/system/fileflags/firmlink)).
- Apple's Filesystems lab forum response says end users cannot create firmlinks and that creation is reserved to the system ([Apple Developer Forums thread 117521](https://developer.apple.com/forums/thread/117521)). This is supporting context, not a formal API contract.
- XNU's 'getattrlistbulk(2)' manual says a firmlink directory entry has 'SF_FIRMLINK' in 'ATTR_CMN_FLAGS', and that bulk-returned attributes describe the firmlink rather than its target ([XNU manual](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/man/man2/getattrlistbulk.2)). XNU 'stat.h' defines 'SF_NOUNLINK' ('0x00100000') and 'SF_FIRMLINK' ('0x00800000') as distinct bits ([XNU stat.h](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/stat.h)).

**Decision:** a same-volume nested-firmlink fixture is not a valid acceptance requirement for Apple's documented System-to-Data firmlink model. The public material establishes the intended volume-group topology, but does not prove that no internal or unsupported same-volume implementation can ever exist. Therefore, do not claim universal impossibility. Replace the disputed criterion with deterministic tests over actual OS-provided nested System/Data firmlink mappings (including deeper descendants), requiring denial before opening/acquiring target content. Keep a separate nested-mount/volume-boundary test for mount semantics; do not label it a same-volume firmlink test. The requirement is changed because the documented platform model does not provide a supported user-created same-volume fixture, not because the security property is waived.

### Nested branch source review refresh

Current branch 'test/macos-nested-firmlink-preflight' is 25 commits ahead / 1 behind 'feat/macos-bulk-firmlink-preflight'; merge base remains '5b78582e99230302ee2936f87c60b5ff7a0d1988'. Its diff against the current PR #104 head includes workflow, adapter, evidence docs and C probe changes. The branch's Rust parser still calls 'attr_reference_range(..., 2, 256)' and does not enforce 4-byte alignment. The C probe caps names at 'NAME_MAX * 3 + 1' but does not explicitly validate attrreference/payload alignment; its 'dump_record' checks final NUL but does not reject embedded NUL in names. The branch's '/System/Library' nested mapping assertions are useful test candidates, but must be transferred only after rebasing onto the hardened PR #104 parser integration and reviewing the full diff. Its adapter documentation's stale 'getattrlistat' description must also be reconciled with the implementation's 'getattrlistbulk' path before transfer.

**Disposition:** acceptance-contract discrepancy resolved at the documentation/research level; same-volume nested-firmlink fixture requirement is replaced as described above. No runtime code or test was changed in this pass. Do not merge the nested branch as-is. Next code task is to selectively port the OS-provided nested System/Data mapping assertions onto the current #104 head, preserving the hardened 766-byte/alignment/overlap parser, then run exact-head macOS CI and inspect its native logs. Issue #74 remains open because the hosted 'SF_NOUNLINK'/documented 'SF_FIRMLINK' discrepancy and a supported native signal/independent review are still unresolved. **All branches reviewed: NO.**


## Nested System/Data mapping regression PR #173 and native C probe review — 2026-11-27

### Selective test transfer

Created draft PR #173, stacked directly on PR #104's parser-integrated head `a52d5b64929c10c3596b45db38ca902ac8f88ecc`. Its only changed file is `core/loco-core/src/macos_snapshot_adapter.rs` (+36 lines). The new macOS-only test reads `/usr/share/firmlinks`, selects mappings below `/System/Library/`, opens only each mapping's parent directory, queries the no-firmlink path through the production hardened `bulk_entry_path_no_firmlink` parser, checks the exact `/System/Volumes/Data/<target>` mapping, and requires `preflight_entry_path` to return `FirmlinkBoundary`. It does not open the mapped target or read its content. It fails if the OS map has no matching nested entries, rather than silently passing without coverage.

The test reuses PR #104's parser and does not import the nested branch's 256-byte/no-alignment parser or its C parser. PR #173 is open/draft at head `33e60738d7f2745fd57b0777f27019e5c34a487f`, base `feat/macos-bulk-firmlink-preflight`; its diff is exactly one file, 36 additions, 0 deletions. Exact-head Contract Validation #46003028674 and Rust Core Tests #46003028684 have started and were still in progress at this review update. No CI result is claimed yet; macOS job completion is required to exercise the host firmlink map.

### Next component review: active PR #104 native C diagnostic probe

Reviewed `scripts/macos-firmlink-record-probe.c` on PR #104's base branch. The probe is diagnostic-only and does not authorize access, but its `dump_record` parser has robustness gaps that should be addressed before it is extended to parse nested mapping records:

- The name attrreference path checks nonnegative offset, record bounds and a maximum buffer size, but does not enforce 4-byte alignment for the attrreference location or payload start.
- It checks the final NUL byte but does not reject an embedded NUL before it; a crafted/invalid record could be truncated for `strcmp` and selected as a different entry.
- The name buffer expression `NAME_MAX * 3 + 1` evaluates to 766 bytes when Darwin `NAME_MAX` is 255, matching the production parser's current serialized UTF-8 payload bound. This is not a demonstrated undersized-buffer defect; make the assumption explicit or use a named constant so the relationship remains auditable.
- It does not assert that the complete requested attribute set was returned before interpreting the name/flags fields. It should fail closed or report a malformed/incomplete record rather than treating partial data as an ordinary diagnostic result.

These findings are scoped to probe input validation. They do not invalidate the production Rust parser integration and do not provide evidence about the `SF_FIRMLINK`/`SF_NOUNLINK` discrepancy. No C source change was made in this pass. Next review/remediation task: harden this C diagnostic parser with the same record bounds, 766-byte name cap, 4-byte alignment, exact NUL and returned-attribute checks, then add malformed-record tests or a standalone parser fixture where practical. Keep the C probe diagnostic-only.

**Disposition:** PR #173 implementation submitted; exact-head CI pending. Native C probe review completed with scoped hardening findings. Issue #74 remains unresolved; no production authorization behavior changed. **All branches reviewed: NO.**


### PR #173 compile failure and correction — 2026-11-27

The first exact-head Rust Core Tests run #46003028684 failed on macOS because the test passed `&OsStr` to `preflight_entry_path`, whose API accepts `&str` (E0308). Windows and Ubuntu jobs passed; Contract Validation #46003028674 passed. Corrected the test to convert the map filename to UTF-8 with an explicit checked `to_str()` before calling preflight, commit `262194542fba081285a1b2ffbdde209c14d2fd65`. This is a test compile fix; it does not change production code. Fresh exact-head Contract Validation #46003225480 and Rust Core Tests #46003225466 have started on the corrected head and were still in progress at this update. PR #173 remains draft/open. Do not count the first run as green; await macOS execution of the nested map assertions.

### Review precision correction — native C probe name buffer

Corrected the previous note about the C probe's `NAME_MAX * 3 + 1` name buffer: on Darwin with `NAME_MAX == 255`, this expression is 766 bytes, matching the production parser's explicit limit. It is therefore not a demonstrated undersized-buffer issue. The review now records only that the expression should be made explicit/auditable, alongside the substantive missing alignment, embedded-NUL and returned-attribute validation checks.
