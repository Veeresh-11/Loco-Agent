import * as vscode from 'vscode';

export class Secrets {
  private static key(service: string): string {
    return `loco.${service}.apiKey`;
  }

  static async getApiKey(context: vscode.ExtensionContext, service: string): Promise<string | undefined> {
    try {
      return await context.secrets.get(this.key(service));
    } catch {
      return undefined;
    }
  }

  static async setApiKey(context: vscode.ExtensionContext, service: string, key: string): Promise<void> {
    try {
      if (!key.trim()) {
        await this.clearApiKey(context, service);
        return;
      }
      await context.secrets.store(this.key(service), key.trim());
    } catch (error) {
      throw new Error(`Could not securely store the ${service} API key: ${String(error)}`);
    }
  }

  static async clearApiKey(context: vscode.ExtensionContext, service: string): Promise<void> {
    await context.secrets.delete(this.key(service));
  }
}
