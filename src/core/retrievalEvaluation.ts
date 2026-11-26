export interface RetrievalEvaluationCase {
  id: string;
  query: string;
  relevantPaths: string[];
}

export interface RetrievalEvaluationResult {
  caseId: string;
  retrievedPaths: string[];
  relevantCount: number;
  retrievedRelevantCount: number;
  recallAtK: number;
  precisionAtK: number;
  reciprocalRank: number;
}

export interface RetrievalEvaluationSummary {
  cases: number;
  recallAtK: number;
  precisionAtK: number;
  meanReciprocalRank: number;
  results: RetrievalEvaluationResult[];
}

/** Evaluate a retrieval implementation against explicitly labelled relevant paths. */
export async function evaluateRetrieval(
  dataset: RetrievalEvaluationCase[],
  retrieve: (query: string, topK: number) => Promise<Array<{ path: string }>>,
  topK = 5
): Promise<RetrievalEvaluationSummary> {
  if (!Number.isInteger(topK) || topK < 1) throw new Error('topK must be a positive integer.');
  if (!dataset.length) throw new Error('Retrieval evaluation dataset must not be empty.');

  const ids = new Set<string>();
  const results: RetrievalEvaluationResult[] = [];
  for (const item of dataset) {
    if (!item.id.trim() || ids.has(item.id)) throw new Error(`Evaluation case id is empty or duplicated: ${item.id}`);
    ids.add(item.id);
    const relevant = new Set(item.relevantPaths.map(normalizePath));
    if (!item.query.trim() || !relevant.size) throw new Error(`Evaluation case ${item.id} requires a query and relevant paths.`);
    const retrieved = await retrieve(item.query, topK);
    const paths = retrieved.slice(0, topK).map(entry => normalizePath(entry.path));
    const relevantIndices = paths.map((path, index) => relevant.has(path) ? index + 1 : 0).filter(Boolean);
    const count = relevantIndices.length;
    results.push({
      caseId: item.id,
      retrievedPaths: paths,
      relevantCount: relevant.size,
      retrievedRelevantCount: count,
      recallAtK: count / relevant.size,
      precisionAtK: count / topK,
      reciprocalRank: relevantIndices.length ? 1 / Math.min(...relevantIndices) : 0
    });
  }

  return {
    cases: results.length,
    recallAtK: average(results.map(result => result.recallAtK)),
    precisionAtK: average(results.map(result => result.precisionAtK)),
    meanReciprocalRank: average(results.map(result => result.reciprocalRank)),
    results
  };
}

function normalizePath(path: string): string {
  return path.replace(/\\/g, '/').replace(/^\.\//, '').toLowerCase();
}

function average(values: number[]): number {
  return values.reduce((sum, value) => sum + value, 0) / values.length;
}
