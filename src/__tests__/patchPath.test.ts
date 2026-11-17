import { isPathWithinWorkspace } from '../core/patchPath';

describe('isPathWithinWorkspace', () => {
  const root = '/work/project';

  test('accepts a nested file path', () => {
    expect(isPathWithinWorkspace(root, '/work/project/src/index.ts')).toBe(true);
  });

  test('rejects the workspace root itself', () => {
    expect(isPathWithinWorkspace(root, root)).toBe(false);
  });

  test('rejects parent traversal', () => {
    expect(isPathWithinWorkspace(root, '/work/project/../secrets.txt')).toBe(false);
  });

  test('rejects sibling paths sharing the same prefix', () => {
    expect(isPathWithinWorkspace(root, '/work/project-backup/file.ts')).toBe(false);
  });

  test('normalizes harmless dot segments', () => {
    expect(isPathWithinWorkspace(root, '/work/project/src/../src/index.ts')).toBe(true);
  });
});
