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
exports.LocalProvider = void 0;
const vscode = __importStar(require("vscode"));
const historyStore_1 = require("../utils/historyStore");
const DEFAULT_LOCAL_ENDPOINTS = [
    'http://localhost:11434',
    'http://127.0.0.1:11434',
    'http://localhost:8000',
    'http://127.0.0.1:8000',
    'http://localhost:9000',
    'http://127.0.0.1:9000'
];
class LocalProvider {
    context;
    endpointKey = 'loco.localEndpoint';
    constructor(context) {
        this.context = context;
    }
    async getEndpoint() {
        const config = vscode.workspace.getConfiguration('locoAgent');
        const stored = this.context.globalState.get(this.endpointKey) ?? config.get('localEndpoint', 'http://localhost:11434');
        const candidates = Array.from(new Set([stored, ...DEFAULT_LOCAL_ENDPOINTS]));
        for (const endpoint of candidates) {
            if (!endpoint) {
                continue;
            }
            try {
                const ok = await this.checkEndpoint(endpoint);
                if (ok) {
                    await this.context.globalState.update(this.endpointKey, endpoint);
                    return endpoint;
                }
            }
            catch {
                // continue to the next endpoint
            }
        }
        return 'http://localhost:11434';
    }
    async ask(job) {
        const endpoint = await this.getEndpoint();
        const prompt = job?.prompt || this.buildPrompt(job);
        try {
            const response = await fetch(`${endpoint.replace(/\/$/, '')}/api/chat`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    model: 'llama3.1',
                    stream: false,
                    messages: [{ role: 'user', content: prompt }]
                })
            });
            if (!response.ok) {
                const fallback = await this.tryGenerateFallback(endpoint, prompt);
                return fallback;
            }
            const json = await response.json();
            const text = json?.message?.content ?? json?.response ?? json?.content ?? '';
            return typeof text === 'string' ? { text } : { text: JSON.stringify(text) };
        }
        catch {
            try {
                const fallback = await this.tryGenerateFallback(endpoint, prompt);
                return fallback;
            }
            catch {
                return null;
            }
        }
    }
    async stream(job, onChunk) {
        const result = await this.ask(job);
        if (result && typeof result === 'object' && 'text' in result && result.text) {
            onChunk(result.text);
            return;
        }
        if (typeof result === 'string' && result) {
            onChunk(result);
        }
    }
    async estimateLatency() {
        return 150;
    }
    async checkEndpoint(endpoint) {
        const candidate = endpoint.replace(/\/$/, '');
        try {
            const response = await fetch(`${candidate}/api/tags`, { method: 'GET', headers: { Accept: 'application/json' } });
            return response.ok;
        }
        catch {
            try {
                const response = await fetch(`${candidate}/health`, { method: 'GET' });
                return response.ok;
            }
            catch {
                return false;
            }
        }
    }
    async tryGenerateFallback(endpoint, prompt) {
        const candidate = endpoint.replace(/\/$/, '');
        const response = await fetch(`${candidate}/api/generate`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                model: 'llama3.1',
                prompt,
                stream: false
            })
        });
        if (!response.ok) {
            return { text: 'Local provider is unavailable.' };
        }
        const json = await response.json();
        const generatedText = json?.response ?? json?.text ?? '';
        return { text: String(generatedText || 'Local generate endpoint responded without text.') };
    }
    buildPrompt(job) {
        const task = job?.task ?? job?.prompt ?? 'General assistance';
        const history = (0, historyStore_1.getRecentHistory)(this.context, 30);
        const recent = history.slice(-10).map(entry => `${entry.role}: ${entry.text}`).join('\n');
        return [
            'You are LOCO, a helpful coding assistant.',
            'Task: ' + task,
            'Context: ' + (job?.context ?? 'general'),
            'Current file:',
            job?.code ?? '',
            'Recent history:',
            recent || 'No recent history.',
            'Answer with a practical response or code patch.'
        ].join('\n');
    }
}
exports.LocalProvider = LocalProvider;
//# sourceMappingURL=localProvider.js.map