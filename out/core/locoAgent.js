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
exports.LOCOAgent = void 0;
const vscode = __importStar(require("vscode"));
const cp = __importStar(require("child_process"));
const util = __importStar(require("util"));
const providerManager_1 = require("../providers/providerManager");
const historyStore_1 = require("../utils/historyStore");
const ragEngine_1 = require("./ragEngine");
const jobQueue_1 = require("./jobQueue");
const threeWayMerge_apply_1 = require("./threeWayMerge.apply");
class LOCOAgent {
    context;
    output;
    providerManager;
    ragEngine;
    jobQueue;
    execFile = util.promisify(cp.execFile);
    constructor(context, output) {
        this.context = context;
        this.output = output;
        this.providerManager = new providerManager_1.ProviderManager(context);
        this.ragEngine = new ragEngine_1.RAGEngine();
        const configuredConcurrency = vscode.workspace.getConfiguration('locoAgent').get('maxConcurrency', 2);
        this.jobQueue = new jobQueue_1.JobQueue(Math.min(16, Math.max(1, Math.floor(configuredConcurrency))));
    }
    async runTask(task) {
        const normalizedTask = {
            ...task,
            prompt: (task.prompt ?? task.task ?? '').trim(),
            complexity: task.complexity ?? this.inferComplexity(task.prompt ?? ''),
            estimatedTokens: task.estimatedTokens ?? Math.max(256, Math.ceil((task.prompt ?? '').length / 4))
        };
        return this.jobQueue.enqueue(normalizedTask, async (job) => {
            try {
                await (0, historyStore_1.addHistoryEntry)(this.context, {
                    id: `${Date.now()}-${Math.random().toString(16).slice(2)}`,
                    role: 'user',
                    text: job.prompt ?? '',
                    timestamp: Date.now()
                });
            }
            catch {
                // history storage is optional
            }
            const plan = this.buildPlan(job);
            const workspaceContext = await this.ragEngine.retrieve(job.prompt ?? '', 5);
            const fileText = job.filePath ? await this.readFile(job.filePath) : '';
            const shellStatus = await this.executeShell('git rev-parse --show-toplevel');
            let response = await this.providerManager.generate(this.composePrompt(job, plan, workspaceContext, fileText, shellStatus), {
                complexity: job.complexity,
                estimatedTokens: job.estimatedTokens,
                context: job.context ?? 'general'
            });
            if (!response || !response.trim()) {
                response = `Plan: ${plan.join(' > ')}\nI inspected the workspace and prepared a response, but no provider text was returned.`;
            }
            try {
                await (0, historyStore_1.addHistoryEntry)(this.context, {
                    id: `${Date.now()}-${Math.random().toString(16).slice(2)}`,
                    role: 'agent',
                    text: response,
                    timestamp: Date.now()
                });
            }
            catch {
                // history storage is optional
            }
            this.output.appendLine(`LOCO task completed: ${job.prompt ?? 'general request'}`);
            return response;
        });
    }
    async ask(job) {
        return this.runTask(job);
    }
    async askStream(job, onChunk) {
        const text = await this.runTask(job);
        onChunk(text);
    }
    async readFile(filePath) {
        try {
            const uri = vscode.Uri.file(filePath);
            const content = await vscode.workspace.fs.readFile(uri);
            return Buffer.from(content).toString('utf8');
        }
        catch {
            return '';
        }
    }
    async executeShell(command) {
        const allowedCommands = {
            'git rev-parse --show-toplevel': ['rev-parse', '--show-toplevel'],
            'git status --short --branch': ['status', '--short', '--branch']
        };
        const args = allowedCommands[command.trim()];
        if (!args) {
            throw new Error('Shell execution is restricted to approved, read-only Git status commands.');
        }
        const cwd = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
        if (!cwd)
            return 'No workspace folder is open.';
        try {
            const { stdout } = await this.execFile('git', args, { cwd, timeout: 5000, windowsHide: true, maxBuffer: 1024 * 1024 });
            return stdout.trim();
        }
        catch (error) {
            return `Git status unavailable: ${String(error instanceof Error ? error.message : error)}`;
        }
    }
    async applyPatch(filePath, patch, baseSnapshot) {
        if (!filePath || !patch.trim()) {
            return 'No patch to apply.';
        }
        try {
            const uri = vscode.Uri.file(filePath);
            const current = await vscode.workspace.fs.readFile(uri);
            const original = Buffer.from(current).toString('utf8');
            const merge = (0, threeWayMerge_apply_1.applyThreeWayMerge)({
                base: baseSnapshot ?? original,
                local: original,
                remote: patch
            });
            await vscode.workspace.fs.writeFile(uri, Buffer.from(merge.merged, 'utf8'));
            return merge.conflicts
                ? `Applied patch to ${filePath} with ${merge.conflicts} conflict(s). Review the LOCAL/REMOTE markers.`
                : `Applied patch to ${filePath}`;
        }
        catch {
            return `Patch could not be written to ${filePath}.`;
        }
    }
    buildPlan(job) {
        const task = job.prompt ?? job.task ?? 'General coding task';
        const plan = ['Inspect workspace', 'Gather relevant snippets', 'Reason about the task'];
        if (task.toLowerCase().includes('fix'))
            plan.push('Patch the root cause');
        if (task.toLowerCase().includes('refactor') || task.toLowerCase().includes('improve'))
            plan.push('Simplify implementation');
        if (task.toLowerCase().includes('write') || task.toLowerCase().includes('create'))
            plan.push('Draft new implementation');
        plan.push('Finish with a concise answer');
        return plan;
    }
    composePrompt(job, plan, relevance, fileText, shellStatus) {
        const query = job.prompt ?? job.task ?? 'General coding task';
        const workspaceSnippet = relevance.length
            ? relevance.map(item => `[${item.path}]\n${item.text}`).join('\n\n')
            : 'No workspace snippet was retrieved.';
        return [
            'You are LOCO, a production-focused coding assistant.',
            'Follow this plan: ' + plan.join(' -> '),
            '',
            'Task:',
            query,
            '',
            'Workspace context:',
            workspaceSnippet,
            '',
            'Current file contents:',
            fileText || 'No file content is available yet.',
            '',
            'Project status:',
            shellStatus || 'Project root could not be resolved.',
            '',
            'Respond with a useful, actionable answer or code patch.'
        ].join('\n');
    }
    inferComplexity(input) {
        const text = input.toLowerCase();
        if (/(refactor|rewrite|architecture|debug|root cause|migration|optimize)/.test(text)) {
            return 'complex';
        }
        if (/(fix|implement|add|update|review)/.test(text)) {
            return 'medium';
        }
        return 'simple';
    }
    getProviderManager() {
        return this.providerManager;
    }
}
exports.LOCOAgent = LOCOAgent;
//# sourceMappingURL=locoAgent.js.map