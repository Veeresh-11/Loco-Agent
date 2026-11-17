# LOCO Agent

LOCO Agent is a local-first AI coding assistant for Visual Studio Code. It combines workspace-aware context retrieval, task-aware local/cloud provider selection, provider fallback, queued task execution and conflict-aware code patching.

The current implementation is an early-stage VS Code extension (version 0.1.0). Its retrieval and routing mechanisms are intentionally lightweight; see [Implementation scope and limitations](#implementation-scope-and-limitations) before interpreting the feature list as an advanced or production-scale AI platform.

## Implemented capabilities

- **Local and cloud model adapters:** local OpenAI-compatible/Ollama-style endpoints, Ollama, GPT4All, OpenAI-compatible cloud inference and Azure OpenAI.
- **Task-aware provider selection:** routes according to estimated token count, simple task-complexity classification and code-task context.
- **Provider fallback:** retries a second configured provider when the preferred provider errors or returns an empty result.
- **Workspace context retrieval:** discovers supported workspace files, chunks their contents, scores chunks by lexical query-term matches and supplies the top results to the model prompt.
- **Queued task processing:** asynchronous job queue with configurable concurrency (default 2, maximum 16).
- **VS Code chat interface:** sidebar webview chat, typing indicator, task submission and completed-response display.
- **Code patch support:** three-way merge logic with conflict markers and a patch-application method.
- **Local history and secrets:** optional interaction history in VS Code extension global state, history retention controls, and cloud API-key storage using VS Code SecretStorage.

## Provider support

| Provider | Implementation path | Notes |
| --- | --- | --- |
| Local compatible server | `src/providers/localProvider.ts` | Probes configured local ports and tries supported OpenAI-compatible and Ollama-style routes. |
| Ollama | `src/providers/ollamaProvider.ts` | Provider adapter using the local provider implementation. |
| GPT4All | `src/providers/gpt4allProvider.ts` | Provider adapter using the local provider implementation. |
| OpenAI | `src/providers/openaiProvider.ts` and `src/providers/cloudProvider.ts` | Uses the configured OpenAI-compatible chat-completions endpoint. |
| Azure OpenAI | `src/providers/azureProvider.ts` | Uses configured Azure resource endpoint, deployment and API version. |

The Claude provider file is a stub and is not included as a supported live provider.

## Architecture

```text
VS Code commands / sidebar webview
              |
              v
         LOCOAgent
      /      |       \
 JobQueue  RAGEngine  workspace/file context
      \      |       /
       ProviderManager
              |
        RoutingPolicy
              |
   preferred provider -> fallback provider
              |
       completed response
              |
      sidebar / output view
```

### Main modules

| Module | Responsibility |
| --- | --- |
| `src/extension.ts` | Extension activation, command registration, settings and sidebar provider wiring. |
| `src/core/locoAgent.ts` | Task normalization, rule-based planning, context assembly, provider invocation and history. |
| `src/core/ragEngine.ts` | Workspace file indexing, overlapping character chunks, lexical scoring and top-K retrieval. |
| `src/core/routingPolicy.ts` | Rule-based local/cloud selection using complexity and estimated token count. |
| `src/core/jobQueue.ts` | Promise-based task queue with bounded concurrency. |
| `src/providers/providerManager.ts` | Provider construction, selection and fallback orchestration. |
| `src/providers/providerFallback.ts` | Sequential retry of configured providers. |
| `src/core/threeWayMerge.apply.ts` | Three-way text merge and conflict reporting. |
| `src/ui/webview/index.ts` | VS Code sidebar chat webview. |
| `src/utils/secrets.ts` | API key operations backed by VS Code SecretStorage. |
| `src/utils/historyStore.ts` | Local interaction-history management. |

## Requirements

- Visual Studio Code `^1.85.0`
- Node.js and npm compatible with the development toolchain
- A configured local model server, or API credentials for a supported cloud provider

## Installation and development

Clone the repository and install dependencies:

```bash
git clone https://github.com/Veeresh-11/Loco-Agent.git
cd Loco-Agent
npm install
```

Compile the extension:

```bash
npm run compile
```

Run the automated tests:

```bash
npm test
```

Run linting:

```bash
npm run lint
```

Package the VS Code extension:

```bash
npm run package
```

The repository's package scripts currently define compile, test, lint and package tasks. A separate React build task is not defined in the current `package.json`.

## Configuration

The extension contributes the following principal settings:

| Setting | Default | Purpose |
| --- | --- | --- |
| `loco.provider` | `local` | Preferred provider: local, OpenAI, Azure, Ollama or GPT4All. |
| `locoAgent.endpoint` | `https://api.openai.com/v1` | OpenAI-compatible cloud endpoint. |
| `loco.localEndpoint` | empty | Optional local model endpoint; otherwise local ports are probed. |
| `locoAgent.localPorts` | `[9000, 9001, 8080, 7860, 5005]` | Additional local server ports to probe. |
| `locoAgent.model` | `gpt-4o-mini` | Cloud model name. |
| `locoAgent.localModel` | `llama3.1` | Local model name. |
| `locoAgent.maxConcurrency` | `2` | Maximum queued task concurrency (1–16). |
| `locoAgent.storeHistory` | `true` | Enable local interaction history. |
| `locoAgent.pruneOnStartup` | `true` | Prune expired history at startup. |
| `locoAgent.pruneDays` | `30` | History retention window. |
| `locoAgent.maxHistoryEntries` | `1000` | Maximum number of retained entries. |
| `locoAgent.azureEndpoint` | empty | Azure OpenAI resource endpoint. |
| `locoAgent.azureDeployment` | empty | Azure OpenAI deployment name. |
| `locoAgent.azureApiVersion` | `2024-10-21` | Azure OpenAI API version. |

Cloud API keys can be configured through the extension commands and are stored using VS Code SecretStorage. Environment variables `OPENAI_API_KEY` and `AZURE_OPENAI_API_KEY` are also read by the respective provider implementations.

## Commands

- **LOCO: Run Agent** — submit a coding task using the active editor context.
- **LOCO: Open Assistant** — open the LOCO sidebar.
- **LOCO: Open Settings** — open extension settings.
- **LOCO: Set Local Endpoint** — configure a local model endpoint.
- **LOCO: Clear Local Endpoint** — clear the explicit local endpoint.
- **LOCO: Set Cloud API Key** — store an OpenAI or Azure API key.
- **LOCO: Clear Cloud API Key** — remove a stored cloud API key.
- **LOCO: View History** — view locally stored interactions.
- **LOCO: Export History** — export history to JSON.
- **LOCO: Clear History** — clear stored interactions.

## Tests

The repository includes Jest tests for:

- Local endpoint normalization and route selection.
- OpenAI-compatible response parsing.
- Provider fallback behavior.
- Rule-based provider routing.
- Job queue result handling and concurrency.
- Three-way merge behavior, including conflict cases.

Run `npm test` to execute the test suite. Test presence is not a substitute for a verified current CI run; check the Actions tab for the status associated with the commit you intend to ship.

## Implementation scope and limitations

The following distinctions are important for users, contributors and reviewers:

- **RAG is lexical, not embedding-based.** The current engine indexes up to 300 workspace files, creates overlapping character-based chunks and scores them by normalized query-term frequency and path matches. The inspected implementation does not contain embedding generation, vector database integration, hybrid sparse/dense search or a reranker.
- **Planning is rule-based.** The agent constructs a simple plan from task keywords and then makes a model request with workspace context. It does not currently implement a closed-loop autonomous planner that repeatedly observes tool results and replans.
- **Streaming is not incremental yet.** The current `askStream()` obtains a completed response and invokes the callback once; the local provider's `stream()` behaves similarly. The sidebar shows a typing indicator while awaiting the completed response.
- **Routing is heuristic.** Provider selection uses estimated token count and task classification. The inspected routing code does not measure provider latency, calculate actual API cost, or benchmark model quality.
- **Provider fallback is sequential.** The manager tries a preferred provider and then a fallback; this is not a distributed inference scheduler.
- **Model evaluation and MLOps are outside the current implementation.** No model training pipeline, experiment tracking, model registry, automated model evaluation suite or inference-monitoring pipeline was found in the inspected source.
- **Code patch application requires review.** The merge helper reports conflicts, but callers should inspect the resulting file and conflict markers before accepting changes. Do not enable unattended patch application for untrusted output.
- **The current project should be treated as an early-stage developer tool.** Validate security, reliability, model-provider behavior and release packaging in the target VS Code environment before production deployment.

## Security and privacy notes

- Cloud API keys are accessed through VS Code SecretStorage.
- Interaction history is stored in VS Code extension global state when enabled and can be pruned, exported or cleared.
- Prompts, selected/current file content and retrieved workspace snippets are included in requests sent to the configured model provider. Review the provider's data handling terms and avoid sending sensitive source code to an unapproved endpoint.
- Local inference requires a local model server to be running and reachable.
- Review AI-generated code and shell-related suggestions before executing or applying them.

## License

MIT. See [LICENSE](LICENSE).
