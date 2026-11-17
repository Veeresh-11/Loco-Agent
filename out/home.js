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
exports.HomePage = void 0;
const vscode = __importStar(require("vscode"));
class HomePage {
    context;
    panel;
    constructor(context) {
        this.context = context;
    }
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
    getHtmlContent() {
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
exports.HomePage = HomePage;
//# sourceMappingURL=home.js.map