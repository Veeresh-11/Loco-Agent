# LOCO Agent — Engineering Notes

This document describes behavior verified in the current source tree. Update it when implementation changes; do not describe planned work as shipped functionality.

## Current execution path

1. `LOCOAgent.runTask()` normalizes the task and infers a simple complexity label from prompt keywords. The default token estimate is based on prompt character length, not tokenizer output or provider usage.
2. The task is submitted to `JobQueue`. The configured concurrency is read from `locoAgent.maxConcurrency` and clamped to 1–16. The queue limits active handlers; it does not currently impose a maximum pending-queue length.
3. `RAGEngine` searches up to 300 supported workspace files, skips files larger than 1 MB and common generated/dependency folders, and creates 900-character chunks with 180-character overlap.
4. Retrieval normalizes the query and chunk text, scores query-term occurrences and path matches, and returns the top five matching chunks. The index cache lifetime is 30 seconds.
5. `LOCOAgent` composes a prompt with the task, simple keyword-derived plan, retrieved snippets, active file content when available, and Git workspace root/status.
6. `ProviderManager` chooses a route using `RoutingPolicy` for the `local` and `cloud` preferences; explicit provider selections such as `openai`, `azure`, `ollama` and `gpt4all` select their corresponding adapters.
7. `runWithProviderFallback()` tries the preferred provider and then a fallback sequentially. Empty responses and thrown errors are recorded as failures.
8. The completed response is stored in local history when history storage is enabled and returned to the command or sidebar UI.

## Provider behavior

- Local endpoint discovery checks an explicit endpoint and configured ports, plus built-in ports `9000`, `9001`, `8080`, `7860`, `5005`, `11434`, `8000` and `4891`.
- Local requests try OpenAI-compatible chat completions, Ollama chat and Ollama generate routes.
- Ollama and GPT4All adapters reuse the local provider implementation; they are not independent inference engines in this repository.
- OpenAI-compatible cloud requests use `locoAgent.endpoint`, the configured model, and an API key from VS Code SecretStorage or `OPENAI_API_KEY`.
- Azure OpenAI requests use the configured endpoint, deployment, API version and a key from SecretStorage or `AZURE_OPENAI_API_KEY`.
- The Claude adapter is a stub and must not be represented as an operational provider.

## Routing behavior

- Complexity inference is keyword-based (`refactor`, `rewrite`, `architecture`, `debug`, `root cause`, `migration`, `optimize` produce a complex label; a smaller set of implementation keywords produce medium).
- Estimated token count defaults to approximately one token per four prompt characters. It is not an actual tokenizer count and does not include an independent measurement of the fully assembled prompt.
- The routing policy uses fixed thresholds and task flags; it does not measure live latency, provider cost, quality or success rate.
- Provider fallback is sequential and does not race providers or dynamically benchmark them.

## Safety-relevant behavior

- The shell helper permits only `git rev-parse --show-toplevel` and `git status --short --branch`, with a five-second timeout. It is not a general shell execution facility.
- `applyPatch()` reads the target file, computes a three-way merge and writes the merged text back to that file. Conflict markers may be written as part of the merged result. This method does not itself create a backup, show a preview or request user confirmation. Callers must not treat it as a safe transactional patch workflow.
- Prompts sent to a model provider may contain active-file text, retrieved workspace snippets and Git status information. Users should only send code to endpoints approved for that data.
- API keys are stored through VS Code SecretStorage when entered through the extension commands. Environment-variable fallbacks are supported.

## Current test coverage

Visible Jest tests cover endpoint normalization/routing, OpenAI-compatible response parsing, provider fallback, simple routing thresholds, job queue concurrency/result handling and three-way merge behavior. They do not constitute:

- End-to-end tests against live model-provider APIs.
- Retrieval precision/recall or groundedness benchmarks.
- VS Code extension-host or webview browser smoke tests.
- Performance/load tests or measured latency/cost evaluation.
- Independent penetration testing or a security certification.

Run locally before submitting:

```bash
npm ci
npm run compile
npm test
npm run lint
```

The current package scripts do not define a separate webview bundle build. Launch the extension in a clean VS Code Extension Development Host (F5) and manually validate commands, sidebar behavior, local endpoint discovery, cloud credentials and package installation. Check Actions for the exact commit SHA before merging.

## Known engineering follow-up

1. Add mocked HTTP integration tests for local, OpenAI-compatible and Azure provider request/response/error paths.
2. Add retrieval fixtures and measurable retrieval-quality tests before considering embeddings or vector storage.
3. Add VS Code extension-host smoke tests and webview interaction tests.
4. Implement true incremental provider streaming and a tested webview chunk protocol if streaming is a product requirement.
5. Add explicit patch preview, user approval, workspace-boundary validation, backup/rollback and conflict-resolution UX before broader patch automation.
6. Add cancellation, queue capacity/backpressure and observable queue status if workloads require them.
7. Measure provider latency, token usage, cost and task quality before making optimization claims.
8. Add an evaluation dataset for retrieval quality, groundedness, task success, latency and cost.
9. Keep provider documentation synchronized with adapter behavior and clearly label stubs.

## Resume / portfolio wording guide

Accurate wording for the current code:

- VS Code AI coding assistant.
- Local/cloud LLM provider integration.
- Heuristic task-aware provider selection.
- Sequential provider fallback.
- Workspace-aware lexical retrieval, character chunking and top-five context selection.
- Concurrency-limited asynchronous job queue.
- React-based VS Code chat webview.
- Three-way text merge and conflict reporting.
- Jest unit tests.

Avoid claiming advanced semantic RAG, vector database-backed hybrid retrieval, reranking, real-time token streaming, autonomous multi-agent planning, measured cost/latency optimization, production MLOps, distributed inference or production readiness unless those capabilities are implemented and validated separately.
