# TypeScript Privileged-Operation Inventory and Migration Contract

**Status:** Design inventory; no runtime behavior changed  
**Baseline:** `feat/typescript-rust-sidecar-client` at `d30c70c5f485b2905aed78dd16bb593af3089d8b`  
**Purpose:** Identify current TypeScript paths that access workspace data or mutate files, and define the conditions for moving those operations behind the Rust authority boundary.

## Boundary statement

The Rust sidecar currently evaluates a bounded `ActionProposal` and returns a policy decision. Its `allow` result is not an execution capability. The current TypeScript extension remains the active implementation and still performs some workspace operations itself. Therefore the sidecar is not yet the enforcement point for the live task path, and the project must not describe the migration as complete or secure-by-default.

This document is an inventory and migration contract, not a claim that the existing paths have been removed or secured.

## Current TypeScript operation inventory

| Location | Current operation | Current mechanism | Security significance | Migration target |
|---|---|---|---|---|
| `src/core/locoAgent.ts:readFile` | Read caller-selected file | `vscode.workspace.fs.readFile(Uri.file(filePath))` | Accepts a filesystem path; this method itself does not establish workspace containment, symlink policy, file-size bound, or descriptor identity | Rust-owned bounded workspace-read proposal and operation |
| `src/core/locoAgent.ts:runTask` / `src/core/ragEngine.ts:indexWorkspace` | Gather retrieval context | `workspace.findFiles(..., 300)`, then `workspace.fs.stat/readFile` for each file | TypeScript enumerates up to 300 matching files; each file is capped at 1 MB, but there is no aggregate-byte or total-chunk cap. Absolute filesystem paths are stored in chunk IDs and metadata. The 30-second cache is process-local and does not establish target identity. | Inventory RAG file acquisition and move trusted workspace traversal into Rust or a separately constrained read-only adapter with aggregate budgets and relative-path identifiers |
| `src/core/locoAgent.ts:executeShell` | Read Git repository status/root | `execFile('git', args, { cwd, ... })` with two exact allowlisted argument arrays | No shell interpolation, but TypeScript owns process creation and chooses workspace cwd; command output is untrusted input | Rust-owned fixed Git metadata capability, or remove the process dependency and use a non-process metadata source |
| `src/core/locoAgent.ts:applyPatch` | Write merged patch to a file | `vscode.workspace.fs.writeFile(Uri.file(filePath), ...)` | Direct write bypasses Rust; no Rust revalidation, atomic-write guarantee, or authorization binding | Rust-owned patch validation and controlled-write operation, bound to target identity and content digest |
| `src/ui/webview/index.ts` | Provide editor text to task | VS Code document APIs | Text is user/workspace data sent to the provider; not itself a privileged filesystem operation | Explicit context disclosure policy, size bounds, and user-visible scope; no authority implied |
| `src/utils/historyStore.ts` | Persist chat history | VS Code `globalState` | Extension-local state, not workspace file mutation; sensitive-data retention concern | Keep separate from workspace authority; define retention/redaction and storage policy |

This inventory is based on the current implementation inspected at the baseline commit. It is not exhaustive for transitive filesystem access: `RAGEngine`, provider adapters, and any future helper must be reviewed before the Rust boundary can be called complete.

## RAG-specific findings\n\nThe current `RAGEngine` excludes `node_modules`, `.git`, `out`, `dist`, and `coverage` by glob, and considers a fixed list of source/text extensions. The 300-result enumeration limit and 1 MB per-file limit are useful local bounds, but the combined read volume can still approach hundreds of megabytes; chunk count and aggregate retained text are not bounded. The code reads after a separate stat call, so the stat result is not a stable opened-handle identity. The chunk cache stores absolute `file.fsPath` values. These are implementation observations, not a claim that the glob or VS Code filesystem abstraction is itself a security boundary.\n\n## Required action classes

The first integration contract should distinguish these operations rather than treating all requests as a generic "tool":

1. **WorkspaceRead** — bounded read of one workspace-relative target. Inputs include canonical workspace identity, relative path, maximum bytes, and expected target identity/version where available. Rust resolves and opens the target under the workspace policy and returns bounded bytes or a structured denial.
2. **WorkspaceSnapshot** — bounded retrieval/index acquisition. Must specify traversal depth, entry count, aggregate bytes, ignored paths, symlink policy, mount/volume policy, and cancellation. No unrestricted recursive walk.
3. **GitMetadataRead** — fixed metadata query only. No arbitrary executable, arguments, environment, config injection, hooks, or caller-provided cwd. If a safe fixed implementation cannot be established, remove this capability rather than broadening it.
4. **PatchProposal** — untrusted patch bytes plus a target-relative path and expected base digest/version. Proposal evaluation does not write.
5. **ControlledWrite** — distinct privileged operation, available only after a future authenticated approval design and final target/content revalidation. The write must be atomic or explicitly report platform limitations; backup/recovery and audit semantics must be specified.

Unknown action classes and unknown fields must fail closed. A policy decision for one action class must not be reused as authority for another.

## Mandatory request/response properties

- Versioned schema with strict unknown-field rejection.
- Bounded UTF-8 framing and payload sizes at every layer.
- Workspace identity is established by the trusted host, not supplied as an arbitrary path by a provider.
- Relative paths are normalized and validated by Rust at point of use; lexical prefix checks are insufficient.
- Rust derives the target identity from opened handles and revalidates immediately before any write.
- Request IDs correlate transport only; they are not authorization tokens.
- Cancellation, timeout, process crash, protocol mismatch, and missing sidecar all fail closed for any future privileged operation.
- No automatic replay after timeout or crash.
- No silent fallback from an explicitly enabled Rust-authority path to direct TypeScript filesystem or process APIs.
- Provider/model output remains untrusted proposal content.
- Logs contain bounded metadata and reason codes, not file contents, secrets, or raw patch payloads by default.

## Migration sequence and exit gates

### Gate A — complete the inventory
- Trace all direct and transitive filesystem and process APIs across TypeScript and Python.
- Include RAG indexing, workspace search, patch helpers, extension commands, and dependencies.
- Add static checks or tests that detect newly introduced direct privileged paths.

### Gate B — define and validate contracts
- Add typed action schemas and shared cross-language test vectors.
- Validate size limits, path forms, unknown fields, cancellation, and error mapping.
- Keep `proposal_evaluation` explicitly separate from execution capabilities.

### Gate C — implement Rust-owned read capabilities
- Start with a narrowly bounded read-only workspace API.
- Add platform adversarial tests for symlinks, mount boundaries, races, oversized files, and outside-workspace paths.
- Integrate only behind an explicit experimental setting; do not change the default path.

### Gate D — controlled writes
- Do not implement or enable production writes until the authenticator and human-approval trust boundary has been selected and independently reviewed.
- Bind approval to the exact canonical action and content digest.
- Revalidate target identity/content and policy immediately before the write.
- Add atomicity, backup, recovery, audit, and failure-injection tests.

### Gate E — retire bypasses
- Demonstrate that every active read/write/process route is either removed or constrained by the approved boundary.
- Run static and integration bypass tests.
- Prove parity and rollback before retiring legacy code.

## Explicit non-goals

This checkpoint does not:
- wire the current sidecar client into `LOCOAgent`;
- grant `allow` the meaning of authorization to read, write, or execute;
- implement Rust workspace reads, Git process execution, or controlled writes;
- claim that VS Code filesystem APIs are inherently unsafe;
- select a cross-platform user authenticator;
- change the active extension behavior or default settings.

## Current residual risks

- `readFile`, RAG acquisition, and `applyPatch` are still TypeScript-owned.
- `executeShell` is an allowlisted, shell-free Git subprocess, but remains outside Rust enforcement.
- The current sidecar only evaluates proposals and is not integrated into the task path.
- No packaged per-platform Rust executable or trusted executable resolver exists.
- Approval evidence remains a prototype and does not prove user presence.
- The legacy implementation remains active until parity, adversarial tests, and rollback are demonstrated.
