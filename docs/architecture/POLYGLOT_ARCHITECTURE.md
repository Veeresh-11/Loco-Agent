# LOCO Agent Polyglot Architecture (Proposal)

**Status:** Proposed  
**Version:** 0.1  
**Scope:** Architecture direction for gradual migration; no implementation changes in this document PR.

## Goal

Evolve LOCO Agent into a local-first AI coding agent with separated responsibilities and enforceable security boundaries. Use each language where it provides the clearest engineering advantage, without rewriting the extension all at once.

## Guiding principles

1. **Security enforcement is independent of AI output.** Model output is untrusted input.
2. **Local-first.** Prefer local providers and local processing when configured and available; cloud use is explicit and configurable.
3. **User control.** File changes and commands require policy checks and appropriate user approval.
4. **Small, versioned contracts.** Components communicate through documented schemas; avoid shared mutable state and undocumented coupling.
5. **Incremental migration.** Preserve the current product while new components are built, tested, and integrated behind interfaces.
6. **Fail closed for security decisions.** If policy, validation, or authorization cannot be completed, privileged actions are denied.
7. **Observable behavior.** Record decisions, failures, and execution outcomes without logging secrets or unnecessary sensitive content.

## Target architecture

### TypeScript — VS Code presentation and integration layer

Responsibilities:
- Extension lifecycle, commands, activity-bar/sidebar, webview, editor APIs, selections, and workspace context.
- Display model streaming, task status, proposed actions, diffs, approvals, and results.
- Manage VS Code SecretStorage for provider credentials and user-facing settings.
- Bridge requests to AI orchestration and security core using typed, versioned contracts.
- Present user approval UX; approval UI does not itself grant authority.

Non-goals:
- Do not make final security authorization decisions.
- Do not execute arbitrary model-proposed commands without security-core authorization.
- Do not bypass security-core policy on errors or timeouts.

### Python — AI and intelligence layer

Responsibilities:
- Provider adapters and protocol compatibility (OpenAI-compatible APIs, Azure, local services).
- Prompt/context assembly, retrieval, embeddings, ranking, planning, and task decomposition.
- Model selection/routing and configurable fallback.
- Evaluation, quality metrics, token/latency/cost measurement, and experimentation.
- Produce structured **proposals** for file edits, commands, and other actions.

Trust model:
- Treat provider responses, retrieved documents, workspace content, and model-generated plans as untrusted.
- Python can propose an action but cannot grant itself permission or invoke privileged execution outside the security-core interface.
- Keep provider credentials in the approved credential store; avoid passing secrets through logs, schemas, or persisted prompts.

### Rust — security core and privileged action gate

Responsibilities:
- Policy evaluation (allow, deny, require approval).
- Workspace root and path validation, including symlink-aware checks where applicable.
- Action schema validation and capability checks.
- Approval binding to a specific action (including relevant parameters, target paths, and content hashes) and rejection of altered/stale approvals.
- Patch validation and controlled file-write primitives.
- Command/execution policy and process lifecycle controls.
- Security event/audit records and integrity-sensitive state.
- Secure defaults and fail-closed behavior.

Rust is the authoritative enforcement point for privileged operations. TypeScript and Python must not provide alternate paths that bypass it.

### Storage — SQLite initially, abstractions where justified

Responsibilities:
- Durable local metadata for tasks, conversation metadata, retrieval metadata, evaluation results, usage metrics, and audit references.
- Schema migrations and transactional updates.

Security:
- Store only what is needed.
- Keep provider secrets in VS Code SecretStorage or an OS credential store—not ordinary database tables.
- Define retention, deletion, and sensitive-data redaction behavior.
- Avoid logging full source files or prompts by default.

### Optional Python-native ML dependencies

Use Python packages for embeddings, ranking, evaluation, and ML workflows where justified. Keep heavy or optional dependencies isolated and do not require them for basic extension activation or core safe file operations.

## Component communication

Use versioned request/response/event contracts with explicit validation.

Initial preferred integration:
- TypeScript ↔ Rust: local sidecar via JSON-RPC over stdio (evaluate native bindings only if performance or deployment evidence warrants them).
- TypeScript ↔ Python: local service with a documented API and lifecycle/health handling.
- Python ↔ Rust: structured proposals sent through the security-core interface; Rust independently validates every action.

All IPC must handle timeouts, malformed messages, version mismatch, service termination, cancellation, and bounded message sizes. Avoid unauthenticated network listeners. If a local network endpoint is unavoidable, bind to loopback and add authentication and anti-replay controls.

## Action lifecycle

1. User submits a task in the VS Code UI.
2. TypeScript collects permitted context and submits a request to AI orchestration.
3. Python returns a structured proposal (not an executable authorization).
4. TypeScript displays a proposal and, where required, obtains explicit user approval for the exact action.
5. The action and approval evidence are passed to Rust.
6. Rust validates schema, workspace boundary, policy, capabilities, approval binding, and current state.
7. Rust rejects, requires approval, or authorizes a narrowly scoped operation.
8. Authorized operation is executed using controlled primitives.
9. Result and security-relevant metadata are returned; user-facing UI displays the outcome.

## Structured action envelope (conceptual)

Every action should include:
- Contract version
- Action identifier and type
- Task/session identifier
- Workspace identity
- Target(s) and normalized path(s), if any
- Proposed parameters/content
- Content digest for change-sensitive actions
- Required capabilities
- Approval reference or approval state (never trust a client-supplied boolean alone)
- Expiry / freshness data where applicable

The Rust core must derive and validate security-critical fields itself rather than trust claims from Python or TypeScript.

## Security boundary rules

- AI output is never executable authority.
- No arbitrary file access outside an approved workspace scope.
- No writes without policy checks and appropriate approval.
- Approval is bound to the exact operation and becomes invalid if relevant parameters/content change.
- Denial and validation failures do not trigger fallback bypasses.
- Tool permissions are least-privilege and scoped.
- External provider/network access follows explicit provider configuration and policy.
- Secrets are redacted from logs and diagnostics.
- Resource limits apply to message sizes, file sizes, task queueing, and execution duration.
- Security-critical operations must be auditable without collecting excessive user content.

## Deployment shape

Initially support development-friendly local components:
- VS Code extension package (TypeScript).
- Rust core binary/sidecar, packaged per supported OS/architecture.
- Optional Python runtime/service or bundled environment for AI features.

The packaging strategy (bundled Python, managed environment, optional runtime download, or separate service) is an open design decision and must be resolved with platform/installation constraints before implementation.

## Non-goals for the initial migration

- Rewrite all UI or user workflows.
- Train or fine-tune models.
- Build a multi-agent swarm.
- Require a remote cloud backend.
- Claim complete sandboxing or security certification from language choice alone.

## Decision gates

A component migrates only when:
- Its contract and ownership are documented.
- Legacy behavior has tests or a clear acceptance checklist.
- New implementation passes unit and integration tests.
- Security checks include adversarial/negative cases.
- Performance, packaging, and failure behavior are assessed.
- Rollback to the previous component is available.
