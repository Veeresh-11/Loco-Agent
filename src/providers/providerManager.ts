import * as vscode from 'vscode';
import { LocalProvider } from './localProvider';
import { CloudProvider } from './cloudProvider';
import { RoutingPolicy, type RoutingContext } from '../core/routingPolicy';

export type ProviderName = 'local' | 'cloud' | 'openai' | 'azure' | 'ollama' | 'gpt4all';

export class ProviderManager {
  private readonly localProvider: LocalProvider;
  private readonly cloudProvider: CloudProvider;
  private readonly routingPolicy: RoutingPolicy;

  constructor(private readonly context: vscode.ExtensionContext) {
    this.localProvider = new LocalProvider(context);
    this.cloudProvider = new CloudProvider(context);
    this.routingPolicy = new RoutingPolicy();
  }

  getProvider(): ProviderName {
    try {
      const value = vscode.workspace.getConfiguration('locoAgent').get<string>('provider', 'local');
      return (value as ProviderName) || 'local';
    } catch {
      return 'local';
    }
  }

  setProvider(provider: ProviderName) {
    try {
      void vscode.workspace.getConfiguration('locoAgent').update('provider', provider, vscode.ConfigurationTarget.Global);
    } catch {
      // no-op
    }
  }

  getLocalProvider(): LocalProvider {
    return this.localProvider;
  }

  getCloudProvider(): CloudProvider {
    return this.cloudProvider;
  }

  async generate(prompt: string, task?: { complexity?: string; estimatedTokens?: number; context?: string }): Promise<string> {
    const normalizedTask = {
      complexity: task?.complexity ?? 'medium',
      estimatedTokens: task?.estimatedTokens ?? Math.max(256, Math.ceil(prompt.length / 4)),
      context: task?.context ?? 'general'
    };

    const routingContext: RoutingContext = {
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
