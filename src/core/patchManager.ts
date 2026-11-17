import * as vscode from 'vscode';
import * as fs from 'fs';
import * as path from 'path';
import { randomUUID } from 'crypto';
import { TextDecoder } from 'util';
import { applyThreeWayMerge } from './threeWayMerge.apply';
import { isPathWithinWorkspace } from './patchPath';

const MAX_PATCH_FILE_BYTES = 2 * 1024 * 1024;
const BACKUP_STATE_KEY = 'loco.patchBackups';
const MAX_BACKUPS = 20;

export interface PatchBackupRecord {
  id: string;
  targetPath: string;
  backupPath: string;
  createdAt: number;
}

interface PatchTarget {
  path: string;
  uri: vscode.Uri;
  bytes: Buffer;
  text: string;
  mode: number;
  language: string;
}

export class PatchManager {
  constructor(private readonly context: vscode.ExtensionContext, private readonly output: vscode.OutputChannel) {}

  async previewAndApply(filePath: string, patch: string, baseSnapshot?: string): Promise<string> {
    if (!filePath?.trim() || !patch?.trim()) return 'No patch to apply.';
    if (Buffer.byteLength(patch, 'utf8') > MAX_PATCH_FILE_BYTES) {
      return 'Patch rejected: proposed content exceeds the 2 MiB safety limit.';
    }
    if (baseSnapshot && Buffer.byteLength(baseSnapshot, 'utf8') > MAX_PATCH_FILE_BYTES) {
      return 'Patch rejected: base snapshot exceeds the 2 MiB safety limit.';
    }

    try {
      const target = await this.resolveTarget(filePath);
      const merge = applyThreeWayMerge({
        base: baseSnapshot ?? target.text,
        local: target.text,
        remote: patch
      });

      if (merge.conflicts > 0) {
        return `Patch rejected: ${merge.conflicts} merge conflict(s) detected. No files were changed.`;
      }
      if (merge.merged === target.text) return 'No changes to apply.';

      await this.openDiff(target, merge.merged, 'LOCO Patch Preview');
      const approval = await vscode.window.showInformationMessage(
        `Review the diff for ${path.basename(target.path)}. Apply these changes?`,
        { modal: true },
        'Apply Patch'
      );
      if (approval !== 'Apply Patch') return 'Patch cancelled. No files were changed.';

      const current = await fs.promises.readFile(target.path);
      if (!current.equals(target.bytes)) {
        return 'Patch aborted: the file changed after preview. Generate a fresh preview before applying.';
      }

      const backup = await this.createBackup(target);
      try {
        await this.writeAtomically(target.path, Buffer.from(merge.merged, 'utf8'), target.mode);
      } catch (error) {
        return `Patch write failed. Original content is preserved in backup ${backup.id}. ${this.errorMessage(error)}`;
      }

      this.log(`Patch applied to ${target.path}; backup ${backup.id}`);
      return `Patch applied to ${target.path}. Backup ID: ${backup.id}. Use “LOCO: Restore Patch Backup” to roll back.`;
    } catch (error) {
      this.log(`Patch rejected: ${this.errorMessage(error)}`);
      return `Patch not applied: ${this.errorMessage(error)}`;
    }
  }

  getBackups(): PatchBackupRecord[] {
    return this.context.globalState.get<PatchBackupRecord[]>(BACKUP_STATE_KEY, []);
  }

  async restoreBackup(backupId: string): Promise<string> {
    const record = this.getBackups().find(item => item.id === backupId);
    if (!record) return 'Backup not found. It may have expired or been removed.';

    try {
      const target = await this.resolveTarget(record.targetPath);
      const backupRoot = await fs.promises.realpath(this.context.globalStorageUri.fsPath);
      const backupPath = await fs.promises.realpath(record.backupPath);
      if (!isPathWithinWorkspace(backupRoot, backupPath)) {
        return 'Restore rejected: backup is outside LOCO extension storage.';
      }

      const backupBytes = await fs.promises.readFile(backupPath);
      if (backupBytes.length > MAX_PATCH_FILE_BYTES || backupBytes.includes(0)) {
        return 'Restore rejected: backup is not a supported text file.';
      }
      const restoredText = new TextDecoder('utf-8', { fatal: true }).decode(backupBytes);
      if (restoredText === target.text) return 'The selected backup already matches the current file.';

      await this.openDiff(target, restoredText, 'LOCO Backup Restore Preview');
      const approval = await vscode.window.showWarningMessage(
        `Restore ${path.basename(target.path)} from backup ${backupId}?`,
        { modal: true },
        'Restore Backup'
      );
      if (approval !== 'Restore Backup') return 'Restore cancelled. No files were changed.';

      const current = await fs.promises.readFile(target.path);
      if (!current.equals(target.bytes)) {
        return 'Restore aborted: the file changed after preview. Review a fresh diff before restoring.';
      }

      const safetyBackup = await this.createBackup(target);
      try {
        await this.writeAtomically(target.path, backupBytes, target.mode);
      } catch (error) {
        return `Restore failed. The pre-restore content is preserved in backup ${safetyBackup.id}. ${this.errorMessage(error)}`;
      }

      this.log(`Backup ${backupId} restored to ${target.path}; pre-restore backup ${safetyBackup.id}`);
      return `Backup restored to ${target.path}. Pre-restore backup ID: ${safetyBackup.id}.`;
    } catch (error) {
      this.log(`Restore rejected: ${this.errorMessage(error)}`);
      return `Restore not completed: ${this.errorMessage(error)}`;
    }
  }

  private async resolveTarget(filePath: string): Promise<PatchTarget> {
    const requestedPath = path.resolve(filePath);
    const realTarget = await fs.promises.realpath(requestedPath);
    const stat = await fs.promises.stat(realTarget);
    if (!stat.isFile()) throw new Error('Patch target must be an existing regular file.');
    if (stat.size > MAX_PATCH_FILE_BYTES) throw new Error('Patch target exceeds the 2 MiB safety limit.');

    const workspaceFolders = vscode.workspace.workspaceFolders ?? [];
    if (!workspaceFolders.length) throw new Error('Open a workspace folder before applying or restoring patches.');

    let inWorkspace = false;
    for (const folder of workspaceFolders) {
      try {
        const realRoot = await fs.promises.realpath(folder.uri.fsPath);
        if (isPathWithinWorkspace(realRoot, realTarget)) {
          inWorkspace = true;
          break;
        }
      } catch {
        // Ignore unavailable workspace roots and check the remaining folders.
      }
    }
    if (!inWorkspace) throw new Error('Patch target must resolve to a file inside the open workspace.');

    const bytes = await fs.promises.readFile(realTarget);
    if (bytes.includes(0)) throw new Error('Binary files are not supported by patch preview.');
    const text = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
    const document = await vscode.workspace.openTextDocument(vscode.Uri.file(realTarget));
    if (document.isDirty) throw new Error('Save the target file before previewing a patch so the diff matches the on-disk content.');

    return {
      path: realTarget,
      uri: vscode.Uri.file(realTarget),
      bytes,
      text,
      mode: stat.mode,
      language: document.languageId
    };
  }

  private async openDiff(target: PatchTarget, proposedText: string, title: string): Promise<void> {
    const preview = await vscode.workspace.openTextDocument({
      language: target.language,
      content: proposedText
    });
    await vscode.commands.executeCommand('vscode.diff', target.uri, preview.uri, `${title}: ${path.basename(target.path)}`);
  }

  private async createBackup(target: PatchTarget): Promise<PatchBackupRecord> {
    await vscode.workspace.fs.createDirectory(this.context.globalStorageUri);
    const id = `${Date.now()}-${randomUUID()}`;
    const backupPath = path.join(this.context.globalStorageUri.fsPath, 'patch-backups', `${id}.bak`);
    await fs.promises.mkdir(path.dirname(backupPath), { recursive: true });
    await fs.promises.writeFile(backupPath, target.bytes, { flag: 'wx', mode: 0o600 });

    const record: PatchBackupRecord = {
      id,
      targetPath: target.path,
      backupPath,
      createdAt: Date.now()
    };
    const records = [...this.getBackups(), record];
    const expired = records.slice(0, Math.max(0, records.length - MAX_BACKUPS));
    for (const old of expired) {
      try {
        await fs.promises.unlink(old.backupPath);
      } catch {
        // Missing old backups do not block a new patch.
      }
    }
    await this.context.globalState.update(BACKUP_STATE_KEY, records.slice(-MAX_BACKUPS));
    return record;
  }

  private async writeAtomically(targetPath: string, content: Buffer, mode: number): Promise<void> {
    const temporaryPath = path.join(path.dirname(targetPath), `.${path.basename(targetPath)}.${randomUUID()}.loco-tmp`);
    try {
      await fs.promises.writeFile(temporaryPath, content, { flag: 'wx', mode });
      await fs.promises.rename(temporaryPath, targetPath);
    } catch (error) {
      try {
        await fs.promises.unlink(temporaryPath);
      } catch {
        // Best-effort cleanup only.
      }
      throw error;
    }
  }

  private errorMessage(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
  }

  private log(message: string): void {
    this.output.appendLine(`[PatchManager] ${message}`);
  }
}
