import * as vscode from 'vscode';

export class HomePage {
  private panel?: vscode.WebviewPanel;

  constructor(private context: vscode.ExtensionContext) {}

  show() {
    if (this.panel) {
      this.panel.reveal();
      return;
    }

    this.panel = vscode.window.createWebviewPanel('locoHome', 'LOCO Home', vscode.ViewColumn.One, {
      enableScripts: true,
      retainContextWhenHidden: true,
    });

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
          h1 { color: #007acc; }
          .section { margin: 20px 0; padding: 10px; background: #f3f3f3; border-radius: 5px; }
        </style>
      </head>
      <body>
        <h1>Welcome to LOCO Agent</h1>
        <div class="section">
          <h2>Quick Start</h2>
          <p>1. Open Settings: Ctrl+, → search "locoAgent"</p>
          <p>2. Choose your provider (local, openai, azure)</p>
          <p>3. Start typing code to see inline completions</p>
        </div>
        <div class="section">
          <h2>Features</h2>
          <ul>
            <li>Copilot-like inline completions</li>
            <li>Local + cloud provider routing</li>
            <li>30-day conversation history</li>
            <li>Smart fallback support</li>
          </ul>
        </div>
        <div class="section">
          <h2>Keybindings</h2>
          <ul>
            <li><code>Tab</code> - Accept suggestion</li>
            <li><code>Alt+]</code> - Next suggestion</li>
            <li><code>Esc</code> - Dismiss</li>
          </ul>
        </div>
      </body>
      </html>
    `;
  }
}
