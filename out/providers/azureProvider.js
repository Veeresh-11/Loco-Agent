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
exports.AzureProvider = void 0;
const vscode = __importStar(require("vscode"));
const secrets_1 = require("../utils/secrets");
class AzureProvider {
    context;
    constructor(context) {
        this.context = context;
    }
    async ask(prompt) {
        const config = vscode.workspace.getConfiguration('locoAgent');
        const endpoint = config.get('azureEndpoint', '').replace(/\/$/, '');
        const deployment = config.get('azureDeployment', '');
        const apiVersion = config.get('azureApiVersion', '2024-10-21');
        const apiKey = await secrets_1.Secrets.getApiKey(this.context, 'azure') || process.env.AZURE_OPENAI_API_KEY;
        if (!endpoint || !deployment || !apiKey) {
            throw new Error('Configure locoAgent.azureEndpoint, locoAgent.azureDeployment, and the Azure API key first.');
        }
        const controller = new AbortController();
        const timeout = setTimeout(() => controller.abort(), 60_000);
        try {
            const url = `${endpoint}/openai/deployments/${encodeURIComponent(deployment)}/chat/completions?api-version=${encodeURIComponent(apiVersion)}`;
            const response = await fetch(url, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json', 'api-key': apiKey },
                body: JSON.stringify({ messages: [{ role: 'user', content: prompt }], temperature: 0.2, max_tokens: 1200 }),
                signal: controller.signal
            });
            if (!response.ok)
                throw new Error(`Azure OpenAI request failed (${response.status}): ${(await response.text()).slice(0, 500)}`);
            const json = await response.json();
            const text = json?.choices?.[0]?.message?.content;
            if (typeof text !== 'string' || !text.trim())
                throw new Error('Azure OpenAI returned an empty response.');
            return text;
        }
        finally {
            clearTimeout(timeout);
        }
    }
}
exports.AzureProvider = AzureProvider;
//# sourceMappingURL=azureProvider.js.map