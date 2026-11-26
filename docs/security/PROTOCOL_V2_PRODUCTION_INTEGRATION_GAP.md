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

## Source-level legacy operation inventory (verified 2026-11-26)

The following inventory was checked against the extension and client source at this assessment revision. It identifies current TypeScript paths that are outside the Rust v2 authority client; it is an inventory, not an assertion that every path is reachable from the same UI command.

| Source | Observed operation | Current boundary | Required disposition before claiming Rust enforcement |
|---|---|---|---|
| `src/core/ragEngine.ts` | `workspace.findFiles`, `workspace.fs.stat`, and `workspace.fs.readFile` over up to 300 workspace files (1 MB per-file limit), then caches/chunks text in TypeScript | Direct workspace enumeration and content acquisition in the extension process; no Rust handle or snapshot binding | Replace with a Rust-owned acquisition/consumption design; prove task-used bytes are the validated Rust-acquired bytes |
| `src/core/locoAgent.ts` — `runTask` / `readFile` | Task flow retrieves RAG context and independently reads `job.filePath` using `workspace.fs.readFile` | Direct TypeScript read is included in provider prompt | Remove this bypass only after a Rust content-consumption interface and end-to-end identity/freshness tests exist |
| `src/core/locoAgent.ts` — `applyPatch` | Reads target file, computes three-way merge, and writes merged bytes with `workspace.fs.writeFile` | Direct workspace write in TypeScript; no Rust authorization or approval transport | Keep disabled/unreachable from production task flow until Rust write enforcement and authenticated approval are implemented and reviewed |
| `src/core/locoAgent.ts` — `executeShell` | Uses `child_process.execFile` for two hard-coded Git commands: `rev-parse --show-toplevel` and `status --short --branch`, with a 5-second timeout | Bounded read-only Git metadata command; not routed through Rust | Explicitly classify as non-privileged metadata or move under a reviewed Rust contract; do not generalize this allowlist into arbitrary command authority |
| `src/extension.ts` — run command | Passes active editor language, filesystem path, and selected/full document text into `LOCOAgent.runTask` | Editor text is acquired by VS Code before the agent task; no Rust snapshot binding | Treat editor-buffer context as a separate acquisition source; define how unsaved buffers are represented and bound before asserting snapshot consistency |
| `src/runners/powershellRunner.ts` | Invokes `powershell.exe` via `execFile` after a modal user confirmation; 20,000-character script cap and 30-second timeout | Direct process execution in TypeScript. Source search did not establish an in-repository call site in the inspected code | Do not claim it is behind Rust approval; audit reachability/callers separately and keep arbitrary command execution disabled in the production task path |

Additional classification:
- `src/extension.ts` exports history to a path explicitly selected through the VS Code save dialog. This is a user-directed history export, not evidence of workspace patch authority.
- `src/utils/historyStore.ts` stores history in VS Code `globalState`; this is extension state, not a workspace file operation.
- The current Rust v2 capture API returns an acknowledgement and byte length only. It does not return task-consumable content, so it cannot yet replace the RAG or `readFile` paths above.
- This inventory is scoped to the files and symbols listed; it is not a complete repository-wide static analysis or proof that no other filesystem/process path exists.

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
- TypeScript/Rust portable-path parity and protocol error-envelope validation: draft PR #138 at corrected head `852c844ec06e79fb8ca50d54501f2f94474b8a38`; TypeScript CI run [#45820432496](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45820432496) completed successfully on Node 20 and Node 22. The current suite includes shared `WORKSPACE_NOT_FOUND`/`CAPTURE_DENIED` error fixtures, unknown-handle and post-revocation capture-denial behavior, strict nested error-envelope validation, and six malformed-envelope fail-closed cases. This remains fake-child client validation, not packaged activation-to-sidecar integration.
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
