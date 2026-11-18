# LOCO Agent Contracts v1.0

These schemas define the first proposed cross-component message shapes for the TypeScript extension, Python AI service, and Rust security core.

## Contract files

- `action-proposal.schema.json`: AI-generated request for a possible action. **Untrusted input; never an authorization.**
- `security-decision.schema.json`: security-core response: `deny`, `approval_required`, or `allow`. The client must correlate it to the request and verify freshness. Example: `examples/security-decision.json`; Rust wire DTO: `core/loco-core/src/security_decision_contract.rs`.
- `audit-event.schema.json`: minimized security and execution metadata; details are limited to 32 scalar fields. Example: `examples/audit-event.json`, validated by `contracts/tests/test_audit_event_schema.py`. Do not put credentials, full prompts, or full file contents in audit details.

## Flow

1. Python creates an action proposal and sends it to the extension.
2. The extension displays it and obtains user confirmation where required by the policy.
3. Rust independently validates proposal, workspace scope, policy, approval evidence, and current target state.
4. Rust returns a security decision.
5. Only an allowed, validated action reaches the controlled execution primitive.
6. Result is reported and a minimized audit event is recorded.

## Action types in v1

- `read_file`: scoped read request; Rust still enforces workspace policy and size limits.
- `propose_patch`: proposed content change; Rust validates target, current-state digest (when supplied/required), patch size, and approval binding before applying through controlled write logic.
- `run_command`: process proposal. Policy should default to approval-required or deny; no schema-valid command is automatically trusted.

## Important security notes

- JSON Schema validation is necessary but not sufficient. Paths must be resolved and validated against the actual filesystem by Rust at the point of use.
- The `workspaceRelativePath` pattern is only an early syntactic filter. It does not protect against symlinks, alternate platform path semantics, or filesystem races.
- The decision message is not proof of authorization by itself. The implementation must correlate request IDs, validate the action digest, and use protected state to prevent replay.
- The `allow` decision must be narrowly scoped and time-limited; approval must not survive mutation of action parameters.
- Unknown action types must fail closed.
- This version is a design contract and is not yet wired into runtime code.
- Schema `format` fields require a validator configured to check formats, not merely accept the schema.

## Compatibility

`contractVersion` is required and currently fixed at `1.0`. Breaking changes should introduce a new version or documented compatibility policy. Do not silently ignore unknown security-relevant fields.

## Examples

See `examples/action-proposal.read-file.json` and `examples/action-proposal.patch.json`.
