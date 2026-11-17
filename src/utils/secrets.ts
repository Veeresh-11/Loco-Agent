import * as vscode from 'vscode';

export class Secrets {
  static async getApiKey(service: string): Promise<string | undefined> {
    try {
      return await vscode.workspace.getConfiguration().get(`${service}.apiKey`) as string | undefined;
    } catch {
      return undefined;
    }
  }

  static async setApiKey(service: string, key: string): Promise<void> {
    try {
      await vscode.workspace.getConfiguration().update(`${service}.apiKey`, key, vscode.ConfigurationTarget.Global);
    } catch {}
  }
}
