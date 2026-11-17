import * as vscode from 'vscode';

export class GPT4AllProvider {
  constructor(private context: vscode.ExtensionContext) {}

  async ask(prompt: string): Promise<string> {
    return 'GPT4All provider not yet implemented';
  }
}
