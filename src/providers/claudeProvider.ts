import * as vscode from 'vscode';

export class ClaudeProvider {
  constructor(private context: vscode.ExtensionContext) {}

  async ask(prompt: string): Promise<string> {
    // This is a stub. Replace with real Claude API call if needed.
    return '[Claude Haiku 4.5] ' + prompt;
  }
}
