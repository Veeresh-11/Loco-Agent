import * as vscode from 'vscode';
import { getRecentHistory } from '../utils/historyStore';

const DEFAULT_LOCAL_ENDPOINTS = [
  'http://localhost:11434',
  'http://127.0.0.1:11434',
  'http://localhost:8000',
  'http://127.0.0.1:8000',
  'http://localhost:9000',
  'http://127.0.0.1:9000'
];

export class LocalProvider {
  private readonly endpointKey = 'loco.localEndpoint';

  constructor(private readonly context: vscode.ExtensionContext) {}

  async getEndpoint(): Promise<string> {
    const config = vscode.workspace.getConfiguration('locoAgent');
    const stored = this.context.globalState.get<string>(this.endpointKey) ?? config.get<string>('localEndpoint', 'http://localhost:11434');
    const candidates = Array.from(new Set([stored, ...DEFAULT_LOCAL_ENDPOINTS]));

    for (const endpoint of candidates) {
      if (!endpoint) {
        continue;
      }
      try {
        const ok = await this.checkEndpoint(endpoint);
        if (ok) {
          await this.context.globalState.update(this.endpointKey, endpoint);
          return endpoint;
        }
      } catch {
        // continue to the next endpoint
      }
    }

    return 'http://localhost:11434';
  }

  async ask(job: any): Promise<{ text: string } | string | null> {
    const endpoint = await this.getEndpoint();
    const prompt = job?.prompt || this.buildPrompt(job);

    try {
      const response = await fetch(`${endpoint.replace(/\/$/, '')}/api/chat`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          model: 'llama3.1',
          stream: false,
          messages: [{ role: 'user', content: prompt }]
        })
      });

      if (!response.ok) {
        const fallback = await this.tryGenerateFallback(endpoint, prompt);
        return fallback;
      }

      const json = await response.json() as any;
      const text = json?.message?.content ?? json?.response ?? json?.content ?? '';
      return typeof text === 'string' ? { text } : { text: JSON.stringify(text) };
    } catch {
      try {
        const fallback = await this.tryGenerateFallback(endpoint, prompt);
        return fallback;
      } catch {
        return null;
      }
    }
  }

  async stream(job: any, onChunk: (chunk: string) => void): Promise<void> {
    const result = await this.ask(job);
    if (result && typeof result === 'object' && 'text' in result && result.text) {
      onChunk(result.text);
      return;
    }
    if (typeof result === 'string' && result) {
      onChunk(result);
    }
  }

  async estimateLatency(): Promise<number> {
    return 150;
  }

  private async checkEndpoint(endpoint: string): Promise<boolean> {
    const candidate = endpoint.replace(/\/$/, '');
    try {
      const response = await fetch(`${candidate}/api/tags`, { method: 'GET', headers: { Accept: 'application/json' } });
      return response.ok;
    } catch {
      try {
        const response = await fetch(`${candidate}/health`, { method: 'GET' });
        return response.ok;
      } catch {
        return false;
      }
    }
  }

  private async tryGenerateFallback(endpoint: string, prompt: string): Promise<{ text: string }> {
    const candidate = endpoint.replace(/\/$/, '');
    const response = await fetch(`${candidate}/api/generate`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        model: 'llama3.1',
        prompt,
        stream: false
      })
    });

    if (!response.ok) {
      return { text: 'Local provider is unavailable.' };
    }

    const json = await response.json() as any;
    const generatedText = json?.response ?? json?.text ?? '';
    return { text: String(generatedText || 'Local generate endpoint responded without text.') };
  }

  private buildPrompt(job: any): string {
    const task = job?.task ?? job?.prompt ?? 'General assistance';
    const history = getRecentHistory(this.context, 30);
    const recent = history.slice(-10).map(entry => `${entry.role}: ${entry.text}`).join('\n');

    return [
      'You are LOCO, a helpful coding assistant.',
      'Task: ' + task,
      'Context: ' + (job?.context ?? 'general'),
      'Current file:',
      job?.code ?? '',
      'Recent history:',
      recent || 'No recent history.',
      'Answer with a practical response or code patch.'
    ].join('\n');
  }
}
