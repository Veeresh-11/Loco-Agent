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
const outputView_1 = require("./ui/outputView");
const settings_1 = require("./ui/settings");
const secrets_1 = require("./utils/secrets");
async function activate(context) {
    const outputView = new outputView_1.OutputView('LOCO Agent');
    const output = outputView.outputChannel;
    const agent = new locoAgent_1.LOCOAgent(context, output);
    const sidebarProvider = new webview_1.SidebarChatProvider(context, agent);
    const config = vscode.workspace.getConfiguration('locoAgent');
    if (config.get('pruneOnStartup', true)) {
        await (0, historyStore_1.pruneHistory)(context, config.get('pruneDays', 30));
    }
    const runAgentCommand = vscode.commands.registerCommand('loco.runAgent', async (promptText) => {
        const editor = vscode.window.activeTextEditor;
        const prompt = promptText ?? await vscode.window.showInputBox({
            prompt: 'Describe the task for LOCO Agent',
            placeHolder: 'Explain, fix, or refactor the selected code.'
        });
        if (!prompt?.trim())
            return;
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
        }
        catch (error) {
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
    const openSettingsCommand = vscode.commands.registerCommand('loco.openSettings', () => settings_1.Settings.openSettings());
    const commands = [
        vscode.commands.registerCommand('loco.setLocalEndpoint', async () => {
            const endpoint = await vscode.window.showInputBox({
                prompt: 'Set the local language-model server endpoint',
                placeHolder: 'http://localhost:9000',
                validateInput: value => {
                    if (!value)
                        return undefined;
                    try {
                        const parsed = new URL(value);
                        return ['http:', 'https:'].includes(parsed.protocol) ? undefined : 'Use an http or https URL.';
                    }
                    catch {
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
            if (!service)
                return;
            const apiKey = await vscode.window.showInputBox({
                prompt: `Enter the ${service === 'azure' ? 'Azure OpenAI' : 'OpenAI'} API key (stored securely by VS Code)`,
                password: true,
                ignoreFocusOut: true
            });
            if (!apiKey?.trim())
                return;
            await secrets_1.Secrets.setApiKey(context, service, apiKey);
            vscode.window.showInformationMessage(`${service === 'azure' ? 'Azure OpenAI' : 'OpenAI'} API key stored securely.`);
        }),
        vscode.commands.registerCommand('loco.clearApiKey', async () => {
            const service = await vscode.window.showQuickPick(['openai', 'azure'], { placeHolder: 'Select the key to remove' });
            if (!service)
                return;
            await secrets_1.Secrets.clearApiKey(context, service);
            vscode.window.showInformationMessage('Cloud API key removed.');
        }),
        vscode.commands.registerCommand('loco.viewHistory', () => {
            const history = (0, historyStore_1.getAllHistory)(context);
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
            const history = (0, historyStore_1.getAllHistory)(context);
            if (!history.length) {
                vscode.window.showInformationMessage('No history to export.');
                return;
            }
            const file = await vscode.window.showSaveDialog({
                defaultUri: vscode.Uri.file(`loco-history-${Date.now()}.json`),
                filters: { JSON: ['json'] }
            });
            if (!file)
                return;
            await vscode.workspace.fs.writeFile(file, Buffer.from(JSON.stringify(history, null, 2), 'utf8'));
            vscode.window.showInformationMessage('LOCO history exported.');
        }),
        vscode.commands.registerCommand('loco.clearHistory', async () => {
            const choice = await vscode.window.showWarningMessage('Clear all stored LOCO history?', { modal: true }, 'Clear History');
            if (choice === 'Clear History') {
                await (0, historyStore_1.clearHistory)(context);
                vscode.window.showInformationMessage('LOCO history cleared.');
            }
        })
    ];
    context.subscriptions.push(outputView, vscode.window.registerWebviewViewProvider(webview_1.SidebarChatProvider.viewType, sidebarProvider), runAgentCommand, openAssistantCommand, openSettingsCommand, ...commands);
    outputView.log('LOCO Agent activated');
}
function deactivate() {
    // VS Code disposes registered resources through ExtensionContext.subscriptions.
}
//# sourceMappingURL=extension.js.map