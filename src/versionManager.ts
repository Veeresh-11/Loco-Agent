import * as vscode from 'vscode';

export class VersionManager {
  private currentVersion: string;

  constructor(private context: vscode.ExtensionContext) {
    this.currentVersion = context.extension.packageJSON.version || '0.1.0';
  }

  getVersion(): string {
    return this.currentVersion;
  }

  async checkForUpdates(): Promise<boolean> {
    const lastSeenVersion = this.context.globalState.get<string>('lastSeenVersion');
    if (lastSeenVersion !== this.currentVersion) {
      await this.context.globalState.update('lastSeenVersion', this.currentVersion);
      return true;
    }
    return false;
  }
}
