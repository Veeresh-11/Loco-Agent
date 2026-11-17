import * as vscode from 'vscode';

export class Settings {
  static async openSettings() {
    vscode.commands.executeCommand('workbench.action.openSettings', 'locoAgent');
  }

  static async getProvider(): Promise<string> {
    try {
      return vscode.workspace.getConfiguration('locoAgent').get('provider', 'local') as string;
    } catch {
      return 'local';
    }
  }

  static async setProvider(provider: string) {
    try {
      await vscode.workspace.getConfiguration('locoAgent').update('provider', provider, vscode.ConfigurationTarget.Workspace);
    } catch {}
  }
}
