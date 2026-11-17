import * as vscode from 'vscode';

export class OllamaProvider {
  constructor(private context: vscode.ExtensionContext) {}

  async ask(prompt: string): Promise<string> {
    return 'Ollama provider not yet implemented';
  }
}
