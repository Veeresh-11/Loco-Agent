# Safe Patch Preview and Recovery

LOCO's patch writer now uses an explicit review-and-approval flow.

## Preview a proposed file

1. Open the existing target file in VS Code.
2. If the target has unsaved edits, save it when prompted. Cancelling leaves the patch unapplied.
3. Run **LOCO: Preview Patch from File** from the Command Palette.
4. Select a text/source file containing the proposed complete replacement contents.
5. Review the VS Code diff. The target is shown against the proposed content.
6. Confirm **Apply Patch** in the modal prompt to write the change. Dismissing the prompt or choosing another action cancels.

This command expects a complete proposed file, not a unified diff. It applies to the active editor's file.

## Safety checks

- The target must already exist, be a regular file, and resolve inside an open workspace folder. Symlinks are resolved before the workspace-boundary check.
- Binary files, invalid UTF-8, and files larger than 2 MiB are rejected.
- Three-way merge conflicts are rejected; conflict markers are never written automatically.
- The file is re-read after the preview and approval. If its bytes changed, the operation aborts and asks for a fresh preview.
- A backup is written before replacement. The replacement is written to a temporary file in the same directory and renamed into place.
- Patch application is never automatic. A modal confirmation is required every time.

## Restore a backup

1. Run **LOCO: Restore Patch Backup**.
2. Select the backup by target file and timestamp.
3. Review the restore diff.
4. Confirm **Restore Backup** in the modal prompt.

Restoring also creates a backup of the current file first, so the restore itself can be reversed. LOCO retains up to 20 backup records in VS Code extension global storage. Backup files are written with owner-only permissions where supported.

## Current limitations

- One existing file per operation; this is not a multi-file transaction.
- The proposed file must contain the complete desired file contents. Unified-diff parsing is not implemented.
- New-file creation, deletion, binary edits, and files over 2 MiB are not supported.
- The assistant chat does not yet convert generated code blocks into patch proposals automatically. The Command Palette workflow is the explicit entry point.
- Backups live in the local VS Code extension storage and are not synchronized as a repository-level backup. Keep normal source control and backup practices.

