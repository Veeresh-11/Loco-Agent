# LOCO Agent

LOCO Agent is an early-stage, local-first AI coding assistant for Visual Studio Code. It brings together local and cloud language-model adapters, lightweight workspace context retrieval, heuristic provider selection, sequential fallback, a concurrency-limited task queue and a sidebar chat interface.

The project is designed to help developers ask questions about the active workspace and get coding assistance without being locked to one inference provider. It is not currently a fully autonomous multi-agent system, a semantic-search platform or a production-hardened coding agent.

## At a glance

- **Editor:** Visual Studio Code extension
- **Runtime:** TypeScript / Node.js extension host
- **Chat UI:** React-based VS Code webview
- **Inference:** Local compatible endpoints, OpenAI-compatible cloud endpoints and Azure OpenAI
- **Workspace context:** File discovery, overlapping character chunks and lexical relevance scoring
- **Tests:** Jest
- **Release state:** Early-stage project; validate in your target VS Code environment before relying on it for production work

## Implemented capabilities

- **Multiple inference routes:** Local OpenAI-compatible and Ollama-style HTTP routes, OpenAI-compatible cloud inference, and Azure OpenAI.
- **Provider selection:** A rule-based policy uses an approximate prompt token count, a simple task-complexity label and code-task detection to choose local or cloud inference.
- **Sequential fallback:** If the selected provider errors or returns an empty response, the manager attempts a fallback provider.
- **Workspace context:** Searches supported files, splits text into overlapping character windows, scores windows by query-term occurrence and path matches, and adds the highest-scoring snippets to the prompt.
- **Task queue:** Runs asynchronous tasks with configurable concurrency (default 2; clamped to 1–16).
- **VS Code interface:** Provides a sidebar chat view and editor command for submitting tasks.
- **Patch helper:** Includes three-way text merge and a method that writes the merged content to the target file, reporting conflict markers.
- **Local state:** Supports optional interaction history with retention, export and clear operations.
- **Secret storage:** Stores OpenAI and Azure API keys using VS Code SecretStorage.

## Architecture

```text
VS Code command / sidebar webview
                |
                v
            LOCOAgent
       /        |          \
  JobQueue   RAGEngine   Workspace/file context
       \        |          /
         Prompt composition
                |
                v
          ProviderManager
                |
          RoutingPolicy
                |
      Preferred provider attempt
                |
       Sequential fallback
                |
                v
        Completed response
                |
          Chat / output view
```

### Main modules

| Module | Responsibility |
| --- | --- |
| `src/extension.ts` | Activation, command registration, history pruning and sidebar registration. |
| `src/core/locoAgent.ts` | Task normalization, simple plan construction, context assembly, provider invocation and history updates. |
| `src/core/ragEngine.ts` | Workspace file discovery, chunking, cache and lexical retrieval. |
| `src/core/routingPolicy.ts` | Rule-based local/cloud route selection. |
| `src/core/jobQueue.ts` | Promise-based queue with configurable concurrent task execution. |
| `src/providers/providerManager.ts` | Provider selection, adapter invocation and fallback orchestration. |
| `src/providers/providerFallback.ts` | Sequential provider attempts and failure collection. |
| `src/providers/localProvider.ts` | Local endpoint discovery and OpenAI-compatible/Ollama-style request routes. |
| `src/providers/cloudProvider.ts` | OpenAI-compatible cloud chat-completions requests. |
| `src/providers/azureProvider.ts` | Azure OpenAI chat-completions requests. |
| `src/core/threeWayMerge.apply.ts` | Three-way text merge and conflict reporting. |
| `src/ui/webview/` | Sidebar chat webview. |
| `src/utils/secrets.ts` | API-key operations through VS Code SecretStorage. |
| `src/utils/historyStore.ts` | Local interaction-history management. |

## Requirements

- Visual Studio Code version `^1.85.0`
- Node.js and npm compatible with the repository toolchain
- Either a reachable local model server or credentials for a supported cloud provider

## Installation and development

```bash
git clone https://github.com/Veeresh-11/Loco-Agent.git
cd Loco-Agent
npm ci
npm run compile
```

Run the tests and lint checks:

```bash
npm test
npm run lint
```

Package a local VS Code extension artifact:

```bash
npm run package
```

The package scripts currently define `compile`, `test`, `lint` and `package`. There is no separate `build:webview` script in the current `package.json`; do not rely on older setup instructions that mention it.

To try the extension locally, open the repository in VS Code, press `F5` to launch an Extension Development Host, and test the commands and sidebar there. This manual smoke test is separate from the Jest suite.

## First-run setup

1. Install dependencies and compile the project using the commands above.
2. Open the project in VS Code and launch the Extension Development Host with `F5`.
3. Open **LOCO: Open Assistant** or use **LOCO: Run Agent**.
4. Select a provider and configure its endpoint/model credentials as described below.
5. Submit a small, non-sensitive task first. Check the LOCO output for provider errors before using it on a larger workspace.

## Provider configuration

### Local model server (default)

Set `loco.provider` to `local`. LOCO probes a configured endpoint and common localhost ports. The built-in probe list includes ports `9000`, `9001`, `8080`, `7860`, `5005`, `11434`, `8000` and `4891`; values in `locoAgent.localPorts` are added to that list.

For a local server with an explicit endpoint, set `loco.localEndpoint` (for example, an Ollama server may be reachable at `http://localhost:11434`). The local adapter probes compatible model/health routes and tries OpenAI-compatible chat completions, Ollama chat and Ollama generate request formats. The model name is controlled by `locoAgent.localModel` (default: `llama3.1`).

Use **LOCO: Set Local Endpoint** to save an endpoint, or **LOCO: Clear Local Endpoint** to return to automatic port discovery.

### OpenAI-compatible cloud provider

Set `loco.provider` to `openai`. The default endpoint is `https://api.openai.com/v1`; `locoAgent.endpoint` can point to another compatible endpoint. Set the API key with **LOCO: Set Cloud API Key** and choose `openai`, or provide `OPENAI_API_KEY` in the extension host environment. The model is configured by `locoAgent.model` (default: `gpt-4o-mini`).

### Azure OpenAI

Set `loco.provider` to `azure`, then configure:

- `locoAgent.azureEndpoint`: Azure resource endpoint
- `locoAgent.azureDeployment`: deployed model name
- `locoAgent.azureApiVersion`: API version (default: `2024-10-21`)

Store the Azure key with **LOCO: Set Cloud API Key** and choose `azure`, or provide `AZURE_OPENAI_API_KEY` in the extension host environment.

### Provider adapters

| Provider choice | Current behavior |
| --- | --- |
| `local` | Discovers a local endpoint and tries compatible local HTTP routes. |
| `ollama` | Uses the local provider implementation and Ollama-compatible routes. |
| `gpt4all` | Uses the local provider implementation; it is not a separate inference runtime implementation. |
| `openai` | Uses the configured OpenAI-compatible cloud endpoint. |
| `azure` | Uses the configured Azure resource, deployment and API version. |

The Claude adapter is a stub and is not a supported operational provider. Do not configure or advertise it as working functionality.

## Settings reference

| Setting | Default | Purpose |
| --- | --- | --- |
| `loco.provider` | `local` | Preferred provider: `local`, `openai`, `azure`, `ollama` or `gpt4all`. |
| `locoAgent.endpoint` | `https://api.openai.com/v1` | OpenAI-compatible cloud endpoint. |
| `loco.localEndpoint` | empty | Explicit local server endpoint; otherwise local ports are probed. |
| `locoAgent.localPorts` | `[9000, 9001, 8080, 7860, 5005]` | Additional local ports to probe (built-in ports are also checked). |
| `locoAgent.model` | `gpt-4o-mini` | Cloud model name. |
| `locoAgent.localModel` | `llama3.1` | Model name sent to local endpoints. |
| `locoAgent.maxConcurrency` | `2` | Concurrent tasks (clamped to 1–16). |
| `locoAgent.storeHistory` | `true` | Enable local interaction history. |
| `locoAgent.pruneOnStartup` | `true` | Prune expired history at activation. |
| `locoAgent.pruneDays` | `30` | History retention window in days. |
| `locoAgent.maxHistoryEntries` | `1000` | Maximum retained history entries. |
| `locoAgent.azureEndpoint` | empty | Azure OpenAI resource endpoint. |
| `locoAgent.azureDeployment` | empty | Azure OpenAI deployment. |
| `locoAgent.azureApiVersion` | `2024-10-21` | Azure OpenAI API version. |

The extension also retains `locoAgent.provider` and `locoAgent.localEndpoint` as deprecated configuration aliases for compatibility. Prefer `loco.provider` and `loco.localEndpoint`.

## Commands

- **LOCO: Run Agent** — submit a task using the active editor context.
- **LOCO: Open Assistant** — open the LOCO sidebar.
- **LOCO: Open Settings** — open extension settings.
- **LOCO: Set Local Endpoint** — configure a local model endpoint.
- **LOCO: Clear Local Endpoint** — clear the explicit local endpoint.
- **LOCO: Set Cloud API Key** — securely store an OpenAI or Azure key.
- **LOCO: Clear Cloud API Key** — remove a stored cloud key.
- **LOCO: View History** — view locally stored interactions.
- **LOCO: Export History** — export history to JSON.
- **LOCO: Clear History** — clear stored interactions.

## How routing works

Routing is deliberately heuristic rather than benchmark-driven:

- Task complexity is inferred from prompt keywords.
- The token estimate is based on prompt character length; it is not provider-reported token usage.
- The current policy prefers local inference for smaller tasks and can choose cloud inference for complex tasks or larger estimated prompts.
- A code task with a sufficiently large estimated prompt can be classified for cloud routing.
- If the preferred route fails or returns an empty response, a fallback is attempted sequentially.

The implementation does not currently measure actual latency, API cost, model quality or task success to make routing decisions. Treat the thresholds as implementation rules, not evidence of optimal performance.

## Workspace retrieval and context

The current retrieval engine is lexical:

- It searches up to 300 files with supported extensions.
- It excludes common generated/dependency folders such as `node_modules`, `.git`, `out`, `dist` and `coverage`.
- Files larger than 1 MB, unreadable files, empty files and files containing null bytes are skipped.
- Text is split into 900-character chunks with 180-character overlap.
- Chunks are scored using normalized query-term occurrences and path matches; the top five are added to the prompt.
- The index is cached for 30 seconds.

This is not embedding-based semantic search. The current implementation does not include a vector database, hybrid retrieval, reranking or a retrieval evaluation benchmark.

## Tests and validation

The Jest suite contains tests for local endpoint behavior, response parsing, provider fallback, routing rules, queue behavior and three-way merge cases.

Run:

```bash
npm ci
npm run compile
npm test
npm run lint
```

These commands are the local validation baseline, not a claim that the current PR has passed CI. Check the GitHub Actions results for the exact head commit before merging. Also launch the extension in a clean VS Code Extension Development Host and manually verify local and cloud provider setup; unit tests do not prove that live model endpoints work.

## Security, privacy and safe use

- OpenAI and Azure API keys are stored through VS Code SecretStorage when configured with the extension command. Environment variables are also supported by the provider implementations.
- Interaction history is stored in VS Code extension global state when enabled. It can be pruned, viewed, exported or cleared.
- The task prompt sent to a provider can include retrieved workspace snippets, active-file content and Git workspace status. Treat this as source-code disclosure to the configured inference endpoint.
- Use only model endpoints approved for the code and data you are working with. Do not submit secrets, credentials, regulated data or proprietary source to an unapproved provider.
- The current shell helper only permits two read-only Git commands: `git rev-parse --show-toplevel` and `git status --short --branch`. It is not a general-purpose command execution tool.
- The three-way patch application method writes merged content to the target file. It does not provide a transactional backup or an approval dialog in that method. Review the diff, preserve version control state and resolve conflict markers before relying on the resulting file. Never apply untrusted model output to important files without review.
- Review generated code and any suggested shell commands before use.

## Current limitations and roadmap

Not currently implemented or verified:

- Embedding-based semantic RAG, vector storage, hybrid search or reranking.
- Closed-loop autonomous planning or multi-agent orchestration.
- Incremental token streaming; the current stream-style methods deliver the completed response in one callback.
- Measured latency/cost optimization or model-quality-aware routing.
- Model training, experiment tracking, model registry, MLOps or automated model evaluation.
- End-to-end tests against live provider APIs, retrieval-quality benchmarks, extension-host smoke tests in CI, performance/load testing or independent security assessment.
- A production-readiness or security-certification claim.

Potential next engineering steps are to add provider HTTP integration tests with mocked responses, retrieval evaluation fixtures, VS Code extension-host smoke tests, genuine incremental streaming, explicit patch preview/approval and safer file-write/backup behavior. These should be implemented and validated before being described as delivered capabilities.

## Contributing

1. Open an issue describing the bug or proposed change.
2. Keep changes focused and update documentation when behavior changes.
3. Add or update tests for changed behavior.
4. Run `npm ci`, `npm run compile`, `npm test` and `npm run lint`.
5. Include the test results and any manual verification in the pull request.

## License

MIT. See [LICENSE](LICENSE).
