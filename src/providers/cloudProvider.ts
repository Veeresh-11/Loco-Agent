import * as vscode from 'vscode';

export interface CloudRequestOptions {
  model?: string;
  temperature?: number;
  maxTokens?: number;
}

export class CloudProvider {
  constructor(private readonly context: vscode.ExtensionContext) {}

  async ask(prompt: string, options: CloudRequestOptions = {}): Promise<string> {
    const config = vscode.workspace.getConfiguration('locoAgent');
    const endpoint = config.get<string>('endpoint', 'https://api.openai.com/v1');
    const apiKey = process.env.OPENAI_API_KEY || config.get<string>('apiKey', '');

    if (!apiKey) {
      return 'Cloud provider is not configured. Add an API key or set the endpoint in VS Code settings.';
    }

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
        })
      });

      if (!response.ok) {
        return `Cloud request failed with status ${response.status}.`;
      }

      const json = await response.json() as any;
      return String(json?.choices?.[0]?.message?.content ?? json?.output_text ?? '');
    } catch (error) {
      return `Cloud provider error: ${String(error)}`;
    }
  }
}
