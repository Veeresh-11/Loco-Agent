import * as vscode from 'vscode';

export interface HistoryEntry {
  id: string;
  role: 'user' | 'agent';
  text: string;
  timestamp: number;
}

export async function addHistoryEntry(context: vscode.ExtensionContext, entry: HistoryEntry): Promise<void> {
  try {
    const config = vscode.workspace.getConfiguration('locoAgent');
    if (!config.get<boolean>('storeHistory', true)) {
      return;
    }
    const hist = context.globalState.get<HistoryEntry[]>('loco.history', []);
    const maxEntries = Math.max(1, config.get<number>('maxHistoryEntries', 1000));
    const entries = [...hist, entry].slice(-maxEntries);
    await context.globalState.update('loco.history', entries);
  } catch {}
}

export function getAllHistory(context: vscode.ExtensionContext): HistoryEntry[] {
  try {
    return context.globalState.get<HistoryEntry[]>('loco.history', []);
  } catch {
    return [];
  }
}

export function getRecentHistory(context: vscode.ExtensionContext, days: number): HistoryEntry[] {
  try {
    const hist = getAllHistory(context);
    const cutoffTime = Date.now() - days * 24 * 60 * 60 * 1000;
    return hist.filter(e => e.timestamp >= cutoffTime);
  } catch {
    return [];
  }
}

export async function clearHistory(context: vscode.ExtensionContext): Promise<void> {
  try {
    await context.globalState.update('loco.history', []);
  } catch {}
}

export async function pruneHistory(context: vscode.ExtensionContext, days: number): Promise<void> {
  try {
    const hist = getAllHistory(context);
    const retentionDays = Number.isFinite(days) ? Math.max(1, days) : 30;
    const cutoffTime = Date.now() - retentionDays * 24 * 60 * 60 * 1000;
    const maxEntries = Math.max(1, vscode.workspace.getConfiguration('locoAgent').get<number>('maxHistoryEntries', 1000));
    const pruned = hist
      .filter(e => Number.isFinite(e.timestamp) && e.timestamp >= cutoffTime)
      .sort((a, b) => a.timestamp - b.timestamp)
      .slice(-maxEntries);
    await context.globalState.update('loco.history', pruned);
  } catch {}
}
