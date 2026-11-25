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
| A5 | Versioned authority registration/capture protocol | **Implemented in draft PR stack; CI green, integration/review pending** | [PR #132](https://github.com/Veeresh-11/Loco-Agent/pull/132) design, [#133](https://github.com/Veeresh-11/Loco-Agent/pull/133) DTOs, [#135](https://github.com/Veeresh-11/Loco-Agent/pull/135) Rust session/handlers, [#136](https://github.com/Veeresh-11/Loco-Agent/pull/136) TypeScript client, [#137](https://github.com/Veeresh-11/Loco-Agent/pull/137) adversarial tests, [#138](https://github.com/Veeresh-11/Loco-Agent/pull/138) path parity | Reconcile and review the stacked protocol chain; confirm exact-head CI, lifecycle semantics, registration/capture/revocation behavior and independent security review; merge in dependency order |
| A6 | Approval issue/consume integration | **In progress — design + store foundation submitted** | [PR #149](https://github.com/Veeresh-11/Loco-Agent/pull/149) protocol design; [PR #150](https://github.com/Veeresh-11/Loco-Agent/pull/150) registration-bound store API and tests; Rust Core Tests #45577413052 and Contract Validation #45577413124 running | Review protocol 2.1 design; integrate issue/consume DTOs and handlers with Rust-owned registry capture; revoke challenges with workspace registration; add TypeScript client and replay/lifecycle tests; keep authenticator ceremony separate and do not claim human approval |
| A7 | Adversarial lifecycle and replay tests | **Pending** | Depends on A5–A6 | Registration replacement, stale capture, replay, duplicate consume, concurrent consume, process restart, deactivation, malformed IPC and race tests |
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
| Pure artifact verification policy | **CI green — draft** | [PR #147](https://github.com/Veeresh-11/Loco-Agent/pull/147) | Run Rust tests on all targets; harden policy tests; do not treat normalized fields as cryptographic proof |

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

1. **Review PR #147**: exact-head Rust Core Tests, Contract Validation and Authority Native Artifact Builds are now green (#45573727400, #45573727596, #45573727647). Review the pure policy boundaries and negative tests; retain draft until independent review.
2. **Review and reconcile A5 (#132–#138)**: protocol v2 DTOs, Rust handlers/session, TypeScript client and adversarial tests are implemented in the open draft stack; confirm dependencies and semantics before merge.
3. **Continue A6**: after #150 CI, wire strict issue/consume messages to registry capture and registration-bound store APIs; ensure workspace revocation clears challenges.
4. Add adversarial issue/consume, replay, concurrent-consume and lifecycle tests.
6. Review and reconcile stacked PR dependencies before merging in order.
7. Continue attestation and runtime-verification gates independently; no sidecar launch.

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

- **2026-11-25:** Initial canonical tracker created from the two user-provided roadmap snapshots and live repository state. Added PRs #128–#131 and #139–#147; confirmed #130 and #131 exact-head CI success and #145 six-job CI success.
- **2026-11-25 (reconciliation):** Checked PRs #132–#138 and exact-head CI. The versioned registration/capture protocol is already implemented in draft PRs #132–#138: #133 Rust DTO CI green (#45561353483/#45561353697), #135 Rust session/handler CI green (#45563628930/#45563628849), #136 TypeScript CI green (#45564926678), #137 Rust/contract CI green (#45568074629/#45568074945), and #138 TypeScript CI green (#45568062826). Reclassified A5 as implemented in an unmerged draft stack. Opened [PR #149](https://github.com/Veeresh-11/Loco-Agent/pull/149) for the A6 approval issue/consume protocol design; it is design-only and draft. PR #147 exact-head run #45573727400 (Rust Core Tests), #45573727596 (Contract Validation), and #45573727647 (Authority Native Artifact Builds) all completed successfully; PR remains draft and unreviewed.
