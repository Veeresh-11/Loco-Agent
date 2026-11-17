# LOCO Agent — Engineering Notes

This note records the behavior visible in the current source tree. Update it when implementation changes; do not describe planned work as shipped functionality.

## Current execution path

1. `LOCOAgent.runTask()` normalizes the prompt, assigns a simple complexity label and estimates tokens from prompt length.
2. The job is submitted to `JobQueue`, whose concurrency is configured by `locoAgent.maxConcurrency` and clamped to 1–16.
3. `RAGEngine` indexes supported workspace files (up to 300 files, excluding common generated/dependency folders), chunks text into 900-character windows with 180-character overlap, and scores chunks by normalized query-term occurrence plus path matches.
4. `LOCOAgent` composes a prompt containing the rule-based plan, retrieved snippets, optional file content and Git workspace status.
5. `ProviderManager` selects a preferred provider through `RoutingPolicy`, invokes it, and sequentially tries a fallback provider when it errors or returns no useful text.
6. The completed response is saved to local history (when enabled) and shown through the VS Code output/chat interfaces.

## Provider behavior

- Local provider discovery probes an explicit endpoint and a set of common localhost ports.
- Local requests try OpenAI-compatible chat completions and Ollama chat/generate routes.
- OpenAI-compatible cloud requests use the configured endpoint and SecretStorage/environment API key.
- Azure OpenAI requests use the configured resource endpoint, deployment and API version.
- Ollama and GPT4All adapters reuse the local-provider implementation.
- The Claude adapter is currently a stub and must not be represented as an operational provider.

## Current test coverage

The visible Jest tests cover endpoint normalization/routing, response parsing, provider fallback, simple routing thresholds, job queue concurrency/result handling and three-way merge behavior. They do not constitute end-to-end tests against live model providers, RAG quality benchmarks, frontend browser tests, performance/load tests or security certification.

Run locally before submitting changes:

```bash
npm ci
npm run compile
npm test
npm run lint
```

The repository's current package scripts do not define a separate webview bundle build. Verify the generated extension package and load it in a clean VS Code test profile before a release.

## Known engineering limitations / follow-up work

- Replace lexical term-frequency retrieval with an evaluated retrieval pipeline if semantic RAG is a product requirement. Add embeddings, durable vector storage, hybrid search and reranking only with tests and documented dependencies.
- Implement real incremental response streaming through provider adapters and the webview message protocol.
- Replace keyword-based planning with bounded, observable tool planning only if required; enforce tool allowlists, cancellation, timeouts and approval gates.
- Measure actual provider latency, token usage and cost before making performance/cost optimization claims.
- Add retrieval and agent evaluation datasets with groundedness, recall/precision, task-success, latency and cost metrics.
- Add integration tests using mocked provider HTTP responses plus an optional opt-in local-model test profile.
- Add webview tests and extension-host smoke tests.
- Revisit patch application safety: validate target workspace boundaries, preserve backups or diffs, make conflict resolution explicit and require user approval before writes.
- Keep provider support docs synchronized with actual adapter behavior and remove stale/stub provider claims.

## Resume / portfolio wording guide

Accurate terms for the current code include: VS Code AI coding assistant; local/cloud LLM provider integration; heuristic task-aware routing; sequential provider fallback; workspace-aware lexical retrieval; chunking and top-K context selection; asynchronous job queue; React-based chat webview; three-way text merge; Jest tests.

Avoid claiming: advanced semantic RAG, vector database-backed hybrid retrieval, reranking, real-time token streaming, autonomous multi-agent planning, measured cost/latency optimization, production MLOps, distributed inference or production readiness unless those capabilities are implemented and validated separately.
