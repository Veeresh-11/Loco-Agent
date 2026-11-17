"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.addHistoryEntry = addHistoryEntry;
exports.getAllHistory = getAllHistory;
exports.getRecentHistory = getRecentHistory;
exports.clearHistory = clearHistory;
exports.pruneHistory = pruneHistory;
async function addHistoryEntry(context, entry) {
    try {
        const hist = context.globalState.get('loco.history', []);
        hist.push(entry);
        await context.globalState.update('loco.history', hist);
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
        const cutoffTime = Date.now() - days * 24 * 60 * 60 * 1000;
        const pruned = hist.filter(e => e.timestamp >= cutoffTime);
        await context.globalState.update('loco.history', pruned);
    }
    catch { }
}
//# sourceMappingURL=historyStore.js.map