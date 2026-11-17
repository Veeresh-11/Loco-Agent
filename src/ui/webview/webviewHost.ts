import * as vscode from 'vscode';

export class WebviewHost {
  private panel?: vscode.WebviewPanel;

  constructor(private context: vscode.ExtensionContext) {}

  show() {
    if (this.panel) {
      this.panel.reveal();
      return;
    }

    this.panel = vscode.window.createWebviewPanel(
      'locoChat',
      'LOCO Chat',
      vscode.ViewColumn.Beside,
      { enableScripts: true, retainContextWhenHidden: true }
    );

    this.panel.webview.html = this.getHtmlContent();
    this.panel.onDidDispose(() => (this.panel = undefined));
  }

  private getHtmlContent(): string {
    return `
      <!DOCTYPE html>
      <html>
      <head>
        <style>
          body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; padding: 20px; }
          .chat { border: 1px solid #ccc; border-radius: 5px; padding: 10px; height: 400px; overflow-y: auto; }
          .message { margin: 5px 0; padding: 5px; background: #f0f0f0; border-radius: 3px; }
          .input { width: 100%; margin-top: 10px; }
        </style>
      </head>
      <body>
        <h2>LOCO Chat</h2>
        <div class="chat" id="chat"></div>
        <input class="input" type="text" id="input" placeholder="Ask me anything...">
      </body>
      </html>
    `;
  }
}
