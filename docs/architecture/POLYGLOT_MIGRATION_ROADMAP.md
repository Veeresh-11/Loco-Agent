# LOCO Agent Polyglot Migration Roadmap

**Status:** Proposed  
**Principle:** Incremental migration with working checkpoints; preserve existing implementation until replacement parity is demonstrated.

## Phase 0 — Baseline stabilization

**Scope**
- Resolve dependency installation and Jest module-resolution issues.
- Fix lint defects without suppressing rules broadly.
- Establish compile, unit-test, lint, packaging, and Extension Development Host baseline.
- Determine whether generated `out/` files are tracked and define build-artifact policy.
- Record Node/npm/TypeScript toolchain and dependency audit findings.

**Exit criteria**
- Reproducible clean install.
- Tests execute (not zero suites) and results are recorded.
- Compile, lint, and packaging have known outcomes.
- Baseline commit and unresolved defects documented.

## Phase 1 — Architecture contracts and design review (this proposal)

**Scope**
- Language ownership boundaries.
- Threat model and security invariants.
- Versioned action/event/error contracts.
- IPC design and deployment options.
- Migration and rollback strategy.

**Exit criteria**
- Review of architecture proposal.
- Contract format chosen (JSON Schema or equivalent typed schema source).
- Security review of authority and bypass model.
- Python runtime packaging and supported platforms decision.

## Phase 2 — Rust security-core prototype

**Initial scope**
1. Versioned action envelope and typed action model.
2. Workspace/path validation with symlink-aware behavior.
3. Policy engine with deny/allow/approval-required outcomes.
4. Approval binding to exact action and content digest.
5. Patch validation and safe controlled-write interface.
6. Audit event structure with sensitive-data minimization.

**Deliverables**
- Rust crate(s) and test suite.
- Threat-driven test fixtures.
- Sidecar protocol prototype.
- Minimal integration harness independent of VS Code.

**Exit criteria**
- Unit tests and adversarial tests pass.
- Unknown actions fail closed.
- Outside-workspace and symlink escapes rejected.
- Stale or altered approvals rejected.
- No privileged operation bypasses the core in prototype.
- Cross-platform behavior assessed.

## Phase 3 — Python AI service extraction

**Scope**
- Provider adapters and API clients.
- Streaming and error normalization.
- Context construction and retrieval.
- Routing and planning.
- Evaluation metrics and benchmark harness.

**Exit criteria**
- Provider contract tests.
- Deterministic tests using mock providers.
- Measured latency/token/cost metrics (not heuristic claims).
- Bounded request/response sizes and timeouts.
- AI output produces validated proposals only.

## Phase 4 — TypeScript bridge and UI preservation

**Scope**
- Preserve extension UX and VS Code API integration.
- Introduce typed bridge clients.
- Handle component health, cancellation, timeout, errors, and protocol mismatch.
- Render structured proposals and approval states.

**Exit criteria**
- UI behavior parity checklist.
- Component lifecycle and crash handling.
- Integration tests for proposal → approval → Rust validation → result.
- Manual Extension Development Host smoke test.

## Phase 5 — Security integration and adversarial testing

**Scope**
- Prompt injection, path escape, approval tampering, execution bypass, provider abuse, IPC attacks, secret leakage, unsafe writes, resource exhaustion, supply chain, policy downgrade.

**Exit criteria**
- Threat model test suite passes.
- Platform-specific filesystem tests run.
- CI covers contracts, Rust, Python, TypeScript, and integration layers.
- No known critical bypass.
- Fail-closed behavior is demonstrated.

## Phase 6 — Packaging and distribution

**Scope**
- Package Rust binaries per OS/architecture.
- Decide Python distribution strategy (bundled runtime, managed environment, optional dependency, or separate local service).
- Install, upgrade, uninstall, cleanup, permissions, and rollback.
- Supply-chain controls and release signing where feasible.

**Exit criteria**
- Clean-machine installation tested.
- Supported platforms documented.
- No undocumented external runtime prerequisites.
- Upgrade/rollback and service lifecycle verified.

## Phase 7 — Gradual retirement of legacy components

**Scope**
- Replace legacy module only when new module passes parity and security gates.
- Maintain a migration map from old responsibility to new component.
- Remove redundant code in small, reviewable PRs.

**Exit criteria**
- Feature parity or intentional product change documented.
- Old implementation has no active callers.
- Regression suite passes.
- Rollback path no longer required and removal approved.

## PR strategy

Prefer one PR per coherent milestone. Keep PRs small, avoid mixing refactors with behavior changes, and avoid destructive replacement before evidence.

Suggested sequence:
1. Architecture proposal and threat model (documentation only).
2. Contract definitions and validation tooling.
3. Rust security core: pure validators/policy.
4. Rust patch and approval controls.
5. Rust sidecar protocol.
6. Python AI service scaffold and provider tests.
7. TypeScript bridge integration.
8. Cross-component adversarial suite.
9. Packaging and release hardening.

## Rollback

- Preserve the legacy extension until migration is proven.
- New services are feature-gated and can be disabled.
- Keep contracts backward-compatible where practical; version breaking changes.
- Do not remove old modules until the new path is default, verified, and reversible.
- Migration PRs are not merged automatically; review and explicit approval are required.

## Progress tracking

For each milestone, record:
- PR/branch and exact commit.
- Scope changed.
- Tests actually run and results.
- Platform(s) verified.
- Security threats addressed and remaining risks.
- Manual checks still pending.
- Rollback steps.
