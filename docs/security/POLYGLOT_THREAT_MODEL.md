# LOCO Agent Polyglot Threat Model (Proposal)

**Status:** Proposed  
**Version:** 0.1  
**Scope:** Threats and controls for the proposed TypeScript/Python/Rust architecture.

## Assets

- Workspace source files and metadata.
- User credentials, API keys, local model endpoints, and tokens.
- User approval decisions.
- Security policy and capability configuration.
- Task state, history, and retrieval index.
- Patch content and backups.
- Integrity of the LOCO Agent executable components.
- Availability and correctness of the extension.

## Trust boundaries

1. **User ↔ VS Code UI:** UI events and settings may be malformed or stale.
2. **Extension ↔ AI layer:** AI service may fail, be compromised, or return maliciously crafted output.
3. **AI ↔ workspace data:** Source files, READMEs, comments, issues, retrieved documents, and instructions are untrusted and may contain prompt injection.
4. **Extension/AI ↔ Rust core:** Requests are untrusted even when they originate from sibling components.
5. **Rust core ↔ OS/filesystem/processes:** OS behavior, filesystem races, symlinks, permissions, and process outcomes must be considered.
6. **LOCO ↔ external providers:** Remote services, network responses, and endpoint configuration may be untrusted.

## Threats and required controls

### T1 — Prompt injection causes unauthorized action
**Threat:** Workspace content or provider output instructs the agent to exfiltrate data, execute commands, or change unrelated files.  
**Controls:** Treat model output as proposals; tool allowlists/capabilities; Rust authorization; explicit approvals; no secrets in prompt context by default.  
**Tests:** Prompt-injection fixtures attempting outside-workspace reads, secret discovery, command execution, and approval bypass.

### T2 — Path traversal or workspace escape
**Threat:** Malicious or malformed paths use parent traversal, sibling-prefix tricks, absolute paths, alternate separators, dot segments, or symlinks to escape the workspace.  
**Controls:** Canonicalize and validate at the point of use; handle symlinks; reject ambiguous/empty paths; validate the final resolved target; use safe write primitives and minimize validation-to-use races.  
**Tests:** Traversal variants, sibling-prefix paths, root itself, relative paths, empty inputs, symlink escape, and race-oriented integration tests. Lexical containment alone is not sufficient.

### T3 — Approval tampering or stale approval
**Threat:** An approved action is changed after review, approval is replayed, or approval for one target is reused for another.  
**Controls:** Bind approval to canonical action parameters, target, content digest, identity, scope, and expiry; Rust verifies binding and freshness; approval is single-use where appropriate.  
**Tests:** Mutate command, path, patch content, target, and expiry after approval; replay approval; concurrent approval use.

### T4 — Execution bypass
**Threat:** TypeScript or Python invokes a shell/file-write route that bypasses the security core.  
**Controls:** Centralize privileged operations; inventory and remove bypass paths; Rust owns execution authorization; static checks and integration tests enforce the boundary.  
**Tests:** Attempt direct execution from each component; assert denial or absence of capability.

### T5 — Compromised or malicious provider
**Threat:** Provider returns malformed data, oversized responses, misleading tool calls, unsafe patches, or credential-leak instructions.  
**Controls:** Validate schema, limit response size, timeout/cancel, redact logs, and pass all actions through Rust policy.  
**Tests:** Malformed JSON, oversized payload, partial streams, error responses, malicious tool proposals, and timeout.

### T6 — IPC spoofing, injection, or resource exhaustion
**Threat:** Local IPC messages are forged, malformed, replayed, oversized, or flood services.  
**Controls:** Prefer stdio IPC for the Rust sidecar where practical; versioned schemas, bounded messages, request correlation, timeouts, cancellation, process ownership, and no unauthenticated exposed listener.  
**Tests:** Wrong protocol version, malformed framing, unexpected IDs, oversized messages, request flooding, service crash/restart.

### T7 — Secret leakage
**Threat:** Credentials or sensitive workspace content leak into logs, telemetry, prompts, crash dumps, or persistent history.  
**Controls:** VS Code SecretStorage/OS credential store, redaction, data minimization, opt-in diagnostics, retention controls, and secrets scanning.  
**Tests:** Known sentinel secrets must not appear in logs, error messages, telemetry, or persisted output.

### T8 — Unsafe patch or filesystem mutation
**Threat:** File is modified unexpectedly, patch conflicts, target changes during review, write partially completes, or rollback is unsafe.  
**Controls:** Preview diffs; approval; reread and compare target state; bounded files; atomic writes where supported; backups; reject conflicts; Rust-controlled write path. Consider filesystem race limitations explicitly.  
**Tests:** Dirty target, stale target, patch conflict, interrupted write, permission error, backup corruption, symlink swap, rollback.

### T9 — Denial of service / resource exhaustion
**Threat:** Unbounded queue, huge files, excessive concurrency, model loops, or provider stream consumes memory/CPU.  
**Controls:** Bounded queues, concurrency limits, file/message limits, timeouts, cancellation, retry budgets, and per-task resource ceilings.  
**Tests:** Queue saturation, oversized files/messages, stalled stream, runaway task, repeated provider failures.

### T10 — Dependency or supply-chain compromise
**Threat:** Malicious, vulnerable, or compromised packages/build artifacts enter the extension or sidecar.  
**Controls:** Lockfiles; reproducible installs; dependency audit; minimized dependency sets; provenance where feasible; pinned toolchain; CI scanning; review of updates.  
**Tests:** CI dependency checks, build provenance checks where available, license and vulnerability policy.

### T11 — Local service misconfiguration
**Threat:** AI service or local model endpoint points to unintended remote host, or service is impersonated.  
**Controls:** Explicit endpoint settings, safe defaults, loopback restrictions for local service modes, endpoint validation, visible connection status, and careful handling of redirects.  
**Tests:** Unexpected remote host, redirects, invalid schemes, unavailable endpoints, and spoofed service responses.

### T12 — Policy downgrade or fail-open behavior
**Threat:** Policy/core unavailable, version mismatch, or error causes operation to proceed with weaker checks.  
**Controls:** Fail closed for privileged actions; compatibility/version negotiation; deny on unknown action types; no silent downgrade.  
**Tests:** Core offline, malformed policy, unknown action, incompatible protocol, timeout, and unexpected exception.

## Security invariants

- No model-generated action executes without security-core authorization.
- No privileged action can be performed through a bypass path.
- Unknown action types are denied.
- Failed security checks never fall back to permissive behavior.
- Approval is specific, fresh, and verifiable.
- Workspace boundaries account for filesystem resolution and symlinks.
- Sensitive data is not logged by default.
- Limits and cancellation apply to every untrusted input path.
- The system can explain why an action was denied without leaking secrets.

## Residual risks

Rust does not automatically provide sandboxing. A Rust process can still contain logic flaws, use unsafe code, call dangerous OS APIs, or suffer TOCTOU races. Security claims must be based on tested controls, architecture, OS enforcement, and review—not language choice alone.

## Validation strategy

- Unit tests for each policy rule and validator.
- Integration tests across TypeScript, Python, and Rust boundaries.
- Adversarial tests for all threats above.
- Platform-specific filesystem tests (Windows, macOS, Linux).
- Dependency and supply-chain checks.
- Manual VS Code Extension Development Host smoke test.
- Security review of privileged interfaces before release.
