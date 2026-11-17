import * as vscode from 'vscode';

export class AzureProvider {
  constructor(private context: vscode.ExtensionContext) {}

  async ask(prompt: string): Promise<string> {
    return 'Azure provider not yet implemented';
  }
}
