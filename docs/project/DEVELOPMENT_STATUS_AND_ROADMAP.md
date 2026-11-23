# LOCO Agent — Development Status, Architecture, and Roadmap

**Status snapshot:** 2026-11-23  
**Repository:** https://github.com/Veeresh-11/Loco-Agent  
**Pull requests:** https://github.com/Veeresh-11/Loco-Agent/pulls  
**Actions:** https://github.com/Veeresh-11/Loco-Agent/actions  
**Latest fully validated stack PR at snapshot:** #59 (CI passed; draft). PR #60 contract validation passed; Ubuntu/Windows Rust jobs passed and macOS was queued at the last check.  
**Current follow-up branch:** `security/linux-no-mount-crossing` (stacked on #62)

This document consolidates the project history, current implementation state, security boundaries, validation evidence, and remaining work. It is a status record, not a production-readiness or security-certification claim. Re-check GitHub before relying on any status after the date above.

---

## 1. Product goal and authority model

LOCO Agent is a local-first AI coding assistant for VS Code. The planned migration is incremental and polyglot:

- **TypeScript:** VS Code extension, chat/webview, editor integration, proposal presentation, and user approvals.
- **Python:** model/provider clients, routing, orchestration, retrieval, evaluation, and proposal generation.
- **Rust:** security authority for policy, workspace/filesystem boundaries, approval verification, patch validation/application, execution controls, and audit.
- **SQLite:** local persistence for metadata and approval/audit state where appropriate.

The governing invariant is:

> **Python proposes. Rust authorizes. TypeScript presents. The user approves. Rust enforces.**

Neither Python nor TypeScript may independently perform privileged filesystem or process operations. AI-generated content is untrusted input. Unknown actions and unsupported contract versions must fail closed.

Migration policy:
- Preserve the working TypeScript extension while replacements are developed.
- Keep new functionality behind explicit, testable boundaries.
- Do not remove legacy code until feature parity, security review, and rollback requirements are met.
- Do not merge the dependent PR stack into `main` until the planned implementation, review, and validation gates are complete.

## 2. Current repository and PR state

At the snapshot date, the repository has 62 pull requests:

| PR range | State | Summary |
|---|---|---|
| #1–#5 | Closed, not merged | Early baseline, CI, patch review, streaming, retrieval evaluation work |
| #6–#8 | Open, not draft | Consolidation, retrieval metric correction, workspace path-boundary tests |
| #9–#62 | Open drafts | Architecture/contracts and the incremental Rust security/snapshot work |

There are **57 open PRs (#6–#62)**. The stack is dependent: later PRs use earlier branches as their base. These are not 50 independent changes. No PR in this stack has been merged to `main`.

The current `main` branch remains the original TypeScript VS Code extension baseline. It does not yet contain the polyglot security stack described below. The top of the current development stack is #62, based on #61.

### PR history and delivered scope

#### Initial extension and feature baseline

- **#1 — Documentation accuracy:** corrected README/docs to describe the actual extension behavior and limitations rather than claiming unimplemented semantic/vector RAG, autonomous multi-agent behavior, measured optimization, or production readiness. Documented existing retrieval, routing, provider, patch, and concurrency behavior accurately.
- **#2 — TypeScript CI baseline:** Node 20/22 checks, locked dependency installation, TypeScript compilation, Jest, and ESLint.
- **#3 — Review-first patch flow:** patch preview and explicit modal approval; rejects conflicts, binary/invalid UTF-8/oversized/out-of-workspace/symlink-escape targets; rechecks target bytes; protected backup and same-directory atomic replacement; restore preview/approval. Scope remains one existing text file up to 2 MiB, complete replacement rather than a general multi-file unified-diff engine.
- **#4 — Provider streaming:** incremental OpenAI-compatible SSE and Ollama NDJSON handling, OpenAI/Azure/local providers, bounded HTTP errors, malformed-frame handling, and fallback only before content has been emitted.
- **#5 — Retrieval evaluation baseline:** deterministic Recall@K, Precision@K, and MRR evaluation over a small seeded query set; not an embeddings/vector database implementation.
- **#6 — Feature-stack consolidation:** brings the early feature work together in the active development line.
- **#7 — Retrieval deduplication metrics:** fixes duplicate accounting by normalizing paths and slash/case differences.
- **#8 — Workspace path-boundary tests:** rejects blank roots/candidates and tests lexical workspace path boundaries; lexical checks alone do not prevent filesystem races.

#### Architecture and contracts

- **#9 — Polyglot architecture and threat model:** records component ownership, security assumptions, and migration direction.
- **#10 — Action Protocol v1:** shared JSON schemas and examples for action proposals, security decisions, and audit events.
- **#17 — Canonical action digest v1:** defines a cross-language binary encoding for action digest calculation.
- **#18 — Cross-language contract validation:** aligns DTO camelCase fields, validates schemas/examples, and covers digest parser edge cases.
- **#19 — Security-decision DTO:** adds a typed Rust wire representation.
- **#20 — Audit-event schema validation:** validates event structure and bounded metadata rules.

#### Rust security-core prototypes

- **#11 — Policy prototype:** initial Rust action evaluation.
- **#12 — Workspace path validation:** canonical-path and prospective-target checks with symlink-aware prefix resolution; still subject to TOCTOU limitations.
- **#13 — Bounded read prototype:** regular-file and size checks with a 10 MiB ceiling; path/open races remain.
- **#14 — Rust CI matrix:** Ubuntu, macOS, and Windows test jobs.
- **#15 — Approval digest binding:** binds a supplied challenge digest to an action; at this stage timestamps are caller supplied and not authenticated.
- **#16 — SQLite approval challenge store:** atomic single-use challenge consumption prototype; trusted time and authenticated approval records are not yet established.
- **#21 — Typed audit DTO:** typed audit metadata; not a secret-redaction engine or durable tamper-evident ledger.
- **#22 — Bounded patch proposal validation:** 1 MiB limit, SHA-256 target binding, and narrow unified-diff envelope validation.
- **#23 — Strict patch envelope validation:** one file/one hunk and hunk-count validation; not a general patch-application engine.
- **#24 — Controlled-write design:** threat model and design groundwork, not an implemented secure writer.

#### Snapshot contracts and adapters

- **#25 — Workspace snapshot contract:** metadata contract for a captured workspace file.
- **#26 — Adapter research/design:** platform acquisition research and design.
- **#27 — Snapshot payload validation:** checks byte length, digest, and identity fields.
- **#28 — Snapshot contract constructor:** builds validated snapshot metadata from already-captured bytes.
- **#29 — Adapter interface:** introduces `CapturedWorkspaceSnapshot` and `WorkspaceSnapshotAdapter`.
- **#30 — Validated capture request:** validates relative path and byte limit before adapter calls.
- **#31 — Portable path hardening:** strengthens lexical path rules.
- **#32 — Validated request at adapter boundary:** makes adapters consume validated capture requests.
- **#33 — Linux openat2 design:** defines descriptor-anchored Linux acquisition.
- **#34 — Linux snapshot adapter:** adds the Linux openat2-based read-only capture prototype.
- **#35–#43 — Linux adapter hardening/tests:** incremental tests and controls for limits, symlinks, FIFO/non-regular files, hard links, concurrent mutation, and atomic pathname replacement.
- **#44 — Windows acquisition design:** defines a trusted-root and handle-relative Windows acquisition gate.
- **#45–#48 — Windows portability tests:** progressively harden portable filename validation, including invalid characters and reserved device names (including superscript variants), and review root API constraints.
- **#49 — Shared portable path validator:** centralizes lexical cross-platform path validation. This is not a filesystem containment or race-resistance proof.
- **#50 — Linux root-handle hardening:** acquires and retains the workspace root using openat2 with no-symlink/no-magic-link resolution and checks opened directory identity against canonical path metadata.
- **#51 — Windows trusted-root design gate:** specifies volume-GUID root acquisition, handle-relative traversal, reparse rejection, and unsupported volume behavior.
- **#52 — Windows native root/directory prototype:** adds fixed-volume root acquisition and component-by-component handle-relative directory opens using native Windows APIs. Manual `NtCreateFile` FFI and runtime compatibility require further review.
- **#53 — Windows bounded final-file snapshot adapter:** adds handle-relative final-file acquisition, reparse/type/hard-link checks, bounded reads, pre/post metadata comparison, and shared snapshot digest validation.
- **#54 — macOS handle-relative snapshot adapter:** adds root acquisition from `/` and descendant traversal with descriptor-relative `openat`, `O_NOFOLLOW`, device checks, regular-file/hard-link/size checks, bounded reads, and best-effort metadata comparison.
- **#55 — Linux intermediate-directory swap race test:** repeatedly renames an intermediate directory and replaces its pathname with an outside symlink. Capture must fail closed or return only the in-workspace bytes; the outside sentinel must never be captured. This is test-only and does not change adapter behavior.
- **#56 — macOS intermediate-directory swap race test:** exercises descriptor-relative `openat(O_NOFOLLOW)` acquisition while an intermediate directory is repeatedly replaced with an outside symlink. Successful captures must contain only in-workspace bytes; fail-closed acquisition errors are accepted. Test-only, with no adapter behavior change.
- **#57 — Windows intermediate junction replacement race tests:** verifies the snapshot adapter rejects an intermediate NTFS mount-point junction and races directory replacement with captures. Windows CI passed; the PR remains draft and is not merged.
- **#58 — Windows symlink reparse-point coverage:** adds snapshot-adapter tests for intermediate directory symlinks and final-file symlinks. The tests require symlink creation to succeed rather than silently skipping; the Windows test job passed with the tests executed.
- **#59 — Windows final-file mutation race:** races complete-content writes against snapshot capture. Successful captures must match one of the complete known payloads and validate the shared digest; fail-closed acquisition errors are accepted. The full Ubuntu/macOS/Windows Rust matrix and contract validation passed.
- **#60 — Windows mounted-folder root admission coverage:** verifies that a DOS path to a mounted folder is not accepted as the trusted volume root; only canonical volume-GUID roots are admitted. This is lexical/root-admission coverage, not a test that creates or traverses a real secondary-volume mount point.
- **#61 — Native filesystem API review:** records source-level findings for Linux `openat2`, macOS `openat/O_NOFOLLOW`, and Windows `CreateFileW/NtCreateFile`. Identifies mount-policy inconsistency, Linux exact-read-length hardening, Windows FFI/identity/filesystem admission gates, and native macOS/Windows validation still required. Documentation only; no adapter behavior changed.
- **#62 — Linux exact snapshot read length:** requires captured byte length to equal the initial opened-file size before snapshot construction; adds deterministic helper tests for exact, short, and over-length cases. Does not make the read atomic or resolve mount-crossing policy.


### Review of older open PRs (#6–#8)

These PRs are retained as historical feature work, not approved for merge based on branch cleanliness alone.

- **#6 — Feature-stack consolidation:** useful feature baseline, but its current head's check-runs show only GitGuardian success; the PR body cites earlier Node 20/22 evidence for a different streaming feature head, not the current consolidated head. Manual VS Code Extension Development Host smoke testing is still recorded as pending. Static patch review also shows the extension's TypeScript layer still performs patch application and invokes shell operations. The streaming error path calls `response.text()` before slicing to 500 characters, so the displayed error is truncated but the response body is still fully buffered; add a true bounded reader before treating provider error handling as resource-bounded. These direct TypeScript privileged paths are incompatible with the final authority invariant until the Rust gate is integrated; keep this legacy behavior preserved but do not merge it as the completed migration.
- **#7 — Retrieval metric deduplication:** current head likewise has only GitGuardian check evidence, not a recorded Node/Jest/ESLint run. The deduplication correction is useful, but the path normalization behavior must be specified for case-sensitive versus case-insensitive filesystems. Leading `..` segments are collapsed by the current normalizer instead of being rejected (for example, an initial parent segment can be popped from an empty segment list); reject escaping paths or define an explicit root-aware canonicalization policy. This is a metric/evaluation correctness concern, not a filesystem authorization primitive.
- **#8 — Workspace path-boundary tests:** current head likewise has only GitGuardian check evidence. The helper uses lexical `path.resolve/path.relative` containment and its own comment says symlinks must be resolved by callers. It is a regression guard, not a secure filesystem boundary; the filesystem operation must use the Rust handle-relative authority and race-resistant checks.

**Disposition:** keep #6–#8 open while the migration stack is completed. Before merge, rerun the current heads through Node 20/22 compile, Jest, ESLint and perform the VS Code smoke test. Reassess #6's direct TypeScript privileged paths and reconcile #7's platform-specific path identity semantics. Do not close these PRs as obsolete until their changes are either incorporated into the eventual reviewed stack or deliberately superseded with a documented replacement.

## 3. Current implementation maturity

### What exists

- Original TypeScript VS Code extension and feature baseline.
- Versioned JSON contracts and cross-language digest groundwork.
- Rust policy, approval, audit, and patch-validation prototypes.
- Shared snapshot request/payload contracts and portable lexical path validation.
- Linux read-only workspace snapshot adapter based on `openat2`.
- Windows root/directory and bounded final-file snapshot prototypes.
- macOS descriptor-relative workspace snapshot adapter.
- CI workflows for contracts and Rust core tests, including platform matrix coverage.
- Adversarial tests for several path, symlink, hard-link, file-type, limit, mutation, and replacement scenarios.

### What does not yet exist as an integrated production path

The current Rust core README explicitly describes the policy, approval, audit, and patch modules as prototypes. The repository does not yet have:
- A complete Rust-authorized filesystem read/write execution pipeline.
- A production approval lifecycle with authenticated evidence, trusted time, replay protection, freshness binding, and concurrency-safe enforcement integrated with execution.
- A complete secure controlled-write implementation with backup/restore/recovery and durability semantics.
- A command/process executor that cannot be bypassed by TypeScript or Python.
- A durable tamper-evident audit ledger and separate sensitive-data redaction layer.
- A production IPC/sidecar connecting TypeScript, Python, and Rust.
- The extracted Python AI service and full provider/retrieval/planning implementation.
- End-to-end tests proving proposal → user approval → Rust enforcement → result across all components.
- Production packaging, clean-machine installation, upgrade, rollback, and release-signing workflows.

Snapshot adapters are **read-only prototypes**. Their metadata comparisons are best-effort mutation detection; they do not promise an atomic snapshot of a file being concurrently modified. CI success demonstrates the tests/builds run on those CI environments, not universal OS/filesystem security guarantees.

## 4. Snapshot adapter security boundaries

### Linux

- Workspace root is opened with `openat2` and `NO_SYMLINKS | NO_MAGICLINKS`; the descriptor is retained.
- Capture resolves relative to that descriptor with `BENEATH | NO_SYMLINKS | NO_MAGICLINKS`.
- Opened-handle metadata must describe a regular file with one hard link.
- Reads are bounded by request and global limits.
- Metadata before/after the read is compared; bytes are hashed into the shared snapshot contract.
- Tests cover root replacement, leaf/intermediate symlinks, FIFO, hard links, size limits, metadata mutation, atomic final-path replacement, and intermediate-directory symlink swaps.
- Mount-crossing policy (including whether to use `NO_XDEV`) remains a compatibility/security decision to assess explicitly.

### Windows

- Root acquisition begins from a strict volume-GUID root and admits fixed local volumes only.
- Descendant directories are opened relative to retained handles using native `NtCreateFile` semantics and no-reparse controls.
- Final-file capture is handle-relative and bounded; reparse points, directories/non-regular files, and multiple hard links are rejected.
- Identity and metadata are checked before and after reading.
- Manual FFI structure/calling-convention correctness, `RootDirectory` and `OBJ_DONT_REPARSE` runtime behavior, reparse/mount-point coverage, filesystem support, and broader Windows compatibility still require native review and testing.

### macOS

- Absolute workspace roots are traversed from `/` one component at a time; symlink components are rejected and the final directory handle is retained.
- Descendant directories and final files are opened relative to retained handles with `O_NOFOLLOW`.
- Files must be regular, single-link, within the root device, and within configured byte limits.
- Metadata is compared before/after reading; the shared digest contract is validated.
- Runtime behavior needs validation on supported macOS releases and filesystems, including APFS/HFS+ where supported, mount behavior, and concurrent path mutation.

### Cross-platform caveats

- A descriptor/handle anchors object identity; it does not freeze file contents.
- Digest validation proves consistency between returned bytes and returned metadata, not that the file remained unchanged after capture.
- Pathname races, rename semantics, mount boundaries, filesystem-specific behavior, and API availability must be tested on native platforms.
- No path-based fallback may silently replace a failed secure handle-relative operation.
- A successful CI matrix is necessary but not sufficient for security acceptance.

## 5. Current follow-up task: native API findings and Linux read-length hardening

Native review: [PR #61 — native filesystem API review](https://github.com/Veeresh-11/Loco-Agent/pull/61) (open, draft)  
Linux hardening: [PR #62 — reject short snapshot reads](https://github.com/Veeresh-11/Loco-Agent/pull/62) (open, draft)  
Current top branch: `fix/linux-snapshot-exact-read-length`, stacked on PR #61.

PR #61 records the source-level review findings in [NATIVE_FILESYSTEM_API_REVIEW.md](../security/NATIVE_FILESYSTEM_API_REVIEW.md). PR #62 implements the low-risk Linux finding: the captured byte count must exactly equal the initial opened-handle size before a snapshot is built. It adds deterministic tests for exact, short, and over-length results. This check is additional consistency validation; it does not provide atomic content snapshots.

**Still open:** Linux mount/bind-mount policy (F-01), Windows FFI ABI and supported-architecture gates, Windows real volume-mount-point and filesystem allowlist tests, and native macOS filesystem/mount validation. No production approval/write/execution path is added.

PR #60 Contract Validation passed (run `45178926525`); its Rust run `45178926550` had Ubuntu and Windows success while macOS remained queued at the last check. PR #61 and #62 are documentation/code follow-ups respectively; #62 CI is pending for its exact head.

### PR #63 — Linux strict descendant mount boundary (in progress)

PR #63 is stacked on #62 and chooses the strict F-01 policy: descendant `openat2` resolution sets `RESOLVE_NO_XDEV`, rejecting mount and bind-mount crossings below the retained workspace root. Root acquisition does not set this flag, so a workspace rooted on a separate mounted filesystem remains admissible. This intentionally disallows nested bind-mounted dependency/source trees. A Linux test attempts a real bind mount and asserts fail-closed capture when privileges are available; hosted unprivileged CI may skip that privileged assertion. A privileged Linux VM must still execute bind-mount and distinct-filesystem mount tests before native coverage is claimed.

At this update, PR #62 Contract Validation run `45180875545` passed, while Rust Core Tests run `45180875554` was still pending with no jobs reported. PR #60 Rust run `45178926550` had Ubuntu and Windows pass and macOS queued. Recheck these runs before claiming final status.

## 6. Roadmap and remaining work

Recommended order; keep each item in a coherent, reviewable PR.

### A. Complete platform snapshot adversarial coverage

- Linux: PR #62 adds an exact captured-byte-length check. Still review mount-crossing behavior, kernel/openat2 availability, final/intermediate replacement and concurrent mutation boundaries.
- Windows: test junctions/reparse tags, component replacement, sharing violations, and final-file mutation. Root admission now rejects DOS/mounted-folder paths; a real second-volume mount-point traversal fixture remains outstanding, along with unsupported-filesystem coverage.
- macOS: add intermediate-component replacement coverage; test symlink swaps, mount boundaries, FIFO/non-regular targets, hard links, and concurrent writes on supported filesystems.
- Keep race tests bounded, synchronized, deterministic where possible, and explicit about accepted fail-closed outcomes.

### B. Native API and filesystem review

- Source-level review is recorded in [NATIVE_FILESYSTEM_API_REVIEW.md](../security/NATIVE_FILESYSTEM_API_REVIEW.md) via PR #61.
- Resolve Linux mount/bind-mount policy and add privileged mount tests; add the exact Linux byte-count check.
- Add Windows ABI/layout assertions and architecture gates; complete real mount-point, sharing, identity, and filesystem allowlist tests.
- Validate macOS APFS/HFS+ and mount/firmlink behavior on native supported systems.
- Publish a supported OS/filesystem matrix and list unsupported cases.

### C. Complete approval and authorization

- Bind approvals to exact action, target identity, content digest, workspace, and policy version.
- Use authenticated approval evidence and trusted time.
- Enforce single-use consumption/replay prevention atomically.
- Revalidate target freshness immediately before enforcement.
- Add concurrency, stale-target, tampering, expiry, and crash/recovery tests.

### D. Implement controlled Rust writes

- Acquire target handles safely and revalidate approved identity/digest.
- Create safe temporary files relative to trusted directory handles.
- Define atomic replacement, fsync/durability, permissions/metadata, backup, restore, and crash recovery behavior.
- Ensure every write passes policy and approval enforcement.
- Add adversarial path replacement, symlink/reparse, hard-link, mount, and concurrent mutation tests.
- Do not expose a TypeScript/Python path that writes around Rust.

### E. Complete policy and command execution gates

- Unknown actions deny by default.
- Policy decisions are enforced by Rust, not merely displayed by the UI.
- Add a controlled process lifecycle, argument/environment/working-directory policy, timeouts, cancellation, output limits, and audit.
- Prove that a compromised or malicious proposal cannot bypass enforcement.

### F. Extract the Python AI service

- Move provider clients, streaming, error normalization, context construction, retrieval/ranking, routing, planning, and evaluation into Python.
- AI service output is proposal-only; it cannot authorize or execute privileged actions.
- Add mock-provider contract tests, bounded payloads/timeouts, cancellation, and measured latency/token/cost evaluation.

### G. Build the TypeScript bridge and preserve UX

- Add versioned IPC clients and sidecar/service lifecycle management.
- Handle health, startup/shutdown, timeouts, cancellation, crashes, protocol mismatch, and streaming.
- Preserve VS Code UX and add structured proposal, approval, and result presentation.
- Test proposal → approval → Rust validation/enforcement → result end-to-end.

### H. Adversarial integration, packaging, and release

- Cover prompt injection, path escape, approval tampering/replay, execution bypass, malicious providers, IPC attacks, secret leakage, unsafe writes, resource exhaustion, supply chain, local service misconfiguration, and policy downgrade.
- Package Rust binaries per supported OS/architecture and decide Python runtime distribution.
- Validate install/upgrade/uninstall, permissions, cleanup, rollback, and clean-machine behavior.
- Complete final PR review, close obsolete/superseded branches, and merge the dependency stack in order only after all gates pass.

## 7. Validation and reporting rules

For every follow-up PR, record:
- Branch, base branch, exact commit SHA, and PR URL.
- Files changed and narrowly defined scope.
- Tests actually run and exact CI result (queued/in progress/success/failure).
- Platforms on which tests actually executed.
- Security properties exercised and properties not proven.
- Known limitations and manual/native validation still required.
- Whether the PR is draft/open/merged. Do not describe a draft as accepted or production-ready.

Keep CI claims precise: compilation on a platform is not equivalent to execution of platform-gated tests. Tests gated to Linux, Windows, or macOS only execute on the corresponding runner.

## 8. Merge and release gate

**Do not merge the stacked PRs into `main` yet.** Merge only after:
1. The platform acquisition designs and native implementations have been reviewed.
2. Required CI and native adversarial tests pass on every supported platform/filesystem.
3. Approval, controlled writes, audit, and execution gates are integrated and independently reviewed.
4. Python and TypeScript integration is complete without privileged bypasses.
5. End-to-end and threat-model test suites pass.
6. Packaging, clean-machine installation, rollback, and documentation are complete.
7. The full dependency stack is reviewed and obsolete/superseded PRs are reconciled.

Until then, preserve the existing TypeScript implementation and keep the new stack unmerged.
