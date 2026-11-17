import * as vscode from 'vscode';

export interface RAGChunk {
  id: string;
  path: string;
  text: string;
  score: number;
}

export class RAGEngine {
  private readonly defaultExtensions = [
    'ts', 'tsx', 'js', 'jsx', 'json', 'md', 'py', 'java', 'cs', 'go', 'rs', 'yaml', 'yml', 'txt', 'sql'
  ];

  async indexWorkspace(): Promise<RAGChunk[]> {
    const files = await vscode.workspace.findFiles(
      '**/*.{ts,tsx,js,jsx,json,md,py,java,cs,go,rs,yaml,yml,txt,sql}',
      '**/node_modules/**',
      250
    );

    const chunks: RAGChunk[] = [];

    for (const file of files) {
      try {
        const raw = await vscode.workspace.fs.readFile(file);
        const text = Buffer.from(raw).toString('utf8');
        if (!text.trim()) {
          continue;
        }

        const blocks = this.chunkText(text, 900, 180);
        for (const [index, block] of blocks.entries()) {
          chunks.push({
            id: `${file.fsPath}:${index}`,
            path: file.fsPath,
            text: block.trim(),
            score: 0
          });
        }
      } catch {
        // Skip unreadable or binary files.
      }
    }

    return chunks;
  }

  async retrieve(query: string, topK: number = 5): Promise<RAGChunk[]> {
    const workspace = await this.indexWorkspace();
    if (!workspace.length || !query.trim()) {
      return [];
    }

    const queryTerms = this.normalize(query).split(/\s+/).filter(Boolean);
    if (!queryTerms.length) {
      return [];
    }

    const scored = workspace.map(chunk => {
      const haystack = this.normalize(chunk.text);
      const score = queryTerms.reduce((total, term) => {
        const pattern = new RegExp(term.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'gi');
        return total + (haystack.match(pattern)?.length ?? 0);
      }, 0);
      return { ...chunk, score };
    });

    return scored
      .filter(item => item.score > 0)
      .sort((a, b) => b.score - a.score)
      .slice(0, topK);
  }

  private chunkText(text: string, maxChars: number, overlap: number): string[] {
    const chunks: string[] = [];
    const normalized = text.replace(/\r/g, '');
    let start = 0;

    while (start < normalized.length) {
      const end = Math.min(start + maxChars, normalized.length);
      const segment = normalized.slice(start, end);
      chunks.push(segment);
      if (end >= normalized.length) {
        break;
      }
      start = Math.max(start + maxChars - overlap, end - overlap);
    }

    return chunks;
  }

  private normalize(input: string): string {
    return input.toLowerCase().replace(/[^a-z0-9\s_\-./]+/g, ' ').replace(/\s+/g, ' ').trim();
  }
}
