import * as vscode from 'vscode';
import { getRecentHistory } from '../utils/historyStore';

const DEFAULT_LOCAL_PORTS = [9000, 9001, 8080, 7860, 5005, 11434, 8000, 4891];
const PROBE_TIMEOUT_MS = 700;

export class LocalProvider {
  private readonly endpointKey = 'loco.localEndpoint';

  constructor(private readonly context: vscode.ExtensionContext) {}

  async getEndpoint(): Promise<string> {
    const config = vscode.workspace.getConfiguration();
    const explicit = config.get<string>('loco.localEndpoint')
      || config.get<string>('locoAgent.localEndpoint')
      || this.context.globalState.get<string>(this.endpointKey)
      || this.context.globalState.get<string>('loco.localEndpoint');
    const configuredPorts = config.get<number[]>('locoAgent.localPorts', DEFAULT_LOCAL_PORTS);
    const ports = Array.from(new Set([...configuredPorts, ...DEFAULT_LOCAL_PORTS]))
      .filter(port => Number.isInteger(port) && port > 0 && port <= 65535);
    const discovered = ports.flatMap(port => [`http://localhost:${port}`, `http://127.0.0.1:${port}`]);
    const candidates = Array.from(new Set([explicit, ...discovered].filter((value): value is string => Boolean(value))));

    const health = await Promise.all(candidates.map(endpoint => this.checkEndpoint(endpoint)));
    const index = health.findIndex(Boolean);
    if (index >= 0) {
      await this.context.globalState.update(this.endpointKey, candidates[index]);
      return candidates[index];
    }

    return '';
  }

  async ask(job: { prompt?: string; task?: string; context?: string; code?: string; provider?: string; model?: string }): Promise<{ text: string } | string | null> {
    const endpoint = await this.getEndpoint();
    if (!endpoint) return null;
    const prompt = job?.prompt || this.buildPrompt(job);
    const model = job.model || vscode.workspace.getConfiguration('locoAgent').get<string>('model', 'llama3.1');

    try {
      const compatible = await this.request(endpoint, '/v1/chat/completions', {
        model,
        messages: [{ role: 'user', content: prompt }],
        stream: false
      });
      if (compatible.ok) {
        const json = await compatible.json() as any;
        const text = json?.choices?.[0]?.message?.content;
        if (typeof text === 'string' && text.trim()) return { text };
      }
    } catch { /* try the native local APIs */ }

    try {
      const response = await this.request(endpoint, '/api/chat', {
        model,
        stream: false,
        messages: [{ role: 'user', content: prompt }]
      });
      if (response.ok) {
        const json = await response.json() as any;
        const text = json?.message?.content ?? json?.response ?? json?.content;
        if (typeof text === 'string' && text.trim()) return { text };
      }
    } catch { /* try generation endpoint */ }

    try {
      const response = await this.request(endpoint, '/api/generate', { model, prompt, stream: false });
      if (response.ok) {
        const json = await response.json() as any;
        const text = json?.response ?? json?.text;
        if (typeof text === 'string' && text.trim()) return { text };
      }
    } catch { /* caller can fall back to a cloud provider */ }
    return null;
  }

  async stream(job: Parameters<LocalProvider['ask']>[0], onChunk: (chunk: string) => void): Promise<void> {
    const result = await this.ask(job);
    const text = typeof result === 'string' ? result : result?.text;
    if (text) onChunk(text);
  }

  async estimateLatency(): Promise<number> {
    return 150;
  }

  private async request(endpoint: string, path: string, body?: unknown): Promise<Response> {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 30_000);
    try {
      return await fetch(`${endpoint.replace(/\/$/, '')}${path}`, {
        method: body === undefined ? 'GET' : 'POST',
        headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
        body: body === undefined ? undefined : JSON.stringify(body),
        signal: controller.signal
      });
    } finally {
      clearTimeout(timeout);
    }
  }

  private async checkEndpoint(endpoint: string): Promise<boolean> {
    const candidate = endpoint.replace(/\/$/, '');
    for (const path of ['/api/tags', '/v1/models', '/health']) {
      const controller = new AbortController();
      const timeout = setTimeout(() => controller.abort(), PROBE_TIMEOUT_MS);
      try {
        const response = await fetch(`${candidate}${path}`, { signal: controller.signal });
        if (response.ok) return true;
      } catch {
        // Continue probing the remaining health endpoints.
      } finally {
        clearTimeout(timeout);
      }
    }
    return false;
  }

  private buildPrompt(job: { task?: string; prompt?: string; context?: string; code?: string }): string {
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
