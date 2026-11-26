# Retrieval evaluation baseline

## Purpose

This is a small, version-controlled smoke benchmark for the current lexical workspace retriever. It creates a repeatable measurement point before any semantic/vector retrieval implementation is introduced.

## Dataset

`src/evaluation/retrieval-cases.json` contains manually labelled query-to-file relevance cases. Relevance is file-level: a file is relevant when it contains the implementation or policy directly needed to answer the query. Expand the set to at least 30 representative queries before using the scores as a release gate.

## Metrics

- Recall@K: relevant files retrieved divided by all labelled relevant files.
- Precision@K: relevant files retrieved divided by K.
- Mean Reciprocal Rank (MRR): mean reciprocal rank of the first relevant file.
- Per-case results are retained to make failures diagnosable.

## Comparison protocol

1. Freeze the dataset and record its revision.
2. Run the same queries and K value against the lexical baseline.
3. Record aggregate and per-case metrics.
4. Change one retrieval component at a time (for example, tokenization, ranking, then semantic candidate generation).
5. Compare metrics against the same dataset; do not tune against the test set and report it as an unbiased holdout.
6. Add a separate holdout set before using results as a release criterion.

## Current limitations

The five initial cases are a smoke-test seed, not a statistically representative benchmark. Labels are manually authored and file-level. This PR does not add embeddings, a vector database, semantic retrieval, latency claims, or cost claims. Those should follow only after a baseline and a larger evaluation set exist.
