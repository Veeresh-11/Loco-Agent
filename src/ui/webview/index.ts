import * as vscode from 'vscode';
import type { LOCOAgent } from '../../core/locoAgent';

export class SidebarChatProvider implements vscode.WebviewViewProvider {
  public static readonly viewType = 'locoAgent.chatView';

  private view?: vscode.WebviewView;

  constructor(
    private readonly context: vscode.ExtensionContext,
    private readonly agent: LOCOAgent
  ) {}

  public resolveWebviewView(
    webviewView: vscode.WebviewView,
    _context: vscode.WebviewViewResolveContext,
    _token: vscode.CancellationToken
  ): void | Thenable<void> {
    this.view = webviewView;
    webviewView.webview.options = {
      enableScripts: true,
      localResourceRoots: [this.context.extensionUri]
    };
    webviewView.webview.html = this.getHtml();

    webviewView.webview.onDidReceiveMessage(async message => {
      if (!message || typeof message.prompt !== 'string') return;
      const prompt = message.prompt.trim();
      if (!prompt) return;

      webviewView.webview.postMessage({ type: 'typing', value: true });
      try {
        const editor = vscode.window.activeTextEditor;
        const reply = await this.agent.runTask({
          prompt,
          context: 'chat',
          filePath: editor?.document.uri.fsPath,
          code: editor?.document.getText(editor.selection) || editor?.document.getText()
        });
        webviewView.webview.postMessage({ type: 'reply', value: reply });
      } catch (error) {
        webviewView.webview.postMessage({
          type: 'reply',
          value: `LOCO could not complete the request: ${error instanceof Error ? error.message : String(error)}`
        });
      } finally {
        webviewView.webview.postMessage({ type: 'typing', value: false });
      }
    });
  }

  public show(): void {
    if (this.view) {
      this.view.show(true);
    }
  }

  private getHtml(): string {
    return `
      <!DOCTYPE html>
      <html lang="en">
      <head>
        <meta charset="UTF-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1.0" />
        <style>
          :root {
            color-scheme: dark;
            --bg: #1f1f1f;
            --panel: #252526;
            --panel-alt: #2d2d30;
            --border: #3c3c3c;
            --text: #f5f5f5;
            --muted: #c4c4c4;
            --accent: #4ea1ff;
            --success: #4ec68f;
          }

          * { box-sizing: border-box; }
          html, body {
            margin: 0;
            height: 100%;
            font-family: var(--vscode-font-family);
            background: var(--bg);
            color: var(--text);
          }

          body {
            display: flex;
            flex-direction: column;
            height: 100vh;
            padding: 12px;
          }

          .header {
            display: flex;
            justify-content: space-between;
            align-items: center;
            margin-bottom: 12px;
          }

          .title {
            font-weight: 600;
            font-size: 13px;
            letter-spacing: 0.02em;
          }

          .dot {
            width: 8px;
            height: 8px;
            border-radius: 999px;
            background: var(--success);
            box-shadow: 0 0 10px rgba(78, 198, 143, 0.8);
          }

          .messages {
            flex: 1;
            overflow-y: auto;
            background: var(--panel);
            border: 1px solid var(--border);
            border-radius: 8px;
            padding: 10px;
            min-height: 220px;
          }

          .message {
            margin-bottom: 10px;
            padding: 10px 12px;
            border-radius: 8px;
            font-size: 12px;
            line-height: 1.5;
            white-space: pre-wrap;
            word-break: break-word;
          }

          .message.user {
            background: rgba(78, 161, 255, 0.15);
            border: 1px solid rgba(78, 161, 255, 0.35);
          }

          .message.agent {
            background: rgba(64, 64, 64, 0.8);
            border: 1px solid var(--border);
          }

          .composer {
            display: flex;
            gap: 8px;
            margin-top: 12px;
          }

          textarea {
            flex: 1;
            background: var(--panel-alt);
            color: var(--text);
            border: 1px solid var(--border);
            border-radius: 8px;
            padding: 10px 12px;
            resize: none;
            min-height: 68px;
            font-family: inherit;
            font-size: 12px;
          }

          button {
            background: var(--accent);
            color: white;
            border: none;
            border-radius: 8px;
            padding: 0 14px;
            min-width: 72px;
            font-weight: 600;
            cursor: pointer;
          }

          .typing {
            display: none;
            font-size: 11px;
            color: var(--muted);
            margin-top: 8px;
          }

          .typing.visible {
            display: block;
          }
        </style>
      </head>
      <body>
        <div class="header">
          <div class="title">LOCO Agent</div>
          <div class="dot" aria-label="online"></div>
        </div>

        <div id="messages" class="messages" aria-live="polite"></div>

        <div class="composer">
          <textarea id="input" rows="3" placeholder="Ask LOCO to fix, refactor, analyze, or explain the code..."></textarea>
          <button id="send">Send</button>
        </div>

        <div id="typing" class="typing">LOCO is thinking...</div>

        <script>
          const vscode = acquireVsCodeApi();
          const messages = document.getElementById('messages');
          const input = document.getElementById('input');
          const send = document.getElementById('send');
          const typing = document.getElementById('typing');

          function appendMessage(role, text) {
            const node = document.createElement('div');
            node.className = 'message ' + role;
            node.textContent = text || (role === 'agent' ? 'No response received.' : '');
            messages.appendChild(node);
            messages.scrollTop = messages.scrollHeight;
          }

          send.addEventListener('click', () => {
            const value = input.value.trim();
            if (!value) {
              return;
            }
            appendMessage('user', value);
            vscode.postMessage({ type: 'prompt', prompt: value });
            input.value = '';
          });

          input.addEventListener('keydown', (event) => {
            if (event.key === 'Enter' && !event.shiftKey) {
              event.preventDefault();
              send.click();
            }
          });

          window.addEventListener('message', event => {
            const message = event.data;
            if (!message) {
              return;
            }

            if (message.type === 'typing') {
              typing.classList.toggle('visible', Boolean(message.value));
              return;
            }

            if (message.type === 'reply') {
              appendMessage('agent', String(message.value ?? ''));
            }
          });
        </script>
      </body>
      </html>
    `;
  }
}
