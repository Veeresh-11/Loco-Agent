"use strict";
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
exports.activate = activate;
exports.deactivate = deactivate;
const vscode = __importStar(require("vscode"));
const locoAgent_1 = require("./core/locoAgent");
const historyStore_1 = require("./utils/historyStore");
const webview_1 = require("./ui/webview");
async function activate(context) {
    const output = vscode.window.createOutputChannel('LOCO Agent');
    const agent = new locoAgent_1.LOCOAgent(context, output);
    const sidebarProvider = new webview_1.SidebarChatProvider(context, agent);
    try {
        const cfg = vscode.workspace.getConfiguration('locoAgent');
        if (cfg.get('pruneOnStartup', true)) {
            await (0, historyStore_1.pruneHistory)(context, cfg.get('pruneDays', 30));
        }
    }
    catch {
        // no-op for config fallback
    }
    const inlineProvider = vscode.languages.registerInlineCompletionItemProvider({ pattern: '**' }, {
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
                        }];
                }
            }
            catch (error) {
                output.appendLine(`Inline completion failed: ${String(error)}`);
            }
            return [];
        }
    });
    const runAgentCommand = vscode.commands.registerCommand('loco.runAgent', async (promptText) => {
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
            const history = (0, historyStore_1.getAllHistory)(context);
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
            const history = (0, historyStore_1.getAllHistory)(context);
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
                await (0, historyStore_1.clearHistory)(context);
                vscode.window.showInformationMessage('LOCO history cleared.');
            }
        })
    ];
    context.subscriptions.push(output, inlineProvider, vscode.window.registerWebviewViewProvider('locoAgent.chatView', sidebarProvider), runAgentCommand, openAssistantCommand, openSettingsCommand, ...historyCommands);
    output.appendLine('LOCO Agent activated');
}
function deactivate() {
    // cleanup is handled by extension subscriptions
}
//# sourceMappingURL=extension.js.map