/** Shape-only v1 contract for a future read-only Rust capability.
 * This validator does not authorize, resolve, or access a filesystem target.
 */
export const WORKSPACE_READ_CONTRACT_VERSION = '1.0' as const;
export const MAX_WORKSPACE_READ_BYTES = 10 * 1024 * 1024;

export interface WorkspaceReadRequest {
  contractVersion: typeof WORKSPACE_READ_CONTRACT_VERSION;
  workspaceId: string;
  relativePath: string;
  maxBytes: number;
}

const RESERVED_WINDOWS_NAMES = /^(CON|PRN|AUX|NUL|COM[1-9¹²³]|LPT[1-9¹²³])$/i;

export function isSafePortableRelativePath(path: string): boolean {
  if (!path || path.startsWith('/') || /[\\:\0]/.test(path)) return false;
  return path.split('/').every(component => {
    if (!component || component === '.' || component === '..' ||
        component.endsWith('.') || component.endsWith(' ') ||
        /[\x00-\x1f<>"|?*]/.test(component)) return false;
    const stem = component.split('.')[0];
    return !RESERVED_WINDOWS_NAMES.test(stem);
  });
}

export function validateWorkspaceReadRequest(value: unknown): value is WorkspaceReadRequest {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
  const request = value as Record<string, unknown>;
  const keys = Object.keys(request).sort();
  if (keys.join(',') !== 'contractVersion,maxBytes,relativePath,workspaceId') return false;
  return request.contractVersion === WORKSPACE_READ_CONTRACT_VERSION &&
    typeof request.workspaceId === 'string' &&
    request.workspaceId.length >= 1 && request.workspaceId.length <= 128 &&
    /^[A-Za-z0-9._:-]+$/.test(request.workspaceId) &&
    typeof request.relativePath === 'string' &&
    Buffer.byteLength(request.relativePath, 'utf8') <= 4096 &&
    isSafePortableRelativePath(request.relativePath) &&
    typeof request.maxBytes === 'number' &&
    Number.isSafeInteger(request.maxBytes) &&
    request.maxBytes >= 1 && request.maxBytes <= MAX_WORKSPACE_READ_BYTES;
}
