import * as vscode from 'vscode';
import { LocalProvider } from './localProvider';

export class OllamaProvider {
  private readonly local: LocalProvider;

  constructor(context: vscode.ExtensionContext) {
    this.local = new LocalProvider(context);
  }

  async ask(prompt: string): Promise<string> {
    const result = await this.local.ask({ prompt, provider: 'ollama' });
    const text = typeof result === 'string' ? result : result?.text;
    if (!text) throw new Error('Ollama was not detected or returned no response.');
    return text;
  }
}
