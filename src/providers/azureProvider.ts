import * as vscode from 'vscode';
import { Secrets } from '../utils/secrets';
import { consumeModelStream } from './streaming';

export class AzureProvider {
  constructor(private readonly context: vscode.ExtensionContext) {}

  private getConfig(): { url: string; apiKey: string } {
    const config = vscode.workspace.getConfiguration('locoAgent');
    const endpoint = config.get<string>('azureEndpoint', '').replace(/\/$/, '');
    const deployment = config.get<string>('azureDeployment', '');
    const apiVersion = config.get<string>('azureApiVersion', '2024-10-21');
    const apiKey = process.env.AZURE_OPENAI_API_KEY || '';
    if (!endpoint || !deployment) throw new Error('Configure locoAgent.azureEndpoint and locoAgent.azureDeployment first.');
    return { url: `${endpoint}/openai/deployments/${encodeURIComponent(deployment)}/chat/completions?api-version=${encodeURIComponent(apiVersion)}`, apiKey };
  }

  async ask(prompt: string): Promise<string> {
    const config = vscode.workspace.getConfiguration('locoAgent');
    const endpoint = config.get<string>('azureEndpoint', '').replace(/\/$/, '');
    const deployment = config.get<string>('azureDeployment', '');
    const apiVersion = config.get<string>('azureApiVersion', '2024-10-21');
    const apiKey = await Secrets.getApiKey(this.context, 'azure') || process.env.AZURE_OPENAI_API_KEY;
    if (!endpoint || !deployment || !apiKey) throw new Error('Configure locoAgent.azureEndpoint, locoAgent.azureDeployment, and the Azure API key first.');
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 60_000);
    try {
      const url = `${endpoint}/openai/deployments/${encodeURIComponent(deployment)}/chat/completions?api-version=${encodeURIComponent(apiVersion)}`;
      const response = await fetch(url, { method: 'POST', headers: { 'Content-Type': 'application/json', 'api-key': apiKey },
        body: JSON.stringify({ messages: [{ role: 'user', content: prompt }], temperature: 0.2, max_tokens: 1200 }), signal: controller.signal });
      if (!response.ok) throw new Error(`Azure OpenAI request failed (${response.status}): ${(await response.text()).slice(0, 500)}`);
      const json = await response.json() as any;
      const text = json?.choices?.[0]?.message?.content;
      if (typeof text !== 'string' || !text.trim()) throw new Error('Azure OpenAI returned an empty response.');
      return text;
    } finally { clearTimeout(timeout); }
  }

  async stream(prompt: string, onChunk: (chunk: string) => void): Promise<void> {
    const config = vscode.workspace.getConfiguration('locoAgent');
    const endpoint = config.get<string>('azureEndpoint', '').replace(/\/$/, '');
    const deployment = config.get<string>('azureDeployment', '');
    const apiVersion = config.get<string>('azureApiVersion', '2024-10-21');
    const apiKey = await Secrets.getApiKey(this.context, 'azure') || process.env.AZURE_OPENAI_API_KEY;
    if (!endpoint || !deployment || !apiKey) throw new Error('Configure locoAgent.azureEndpoint, locoAgent.azureDeployment, and the Azure API key first.');
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 60_000);
    try {
      const url = `${endpoint}/openai/deployments/${encodeURIComponent(deployment)}/chat/completions?api-version=${encodeURIComponent(apiVersion)}`;
      const response = await fetch(url, { method: 'POST', headers: { 'Content-Type': 'application/json', 'api-key': apiKey, Accept: 'text/event-stream' },
        body: JSON.stringify({ messages: [{ role: 'user', content: prompt }], temperature: 0.2, max_tokens: 1200, stream: true }), signal: controller.signal });
      await consumeModelStream(response, 'openai-sse', onChunk);
    } finally { clearTimeout(timeout); }
  }
}
