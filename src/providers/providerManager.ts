import * as vscode from 'vscode';
import { LocalProvider } from './localProvider';
import { CloudProvider } from './cloudProvider';
import { OpenAIProvider } from './openaiProvider';
import { AzureProvider } from './azureProvider';
import { OllamaProvider } from './ollamaProvider';
import { GPT4AllProvider } from './gpt4allProvider';
import { runWithProviderFallback } from './providerFallback';
import { RoutingPolicy, type RoutingContext } from '../core/routingPolicy';

export type ProviderName = 'local' | 'cloud' | 'openai' | 'azure' | 'ollama' | 'gpt4all';

export class ProviderManager {
  private readonly localProvider: LocalProvider;
  private readonly cloudProvider: CloudProvider;
  private readonly openAIProvider: OpenAIProvider;
  private readonly azureProvider: AzureProvider;
  private readonly ollamaProvider: OllamaProvider;
  private readonly gpt4allProvider: GPT4AllProvider;
  private readonly routingPolicy: RoutingPolicy;

  constructor(private readonly context: vscode.ExtensionContext) {
    this.localProvider = new LocalProvider(context);
    this.cloudProvider = new CloudProvider(context);
    this.openAIProvider = new OpenAIProvider(context);
    this.azureProvider = new AzureProvider(context);
    this.ollamaProvider = new OllamaProvider(context);
    this.gpt4allProvider = new GPT4AllProvider(context);
    this.routingPolicy = new RoutingPolicy();
  }

  getProvider(): ProviderName {
    try {
      const config = vscode.workspace.getConfiguration();
      const value = config.get<string>('loco.provider') ?? config.get<string>('locoAgent.provider', 'local');
      return ['local', 'cloud', 'openai', 'azure', 'ollama', 'gpt4all'].includes(value) ? value as ProviderName : 'local';
    } catch { return 'local'; }
  }

  async setProvider(provider: ProviderName): Promise<void> {
    await vscode.workspace.getConfiguration().update('loco.provider', provider, vscode.ConfigurationTarget.Global);
  }
  getLocalProvider(): LocalProvider { return this.localProvider; }
  getCloudProvider(): CloudProvider { return this.cloudProvider; }

  private selectRoute(prompt: string, task?: { complexity?: string; estimatedTokens?: number; context?: string }): { route: ProviderName; fallback: ProviderName; localModel: string } {
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
    const preference = this.getProvider();
    const route = preference === 'local' || preference === 'cloud' ? this.routingPolicy.selectProvider(routingContext) : preference;
    const localModel = vscode.workspace.getConfiguration('locoAgent').get<string>('localModel', 'llama3.1');
    return { route, fallback: ['local', 'ollama', 'gpt4all'].includes(route) ? 'openai' : 'local', localModel };
  }

  async generate(prompt: string, task?: { complexity?: string; estimatedTokens?: number; context?: string }): Promise<string> {
    const { route, fallback, localModel } = this.selectRoute(prompt, task);
    const attempt = async (provider: ProviderName): Promise<string | undefined> => {
      switch (provider) {
        case 'azure': return this.azureProvider.ask(prompt);
        case 'openai':
        case 'cloud': return this.openAIProvider.ask(prompt);
        case 'ollama': return this.ollamaProvider.ask(prompt);
        case 'gpt4all': return this.gpt4allProvider.ask(prompt);
        default: {
          const localReply = await this.localProvider.ask({ prompt, model: localModel });
          return typeof localReply === 'string' ? localReply : localReply?.text;
        }
      }
    };
    const results = await runWithProviderFallback([route, fallback], attempt);
    if (results.value) return results.value;
    const errors = results.failures.map(failure => `${failure.provider}: ${failure.error}`).join(' | ');
    throw new Error(errors ? `All configured providers failed. ${errors}` : 'No configured provider returned a response.');
  }

  async generateStream(prompt: string, onChunk: (chunk: string) => void, task?: { complexity?: string; estimatedTokens?: number; context?: string }): Promise<void> {
    const { route, fallback, localModel } = this.selectRoute(prompt, task);
    const attempt = async (provider: ProviderName, emit: (chunk: string) => void): Promise<void> => {
      switch (provider) {
        case 'azure': return this.azureProvider.stream(prompt, emit);
        case 'openai':
        case 'cloud': return this.openAIProvider.stream(prompt, emit);
        case 'ollama': return this.localProvider.stream({ prompt, provider: 'ollama', model: localModel }, emit);
        case 'gpt4all': return this.localProvider.stream({ prompt, provider: 'gpt4all', model: 'gpt4all' }, emit);
        default: return this.localProvider.stream({ prompt, model: localModel }, emit);
      }
    };

    let emitted = false;
    const errors: string[] = [];
    for (const provider of [route, fallback]) {
      let providerEmitted = false;
      try {
        await attempt(provider, chunk => { providerEmitted = true; emitted = true; onChunk(chunk); });
        if (providerEmitted || emitted) return;
        throw new Error('Provider completed without emitting any response text.');
      } catch (error) {
        errors.push(`${provider}: ${error instanceof Error ? error.message : String(error)}`);
        if (providerEmitted || emitted) throw error;
      }
    }
    throw new Error(`All configured streaming providers failed. ${errors.join(' | ')}`);
  }
}
