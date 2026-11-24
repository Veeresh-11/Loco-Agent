# LOCO Agent — consolidated project roadmap

**Status date:** 2026-11-24  
**Source of truth:** The user-prioritized sequence confirmed in the project conversation, reconciled against the repository's open issues, PR descriptions, exact-head CI evidence, and the implementation limitations recorded in the security documentation.

This roadmap preserves the requested order. Work on a later priority may exist as a design, contract, or isolated prototype, but that does not make the priority complete. A task is complete only when its acceptance criteria, integration, tests, security review, and release gates are satisfied.

## Ordered priorities

| Priority | Task | Status | Exit condition |
|---:|---|---|---|
| 1 | Find and validate reliable macOS firmlink detection | **In progress — unresolved platform signal** | Supported, handle-relative firmlink signal; deterministic native denial test; exact OS/API evidence; concurrency-safe enumeration; independent review |
| 2 | Complete native filesystem API review | **Partially complete** | Finish Linux/macOS/Windows API and filesystem/version matrix, race semantics, unsupported cases, and documented fail-closed decisions |
| 3 | Integrate approval with snapshot identity and freshness | **Pending integration** | Approval binds exact action + snapshot digest/identity/freshness; replay-safe authenticated approval; Rust independently verifies before use |
| 4 | Implement controlled Rust workspace writes | **Pending** | Handle-relative, race-resistant write/replace operations with bounded patch validation, authorization, authenticated approval, audit, rollback/recovery and adversarial tests |
| 5 | Complete Rust authorization and command execution controls | **Pending** | Rust is the enforcing authority for live operations; explicit command policy, executable identity/argument/environment/cwd bounds, cancellation/timeouts/output limits and no TS bypass |
| 6 | Extract Python AI service | **Pending** | Defined service boundary/protocol, isolated Python service, bounded IPC/lifecycle, tests and packaging; no implicit trust of model output |
| 7 | Build TypeScript-to-Python/Rust bridges | **Partially complete — Rust transport only** | Production-quality TS↔Rust and TS↔Python clients, versioned contracts, lifecycle/error/cancellation tests, packaged binaries/services |
| 8 | End-to-end security and integration tests | **Partially complete — component tests exist** | Full VS Code-to-TS-to-Rust/Python workflows, adversarial cross-platform tests, approval/write/command integration, regression and security review |
| 9 | Packaging and release | **Pending** | Reproducible per-platform builds, signed/verified artifacts, install/upgrade/uninstall tests, SBOM/provenance, release notes and marketplace/release gates |

## Priority 1 — macOS firmlink detection

**Tracking issue:** [#74](https://github.com/Veeresh-11/Loco-Agent/issues/74)

Relevant PRs: #73, #75, #76, #77, #78, #100, #102. PR #102's exact-head macOS run #45335594962 confirmed distinct descriptor paths for `/Users`, `/Applications`, and `/Library`: `F_GETPATH` returned the visible firmlink path while `F_GETPATH_NOFIRMLINK` returned `/System/Volumes/Data/...`. The probe now adds a temporary-directory rename/replacement assertion to test whether both path forms follow the retained descriptor rather than a replacement at the old name; this latest assertion-bearing source head still needs fresh exact-head CI.

### Established evidence
- The macOS 26.7.2 hosted runner (build 25G332, arm64) lists mappings such as `/Users`, `/Applications`, and `/Library` in the system firmlink map.
- The original native `getattrlistbulk` probe returned `0x00100000` (`SF_NOUNLINK`), not `SF_FIRMLINK = 0x00800000`.
- Follow-up probes of `getattrlistat(FSOPT_NOFOLLOW_ANY | FSOPT_RESOLVE_BENEATH)` and `fgetattrlist` returned the same `SF_NOUNLINK` value.
- PR #77 added a descriptor-derived `f_fsid` comparison. Its exact-head run #45322641807 passed Ubuntu, Windows, and macOS; the observed System/Data crossing is rejected even when `st_dev` is shared. This is a distinct-volume boundary, not a universal same-volume firmlink detector.
- PR #100's initial diagnostic run #45334961812 passed Ubuntu, Windows, and macOS; after the newline-format and evidence-documentation updates, the exact updated head `c212b45a15053482473fb8c2e34e6dc5ecb3b3b6` also passed all three platforms in run #45335436957. On the macOS runner, `getattrlistbulk`, `getattrlistat`, `fgetattrlist`, `fstatat(AT_SYMLINK_NOFOLLOW)`, followed `fstatat`, and opened-handle `fstat` all returned `0x00100000` for the three mapped entries. Raw bulk records showed the returned attribute mask and parsed flags field at the expected offset.
- Apple's public `getattrlistbulk(2)` manual documents `SF_FIRMLINK` in `ATTR_CMN_FLAGS` for a firmlink entry; Apple's `FileFlags.firmlink` documentation identifies the corresponding C constant. The hosted runner's behavior therefore remains unexplained.

### Remaining work
1. Get an authoritative Apple DTS / Developer Forums explanation for the mismatch, or an Apple-provided reproducible OS firmlink fixture.
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

Remaining: review the actual platform implementation and supported OS/filesystem matrix end-to-end; explicitly document unsupported filesystems and kernel/API versions; verify handle-relative semantics, concurrent mutation, mount identity, metadata freshness, and cleanup behavior. CI success on a fixture is evidence for that fixture only, not every filesystem.

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
- PR #100's updated exact head `c212b45a15053482473fb8c2e34e6dc5ecb3b3b6` passed all three platforms in run #45335436957. PR #102's first exact-head run #45335594962 passed all three platforms and its macOS log confirmed the firmlink-path distinction. A subsequent rename/replacement regression fixture has been added; its latest source head still awaits exact-head CI.
- Do not close a PR because a later PR supersedes it unless the code, tests, and dependency relationship have been reviewed and the superseding PR is confirmed to contain the required changes.
- Do not merge a stacked PR out of dependency order. Merge only after the complete stack is validated and reviewed.
- Keep issue #74 and issue #82 open until their acceptance criteria are satisfied.
- Update this roadmap when evidence changes; a design document, prototype, or passing isolated CI job must not be marked as production-complete.
