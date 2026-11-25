# Protocol v2 production integration gap assessment

**Assessment date:** 2026-11-25  
**Scope:** Rust authority protocol v2, its TypeScript client, and the current VS Code extension task path.

## Executive finding

The v2 protocol implementation and client are useful, separately tested prototypes, but they are not yet the enforcement boundary for normal LOCO tasks. The extension activation path does not construct a `RustSidecarClient`, the agent task path does not register a workspace with Rust, and current file/shell/patch operations do not flow through the Rust authority.

Do not describe the sidecar as production-integrated and do not enable privileged operations on the strength of the current protocol tests.

## Source inspection

The following repository paths were inspected on 2026-11-25:

| Area | Observed implementation | Integration consequence |
|---|---|---|
| Extension activation | `src/extension.ts` constructs `LOCOAgent` and the sidebar provider; it does not construct or own a Rust sidecar client | No activation-scoped sidecar lifecycle or workspace registration exists |
| TypeScript client | `src/core/rustSidecarClient.ts` (protocol v2 client branch) requires an injected `resolveExecutable` function | No default packaged-binary resolver is supplied by the extension |
| Extension package | `package.json` has compile/test/package scripts and no Rust binary packaging or platform artifact declaration | The inspected package configuration does not establish a distributable native authority executable |
| Rust sidecar | `core/loco-core/src/bin/loco-authority.rs` and v2 handler implement stdio protocol, registration, capture acknowledgement, revoke and shutdown | Native protocol behavior exists, but this does not prove the packaged extension launches it |
| Agent task path | `src/core/locoAgent.ts` performs file reads through `vscode.workspace.fs`, runs bounded Git status commands with `execFile`, and contains a direct patch-write method | The current task path bypasses the Rust authority for these operations |
| Capture response | The v2 client returns only `{ captured: true, byteLength }`; file bytes and trusted snapshot metadata are not exposed to TypeScript | The current capture API cannot replace the agent's text/file-context reads without a separately designed Rust-owned consumption path |

## Required integration sequence

### 1. Native executable delivery and launch contract

- Define supported OS/architecture targets and reproducible Rust build jobs.
- Package the correct `loco-authority` executable with the extension for each supported target.
- Resolve only the extension-packaged executable; never resolve from workspace content, PATH, or user-controlled settings.
- Verify executable presence, architecture, integrity/provenance, permissions, and startup failure behavior.
- Add package-level tests proving the selected artifact is the packaged one and that missing/invalid artifacts fail closed.

**Exit condition:** a cleanly packaged extension can launch the matching native executable on every claimed platform, with artifact provenance and integrity checks.

### 2. Activation-scoped authority lifecycle

- Construct one sidecar client under extension activation and register disposal under `ExtensionContext.subscriptions`.
- Define bounded startup, restart, shutdown, and error reporting behavior.
- Register workspace roots only from an explicit VS Code workspace selection event/state; never infer a root from a proposal path.
- Keep opaque handles in the authority owner, revoke them when folders are removed or the extension deactivates, and handle multi-root workspaces explicitly.
- Do not log selected absolute paths, captured file contents, opaque handles, or raw child stderr.

**Exit condition:** tests cover activation, multiple folders, folder removal, sidecar crash/restart, shutdown, and retained-handle release.

### 3. Rust-owned read consumption before task-path migration

The current v2 capture response is an acknowledgement, not a content transport. Do not add a TypeScript fallback that reads the same path after a Rust capture acknowledgement: that would create a second, raceable acquisition path and would not bind the bytes consumed by the agent to the Rust snapshot.

Before replacing `RAGEngine` or `LOCOAgent.readFile` access, design a Rust-owned consumption boundary. Options must preserve the invariant that the bytes used by the task are the bytes acquired and validated by Rust. Any IPC content delivery must be bounded, framed, authenticated to the live sidecar session, and covered by snapshot identity/freshness tests. Alternatively, keep sensitive content processing inside Rust and return only a deliberately minimized result.

**Exit condition:** an end-to-end test proves that task context is derived from the exact Rust-acquired content and cannot be substituted by a post-capture TypeScript path read.

### 4. Remove or constrain legacy bypasses

- Inventory every direct filesystem read/write and process-launch path in the extension, including RAG indexing, file context, patch application, and command execution.
- Route each supported operation through an explicit Rust-authorized contract before claiming Rust is the enforcing authority.
- Until the corresponding Rust operation and approval gate exist, keep writes, patch application, and command execution disabled in the production task path; do not treat policy `allow` as permission to execute.
- Add regression tests that fail if TypeScript directly reaches a privileged API outside an explicitly documented, non-privileged adapter.

**Exit condition:** a source-level operation inventory and tests demonstrate there is no unreviewed TypeScript bypass for each operation claimed to be Rust-enforced.

### 5. Cross-layer conformance and release gate

- Share protocol fixtures between Rust and TypeScript for valid/invalid frames, DTOs, errors, limits, request IDs, and lifecycle transitions.
- Test the real extension activation-to-sidecar path on each supported OS, not only fake-child client tests and isolated Rust handler tests.
- Verify cancellation, timeouts, malformed UTF-8/JSON, oversized frames, duplicate IDs, sidecar crash, workspace revocation, and shutdown cleanup end to end.
- Require independent security review before enabling approval, writes, patch application, or command execution.

**Exit condition:** exact-head cross-platform CI and an independent review cover the integrated packaged workflow.

## Current status snapshot

- Rust v2 handler/session implementation: draft PR #135; recorded exact-head Rust and contract CI passed.
- TypeScript v2 client: draft PR #136; recorded exact-head TypeScript CI passed.
- Direct adversarial Rust handler tests: draft PR #137; recorded exact-head nine-platform Rust matrix and contract validation passed.
- TypeScript/Rust portable-path parity: draft PR #138; recorded exact-head TypeScript CI passed.
- Production extension integration, packaged native executable, Rust-owned task-content consumption, legacy bypass removal, authenticated approval, and independent review: **not complete**.

## Security invariants retained

- Fail closed when native authority is unavailable or the protocol is invalid.
- Rust owns native adapters and retained workspace-root handles.
- A workspace handle is process-local and opaque; it is not a durable authorization token.
- Capture acknowledgement and snapshot digest are not proof of human approval.
- Policy `allow` is not execution authority.
- Snapshot acquisition and the bytes consumed by a task must be the same trusted acquisition, not two independent path reads.
- No workspace writes, patch application, or command execution until their Rust enforcement and approval gates are complete.
- Same-user tampering of local SQLite state remains a documented limitation; independent security review is required.
