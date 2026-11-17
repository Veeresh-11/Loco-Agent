import * as vscode from 'vscode';
import { getRecentHistory } from '../utils/historyStore';
import { getLocalChatRoutes, getLocalProbePaths, normalizeLocalEndpoint } from './localEndpoint';

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
    const model = job.model || vscode.workspace.getConfiguration('locoAgent').get<string>('localModel', 'llama3.1');

    for (const route of getLocalChatRoutes(endpoint)) {
      try {
        const body = route.format === 'openai'
          ? { model, messages: [{ role: 'user', content: prompt }], stream: false }
          : route.format === 'ollama-chat'
            ? { model, messages: [{ role: 'user', content: prompt }], stream: false }
            : { model, prompt, stream: false };
        const response = await this.request(endpoint, route.path, body);
        if (!response.ok) continue;

        const json = await response.json() as any;
        const text = route.format === 'openai'
          ? json?.choices?.[0]?.message?.content
          : route.format === 'ollama-chat'
            ? json?.message?.content ?? json?.response ?? json?.content
            : json?.response ?? json?.text;
        if (typeof text === 'string' && text.trim()) return { text: text.trim() };
      } catch {
        // Continue through OpenAI-compatible and native local API routes.
      }
    }
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
      return await fetch(`${normalizeLocalEndpoint(endpoint)}${path}`, {
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
    const candidate = normalizeLocalEndpoint(endpoint);
    for (const path of getLocalProbePaths(candidate)) {
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
