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
exports.WebviewHost = void 0;
const vscode = __importStar(require("vscode"));
class WebviewHost {
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
        this.panel = vscode.window.createWebviewPanel('locoChat', 'LOCO Chat', vscode.ViewColumn.Beside, { enableScripts: true, retainContextWhenHidden: true });
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
exports.WebviewHost = WebviewHost;
//# sourceMappingURL=webviewHost.js.map