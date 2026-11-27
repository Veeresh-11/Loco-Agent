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
