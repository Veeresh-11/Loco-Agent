# LOCO Agent — consolidated project roadmap

**Status date:** 2026-11-24  
**Source of truth:** The user-prioritized sequence confirmed in the project conversation, reconciled against the repository's open issues, PR descriptions, exact-head CI evidence, and the implementation limitations recorded in the security documentation.

This roadmap preserves the requested order. Work on a later priority may exist as a design, contract, or isolated prototype, but that does not make the priority complete. A task is complete only when its acceptance criteria, integration, tests, security review, and release gates are satisfied.

## Ordered priorities

| Priority | Task | Status | Exit condition |
|---:|---|---|---|
| 1 | Find and validate reliable macOS firmlink detection | **In progress — bulk pre-open path signal integrated; independent review pending** | Supported, handle-relative firmlink signal; deterministic native denial test; exact OS/API evidence; concurrency-safe per-entry queries; independent review |
| 2 | Complete native filesystem API review | **Partially complete** | Finish Linux/macOS/Windows API and filesystem/version matrix, race semantics, unsupported cases, and documented fail-closed decisions |
| 3 | Integrate approval with snapshot identity and freshness | **Pending integration** | Approval binds exact action + snapshot digest/identity/freshness; replay-safe authenticated approval; Rust independently verifies before use |
| 4 | Implement controlled Rust workspace writes | **Pending** | Handle-relative, race-resistant write/replace operations with bounded patch validation, authorization, authenticated approval, audit, rollback/recovery and adversarial tests |
| 5 | Complete Rust authorization and command execution controls | **Pending** | Rust is the enforcing authority for live operations; explicit command policy, executable identity/argument/environment/cwd bounds, cancellation/timeouts/output limits and no TS bypass |
| 6 | Extract Python AI service | **Pending** | Defined service boundary/protocol, isolated Python service, bounded IPC/lifecycle, tests and packaging; no implicit trust of model output |
| 7 | Build TypeScript-to-Python/Rust bridges | **Partially complete — Rust transport plus macOS runtime capability handshake** | Production-quality TS↔Rust and TS↔Python clients, versioned contracts, lifecycle/error/cancellation tests, packaged binaries/services |
| 8 | End-to-end security and integration tests | **Partially complete — component tests exist** | Full VS Code-to-TS-to-Rust/Python workflows, adversarial cross-platform tests, approval/write/command integration, regression and security review |
| 9 | Packaging and release | **Pending** | Reproducible per-platform builds, signed/verified artifacts, install/upgrade/uninstall tests, SBOM/provenance, release notes and marketplace/release gates |

## Priority 1 — macOS firmlink detection

**Tracking issue:** [#74](https://github.com/Veeresh-11/Loco-Agent/issues/74)

Relevant PRs: #73, #75, #76, #77, #78, #100, #102, #103, #104. PR #102's exact-head macOS run #45335594962 confirmed distinct descriptor paths for `/Users`, `/Applications`, and `/Library`: `F_GETPATH` returned the visible firmlink path while `F_GETPATH_NOFIRMLINK` returned `/System/Volumes/Data/...`. The probe now adds a temporary-directory rename/replacement assertion to test whether both path forms follow the retained descriptor rather than a replacement at the old name; the assertion-bearing head passed all three platforms in run #45336024346. Its macOS log confirmed the retained descriptor reports the renamed `/after` path while a new descriptor at the old `/before` path resolves to the distinct replacement object. The concurrent rename stress also passed in run #45336267947: 2,000 rename cycles and 4,000 paired path observations produced zero API errors, zero unexpected paths, zero writer errors, and stable descriptor identity. PR #103's initial implementation integrated `ATTR_CMNEXT_NOFIRMLINKPATH` as a parent-handle-relative pre-open check using `getattrlistat`. Before opening each descendant component, Rust queries `getattrlistat` with `FSOPT_NOFOLLOW_ANY | FSOPT_RESOLVE_BENEATH | FSOPT_ATTR_CMN_EXTENDED`, compares the returned firmlink-free path to the exact expected direct-child path, and returns `FirmlinkBoundary` before `openat` on mismatch. After opening, it cross-checks the descriptor with `F_GETPATH_NOFIRMLINK` and retains `f_fsid`, `O_NOFOLLOW`, and metadata revalidation. The native C probe verified four mappings — `/Users`, `/Applications`, `/Library`, and `/private` — resolve to `/System/Volumes/Data/...`, while ordinary `/System` and `/usr` remain unchanged (run #45341558203). `/private` is a firmlink directory, not a POSIX symlink; the nofollow extended attribute still identifies its firmlink-free path. The XNU source now reviewed shows `ATTR_CMNEXT_NOFIRMLINKPATH` routes through `attrlist_build_path` and `build_path(..., BUILDPATH_NO_FIRMLINK)`, which explicitly avoids firmlink redirection while reconstructing the vnode path. This is a pre-open metadata/path query before the userland target descriptor is opened or content is read; the kernel still resolves the directory entry and builds its vnode path, so we do not claim zero kernel traversal. PR #103 exact head `15e9a8ce6d7d1a974fc04e369e8fffb899119916` passed Rust, TypeScript Node 20/22, and contract validation (runs #45342908867, #45342908783, #45342908797); macOS reported 118 passed, 0 failed, 1 ignored. Tests cover pre-open denial of `/Users`, `/Applications`, `/Library`, and `/private`; ordinary-directory acceptance; a workspace rooted under `/Users`; malformed/truncated attribute records; and concurrent preflight queries without shared directory offsets. Follow-on PR #104 replaces the `getattrlistat` preflight with a bounded `getattrlistbulk` directory-entry scan requesting the same extended attribute. Each scan opens a fresh directory file description, parses bounded variable-length records, and compares the returned entry path with the retained parent path plus the child name before `openat`. The native C probe distinguishes firmlink entries from ordinary directories and symbolic-link aliases: four mapped directories return Data-volume no-firmlink paths, ordinary `/System` and `/usr` remain unchanged, and a real symlink retains its alias path. PR #104 exact head `5b78582e99230302ee2936f87c60b5ff7a0d1988` passed Rust Core Tests on Ubuntu, Windows, and macOS and contract validation (runs #45344512869 and #45344512880); macOS reported 121 passed, 0 failed, 1 ignored. The bulk-record parser now validates attribute-reference ranges, bounds names and paths, and fails closed on malformed records. New tests cover symlink alias preservation, `O_NOFOLLOW` rejection, concurrent independent directory offsets, and early `FirmlinkBoundary` denial during a directory-swap race. The bulk path remains a deny-only native observation, not a standalone authorization grant; nested System/Data firmlink coverage and independent review remain open. The sidecar advertises the extended-attribute and fcntl capabilities independently; under the restrictive Seatbelt profile, the extended attribute remains available while command 102 is denied, and the adapter fails closed without both capabilities. Earlier common metadata probes did not expose `SF_FIRMLINK` (run #45339440539), and deprecated `getdirentriesattr` returned `ENOTSUP` on APFS (run #45339809143). Remaining gates are independent security review, supported macOS/filesystem/version matrix, sandboxed host variants, long-path coverage, and broader nested System/Data firmlink coverage across supported macOS releases. XNU source confirms command 102 selects `VN_GETPATH_NO_FIRMLINK`, maps to `BUILDPATH_NO_FIRMLINK`, and uses name-cache-only path reconstruction; a public WebKit sandbox profile allows `F_GETPATH` but does not list `F_GETPATH_NOFIRMLINK`, so a custom Seatbelt test now proves command 102 can be denied while command 50 remains allowed (run #45337196884). The custom Seatbelt denial test passed and is recorded in run #45337375932. The initial host scope is the official Microsoft VS Code desktop distribution; a current upstream VS Code issue states its macOS app is not App Sandbox-enabled. Forks, remote hosts, App Store/enterprise-wrapped distributions remain unverified. The sidecar now probes command 102 at runtime and omits the optional capability if the actual host blocks it; the adapter repeats the query on each descriptor and fails closed.

### Established evidence
- The macOS 26.7.2 hosted runner (build 25G332, arm64) lists mappings such as `/Users`, `/Applications`, and `/Library` in the system firmlink map.
- The original native `getattrlistbulk` probe returned `0x00100000` (`SF_NOUNLINK`), not `SF_FIRMLINK = 0x00800000`.
- Follow-up probes of `getattrlistat(FSOPT_NOFOLLOW_ANY | FSOPT_RESOLVE_BENEATH)` and `fgetattrlist` returned the same `SF_NOUNLINK` value.
- PR #77 added a descriptor-derived `f_fsid` comparison. Its exact-head run #45322641807 passed Ubuntu, Windows, and macOS; the observed System/Data crossing is rejected even when `st_dev` is shared. This is a distinct-volume boundary, not a universal same-volume firmlink detector.
- PR #100's initial diagnostic run #45334961812 passed Ubuntu, Windows, and macOS; after the newline-format and evidence-documentation updates, the exact updated head `c212b45a15053482473fb8c2e34e6dc5ecb3b3b6` also passed all three platforms in run #45335436957. On the macOS runner, `getattrlistbulk`, `getattrlistat`, `fgetattrlist`, `fstatat(AT_SYMLINK_NOFOLLOW)`, followed `fstatat`, and opened-handle `fstat` all returned `0x00100000` for the three mapped entries. Raw bulk records showed the returned attribute mask and parsed flags field at the expected offset.
- Apple's public `getattrlistbulk(2)` manual documents `SF_FIRMLINK` in `ATTR_CMN_FLAGS` for a firmlink entry; Apple's `FileFlags.firmlink` documentation identifies the corresponding C constant. The hosted runner's behavior therefore remains unexplained. The `ATTR_CMNEXT_NOFIRMLINKPATH` bulk probe matches `/System/Volumes/Data/...` for four mapped entries (including `/private`) and unchanged paths for ordinary controls, but it is a path observation, not a documented firmlink-entry bit.

### Remaining work
1. Get authoritative Apple DTS / Developer Forums clarification on whether bulk `ATTR_CMNEXT_NOFIRMLINKPATH` describes the firmlink entry without resolving its target, and an explanation for the missing `SF_FIRMLINK` flag; otherwise obtain an Apple-provided reproducible OS firmlink fixture.
2. Do not treat `SF_NOUNLINK`, `sunlnk`, `/usr/share/firmlinks`, or pathname heuristics as an equivalent authorization signal.
3. Add a deterministic test proving the native signal describes the parent entry without traversing the target, and denies before content acquisition.
4. Prove directory enumeration is safe for concurrent captures; duplicated descriptors may share an open-file-description offset, so no shared scan state may leak between captures.
5. Preserve malformed/truncated-record fail-closed behavior and document exact supported OS/filesystem versions.
6. Keep issue #74 open and the dependent PR stack draft until the acceptance criteria are met.

**Current conclusion:** The tested `f_fsid` guard is useful and validated for cross-volume traversal. General firmlink identification remains unsupported/unverified. Do not broaden the security claim.

## Priority 2 — native filesystem API review

Relevant work: PRs #44–#72, especially #49–#61 and #62–#72.

- #61 records the native filesystem snapshot API review.
- #49 defines shared portable path validation; #50–#54 cover Linux/Windows/macOS root acquisition and snapshot prototypes.
- #55–#60 add race, mutation, symlink/reparse-point, and mounted-folder test coverage.
- #62 addresses short reads; #63 covers Linux descendant mount crossings.
- #64–#68 cover Windows FFI layout, file identity, sharing conflicts, NTFS root admission, and macOS APFS root admission.
- #69–#72 add real APFS, NTFS, Linux bind-mount, and distinct-filesystem mount fixtures.

### Implemented controls and hosted native evidence (2026-11-24)

- **macOS firmlink follow-up (PR #104):** bulk `getattrlistbulk` preflight is implemented on top of PR #103. Exact-head Rust matrix #45344512869 passed Ubuntu, Windows, and macOS; Contract Validation #45344512880 passed. macOS reported 121 passed, 0 failed, 1 ignored; it exercised the four mapped-entry checks, ordinary controls, symlink alias behavior, concurrent independent directory offsets, namespace-swap race, sandbox capability probe, nested APFS fixture, and malformed attrreference-range cases. The implementation remains draft: the runner's `ATTR_CMN_FLAGS` values still conflict with XNU's documented `SF_FIRMLINK` entry behavior; complete synthetic malformed bulk-record coverage and independent security review remain open. Issue #74 remains open; no production acceptance or merge claim is made.

- **Bulk parser hardening (PR #107, stacked on #106 and #104):** a pure record parser validates per-record length, required returned attributes, optional per-entry error, NUL termination, absolute path, checked offsets, and non-overlapping name/path payloads; raw flags are retained for diagnostic assertions. Exact-head Rust matrix #45346723420 passed Ubuntu, Windows, and macOS; Contract Validation #45346723355 passed. macOS reported 128 passed, 0 failed, 1 ignored, including optional-error-field, malformed-record, fixed-field-overlap, and 4-byte attrreference-alignment tests. The earlier PR #105 duplicate draft was closed as superseded by #107. These parser tests do not resolve issue #74's SF_FIRMLINK discrepancy; #104/#106/#107 remain draft pending authoritative reconciliation and independent security review.
- **Nested System/Data firmlink preflight (PR #108, stacked on #107):** replace the incorrect same-volume fixture requirement with Apple's documented System-to-Data volume-group model. The test reads the installed `/usr/share/firmlinks` map, selects direct-child nested mappings under `/System/Library`, and requires the Rust bulk preflight to return `FirmlinkBoundary` before `openat`; the native C probe independently asserts the `ATTR_CMNEXT_NOFIRMLINKPATH` target path for each mapping. Apple WWDC19 describes one-to-one System-to-Data firmlink traversal within a volume group and says mappings cannot cross volume-group boundaries; Apple Developer Forums states end users cannot create firmlinks. PR #108 is draft and exact-head CI is pending.


- Linux: PR #62 requires exact captured byte length; PR #63 applies `RESOLVE_NO_XDEV` to descendant `openat2` resolution while allowing the workspace root itself to reside on another mount. PR #71's same-device bind-mount rejection passed on Ubuntu (Rust #45187459253, contract #45187459247). PR #72's distinct ext4 loop-mount rejection passed (Rust #45188239997, contract #45188239539).
- Windows: PR #64 adds x64 ABI layout assertions and explicitly excludes x86/ARM64. PR #65's incompatible pre-existing writer-handle test passed in Windows run #45181874421. PR #66 uses `FileIdInfo` (64-bit volume serial plus 128-bit file ID) with no legacy fallback; its Windows test passed in #45182566472. PR #67 admits NTFS only; #45182767243 passed. PR #70's real secondary NTFS VHDX mountpoint test passed in #45186946850 and reran in #45188239997.
- macOS: PR #68 admits APFS only through `fstatfs`; PR #69's nested APFS sparse-image mount test passed in #45185300723 and reran in #45188239997.
- Windows symlink/reparse coverage from PR #58 was exercised in the PR #65 Windows run; the directory-symlink, final-file-symlink, junction and post-open checks passed.
- PR #72's latest code/workflow head `6acb108a3a6342a3e3c64b1e39a699ede48dbc1e` passed the full Rust matrix #45342704201 and contract validation #45342704159. The follow-up CI now records runner versions: Ubuntu 26.04.1/kernel 7.0.0-1020-azure with ext4 temp storage; macOS 26.7.2 build 25G332; Windows Server 2025 Datacenter build 26100 x64 with NTFS temp storage. The subsequent PR #72 commit `a816d32c01b9bc910d0345f7648b74e823299e69` only adds those observed version details to the review document.

### Narrow prototype support baseline

| Platform | Admitted baseline | Enforced descendant boundary | Explicitly unsupported or unverified |
|---|---|---|---|
| Linux | Linux 5.6+ with `openat2`; hosted Ubuntu tests use ext4 | `RESOLVE_NO_XDEV` rejects descendant mounts and bind mounts; no pathname fallback | Other kernel versions/filesystems/mount namespaces |
| macOS | APFS roots only | Component-wise `openat(O_NOFOLLOW)` and descendant device identity | HFS+, network/removable filesystems, other APFS arrangements; firmlink entry detection remains issue #74 |
| Windows | x64; Windows 8 / Server 2012 or later for `FileIdInfo`; canonical fixed NTFS volume-GUID roots | `NtCreateFile` relative to retained handles, `OBJ_DONT_REPARSE`, post-open reparse checks, strict sharing and handle-derived identity | x86/ARM64, ReFS, FAT/exFAT, network/removable filesystems |

Remaining: independent Windows SDK/FFI review; explicit OS/kernel build matrix beyond moving hosted-runner labels; decide whether additional Linux filesystems or mount namespaces are supported; preserve fail-closed behavior for all unsupported cases; and independent security review. The native CI tests validate only the named runner and fixture, not every filesystem or release. Priority 2 is materially advanced but remains partially complete until these gates are met.

## Priority 3 — approval bound to snapshot identity and freshness

Relevant work: PRs #15–#21, #25–#43, and #79–#90; issue #82.

- #15–#21 cover action digest binding, persistent single-use challenges, canonical digest and wire-contract alignment, and audit DTO/contract work.
- #25–#43 cover snapshot contracts, validation, Linux openat2 acquisition, metadata/content identity, hard-link provenance, special-file rejection, mutation/replacement stress, and related tests.
- #79–#90 harden approval bounds, atomic consumption, clock ownership, cryptographic nonces, restart invalidation, monotonic deadlines, platform capability research, in-memory/persistent retention, and SQLite resource bounds.
- #81 records the authenticated approval trust-boundary design.
- #82 tracks platform authenticator research. Windows WebAuthn, macOS LocalAuthentication/Keychain, and Linux FIDO2 options are research directions, not an implemented cross-platform authenticated approval system.

Remaining: integrate the independently validated snapshot identity/digest/freshness into the exact approval challenge; define the same-user malware and credential lifecycle model; prototype supported OS authenticators; have Rust verify the signed/attested evidence, nonce, relying-party/application identity, and exact action digest; fail closed when unavailable. Storage limits and replay protection alone do not authenticate a human approval.

## Priority 4 — controlled Rust workspace writes

Relevant work: PRs #22–#24 and the workspace/snapshot work above.

- #22 and #23 validate bounded patch proposals and unified diffs.
- #24 defines the controlled workspace write boundary.
- These are proposal validation/design components, not a production write capability.

Remaining: implement writes through retained handles and platform-specific race-resistant primitives; bind the write target to the approved snapshot and exact patch digest; prevent symlink, reparse-point, hard-link, mount and replacement races; bound bytes/files; define atomicity, rollback/recovery, audit, cancellation and crash behavior. Do not enable writes until priority 3 approval is integrated.

## Priority 5 — Rust authorization and command execution controls

Relevant work: PRs #9–#14 and #91–#99.

- #9 establishes the polyglot architecture proposal; #10–#14 establish the action protocol, Rust policy-core scaffold, path/read prototypes and cross-platform CI.
- #91 documents local storage and IPC security boundaries; #92 defines the sidecar stdio protocol; #93 implements a bounded Rust sidecar that evaluates proposals; #94 adds a TypeScript parent client.
- The sidecar currently evaluates proposals only. Its `allow` decision is not an execution capability and it does not yet authorize production filesystem writes or execute commands.
- #95–#99 inventory TypeScript privileged operations, define the bounded workspace-read request contract, design Rust-owned read acquisition, complete the TS operation inventory, and add a TypeScript direct-API regression gate.

Remaining: make Rust the actual enforcement point for the live operation path, integrate approval and snapshot checks, define command allow/deny policy and executable identity, argument/environment/cwd/output/time bounds, process-tree cancellation and audit, and remove or explicitly constrain TypeScript bypasses. No command execution should be enabled by a policy-only response.

## Priority 6 — Python AI service extraction

No completed extraction is evidenced by the reviewed open PR inventory. This remains pending.

Remaining: inventory Python AI functionality and dependencies; define a versioned, bounded service protocol; isolate model/provider execution; handle secrets, network access, resource limits, timeouts, cancellation, model-output validation, service restart and error propagation; add tests and platform packaging.

## Priority 7 — TypeScript-to-Python/Rust bridges

- PR #94 is a TypeScript-to-Rust stdio transport prototype with handshake, framing, request correlation, deadlines, cancellation and bounded stderr.
- PR #103 extends that handshake with validated optional macOS `F_GETPATH_NOFIRMLINK` and `ATTR_CMNEXT_NOFIRMLINKPATH` capabilities; the TypeScript client exposes them without treating them as authorization grants.
- It is not evidence of complete production integration into the live agent task path.
- No completed TypeScript-to-Python bridge is evidenced in the reviewed PR list.

Remaining: finish Rust bridge integration, design and implement Python service transport, share versioned schemas/conformance fixtures, test startup/restart/timeout/cancellation/crash/backpressure, and package the service/binary for each supported platform.

## Priority 8 — end-to-end security and integration tests

Existing component tests and cross-platform CI are valuable but do not satisfy full integration.

Remaining: test the actual VS Code extension workflow across TS, Rust and Python; exercise approved reads/writes and command decisions with exact snapshot identity; test malicious paths, mount/firmlink boundaries, concurrent replacement, stale approvals, replay, IPC spoofing, malformed/oversized frames, service crashes, cancellation, data disclosure, and upgrade behavior on each supported OS. Add independent security review and record residual risks.

## Priority 9 — packaging and release

Remaining: reproducible builds for supported OS/architectures; Rust and Python runtime packaging; code signing/notarization where required; binary integrity verification; SBOM and provenance; dependency/license review; clean install, upgrade, downgrade and uninstall tests; VS Code extension package validation; release notes, migration guidance, support matrix, and rollback plan.

## Earlier LOCO feature and quality backlog

These items predate the nine security/migration priorities and remain visible in GitHub:
- PR #6 is the open, non-draft consolidation PR for the earlier validated feature stack (quality gates, safe patch preview/approval/rollback, streaming/provider integration, retrieval evaluation). Its description says retrieval-evaluator CI was not confirmed and a manual VS Code Extension Development Host smoke test remained pending.
- PR #7 is the retrieval-evaluation deduplication fix; PR #8 is the workspace path-boundary regression test stack. Both remain open.
- PRs #1–#5 are closed but unmerged. Their work must not be represented as merged into `main` solely because the PRs are closed; PR #6 is the consolidated review path for the feature stack.
- Earlier work includes documentation alignment, TypeScript CI, patch preview/rollback, incremental streaming/provider integration, retrieval evaluation, path-boundary tests, and architecture/security foundations (#1–#14).

These legacy feature items should be reviewed as a separate release/consolidation gate. Do not mix their status with the nine ordered priorities.

## PR and merge policy

- All reviewed security/migration PRs remain open and draft until their stated dependencies, exact-head CI, acceptance criteria, and independent review are complete.
- PR #100's updated exact head `c212b45a15053482473fb8c2e34e6dc5ecb3b3b6` passed all three platforms in run #45335436957.
- PR #102's pinned sandbox test head passed all three platforms in run #45337375932.
- PR #102's no-follow parent-entry metadata probe passed all three platforms but found no distinct pre-open firmlink signal in run #45339440539.
- PR #102's extended bulk no-firmlink path probe, four mapped entries (including `/private`), no-follow symlink semantics, and ordinary-directory controls passed all three platforms in run #45341558203; the result is documented as a candidate observation, not an accepted authorization signal.
- PR #102's deprecated `getdirentriesattr` probe returned `ENOTSUP` on APFS in run #45339809143; the documented `SF_FIRMLINK` path is unavailable on the hosted volume.
- PR #103's exact head passed Ubuntu, Windows, macOS, TypeScript Node 20/22, and contract validation in runs #45338568485, #45338568571, and #45338568507. PR #102's first exact-head run #45335594962 passed all three platforms and its macOS log confirmed the firmlink-path distinction. The rename/replacement regression fixture passed on all three platforms in run #45336024346; macOS logs confirmed the original descriptor tracks the moved name and a new descriptor at the old name identifies the replacement. Concurrent rename stress and sandbox/API compatibility remain open.
- Do not close a PR because a later PR supersedes it unless the code, tests, and dependency relationship have been reviewed and the superseding PR is confirmed to contain the required changes.
- Do not merge a stacked PR out of dependency order. Merge only after the complete stack is validated and reviewed.
- Keep issue #74 and issue #82 open until their acceptance criteria are satisfied.
- Update this roadmap when evidence changes; a design document, prototype, or passing isolated CI job must not be marked as production-complete.
