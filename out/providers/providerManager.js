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
const routingPolicy_1 = require("../core/routingPolicy");
class ProviderManager {
    context;
    localProvider;
    cloudProvider;
    routingPolicy;
    constructor(context) {
        this.context = context;
        this.localProvider = new localProvider_1.LocalProvider(context);
        this.cloudProvider = new cloudProvider_1.CloudProvider(context);
        this.routingPolicy = new routingPolicy_1.RoutingPolicy();
    }
    getProvider() {
        try {
            const value = vscode.workspace.getConfiguration('locoAgent').get('provider', 'local');
            return value || 'local';
        }
        catch {
            return 'local';
        }
    }
    setProvider(provider) {
        try {
            void vscode.workspace.getConfiguration('locoAgent').update('provider', provider, vscode.ConfigurationTarget.Global);
        }
        catch {
            // no-op
        }
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
        const selectedProvider = this.routingPolicy.selectProvider(routingContext);
        if (selectedProvider === 'local') {
            const localReply = await this.localProvider.ask({ prompt, task: normalizedTask.context });
            if (localReply && typeof localReply === 'object' && 'text' in localReply && localReply.text) {
                return String(localReply.text);
            }
            if (typeof localReply === 'string' && localReply.trim()) {
                return localReply;
            }
        }
        const cloudReply = await this.cloudProvider.ask(prompt, { model: 'gpt-4o-mini', temperature: 0.2, maxTokens: 1200 });
        if (cloudReply && cloudReply.trim()) {
            return cloudReply;
        }
        return 'Unable to generate a response from the selected provider.';
    }
}
exports.ProviderManager = ProviderManager;
//# sourceMappingURL=providerManager.js.map