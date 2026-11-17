import * as vscode from 'vscode';
import { LOCOAgent } from './core/locoAgent';
import { pruneHistory, getAllHistory, clearHistory } from './utils/historyStore';
import { SidebarChatProvider } from './ui/webview';

export async function activate(context: vscode.ExtensionContext) {
  const output = vscode.window.createOutputChannel('LOCO Agent');
  const agent = new LOCOAgent(context, output);
  const sidebarProvider = new SidebarChatProvider(context, agent);

  try {
    const cfg = vscode.workspace.getConfiguration('locoAgent');
    if (cfg.get<boolean>('pruneOnStartup', true)) {
      await pruneHistory(context, cfg.get<number>('pruneDays', 30));
    }
  } catch {
    // no-op for config fallback
  }

  const inlineProvider = vscode.languages.registerInlineCompletionItemProvider(
    { pattern: '**' },
    {
      async provideInlineCompletionItems(document, position) {
        try {
          const prompt = document.getText();
          const reply = await agent.runTask({
            prompt,
            context: document.languageId,
            filePath: document.uri.fsPath,
            code: document.getText()
          });

          if (reply && reply.trim()) {
            return [{
              insertText: reply,
              range: new vscode.Range(position, position)
            } as vscode.InlineCompletionItem];
          }
        } catch (error) {
          output.appendLine(`Inline completion failed: ${String(error)}`);
        }
        return [];
      }
    }
  );

  const runAgentCommand = vscode.commands.registerCommand('loco.runAgent', async (promptText?: string) => {
    const prompt = promptText ?? await vscode.window.showInputBox({
      prompt: 'Describe the change or task for LOCO Agent',
      placeHolder: 'Refactor the current module, fix the bug, or explain the code.'
    });

    if (!prompt || !prompt.trim()) {
      return;
    }

    const reply = await agent.runTask({
      prompt,
      context: 'chat',
      filePath: vscode.window.activeTextEditor?.document.uri.fsPath,
      code: vscode.window.activeTextEditor?.document.getText()
    });

    output.appendLine(`User: ${prompt}`);
    output.appendLine(`LOCO: ${reply}`);
    output.show(true);
    vscode.window.showInformationMessage('LOCO Agent finished the task.');
    return reply;
  });

  const openAssistantCommand = vscode.commands.registerCommand('loco.openAssistant', async () => {
    vscode.commands.executeCommand('workbench.view.extension.loco-sidebar');
    const provider = sidebarProvider; 
    provider.show();
  });

  const openSettingsCommand = vscode.commands.registerCommand('loco.openSettings', async () => {
    await vscode.commands.executeCommand('workbench.action.openSettings', 'locoAgent');
  });

  const historyCommands = [
    vscode.commands.registerCommand('locoAgent.setEndpoint', async () => {
      const endpoint = await vscode.window.showInputBox({
        prompt: 'Set endpoint',
        placeHolder: 'http://localhost:11434'
      });
      if (endpoint) {
        await context.globalState.update('loco.localEndpoint', endpoint);
        vscode.window.showInformationMessage(`Endpoint set to ${endpoint}`);
      }
    }),
    vscode.commands.registerCommand('locoAgent.clearEndpoint', async () => {
      await context.globalState.update('loco.localEndpoint', undefined);
      vscode.window.showInformationMessage('Endpoint cleared.');
    }),
    vscode.commands.registerCommand('locoAgent.viewHistory', async () => {
      const history = getAllHistory(context);
      if (!history.length) {
        vscode.window.showInformationMessage('No history available.');
        return;
      }

      const rendered = history
        .map(entry => `[${new Date(entry.timestamp).toISOString()}] ${entry.role.toUpperCase()}: ${entry.text}`)
        .join('\n');

      output.clear();
      output.show(true);
      output.appendLine(rendered);
    }),
    vscode.commands.registerCommand('locoAgent.exportHistory', async () => {
      const history = getAllHistory(context);
      if (!history.length) {
        vscode.window.showInformationMessage('No history to export.');
        return;
      }

      const file = await vscode.window.showSaveDialog({
        defaultUri: vscode.Uri.file(`loco-history-${Date.now()}.json`),
        filters: { JSON: ['json'] }
      });

      if (!file) {
        return;
      }

      await vscode.workspace.fs.writeFile(file, Buffer.from(JSON.stringify(history, null, 2), 'utf8'));
      vscode.window.showInformationMessage(`History exported to ${file.fsPath}`);
    }),
    vscode.commands.registerCommand('locoAgent.clearHistory', async () => {
      const choice = await vscode.window.showQuickPick(['Yes', 'No'], {
        placeHolder: 'Clear all stored LOCO history?'
      });
      if (choice === 'Yes') {
        await clearHistory(context);
        vscode.window.showInformationMessage('LOCO history cleared.');
      }
    })
  ];

  context.subscriptions.push(
    output,
    inlineProvider,
    vscode.window.registerWebviewViewProvider('locoAgent.chatView', sidebarProvider),
    runAgentCommand,
    openAssistantCommand,
    openSettingsCommand,
    ...historyCommands
  );

  output.appendLine('LOCO Agent activated');
}

export function deactivate() {
  // cleanup is handled by extension subscriptions
}
