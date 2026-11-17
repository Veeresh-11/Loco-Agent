import * as vscode from 'vscode';
import * as cp from 'child_process';
import * as util from 'util';

export class PowerShellRunner {
  private static readonly execFile = util.promisify(cp.execFile);

  static async run(script: string): Promise<string> {
    const trimmed = script.trim();
    if (!trimmed) return 'No PowerShell script was provided.';
    if (trimmed.length > 20_000) return 'Script rejected: it exceeds the 20,000 character safety limit.';

    const confirm = await vscode.window.showWarningMessage(
      'Run this PowerShell script on your machine? Review the script before continuing.',
      { modal: true },
      'Run Script'
    );
    if (confirm !== 'Run Script') return 'Script execution cancelled.';

    try {
      const { stdout, stderr } = await this.execFile(
        'powershell.exe',
        ['-NoLogo', '-NoProfile', '-NonInteractive', '-Command', trimmed],
        { windowsHide: true, timeout: 30_000, maxBuffer: 1024 * 1024 }
      );
      return (stdout || stderr || 'Script completed without output.').trim().slice(0, 20_000);
    } catch (error) {
      const detail = error instanceof Error ? error.message : String(error);
      return `PowerShell execution failed: ${detail.slice(0, 1000)}`;
    }
  }
}
