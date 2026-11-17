import * as vscode from 'vscode';

export class Settings {
  static async openSettings() {
    await vscode.commands.executeCommand('workbench.action.openSettings', '@ext:veeresh.loco-agent');
  }

  static async getProvider(): Promise<string> {
    try {
      const config = vscode.workspace.getConfiguration();
      return config.get<string>('loco.provider') ?? config.get<string>('locoAgent.provider', 'local');
    } catch {
      return 'local';
    }
  }

  static async setProvider(provider: string) {
    const allowed = ['local', 'openai', 'azure', 'ollama', 'gpt4all'];
    if (!allowed.includes(provider)) {
      throw new Error(`Unsupported LOCO provider: ${provider}`);
    }
    await vscode.workspace.getConfiguration().update('loco.provider', provider, vscode.ConfigurationTarget.Global);
  }
}
