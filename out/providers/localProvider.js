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
const localEndpoint_1 = require("./localEndpoint");
const DEFAULT_LOCAL_PORTS = [9000, 9001, 8080, 7860, 5005, 11434, 8000, 4891];
const PROBE_TIMEOUT_MS = 700;
class LocalProvider {
    context;
    endpointKey = 'loco.localEndpoint';
    constructor(context) {
        this.context = context;
    }
    async getEndpoint() {
        const config = vscode.workspace.getConfiguration();
        const explicit = config.get('loco.localEndpoint')
            || config.get('locoAgent.localEndpoint')
            || this.context.globalState.get(this.endpointKey)
            || this.context.globalState.get('loco.localEndpoint');
        const configuredPorts = config.get('locoAgent.localPorts', DEFAULT_LOCAL_PORTS);
        const ports = Array.from(new Set([...configuredPorts, ...DEFAULT_LOCAL_PORTS]))
            .filter(port => Number.isInteger(port) && port > 0 && port <= 65535);
        const discovered = ports.flatMap(port => [`http://localhost:${port}`, `http://127.0.0.1:${port}`]);
        const candidates = Array.from(new Set([explicit, ...discovered].filter((value) => Boolean(value))));
        const health = await Promise.all(candidates.map(endpoint => this.checkEndpoint(endpoint)));
        const index = health.findIndex(Boolean);
        if (index >= 0) {
            await this.context.globalState.update(this.endpointKey, candidates[index]);
            return candidates[index];
        }
        return '';
    }
    async ask(job) {
        const endpoint = await this.getEndpoint();
        if (!endpoint)
            return null;
        const prompt = job?.prompt || this.buildPrompt(job);
        const model = job.model || vscode.workspace.getConfiguration('locoAgent').get('localModel', 'llama3.1');
        for (const route of (0, localEndpoint_1.getLocalChatRoutes)(endpoint)) {
            try {
                const body = route.format === 'openai'
                    ? { model, messages: [{ role: 'user', content: prompt }], stream: false }
                    : route.format === 'ollama-chat'
                        ? { model, messages: [{ role: 'user', content: prompt }], stream: false }
                        : { model, prompt, stream: false };
                const response = await this.request(endpoint, route.path, body);
                if (!response.ok)
                    continue;
                const json = await response.json();
                const text = route.format === 'openai'
                    ? json?.choices?.[0]?.message?.content
                    : route.format === 'ollama-chat'
                        ? json?.message?.content ?? json?.response ?? json?.content
                        : json?.response ?? json?.text;
                if (typeof text === 'string' && text.trim())
                    return { text: text.trim() };
            }
            catch {
                // Continue through OpenAI-compatible and native local API routes.
            }
        }
        return null;
    }
    async stream(job, onChunk) {
        const result = await this.ask(job);
        const text = typeof result === 'string' ? result : result?.text;
        if (text)
            onChunk(text);
    }
    async estimateLatency() {
        return 150;
    }
    async request(endpoint, path, body) {
        const controller = new AbortController();
        const timeout = setTimeout(() => controller.abort(), 30_000);
        try {
            return await fetch(`${(0, localEndpoint_1.normalizeLocalEndpoint)(endpoint)}${path}`, {
                method: body === undefined ? 'GET' : 'POST',
                headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
                body: body === undefined ? undefined : JSON.stringify(body),
                signal: controller.signal
            });
        }
        finally {
            clearTimeout(timeout);
        }
    }
    async checkEndpoint(endpoint) {
        const candidate = (0, localEndpoint_1.normalizeLocalEndpoint)(endpoint);
        for (const path of (0, localEndpoint_1.getLocalProbePaths)(candidate)) {
            const controller = new AbortController();
            const timeout = setTimeout(() => controller.abort(), PROBE_TIMEOUT_MS);
            try {
                const response = await fetch(`${candidate}${path}`, { signal: controller.signal });
                if (response.ok)
                    return true;
            }
            catch {
                // Continue probing the remaining health endpoints.
            }
            finally {
                clearTimeout(timeout);
            }
        }
        return false;
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