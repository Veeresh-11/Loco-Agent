import * as vscode from 'vscode';
import { LOCOAgent } from './core/locoAgent';
import { clearHistory, getAllHistory, pruneHistory } from './utils/historyStore';
import { SidebarChatProvider } from './ui/webview';
import { OutputView } from './ui/outputView';
import { Settings } from './ui/settings';
import { Secrets } from './utils/secrets';

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  const outputView = new OutputView('LOCO Agent');
  const output = outputView.outputChannel;
  const agent = new LOCOAgent(context, output);
  const sidebarProvider = new SidebarChatProvider(context, agent);

  const config = vscode.workspace.getConfiguration('locoAgent');
  if (config.get<boolean>('pruneOnStartup', true)) {
    await pruneHistory(context, config.get<number>('pruneDays', 30));
  }

  const runAgentCommand = vscode.commands.registerCommand('loco.runAgent', async (promptText?: string) => {
    const editor = vscode.window.activeTextEditor;
    const prompt = promptText ?? await vscode.window.showInputBox({
      prompt: 'Describe the task for LOCO Agent',
      placeHolder: 'Explain, fix, or refactor the selected code.'
    });
    if (!prompt?.trim()) return;

    try {
      const reply = await agent.runTask({
        prompt: prompt.trim(),
        context: editor?.document.languageId ?? 'chat',
        filePath: editor?.document.uri.fsPath,
        code: editor?.document.getText(editor.selection) || editor?.document.getText()
      });
      outputView.log(`User: ${prompt.trim()}`);
      outputView.log(`LOCO: ${reply}`);
      output.show(true);
      return reply;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      outputView.log(`Task failed: ${message}`);
      vscode.window.showErrorMessage(`LOCO Agent failed: ${message}`);
      return undefined;
    }
  });

  const openAssistantCommand = vscode.commands.registerCommand('loco.openAssistant', async () => {
    await vscode.commands.executeCommand('workbench.view.extension.loco-sidebar');
    sidebarProvider.show();
  });

  const openSettingsCommand = vscode.commands.registerCommand('loco.openSettings', () => Settings.openSettings());

  const commands = [
    vscode.commands.registerCommand('loco.setLocalEndpoint', async () => {
      const endpoint = await vscode.window.showInputBox({
        prompt: 'Set the local language-model server endpoint',
        placeHolder: 'http://localhost:9000',
        validateInput: value => {
          if (!value) return undefined;
          try {
            const parsed = new URL(value);
            return ['http:', 'https:'].includes(parsed.protocol) ? undefined : 'Use an http or https URL.';
          } catch {
            return 'Enter a valid http or https URL.';
          }
        }
      });
      if (endpoint?.trim()) {
        await vscode.workspace.getConfiguration().update('loco.localEndpoint', endpoint.trim(), vscode.ConfigurationTarget.Global);
        vscode.window.showInformationMessage('Local endpoint saved.');
      }
    }),
    vscode.commands.registerCommand('loco.clearLocalEndpoint', async () => {
      await vscode.workspace.getConfiguration().update('loco.localEndpoint', undefined, vscode.ConfigurationTarget.Global);
      await context.globalState.update('loco.localEndpoint', undefined);
      vscode.window.showInformationMessage('Local endpoint cleared; automatic port detection is enabled.');
    }),
    vscode.commands.registerCommand('loco.setApiKey', async () => {
      const service = await vscode.window.showQuickPick(['openai', 'azure'], { placeHolder: 'Select the cloud provider' });
      if (!service) return;
      const apiKey = await vscode.window.showInputBox({
        prompt: `Enter the ${service === 'azure' ? 'Azure OpenAI' : 'OpenAI'} API key (stored securely by VS Code)`,
        password: true,
        ignoreFocusOut: true
      });
      if (!apiKey?.trim()) return;
      await Secrets.setApiKey(context, service, apiKey);
      vscode.window.showInformationMessage(`${service === 'azure' ? 'Azure OpenAI' : 'OpenAI'} API key stored securely.`);
    }),
    vscode.commands.registerCommand('loco.clearApiKey', async () => {
      const service = await vscode.window.showQuickPick(['openai', 'azure'], { placeHolder: 'Select the key to remove' });
      if (!service) return;
      await Secrets.clearApiKey(context, service);
      vscode.window.showInformationMessage('Cloud API key removed.');
    }),
    vscode.commands.registerCommand('loco.viewHistory', () => {
      const history = getAllHistory(context);
      if (!history.length) {
        vscode.window.showInformationMessage('No history available.');
        return;
      }
      outputView.clear();
      outputView.log(history
        .map(entry => `[${new Date(entry.timestamp).toISOString()}] ${entry.role.toUpperCase()}: ${entry.text}`)
        .join('\n'));
      output.show(true);
    }),
    vscode.commands.registerCommand('loco.exportHistory', async () => {
      const history = getAllHistory(context);
      if (!history.length) {
        vscode.window.showInformationMessage('No history to export.');
        return;
      }
      const file = await vscode.window.showSaveDialog({
        defaultUri: vscode.Uri.file(`loco-history-${Date.now()}.json`),
        filters: { JSON: ['json'] }
      });
      if (!file) return;
      await vscode.workspace.fs.writeFile(file, Buffer.from(JSON.stringify(history, null, 2), 'utf8'));
      vscode.window.showInformationMessage('LOCO history exported.');
    }),
    vscode.commands.registerCommand('loco.clearHistory', async () => {
      const choice = await vscode.window.showWarningMessage('Clear all stored LOCO history?', { modal: true }, 'Clear History');
      if (choice === 'Clear History') {
        await clearHistory(context);
        vscode.window.showInformationMessage('LOCO history cleared.');
      }
    }),
    vscode.commands.registerCommand('loco.restorePatchBackup', async () => {
      const backups = agent.getPatchBackups();
      if (!backups.length) {
        vscode.window.showInformationMessage('No LOCO patch backups are available.');
        return;
      }
      const selected = await vscode.window.showQuickPick(
        backups.slice().reverse().map(backup => ({
          label: backup.targetPath.split(/[\\\\/]/).pop() ?? backup.targetPath,
          description: new Date(backup.createdAt).toLocaleString(),
          detail: backup.targetPath,
          id: backup.id
        })),
        { placeHolder: 'Select a patch backup to preview and restore' }
      );
      if (!selected) return;
      const result = await agent.restorePatchBackup(selected.id);
      if (result.startsWith('Backup restored')) {
        vscode.window.showInformationMessage(result);
      } else {
        vscode.window.showWarningMessage(result);
      }
    })
  ];

  context.subscriptions.push(
    outputView,
    vscode.window.registerWebviewViewProvider(SidebarChatProvider.viewType, sidebarProvider),
    runAgentCommand,
    openAssistantCommand,
    openSettingsCommand,
    ...commands
  );
  outputView.log('LOCO Agent activated');
}

export function deactivate(): void {
  // VS Code disposes registered resources through ExtensionContext.subscriptions.
}
