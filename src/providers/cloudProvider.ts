import * as vscode from 'vscode';
import { Secrets } from '../utils/secrets';
import { parseOpenAICompatibleResponse } from './responseParser';

export interface CloudRequestOptions {
  model?: string;
  temperature?: number;
  maxTokens?: number;
}

export class CloudProvider {
  constructor(private readonly context: vscode.ExtensionContext) {}

  async ask(prompt: string, options: CloudRequestOptions = {}): Promise<string> {
    const config = vscode.workspace.getConfiguration();
    const endpoint = config.get<string>('locoAgent.endpoint', 'https://api.openai.com/v1').trim();
    const apiKey = await Secrets.getApiKey(this.context, 'openai') || process.env.OPENAI_API_KEY;

    if (!apiKey) {
      throw new Error('OpenAI API key is not configured. Set it with the LOCO secret command or OPENAI_API_KEY.');
    }

    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 60_000);
    try {
      const response = await fetch(`${endpoint.replace(/\/$/, '')}/chat/completions`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          Authorization: `Bearer ${apiKey}`
        },
        body: JSON.stringify({
          model: options.model ?? 'gpt-4o-mini',
          temperature: options.temperature ?? 0.2,
          max_tokens: options.maxTokens ?? 1200,
          messages: [{ role: 'user', content: prompt }]
        }),
        signal: controller.signal
      });

      if (!response.ok) {
        const details = (await response.text()).slice(0, 500);
        throw new Error(`OpenAI-compatible request failed (${response.status}): ${details}`);
      }

      const content = parseOpenAICompatibleResponse(await response.json());
      if (!content) throw new Error('Cloud provider returned an empty response.');
      return content;
    } catch (error) {
      throw error instanceof Error ? error : new Error(`Cloud provider error: ${String(error)}`);
    } finally {
      clearTimeout(timeout);
    }
  }
}
