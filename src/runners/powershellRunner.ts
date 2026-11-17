import * as vscode from 'vscode';

export class PowerShellRunner {
  static async run(script: string): Promise<string> {
    try {
      const terminal = vscode.window.createTerminal({ name: 'LOCO PowerShell' });
      terminal.sendText(script);
      terminal.show();
      return 'Script executed';
    } catch (e) {
      return `Error: ${e}`;
    }
  }
}
