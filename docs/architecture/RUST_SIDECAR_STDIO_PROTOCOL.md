# Rust Sidecar stdio Protocol

**Status:** V1 contract plus Rust child-side prototype; TypeScript parent integration pending  
**Date:** 2026-11-24  
**Dependency:** PR #91 local storage and IPC security boundary

## Purpose and process topology

The first Rust integration should be a child process spawned and owned by the VS Code extension host. The parent launches a packaged Rust executable directly (never through a shell), supplies stdin/stdout pipes, and owns its lifetime. Python and model providers remain proposal producers; they do not connect to the Rust authority directly in v1.

This is a control-plane protocol for proposal evaluation only. The current Rust crate explicitly does not execute commands, read/write workspace files, or establish authorization for OS operations. A `decision: allow` is not an execution capability. No file bytes, patches, commands, credentials, or approval assertions are returned as executable authority.

## Framing and streams

- Use UTF-8 newline-delimited JSON (NDJSON): exactly one JSON object per line, terminated by LF. The receiver may accept CRLF by removing one trailing CR before parsing.
- Maximum encoded message size is 12 MiB including the newline. Reject an over-limit line before JSON parsing; do not buffer without a bound.
- A message must be a single JSON object. Reject blank lines, invalid UTF-8, duplicate/unknown fields (schema validation), malformed JSON, and trailing non-whitespace bytes.
- stdout is protocol-only. The Rust process must never print logs, compiler diagnostics, panic text, or progress output to stdout. Diagnostics go to stderr and must redact secrets and workspace content.
- Parent and child each maintain one reader and one serialized writer per pipe. Responses echo the requestId. No unsolicited messages are permitted.
- This is a single-parent/single-child, one-request-at-a-time v1 protocol. Do not pipeline or multiplex until bounded concurrency and cancellation semantics are designed and tested.

## Handshake and state machine

1. On launch, child waits for the first complete message and starts no work.
2. Parent sends `hello` with protocolVersion `1.0`, requestId, and fixed client name plus extension version.
3. Child responds with `hello` and supported capability list. V1 advertises only `proposal_evaluation`.
4. Parent must verify protocol version, expected server name, version presence, and requested capability before sending `evaluate`.
5. Child accepts evaluation only after successful handshake. It validates the envelope and nested ActionProposal schema, evaluates using Rust policy, and returns a `decision` with the same requestId.
6. Parent rejects mismatched/duplicate response IDs and unexpected response types. It never interprets a decision as permission to perform a privileged operation.
7. Parent sends `shutdown`; child replies `shutdown` with accepted=true, flushes stdout, and exits. EOF, broken pipe, malformed frame, handshake timeout, or parent termination initiates bounded shutdown and cleanup.

A protocol-version mismatch, malformed/oversized message, duplicate request ID, unknown message type, or request before handshake fails closed. No fallback to a local HTTP endpoint or direct TypeScript execution path is permitted.

## Message types

The machine-readable contract is `contracts/sidecar-stdio.schema.json`.

| Direction | Type | Purpose |
|---|---|---|
| Parent → child | `hello` | Negotiate protocol and identify the expected parent application/version |
| Child → parent | `hello` | Advertise implementation version and allowed v1 capability |
| Parent → child | `evaluate` | Submit one untrusted ActionProposal |
| Child → parent | `decision` | Return the Rust policy decision, correlated by requestId |
| Either direction | `error` | Bounded protocol/application error; no secrets or stack traces |
| Parent → child | `shutdown` | Request graceful process termination |
| Child → parent | `shutdown` | Acknowledge graceful termination |

## Resource and lifecycle limits

- Frame maximum: 12 MiB; nested action limits remain those in the ActionProposal schema (patch 1 MiB, command args bounded, read maxBytes 10 MiB).
- Request IDs: 1–128 ASCII characters from letters, digits, period, underscore, colon, and hyphen. IDs are correlation values, not secrets or authentication tokens.
- At most one outstanding request in v1. Parent applies a 5-second handshake timeout and a bounded per-request timeout; a timeout kills/reaps the child rather than assuming the operation succeeded.
- Parent owns child PID/handle, cancellation, restart budget, and disposal. Restart must create a new child and repeat handshake. Do not replay an in-flight request after crash unless a future idempotency contract explicitly permits it.
- Child exits nonzero on fatal protocol failure; parent treats any unexpected exit as unavailable and fails closed.
- stdout EOF, stderr growth, and child exit must all be handled without deadlock. Parent drains stderr with a strict bounded ring buffer and redacts before diagnostics.
- No secrets in command-line arguments or environment variables. Pass only required inherited pipe handles and minimal non-secret configuration.

## Trust and security properties

- Inherited pipes reduce endpoint discovery and socket-squatting risks but do not authenticate a human. A compromised parent controls the pipe; a compromised child controls its responses.
- The client name/version fields are self-asserted metadata, not peer authentication.
- Every proposal and every field remains untrusted. Rust validates the protocol and proposal schema before policy evaluation.
- The child never trusts client-supplied PID, UID, executable path, approval state, timestamps, or policy configuration as authority.
- A TypeScript UI click is not authenticated approval. A future authenticator assertion must be independently verified and consumed by Rust under the separate approval trust-boundary design.
- No production privileged write or command execution is reachable through this v1 protocol.

## Required implementation tests

- Handshake success, unsupported version, wrong server name, missing capability, and evaluate-before-handshake.
- Valid proposal/decision correlation; mismatched and duplicate IDs; unknown message types and extra fields.
- Invalid UTF-8, malformed JSON, blank lines, multiple JSON values on one line, CRLF, EOF mid-frame, and exact frame boundary.
- Over-limit frame rejected before JSON allocation; oversized nested fields and arrays rejected by schema.
- stdout remains parseable while stderr receives diagnostics; panic/error never leaks onto stdout.
- Parent cancellation, child crash, timeout, broken pipe, restart, shutdown acknowledgement, and child reaping.
- Fuzz framing and JSON parser boundaries; test slowloris/partial writes and bounded stderr draining.

## Implementation exit criteria

1. Confirm minimum supported OS/version and packaged binary delivery for all three platforms.
2. Implement a Rust binary target and bounded NDJSON reader/writer with no stdout logging.
3. Implement parent spawn/pipe lifecycle in TypeScript and make handshake mandatory.
4. Validate both envelope and nested proposal against the shared contracts; add TS/Rust/Python contract vectors.
5. Demonstrate cancellation, crash recovery, restart budget, and fail-closed behavior in integration tests.
6. Keep privileged operations disabled until authenticated approval, IPC threat review, controlled-write implementation, and end-to-end security review are complete.

This contract deliberately exposes proposal evaluation only. It is not a stable public API or a production authorization boundary.


## Rust child-side implementation checkpoint

The Rust binary target `loco-authority` now implements the child side of this v1 protocol:

- Performs the initial hello handshake and advertises only `proposal_evaluation`.
- Accepts sequential evaluate requests and returns the existing Rust `SecurityDecision` DTO, correlated by requestId.
- Accepts graceful shutdown and acknowledges before exiting.
- Reads at most 12 MiB plus one byte per frame, rejects unterminated/oversized/blank/non-UTF-8/invalid JSON frames, and accepts CRLF.
- Writes protocol messages only to stdout; diagnostics go to stderr.
- Rejects duplicate/invalid request IDs, unknown message fields, unsupported protocol versions, and evaluate-before-handshake. The proposal DTO now rejects unknown top-level/action fields during deserialization.
- Caps process lifetime at 4,096 request identifiers/requests to bound replay-tracking memory.

Rust integration tests cover handshake/evaluate/shutdown, correlation, evaluate-before-handshake, unknown proposal fields, duplicate IDs, and oversized frames.

The child process cannot enforce a handshake timeout while blocked on stdin; the parent must enforce handshake and request deadlines and terminate/reap the child. EOF exits cleanly. Malformed frames and fatal protocol violations exit nonzero with stderr-only diagnostics. The TypeScript process launcher, packaging of per-platform binaries, parent-side framing, restart budget, and end-to-end cancellation/crash tests are not implemented yet. Therefore this is not yet an integrated sidecar service and cannot be used to authorize privileged operations.


## TypeScript parent-side client checkpoint

The TypeScript extension now has an isolated `RustSidecarClient` prototype in `src/core/rustSidecarClient.ts`:

- Spawns the resolved executable directly with `shell: false` and stdin/stdout/stderr pipes; the caller must resolve the executable from the extension's packaged resources, not workspace or provider input.
- Requires the v1 hello handshake and checks server identity, version, and `proposal_evaluation` capability before evaluating proposals.
- Serializes requests, permits one outstanding request, correlates responses by requestId, and rejects invalid UTF-8/JSON, invalid envelopes, unexpected IDs, blank frames, and frames over 12 MiB.
- Enforces handshake, evaluation, and shutdown deadlines; terminates an unresponsive or failed child and uses a bounded three-start lifetime budget with exponential delay for later caller-initiated starts.
- Retains only a 16 KiB stderr tail and never forwards child stderr to the UI or logs.
- Does not replay a request after a timeout/crash. A returned `allow` remains a policy result only, not an execution or write capability.

This checkpoint is transport code only. It is not yet wired into the production agent task path, and packaged per-platform binaries are not yet delivered. The legacy TypeScript extension remains the active path. The current prototype tests exercise handshake/correlation, wrong IDs, oversized frames, and single-request behavior; the full crash/cancellation/restart/packaging test matrix remains an exit criterion. The client must not be treated as an authorization boundary.


The TypeScript client now accepts an optional `AbortSignal` for evaluation. Cancellation during handshake or evaluation rejects the operation and terminates the child; the request is never replayed. Tests also cover unexpected child exit. This does not yet provide transparent automatic restart: a later caller may retry within the client's bounded three-start lifetime budget.
