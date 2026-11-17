import * as vscode from 'vscode';
import * as cp from 'child_process';
import * as util from 'util';
import { ProviderManager } from '../providers/providerManager';
import { addHistoryEntry } from '../utils/historyStore';
import { RAGEngine, type RAGChunk } from './ragEngine';
import { JobQueue } from './jobQueue';

export interface AgentTask {
  id?: string;
  task?: string;
  prompt?: string;
  context?: string;
  filePath?: string;
  code?: string;
  baseSnapshot?: string;
  signal?: vscode.CancellationToken;
  complexity?: 'simple' | 'medium' | 'complex';
  estimatedTokens?: number;
}

export class LOCOAgent {
  private readonly providerManager: ProviderManager;
  private readonly ragEngine: RAGEngine;
  private readonly jobQueue: JobQueue<AgentTask>;
  private readonly execFile = util.promisify(cp.exec);

  constructor(private readonly context: vscode.ExtensionContext, private readonly output: vscode.OutputChannel) {
    this.providerManager = new ProviderManager(context);
    this.ragEngine = new RAGEngine();
    this.jobQueue = new JobQueue<AgentTask>(2);
  }

  async runTask(task: AgentTask): Promise<string> {
    const normalizedTask: AgentTask = {
      ...task,
      prompt: (task.prompt ?? task.task ?? '').trim(),
      complexity: task.complexity ?? this.inferComplexity(task.prompt ?? ''),
      estimatedTokens: task.estimatedTokens ?? Math.max(256, Math.ceil((task.prompt ?? '').length / 4))
    };

    return this.jobQueue.enqueue(normalizedTask, async (job) => {
      try {
        await addHistoryEntry(this.context, {
          id: `${Date.now()}-${Math.random().toString(16).slice(2)}`,
          role: 'user',
          text: job.prompt ?? '',
          timestamp: Date.now()
        });
      } catch {
        // history storage is optional
      }

      const plan = this.buildPlan(job);
      const workspaceContext = await this.ragEngine.retrieve(job.prompt ?? '', 5);
      const fileText = job.filePath ? await this.readFile(job.filePath) : '';
      const shellStatus = await this.executeShell('git rev-parse --show-toplevel 2>/dev/null || pwd');

      let response = await this.providerManager.generate(this.composePrompt(job, plan, workspaceContext, fileText, shellStatus), {
        complexity: job.complexity,
        estimatedTokens: job.estimatedTokens,
        context: job.context ?? 'general'
      });

      if (!response || !response.trim()) {
        response = `Plan: ${plan.join(' > ')}\nI inspected the workspace and prepared a response, but no provider text was returned.`;
      }

      try {
        await addHistoryEntry(this.context, {
          id: `${Date.now()}-${Math.random().toString(16).slice(2)}`,
          role: 'agent',
          text: response,
          timestamp: Date.now()
        });
      } catch {
        // history storage is optional
      }

      this.output.appendLine(`LOCO task completed: ${job.prompt ?? 'general request'}`);
      return response;
    });
  }

  async ask(job: AgentTask): Promise<string> {
    return this.runTask(job);
  }

  async askStream(job: AgentTask, onChunk: (chunk: string) => void): Promise<void> {
    const text = await this.runTask(job);
    onChunk(text);
  }

  async readFile(filePath: string): Promise<string> {
    try {
      const uri = vscode.Uri.file(filePath);
      const content = await vscode.workspace.fs.readFile(uri);
      return Buffer.from(content).toString('utf8');
    } catch {
      return '';
    }
  }

  async executeShell(command: string): Promise<string> {
    try {
      const { stdout } = await this.execFile(command);
      return stdout.trim();
    } catch (error) {
      return String(error instanceof Error ? error.message : error);
    }
  }

  async applyPatch(filePath: string, patch: string): Promise<string> {
    if (!filePath || !patch.trim()) {
      return 'No patch to apply.';
    }

    try {
      const uri = vscode.Uri.file(filePath);
      const current = await vscode.workspace.fs.readFile(uri);
      const original = Buffer.from(current).toString('utf8');
      const updated = original + (original.endsWith('\n') ? '' : '\n') + patch;
      await vscode.workspace.fs.writeFile(uri, Buffer.from(updated, 'utf8'));
      return `Applied patch to ${filePath}`;
    } catch {
      return `Patch could not be written to ${filePath}.`;
    }
  }

  private buildPlan(job: AgentTask): string[] {
    const task = job.prompt ?? job.task ?? 'General coding task';
    const plan = ['Inspect workspace', 'Gather relevant snippets', 'Reason about the task'];
    if (task.toLowerCase().includes('fix')) plan.push('Patch the root cause');
    if (task.toLowerCase().includes('refactor') || task.toLowerCase().includes('improve')) plan.push('Simplify implementation');
    if (task.toLowerCase().includes('write') || task.toLowerCase().includes('create')) plan.push('Draft new implementation');
    plan.push('Finish with a concise answer');
    return plan;
  }

  private composePrompt(
    job: AgentTask,
    plan: string[],
    relevance: RAGChunk[],
    fileText: string,
    shellStatus: string
  ): string {
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

  private inferComplexity(input: string): 'simple' | 'medium' | 'complex' {
    const text = input.toLowerCase();
    if (/(refactor|rewrite|architecture|debug|root cause|migration|optimize)/.test(text)) {
      return 'complex';
    }
    if (/(fix|implement|add|update|review)/.test(text)) {
      return 'medium';
    }
    return 'simple';
  }

  getProviderManager(): ProviderManager {
    return this.providerManager;
  }
}
