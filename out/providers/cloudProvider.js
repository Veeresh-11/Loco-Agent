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
exports.CloudProvider = void 0;
const vscode = __importStar(require("vscode"));
const secrets_1 = require("../utils/secrets");
class CloudProvider {
    context;
    constructor(context) {
        this.context = context;
    }
    async ask(prompt, options = {}) {
        const config = vscode.workspace.getConfiguration();
        const endpoint = config.get('locoAgent.endpoint', 'https://api.openai.com/v1').trim();
        const apiKey = await secrets_1.Secrets.getApiKey(this.context, 'openai') || process.env.OPENAI_API_KEY;
        if (!apiKey) {
            throw new Error('OpenAI API key is not configured. Set it with the LOCO secret command or OPENAI_API_KEY.');
        }
        const controller = new AbortController();
        const timeout = setTimeout(() => controller.abort(), 60_000);
        try {
            const response = await fetch(`${endpoint.replace(/\/$/, '')}/chat/completions`, {
                method: 'POST',
                headers: {
                    'Content-Type': 'application/json',
                    Authorization: `Bearer ${apiKey}`
                },
                body: JSON.stringify({
                    model: options.model ?? 'gpt-4o-mini',
                    temperature: options.temperature ?? 0.2,
                    max_tokens: options.maxTokens ?? 1200,
                    messages: [{ role: 'user', content: prompt }]
                }),
                signal: controller.signal
            });
            if (!response.ok) {
                const details = (await response.text()).slice(0, 500);
                throw new Error(`OpenAI-compatible request failed (${response.status}): ${details}`);
            }
            const json = await response.json();
            const content = json?.choices?.[0]?.message?.content ?? json?.output_text;
            if (typeof content !== 'string' || !content.trim())
                throw new Error('Cloud provider returned an empty response.');
            return content;
        }
        catch (error) {
            throw error instanceof Error ? error : new Error(`Cloud provider error: ${String(error)}`);
        }
        finally {
            clearTimeout(timeout);
        }
    }
}
exports.CloudProvider = CloudProvider;
//# sourceMappingURL=cloudProvider.js.map