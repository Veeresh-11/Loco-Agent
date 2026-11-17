import * as vscode from 'vscode';
import { CloudProvider } from './cloudProvider';

export class OpenAIProvider extends CloudProvider {
  constructor(context: vscode.ExtensionContext) {
    super(context);
  }

  async ask(prompt: string): Promise<string> {
    const model = vscode.workspace.getConfiguration('locoAgent').get<string>('model', 'gpt-4o-mini');
    return super.ask(prompt, { model, temperature: 0.2, maxTokens: 1200 });
  }
}
