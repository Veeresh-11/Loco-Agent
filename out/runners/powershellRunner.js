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
exports.PowerShellRunner = void 0;
const vscode = __importStar(require("vscode"));
const cp = __importStar(require("child_process"));
const util = __importStar(require("util"));
class PowerShellRunner {
    static execFile = util.promisify(cp.execFile);
    static async run(script) {
        const trimmed = script.trim();
        if (!trimmed)
            return 'No PowerShell script was provided.';
        if (trimmed.length > 20_000)
            return 'Script rejected: it exceeds the 20,000 character safety limit.';
        const confirm = await vscode.window.showWarningMessage('Run this PowerShell script on your machine? Review the script before continuing.', { modal: true }, 'Run Script');
        if (confirm !== 'Run Script')
            return 'Script execution cancelled.';
        try {
            const { stdout, stderr } = await this.execFile('powershell.exe', ['-NoLogo', '-NoProfile', '-NonInteractive', '-Command', trimmed], { windowsHide: true, timeout: 30_000, maxBuffer: 1024 * 1024 });
            return (stdout || stderr || 'Script completed without output.').trim().slice(0, 20_000);
        }
        catch (error) {
            const detail = error instanceof Error ? error.message : String(error);
            return `PowerShell execution failed: ${detail.slice(0, 1000)}`;
        }
    }
}
exports.PowerShellRunner = PowerShellRunner;
//# sourceMappingURL=powershellRunner.js.map