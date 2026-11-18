import { isPathWithinWorkspace } from '../core/patchPath';

describe('isPathWithinWorkspace', () => {
  const root = '/work/project';

  test('accepts a nested file path', () => {
    expect(isPathWithinWorkspace(root, '/work/project/src/index.ts')).toBe(true);
  });

  test('rejects the workspace root itself', () => {
    expect(isPathWithinWorkspace(root, root)).toBe(false);
  });

  test('rejects parent traversal outside the workspace', () => {
    expect(isPathWithinWorkspace(root, '/work/project/../secrets.txt')).toBe(false);
    expect(isPathWithinWorkspace(root, '/work/project/src/../../../../etc/passwd')).toBe(false);
  });

  test('rejects sibling paths sharing the same prefix', () => {
    expect(isPathWithinWorkspace(root, '/work/project-backup/file.ts')).toBe(false);
    expect(isPathWithinWorkspace(root, '/work/projects/file.ts')).toBe(false);
  });

  test('normalizes harmless dot segments inside workspace', () => {
    expect(isPathWithinWorkspace(root, '/work/project/src/../src/index.ts')).toBe(true);
  });

  test('rejects paths that resolve to the root via dot segments', () => {
    expect(isPathWithinWorkspace(root, '/work/project/src/..')).toBe(false);
  });

  test('rejects empty roots, empty candidates, and absolute escape paths', () => {
    expect(isPathWithinWorkspace('', '/work/project/file.ts')).toBe(false);
    expect(isPathWithinWorkspace(root, '')).toBe(false);
    expect(isPathWithinWorkspace(root, '/tmp/outside.txt')).toBe(false);
  });

  test('supports relative roots and candidates when they resolve inside', () => {
    expect(isPathWithinWorkspace('/work/project', '/work/project/src/main.ts')).toBe(true);
  });
});