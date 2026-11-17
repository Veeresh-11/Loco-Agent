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
exports.addHistoryEntry = addHistoryEntry;
exports.getAllHistory = getAllHistory;
exports.getRecentHistory = getRecentHistory;
exports.clearHistory = clearHistory;
exports.pruneHistory = pruneHistory;
const vscode = __importStar(require("vscode"));
async function addHistoryEntry(context, entry) {
    try {
        const config = vscode.workspace.getConfiguration('locoAgent');
        if (!config.get('storeHistory', true)) {
            return;
        }
        const hist = context.globalState.get('loco.history', []);
        const maxEntries = Math.max(1, config.get('maxHistoryEntries', 1000));
        const entries = [...hist, entry].slice(-maxEntries);
        await context.globalState.update('loco.history', entries);
    }
    catch { }
}
function getAllHistory(context) {
    try {
        return context.globalState.get('loco.history', []);
    }
    catch {
        return [];
    }
}
function getRecentHistory(context, days) {
    try {
        const hist = getAllHistory(context);
        const cutoffTime = Date.now() - days * 24 * 60 * 60 * 1000;
        return hist.filter(e => e.timestamp >= cutoffTime);
    }
    catch {
        return [];
    }
}
async function clearHistory(context) {
    try {
        await context.globalState.update('loco.history', []);
    }
    catch { }
}
async function pruneHistory(context, days) {
    try {
        const hist = getAllHistory(context);
        const retentionDays = Number.isFinite(days) ? Math.max(1, days) : 30;
        const cutoffTime = Date.now() - retentionDays * 24 * 60 * 60 * 1000;
        const maxEntries = Math.max(1, vscode.workspace.getConfiguration('locoAgent').get('maxHistoryEntries', 1000));
        const pruned = hist
            .filter(e => Number.isFinite(e.timestamp) && e.timestamp >= cutoffTime)
            .sort((a, b) => a.timestamp - b.timestamp)
            .slice(-maxEntries);
        await context.globalState.update('loco.history', pruned);
    }
    catch { }
}
//# sourceMappingURL=historyStore.js.map