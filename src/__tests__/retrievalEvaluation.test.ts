import { evaluateRetrieval, type RetrievalEvaluationCase } from '../core/retrievalEvaluation';

const dataset: RetrievalEvaluationCase[] = [
  { id: 'provider-routing', query: 'provider routing policy', relevantPaths: ['src/core/routingPolicy.ts'] },
  { id: 'patch-rollback', query: 'restore patch backup', relevantPaths: ['src/core/patchManager.ts', 'src/core/patchPath.ts'] },
  { id: 'stream-parser', query: 'parse OpenAI SSE stream', relevantPaths: ['src/providers/streaming.ts'] }
];

describe('retrieval evaluation metrics', () => {
  it('calculates recall@k, precision@k and mean reciprocal rank', async () => {
    const docs = [
      { path: 'src/core/routingPolicy.ts' },
      { path: 'src/core/patchManager.ts' },
      { path: 'src/core/patchPath.ts' },
      { path: 'src/providers/streaming.ts' }
    ];
    const summary = await evaluateRetrieval(dataset, async query => {
      if (query.includes('provider')) return [docs[0], docs[3]];
      if (query.includes('patch')) return [docs[1], docs[2]];
      return [docs[3], docs[0]];
    }, 2);

    expect(summary.cases).toBe(3);
    expect(summary.recallAtK).toBe(1);
    expect(summary.precisionAtK).toBeCloseTo(4 / 6);
    expect(summary.meanReciprocalRank).toBe(1);
  });

  it('limits evaluated results to k and handles misses as zero', async () => {
    const summary = await evaluateRetrieval([dataset[0]], async () => [
      { path: 'src/other.ts' }, { path: 'src/core/routingPolicy.ts' }, { path: 'src/core/routingPolicy.ts' }
    ], 1);
    expect(summary.results[0].retrievedPaths).toHaveLength(1);
    expect(summary.recallAtK).toBe(0);
    expect(summary.meanReciprocalRank).toBe(0);
  });

  it('deduplicates repeated paths so duplicates cannot inflate metrics', async () => {
    const summary = await evaluateRetrieval([dataset[0]], async () => [
      { path: 'src/core/routingPolicy.ts' },
      { path: './src/core/routingPolicy.ts' },
      { path: 'SRC\\CORE\\ROUTINGPOLICY.TS' }
    ], 3);
    expect(summary.results[0].retrievedPaths).toEqual(['src/core/routingpolicy.ts']);
    expect(summary.results[0].retrievedRelevantCount).toBe(1);
    expect(summary.recallAtK).toBe(1);
    expect(summary.precisionAtK).toBeCloseTo(1 / 3);
    expect(summary.meanReciprocalRank).toBe(1);
  });

  it('rejects invalid k, empty datasets, and duplicate ids', async () => {
    await expect(evaluateRetrieval([], async () => [])).rejects.toThrow('must not be empty');
    await expect(evaluateRetrieval(dataset, async () => [], 0)).rejects.toThrow('topK');
    await expect(evaluateRetrieval([dataset[0], dataset[0]], async () => [])).rejects.toThrow('duplicated');
  });
});