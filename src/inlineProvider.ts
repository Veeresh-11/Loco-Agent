import * as vscode from 'vscode';
import { LOCOAgent } from './core/locoAgent';

export class InlineCompletionProvider implements vscode.InlineCompletionItemProvider {
  constructor(private agent: LOCOAgent) {}

  async provideInlineCompletionItems(document: vscode.TextDocument, position: vscode.Position, context: vscode.InlineCompletionContext, token: vscode.CancellationToken): Promise<vscode.InlineCompletionItem[]> {
    try {
      if (token.isCancellationRequested) return [];

      const prompt = document.getText();
      const text = await this.agent.ask({ prompt, context: document.languageId });

      if (text && text.trim()) {
        return [
          {
            insertText: text,
            range: new vscode.Range(position, position),
          } as vscode.InlineCompletionItem,
        ];
      }
    } catch (e) {
      console.error('Inline provider error:', e);
    }
    return [];
  }
}
