import * as path from 'path';

/**
 * Returns true only when candidate is a descendant of root after path
 * normalization. Callers must resolve symlinks before using this check.
 */
export function isPathWithinWorkspace(root: string, candidate: string): boolean {
  const relative = path.relative(path.resolve(root), path.resolve(candidate));
  return relative !== '' &&
    relative !== '..' &&
    !relative.startsWith(`..${path.sep}`) &&
    !path.isAbsolute(relative);
}
