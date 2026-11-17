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
exports.ProviderManager = void 0;
const vscode = __importStar(require("vscode"));
const localProvider_1 = require("./localProvider");
const cloudProvider_1 = require("./cloudProvider");
const openaiProvider_1 = require("./openaiProvider");
const azureProvider_1 = require("./azureProvider");
const ollamaProvider_1 = require("./ollamaProvider");
const gpt4allProvider_1 = require("./gpt4allProvider");
const providerFallback_1 = require("./providerFallback");
const routingPolicy_1 = require("../core/routingPolicy");
class ProviderManager {
    context;
    localProvider;
    cloudProvider;
    openAIProvider;
    azureProvider;
    ollamaProvider;
    gpt4allProvider;
    routingPolicy;
    constructor(context) {
        this.context = context;
        this.localProvider = new localProvider_1.LocalProvider(context);
        this.cloudProvider = new cloudProvider_1.CloudProvider(context);
        this.openAIProvider = new openaiProvider_1.OpenAIProvider(context);
        this.azureProvider = new azureProvider_1.AzureProvider(context);
        this.ollamaProvider = new ollamaProvider_1.OllamaProvider(context);
        this.gpt4allProvider = new gpt4allProvider_1.GPT4AllProvider(context);
        this.routingPolicy = new routingPolicy_1.RoutingPolicy();
    }
    getProvider() {
        try {
            const config = vscode.workspace.getConfiguration();
            const value = config.get('loco.provider') ?? config.get('locoAgent.provider', 'local');
            return ['local', 'cloud', 'openai', 'azure', 'ollama', 'gpt4all'].includes(value) ? value : 'local';
        }
        catch {
            return 'local';
        }
    }
    async setProvider(provider) {
        await vscode.workspace.getConfiguration().update('loco.provider', provider, vscode.ConfigurationTarget.Global);
    }
    getLocalProvider() {
        return this.localProvider;
    }
    getCloudProvider() {
        return this.cloudProvider;
    }
    async generate(prompt, task) {
        const normalizedTask = {
            complexity: task?.complexity ?? 'medium',
            estimatedTokens: task?.estimatedTokens ?? Math.max(256, Math.ceil(prompt.length / 4)),
            context: task?.context ?? 'general'
        };
        const routingContext = {
            tokenCount: normalizedTask.estimatedTokens,
            taskComplexity: normalizedTask.complexity,
            isCodeTask: normalizedTask.context.includes('code') || /fix|refactor|implement|debug|patch/.test(prompt.toLowerCase()),
            projectContext: normalizedTask.context,
            preferLocal: true
        };
        const preference = this.getProvider();
        const route = preference === 'local' || preference === 'cloud'
            ? this.routingPolicy.selectProvider(routingContext)
            : preference;
        const config = vscode.workspace.getConfiguration('locoAgent');
        const cloudModel = config.get('model', 'gpt-4o-mini');
        const localModel = config.get('localModel', 'llama3.1');
        const attempt = async (provider) => {
            switch (provider) {
                case 'azure':
                    return this.azureProvider.ask(prompt);
                case 'openai':
                case 'cloud':
                    return this.openAIProvider.ask(prompt);
                case 'ollama':
                    return this.ollamaProvider.ask(prompt);
                case 'gpt4all':
                    return this.gpt4allProvider.ask(prompt);
                default: {
                    const localReply = await this.localProvider.ask({ prompt, task: normalizedTask.context, model: localModel });
                    return typeof localReply === 'string' ? localReply : localReply?.text;
                }
            }
        };
        const fallback = ['local', 'ollama', 'gpt4all'].includes(route) ? 'openai' : 'local';
        const results = await (0, providerFallback_1.runWithProviderFallback)([route, fallback], attempt);
        if (results.value)
            return results.value;
        const errors = results.failures.map(failure => `${failure.provider}: ${failure.error}`).join(' | ');
        throw new Error(errors ? `All configured providers failed. ${errors}` : 'No configured provider returned a response.');
    }
}
exports.ProviderManager = ProviderManager;
//# sourceMappingURL=providerManager.js.map