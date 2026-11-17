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
exports.Settings = void 0;
const vscode = __importStar(require("vscode"));
class Settings {
    static async openSettings() {
        await vscode.commands.executeCommand('workbench.action.openSettings', '@ext:veeresh.loco-agent');
    }
    static async getProvider() {
        try {
            const config = vscode.workspace.getConfiguration();
            return config.get('loco.provider') ?? config.get('locoAgent.provider', 'local');
        }
        catch {
            return 'local';
        }
    }
    static async setProvider(provider) {
        const allowed = ['local', 'openai', 'azure', 'ollama', 'gpt4all'];
        if (!allowed.includes(provider)) {
            throw new Error(`Unsupported LOCO provider: ${provider}`);
        }
        await vscode.workspace.getConfiguration().update('loco.provider', provider, vscode.ConfigurationTarget.Global);
    }
}
exports.Settings = Settings;
//# sourceMappingURL=settings.js.map