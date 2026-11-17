import * as vscode from 'vscode';
import { CloudProvider } from './cloudProvider';

export class OpenAIProvider extends CloudProvider {
  constructor(context: vscode.ExtensionContext) {
    super(context);
  }

  async ask(prompt: string): Promise<string> {
    return super.ask(prompt, { model: 'gpt-4o-mini', temperature: 0.2, maxTokens: 1200 });
  }
}
