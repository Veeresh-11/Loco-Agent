# LOCO Agent — Project Progress Tracker

**Last reconciled:** 2026-11-25  
**Canonical tracker:** This file is the source of truth for roadmap status, task ordering, acceptance criteria, dependencies, and estimates. Update it in the same PR whenever scope or task status changes.

## How to use this tracker

1. Before starting work, check this file and the linked PR/issue state.
2. Work only on the next unblocked task in the ordered plan unless a security dependency requires otherwise.
3. Every task must have an acceptance condition and evidence (tests, exact-head CI, review, or release verification).
4. Update the status, evidence, dependency, and estimate when work changes. Do not mark a task complete because code was submitted or a PR is mergeable.
5. A PR being CI-green is not equivalent to security acceptance or merge. Stacked PRs must be merged in dependency order after review.
6. Keep this tracker current in the same PR that changes the plan. Add a dated entry to the change log below.
7. Do not close a PR until its change is integrated or intentionally superseded and its dependent branches have been reconciled.

## Status definitions

- **Not started:** no implementation/design work yet.
- **Design:** requirements or architecture being specified; no accepted implementation.
- **In progress:** implementation or tests underway.
- **Submitted:** implementation exists in an open PR; validation/review incomplete.
- **CI green:** exact PR head passed its required CI, but review/integration may remain.
- **Partially complete:** meaningful evidence exists, but one or more exit criteria remain.
- **Blocked:** cannot safely proceed until a named dependency is resolved.
- **Complete:** all acceptance criteria pass, required independent review is recorded, integrated into the intended branch, and release/security limitations are documented.

## A. Security-core workstream — snapshot, authority and approvals

| Order | Task | Current status | PR / evidence | Remaining exit criteria |
|---:|---|---|---|---|
| A1 | Snapshot-bound approval prototype | **Submitted — draft** | [PR #128](https://github.com/Veeresh-11/Loco-Agent/pull/128) | Review action + snapshot identity/freshness binding; authenticator and replay resistance; Rust-side independent verification; adversarial tests |
| A2 | Authority trust-boundary design | **Design — draft** | [PR #129](https://github.com/Veeresh-11/Loco-Agent/pull/129) | Independent security review; reconcile design with implemented registration/capture/approval contracts |
| A3 | Process-local workspace registry | **CI green — draft** | [PR #130](https://github.com/Veeresh-11/Loco-Agent/pull/130), exact-head run #45431484151: Contract Validation and Rust Core Tests passed | Review registry lifecycle, ownership, invalidation and concurrency; integration with native adapter |
| A4 | Native adapter factory and registration | **CI green / implementation submitted — draft** | [PR #131](https://github.com/Veeresh-11/Loco-Agent/pull/131), exact-head run #45437420337: Contract Validation and Rust Core Tests passed | Review platform selection and registration lifecycle; connect to versioned registration/capture protocol; cross-platform adversarial coverage |
| A5 | Versioned authority registration/capture protocol | **Implemented in draft PR stack; exact-head CI green and protocol-chain review completed; integration/independent review pending** | [PR #132](https://github.com/Veeresh-11/Loco-Agent/pull/132) design, [#133](https://github.com/Veeresh-11/Loco-Agent/pull/133) DTOs, [#135](https://github.com/Veeresh-11/Loco-Agent/pull/135) Rust session/handlers, [#136](https://github.com/Veeresh-11/Loco-Agent/pull/136) TypeScript client, [#137](https://github.com/Veeresh-11/Loco-Agent/pull/137) adversarial tests, [#138](https://github.com/Veeresh-11/Loco-Agent/pull/138) path parity | Obtain independent security review, then integrate only in dependency order: #132 → #133 → #135 → (#136 → #138, with #137 as a parallel child of #135); preserve v1 compatibility and keep privileged operations disabled |
| A6 | Approval issue/consume integration | **A6 store CI green; A7 exact-head CI green; independent security review pending** | [PR #149](https://github.com/Veeresh-11/Loco-Agent/pull/149) protocol design; [PR #150](https://github.com/Veeresh-11/Loco-Agent/pull/150) latest head `387ea67fc6bae0d8352e566890f0b7b195c6dbcf`: Contract #45581381364 passed; Rust Core Tests #45581381473 passed all 9 targets. Review found a race between unscoped insert and later registration-digest update; fixed by inserting the digest atomically and added independent-connection regression coverage. [PR #151](https://github.com/Veeresh-11/Loco-Agent/pull/151) latest head `47665a5824cd94d7b06c47f7087db12b22f4f140` includes three sidecar lifecycle regressions: stale-capture mismatch does not consume the challenge (then succeeds when original bytes are restored), revoke/re-register of the same root produces a new handle while old challenge/handle cannot be reused, and strict unknown-field IPC rejection. Prior rebased head `8f5bdc351896962d052058353cdf33acf88e1aa5` passed Contract #45581872681 and TypeScript #45581872683 (Node 20/22); its Rust run #45581872657 had 8/9 targets pass and macOS 15 cancelled by the superseding commit. Fresh previous commit's Rust #45583064509, Contract #45583064526 and TypeScript #45583064494 were running when superseded by the malformed-IPC regression commit; fresh exact-head TypeScript CI #45583199603 passed on Node 20/22; Contract Validation #45583199565 passed; Rust Core Tests #45583199600 passed all nine configured targets | Latest exact-head #151 CI is green: Contract #45583199565, TypeScript #45583199603 (Node 20/22), Rust #45583199600 (all nine targets); obtain independent security review; keep authenticator separate and do not claim human approval |
| A7 | Adversarial lifecycle and replay tests | **Implemented in draft PR #151; exact-head Contract, TypeScript and all nine Rust targets green; independent review pending** | [PR #151](https://github.com/Veeresh-11/Loco-Agent/pull/151), head `47665a5824cd94d7b06c47f7087db12b22f4f140`; adds re-registration/old-challenge isolation, stale-capture non-consumption/retry, and strict malformed-IPC rejection integration tests. Existing coverage includes replay, revocation, duplicate consume, store concurrency, process-epoch restart and malformed protocol DTOs | Exact-head CI green; malformed IPC, stale-capture retry, re-registration isolation, revocation invalidation and shutdown/EOF lifecycle covered; independent review pending |
| A8 | Independent security review | **Pending** | Applies to A1–A7 | Independent reviewer sign-off on threat model, trust boundary, protocol, platform adapters, approval lifecycle and residual risks |

## B. Platform filesystem and snapshot workstream

| Task | Current status from consolidated roadmap | Remaining exit criteria |
|---|---|---|
| Architecture and contracts | **Established** | Preserve compatibility; update versioned contracts through reviewed changes |
| Rust security core | **In progress** | Finish trusted authority integration, authorization, controlled writes and command boundary |
| Action approval binding | **Implemented prototype / partially complete** | Authenticator, replay safety, Rust enforcement and independent review |
| Linux workspace snapshot acquisition | **Implemented** | Retain Linux-specific support assumptions and regression coverage |
| Linux mutation detection | **Implemented, best-effort** | Document limits; keep captured-byte digest authoritative; test race outcomes |
| Concurrent replacement stress | **Added and CI verified** | Continue regression coverage when adapter changes |
| Windows path validation | **Expanded and CI verified** | Distinguish lexical checks from native handle-based containment |
| Windows acquisition | **Design / prototype stage** | Finish handle-relative root acquisition, reparse rejection, identity and NTFS/volume support matrix |
| macOS adapter | **Partially implemented in native stack; broader acceptance pending** | macOS 15/26 arm64/Intel evidence exists for firmlink and descriptor checks; resolve authoritative firmlink-entry semantics, expand supported matrix, independent review |
| Firmlink detection (priority 1) | **Partially complete** | Resolve Apple `SF_FIRMLINK` discrepancy / authoritative entry semantics; no pathname heuristics; keep issue #74 open until accepted |
| Native filesystem API review (priority 2) | **Partially complete** | Complete OS/filesystem/version matrix, race semantics, unsupported cases and independent review |

## C. Runtime packaging and provenance workstream

| Task | Current status | PR / evidence | Remaining exit criteria |
|---|---|---|---|
| Protocol v2 production integration gap | **Documented** | [PR #139](https://github.com/Veeresh-11/Loco-Agent/pull/139) | Close integration gaps with production TS client and lifecycle tests |
| Native authority artifact build matrix | **CI implementation submitted** | [PR #140](https://github.com/Veeresh-11/Loco-Agent/pull/140) | Verify package and supported target matrix; review |
| Build provenance attestations | **Workflow submitted; trusted release evidence pending** | [PR #141](https://github.com/Veeresh-11/Loco-Agent/pull/141) | Trusted-main run; independently verify each executable and VSIX subject |
| Runtime verification architecture | **Proposed design** | [PR #142](https://github.com/Veeresh-11/Loco-Agent/pull/142) | Independent review and validate signing/platform feasibility |
| Immutable workflow action pins | **CI implementation submitted** | [PR #143](https://github.com/Veeresh-11/Loco-Agent/pull/143) | Review pinned action code and workflow permissions |
| Immutable pin validator | **CI implementation submitted** | [PR #144](https://github.com/Veeresh-11/Loco-Agent/pull/144) | Review validator and false-positive/negative coverage |
| Checkout prerequisite for validator | **CI green** | [PR #145](https://github.com/Veeresh-11/Loco-Agent/pull/145), exact-head run #45571797788: six jobs passed | Keep stacked order; review and merge with dependencies |
| Attestation verification readiness | **Assessment submitted** | [PR #146](https://github.com/Veeresh-11/Loco-Agent/pull/146) | Validate findings against real trusted release and repository rules |
| Pure artifact verification policy | **Review hardening submitted — exact-head CI green; independent review pending** | [PR #147](https://github.com/Veeresh-11/Loco-Agent/pull/147), head `71bfe8cda54cc58f60f0028e970f70fa6819e428`; Contract #45580071069 passed; Authority Native Artifact Builds #45580071097 passed all 4 target builds and VSIX packaging; Rust Core Tests #45580071318 passed all 9 targets | Retain policy-only boundary; obtain independent security review; do not treat normalized fields as cryptographic proof |

### Runtime security gates (must remain disabled until approved)

- No production sidecar launch from extension activation.
- No workspace writes, patch application, command execution, approval grants or execution tokens.
- No PATH, shell, workspace, environment-variable, user-setting or temporary-file fallback for executable resolution.
- No positive verification claim until a maintained verifier authenticates structured provenance and platform integrity is bound to the exact executable selected for launch.
- Windows Authenticode signing/publisher policy and macOS Developer ID signing/notarization are not yet evidenced in the current build workflow. Linux needs a reviewed protected-install strategy and explicit same-user attacker model.
- PR CI artifacts are not trusted release evidence. Require a trusted-main/release attestation and verify executable subjects independently of the VSIX subject.

## D. Product integration and release roadmap

| Priority | Workstream | Current status | Exit condition |
|---:|---|---|---|
| D1 | Controlled Rust workspace writes | **Pending** | Handle-relative race-resistant writes/replacements; bounded patch validation; authorization; authenticated approval; audit; rollback/recovery; adversarial tests |
| D2 | Rust authorization and command execution controls | **Pending** | Rust enforcement; explicit command policy; executable/args/env/cwd bounds; cancellation/timeouts/output limits; no TypeScript bypass |
| D3 | Python AI service extraction | **Pending** | Defined service boundary/protocol; isolated service; bounded IPC/lifecycle; tests and packaging; model output never treated as authority |
| D4 | TypeScript integration | **Partially complete** | Production TS↔Rust and TS↔Python clients; versioned contracts; lifecycle/error/cancellation tests; packaged runtime |
| D5 | End-to-end agent | **Pending** | Full VS Code→TypeScript→Rust/Python flows, approval/write/command integration, adversarial cross-platform regression |
| D6 | Packaging and release | **Pending** | Reproducible per-platform builds; signed/verified artifacts; install/upgrade/uninstall tests; SBOM/provenance; release notes and marketplace gates |

## E. Immediate execution queue

1. **A8 independent security review — BLOCKED on reviewer availability.** Live PR metadata checked on 2026-11-25: PRs #147, #149, #150 and #151 have no requested reviewers; PRs #132–#138 are also open/draft with no requested reviewers. No independent review submission was present in the inspected PR discussions for #147, #150 or #151. The current author is the PR author, so this project-side audit cannot satisfy the independent-review gate. A repository maintainer must nominate/request a reviewer who did not author these changes. Scope: A1–A7 threat model, trust boundaries, protocol compatibility, registry/native adapters, challenge issue/consume/revocation, cross-platform behavior, and residual artifact-verification risks.
2. **Pre-review audit completed (not independent approval).** Rechecked current PR metadata and change summaries/patches for #147, #150 and #151. No new actionable defect was established by this limited pass. Known limitations remain: #147's `VerifiedProvenance` is caller-constructible normalized evidence and is not cryptographic proof; #150's registration digest is now inserted atomically with the challenge row and has independent-connection regression coverage; #151's `bindingVerified` result is only a binding check, not human approval, authorization, or an execution grant. Do not promote any of these limitations into claims of verified runtime security.
3. **A6/A7 exact-head CI remains green.** #150 head `387ea67fc6bae0d8352e566890f0b7b195c6dbcf`: Contract #45581381364 and Rust #45581381473 passed (all nine targets). #151 head `47665a5824cd94d7b06c47f7087db12b22f4f140`: Contract #45583199565, TypeScript #45583199603 (Node 20/22), and Rust #45583199600 (all nine targets) passed.
4. **PR #147 exact-head CI remains green.** Head `71bfe8cda54cc58f60f0028e970f70fa6819e428`: Contract #45580071069 passed; Native Artifact Builds #45580071097 passed all four target builds and VSIX packaging; Rust Core Tests #45580071318 passed all nine targets. Keep draft pending independent review.
5. **A5 protocol stack (#132–#138) remains unmerged.** Integration order: #132 → #133 → #135 → (#136 → #138, with #137 as a parallel child of #135). Exact-head CI is green as recorded above; independent review and ordered integration remain outstanding.
6. **A6 design/store/protocol chain (#149–#151) remains unmerged.** #149 is design-only; #150 is the registration-scoped store; #151 is protocol 2.1 integration. Keep drafts until independent review and dependency reconciliation are complete.
7. Keep platform-authenticator verification and independent security review as separate gates; do not enable privileged operations.
8. Continue attestation and runtime-verification gates independently; no sidecar launch, workspace writes, patch application, command execution, approval grants, or execution tokens.

## F. Delivery estimate

**Estimate from the 2026-11-25 checkpoint:** approximately **12–18 focused working days** (about **3–4 calendar weeks**) to reach an integrated, tested security-core milestone, assuming one developer working full-time and CI/review feedback arriving within a day.

Planning breakdown (not a guarantee):

| Work package | Working-day estimate |
|---|---:|
| PR #147 CI validation and correction | 0.5–1 |
| Versioned registration/capture protocol and tests | 2–3 |
| Registry + native adapter integration and lifecycle tests | 1–2 |
| Approval issue/consume integration | 2–3 |
| Replay, concurrency, replacement and deactivation adversarial tests | 2–3 |
| Reconcile stacked PRs, cross-platform CI and fixes | 2–3 |
| Independent review remediation and final acceptance | 2–3 |
| **Total** | **12–18** |

This estimate does **not** promise production sidecar launch or a marketplace release. Those depend on external signing credentials, trusted-main attestation evidence, verifier/trust-root selection, repository governance, platform-specific exact-file binding and independent security approval. Those gates may add **5–10+ working days** or remain blocked by external provisioning/review. Python extraction, full command execution, controlled workspace writes, complete end-to-end integration and release are separate roadmap milestones and are not included in the 12–18-day estimate.

The historical pace of creating several focused PRs over three days is useful for estimating documentation and isolated changes, but PR count is not a reliable throughput measure for security acceptance: integration, cross-platform CI, review, and adversarial testing dominate the remaining schedule.

## G. Change log

- **2026-11-25 (A8 review-gate reconciliation):** Rechecked live PR metadata for #132–#138 and #147/#149–#151. No requested reviewer was present on the inspected open/draft PRs; discussions checked on #147, #150 and #151 contained no independent review submission. Completed a limited pre-review patch/status audit of #147/#150/#151; it did not establish a new actionable defect and is explicitly not independent security approval. Corrected the immediate queue's stale contradictory #151 Rust status (eight of nine vs all nine) and reordered it around the actual A8 reviewer dependency. The next action requires a reviewer who did not author the changes; all privileged runtime gates remain disabled.

- **2026-11-25:** Initial canonical tracker created from the two user-provided roadmap snapshots and live repository state. Added PRs #128–#131 and #139–#147; confirmed #130 and #131 exact-head CI success and #145 six-job CI success.
- **2026-11-25 (reconciliation):** Checked PRs #132–#138 and exact-head CI. The versioned registration/capture protocol is already implemented in draft PRs #132–#138: #133 Rust DTO CI green (#45561353483/#45561353697), #135 Rust session/handler CI green (#45563628930/#45563628849), #136 TypeScript CI green (#45564926678), #137 Rust/contract CI green (#45568074629/#45568074945), and #138 TypeScript CI green (#45568062826). Reclassified A5 as implemented in an unmerged draft stack. Opened [PR #149](https://github.com/Veeresh-11/Loco-Agent/pull/149) for the A6 approval issue/consume protocol design; it is design-only and draft. PR #147 exact-head run #45573727400 (Rust Core Tests), #45573727596 (Contract Validation), and #45573727647 (Authority Native Artifact Builds) all completed successfully; PR remains draft and unreviewed.

- **2026-11-25 (A6 store implementation):** PR #150 adds workspace-registration-scoped challenge issue/consume APIs, stores only a SHA-256 digest of the opaque registration handle, prevents the legacy unscoped consume API from consuming scoped challenges, and deletes challenges on registration revocation. Four tests cover wrong-handle rejection, legacy API isolation, revocation invalidation and malformed handles. Initial exact-head Rust Core Tests #45577413052 exposed a Rust borrow-lifetime error in the new schema migration; corrected in commit `4b932d8cb74acddf9e2910574b0f6284f6d92bb7`. New Contract Validation run #45577556503 was in progress at this update; no green CI claim yet.

- **2026-11-25 (A6 protocol integration):** PR #150's corrected exact head `4b932d8cb74acddf9e2910574b0f6284f6d92bb7` passed Rust Core Tests #45577556479 and Contract Validation #45577556503. Opened [PR #151](https://github.com/Veeresh-11/Loco-Agent/pull/151) stacked on #150: added strict protocol 2.1 DTOs and explicit negotiation, retained protocol 2.0 unchanged, wired Rust-owned registry capture to registration-bound challenge issue/consume, revoked challenges on workspace revocation, and added TypeScript 2.1 client methods plus Rust/TypeScript lifecycle tests. At tracker update, exact-head Rust Core Tests #45579121240, Contract Validation #45579121249 and TypeScript CI #45579121194 were running. #151 remains draft; binding verification is not authenticated human approval, and no privileged operation is enabled.

- **2026-11-25 (A6 validation correction):** PR #151's initial TypeScript CI #45579121194 found a TypeScript narrowing error when checking negotiated server capabilities. Corrected the capability-list cast in commit `ff8987bd2946298505bcc5e48b4978a3057e7415`. Latest exact-head validation is Contract #45579362411 (running), TypeScript #45579363592 (running), and Rust Core Tests #45579363081 (pending). Earlier head #c8350de13be10def8c382f7efdd32173714b9b6d had Contract Validation pass and Ubuntu 22.04/26.04 Rust jobs pass; its Node 22 compile failed on the now-corrected type error. No all-green claim is made.

- **2026-11-25 (A6 CI update):** On PR #151 exact head `ff8987bd2946298505bcc5e48b4978a3057e7415`, Contract Validation #45579362411 passed and TypeScript CI #45579363592 passed on Node 20 and Node 22. Rust Core Tests #45579363081 passed Ubuntu 22.04, Ubuntu 24.04, Ubuntu 26.04 and macOS 26; Windows 2022, Windows 2025, macOS 15, macOS 15 Intel and macOS 26 Intel remained in progress at this checkpoint. The earlier TypeScript TS18046 error was fixed before these green runs. A6 remains submitted/in progress, not complete; the cross-platform Rust matrix and independent security review are outstanding.

- **2026-11-25 (A6 cross-platform CI progress):** Rust Core Tests #45579363081 additionally passed Windows 2022, Windows 2025 and macOS 15 Intel. Only macOS 15 and macOS 26 Intel remained in progress at this check; Contract Validation and TypeScript (Node 20/22) remain green on the same PR #151 head.

- **2026-11-25 (A6 exact-head CI complete):** PR #151 head `ff8987bd2946298505bcc5e48b4978a3057e7415` passed Rust Core Tests #45579363081 on all nine configured targets (Ubuntu 22.04/24.04/26.04, Windows 2022/2025, macOS 15/15 Intel/26/26 Intel), Contract Validation #45579362411, and TypeScript CI #45579363592 on Node 20 and Node 22. Store tests also cover internal-time expiry, stale snapshots, action mismatch without consumption, process-epoch restart invalidation and concurrent single-use consumption. PR #151 remains draft pending review; binding verification is not human approval. Next queue item is review of PR #147.

- **2026-11-25 (PR #147 review):** Reviewed `authority_artifact_verification.rs` against the attestation-readiness constraints. The package member validator rejected POSIX absolute paths, backslashes and dot traversal, but accepted Windows drive-qualified/drive-relative paths (for example `C:/...`) and ADS syntax because it did not reject colons. Added colon and ASCII control-character rejection, tests for drive-qualified/drive-relative/ADS paths, and direct coverage of the `WrongTarget` branch on PR #147. Latest exact-head `71bfe8cda54cc58f60f0028e970f70fa6819e428`: Contract Validation #45580071069 running, Rust Core Tests #45580071318 pending, Authority Native Artifact Builds #45580071097 queued. PR remains draft; normalized verifier booleans are not cryptographic proof and no runtime launch integration is added.

- **2026-11-25 (PR #147 CI progress):** On exact head `71bfe8cda54cc58f60f0028e970f70fa6819e428`, Contract Validation #45580071069 passed. Authority Native Artifact Builds #45580071097 passed all four native target builds and VSIX packaging. Rust Core Tests #45580071318 remained pending at this check. PR review COMMENT #6054439923 records the archive-path remediation and the caller-constructible normalized-evidence limitation; PR remains draft.


- **2026-11-25 (A5 protocol-chain review and PR #147 CI completion):** Rechecked draft PRs #132–#138 and their exact-head CI. PR #133 passed Rust Core Tests #45561353483 and Contract Validation #45561353697; #135 passed Rust #45563628930 and Contract #45563628849; #136 passed TypeScript CI #45564926678; #137 passed Rust #45568074945 and Contract #45568074629; #138 passed TypeScript CI #45568062826. Reviewed the protocol proposal, strict DTOs, session state machine, Rust handlers, TypeScript client, adversarial tests and Rust/TypeScript path-validator parity. The #138 ASCII-only case-folding change aligns TypeScript with Rust's ASCII reserved-name normalization; C1 control rejection is covered on both sides. No blocking issue was identified in this pass; this is not independent security approval. PRs #132–#138 remain open/draft and unmerged. Also confirmed PR #147 exact-head Rust Core Tests #45580071318 passed all nine configured targets; Contract #45580071069 and Native Artifact Builds #45580071097 were already green. Updated PR #147 validation text and retained the independent-review gate.


- **2026-11-25 (A6 store concurrency review):** Reviewed PRs #149–#151. Found that #150 originally inserted a challenge with an empty registration digest and updated the digest in a second transaction, allowing a legacy consumer on another SQLite connection to race the scope update. Refactored challenge creation through a shared internal method so the registration digest is inserted atomically with the challenge row; added a regression test using an independent SQLite connection. Submitted review COMMENT #6054558494 on PR #150. Updated exact head is `387ea67fc6bae0d8352e566890f0b7b195c6dbcf`; Contract Validation #45581381364 and Rust Core Tests #45581381473 passed on the updated #150 head (all nine Rust targets). Rebased #151 onto that exact #150 head as `8f5bdc351896962d052058353cdf33acf88e1aa5`; fresh Rust #45581872657 is pending, Contract #45581872681 and TypeScript #45581872683 are queued. No privileged behavior is enabled; `bindingVerified` remains a binding result, not human approval.


- **2026-11-25 (A6 rebased protocol CI update):** PR #151 rebased head `8f5bdc351896962d052058353cdf33acf88e1aa5` now passes Contract Validation #45581872681 and TypeScript CI #45581872683 on Node 20 and Node 22. Rust Core Tests #45581872657 is still running across the nine-target matrix; no all-green claim yet. PR #150 head `387ea67fc6bae0d8352e566890f0b7b195c6dbcf` has passed both Contract Validation #45581381364 and all nine Rust Core Tests #45581381473.


- **2026-11-25 (A7 lifecycle regression coverage):** Inspected the existing PR #151 sidecar tests, workspace registry and protocol session. Existing tests already cover challenge issue/consume/replay, workspace revocation invalidation, unsupported command rejection, registry handle revocation, store concurrent single-use consume, process-epoch restart invalidation, and strict malformed DTO rejection. Added two missing end-to-end protocol 2.1 cases on PR #151: (1) mutate the captured file after challenge issue, assert consume returns `BINDING_MISMATCH` without consuming the challenge, restore the original bytes and verify retry succeeds; (2) revoke a workspace registration, re-register the same root, assert a fresh handle is minted, the old challenge cannot be consumed with the new handle, the old handle cannot capture, and the new handle can capture; (3) send caller-controlled snapshot metadata as an unknown issue-message field and assert strict IPC deserialization fails closed. Latest head `90ef0f3e851dabb77fa759c9576026a831613eae`. Fresh Rust Core Tests #45583064509 is pending, Contract Validation #45583064526 is in progress, and TypeScript CI #45583064494 is in progress. Prior rebased head `8f5bdc351896962d052058353cdf33acf88e1aa5` had Contract #45581872681 and TypeScript #45581872683 pass; its Rust matrix had eight targets pass and macOS 15 was cancelled when superseded by the new test commit. No latest-head all-green claim is made.


- **2026-11-25 (A7 malformed IPC regression):** Added a third protocol 2.1 sidecar integration test to PR #151: an `issueBindingChallenge` message containing caller-controlled `snapshotDigest` metadata is rejected by the strict `deny_unknown_fields` DTO and the sidecar exits unsuccessfully instead of processing it. Latest head is `47665a5824cd94d7b06c47f7087db12b22f4f140`. CI from the preceding A7 commit was superseded; fresh exact-head workflow runs had not appeared at the time of this tracker update. A7 remains partially implemented and is not considered complete until exact-head CI and independent review.


- **2026-11-25 (A7 exact-head CI started):** On PR #151 head `47665a5824cd94d7b06c47f7087db12b22f4f140`, fresh Rust Core Tests #45583199600 started across nine targets, Contract Validation #45583199565 started, and TypeScript CI #45583199603 started on Node 20/22. All three workflows were still in progress at this check; no pass/fail claim yet.


- **2026-11-25 (A7 TypeScript CI result):** Latest PR #151 head `47665a5824cd94d7b06c47f7087db12b22f4f140` passed TypeScript CI #45583199603 on both Node 20 and Node 22. Contract Validation #45583199565 is still running; Rust Core Tests #45583199600 is running across nine targets. A7 remains partially implemented pending the two remaining workflow results and independent review.


- **2026-11-25 (A7 Contract Validation result):** Latest PR #151 head `47665a5824cd94d7b06c47f7087db12b22f4f140` passed Contract Validation #45583199565 and TypeScript CI #45583199603 (Node 20/22). Rust Core Tests #45583199600 is the remaining exact-head gate and is still running across the nine-target matrix.


- **2026-11-25 (A7 Rust matrix progress):** On latest PR #151 head `47665a5824cd94d7b06c47f7087db12b22f4f140`, Contract Validation #45583199565 and TypeScript CI #45583199603 (Node 20/22) have passed. Rust Core Tests #45583199600 has passed Ubuntu 22.04, Ubuntu 24.04, Ubuntu 26.04, Windows 2022, Windows 2025 and macOS 26; macOS 15 Intel and macOS 26 Intel were still running; macOS 15 was queued. Exact-head Rust CI is therefore not yet green.


- **2026-11-25 (A7 Rust matrix progress update):** Rust Core Tests #45583199600 on latest head `47665a5824cd94d7b06c47f7087db12b22f4f140` now has six passing targets: Ubuntu 22.04/24.04/26.04, Windows 2022/2025 and macOS 26. macOS 15 Intel and macOS 26 Intel remain in progress; macOS 15 remains queued. Contract Validation #45583199565 and TypeScript CI #45583199603 remain green. Do not mark A7 CI complete until all nine Rust targets finish.


- **2026-11-25 (A7 exact-head Rust matrix recheck):** Re-fetched jobs for Rust Core Tests #45583199600 on PR #151 head `47665a5824cd94d7b06c47f7087db12b22f4f140`. Eight of nine targets have completed successfully: Ubuntu 22.04, Ubuntu 24.04, Ubuntu 26.04, Windows 2022, Windows 2025, macOS 15 Intel, macOS 26 and macOS 26 Intel. The macOS 15 job remains queued (not failed, but not yet passed). Contract Validation #45583199565 and TypeScript CI #45583199603 (Node 20/22) are green. A7 remains partially complete; no merge or independent security approval is implied.


- **2026-11-25 (A7 lifecycle coverage audit):** Re-opened the exact PR #151 sidecar integration test source and reviewed the protocol 2.1 lifecycle cases. Confirmed existing tests exercise issue/consume/replay, challenge invalidation on workspace revocation, stale-handle rejection after same-root re-registration, stale-capture failure without challenge consumption followed by successful retry, strict unknown-field IPC rejection, unsupported command rejection, and graceful shutdown. The 2.0 sidecar test also covers capture denial after revocation. No additional deactivation test was added because revocation is the implemented deactivation operation and already has end-to-end coverage. Rust Core Tests #45583199600 passed all nine targets; Contract #45583199565 and TypeScript #45583199603 passed. This is a source-coverage audit, not independent security approval.


- **2026-11-25 (A7 exact-head CI completion):** Rechecked workflow run #45583199600 and its nine jobs on PR #151 head `47665a5824cd94d7b06c47f7087db12b22f4f140`. All nine targets are now completed successfully: Ubuntu 22.04/24.04/26.04, Windows 2022/2025, macOS 15, macOS 15 Intel, macOS 26 and macOS 26 Intel. Contract Validation #45583199565 and TypeScript CI #45583199603 (Node 20/22) also passed. A7 is now exact-head CI green, but remains unaccepted pending independent security review and integration. Corrected the stale #150 Rust status: #45581381473 passed all nine targets.


- **2026-11-25 (execution queue advanced):** Reordered the immediate queue after the final macOS 15 job completed successfully. A7 implementation/CI work is no longer the immediate task; the next blocking task is independent security review by a reviewer who did not author the changes. Local review and CI evidence are not substitutes. PR #147 and A5–A7 remain unmerged/draft; no privileged runtime path is enabled.
