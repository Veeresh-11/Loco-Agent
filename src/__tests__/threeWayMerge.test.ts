import { applyThreeWayMerge } from '../core/threeWayMerge.apply';

describe('Three-Way Merge', () => {
  test('clean merge', () => {
    const result = applyThreeWayMerge({
      base: 'a\nb\nc',
      local: 'a\nx\nc',
      remote: 'a\nb\ny',
    });
    expect(result.conflicts).toBe(0);
  });

  test('detects conflicts', () => {
    const result = applyThreeWayMerge({
      base: 'a',
      local: 'x',
      remote: 'y',
    });
    expect(result.conflicts).toBeGreaterThan(0);
    expect(result.merged).toContain('<<<<<<< LOCAL');
    expect(result.merged).toContain('=======');
    expect(result.merged).toContain('>>>>>>> REMOTE');
  });

  test('preserves independent insertions on both sides', () => {
    const result = applyThreeWayMerge({
      base: 'start\nend',
      local: 'start\nlocal\nend',
      remote: 'start\nend\nremote'
    });
    expect(result.conflicts).toBe(0);
    expect(result.merged).toBe('start\nlocal\nend\nremote');
  });

  test('accepts the same change made by both sides', () => {
    const result = applyThreeWayMerge({
      base: 'one\ntwo',
      local: 'one\nchanged',
      remote: 'one\nchanged'
    });
    expect(result.conflicts).toBe(0);
    expect(result.merged).toBe('one\nchanged');
  });

  test('merges local-only and remote-only replacements', () => {
    const result = applyThreeWayMerge({
      base: 'a\nb\nc\nd',
      local: 'A\nb\nc\nd',
      remote: 'a\nb\nc\nD'
    });
    expect(result.conflicts).toBe(0);
    expect(result.merged).toBe('A\nb\nc\nD');
  });

  test('merges an insertion at a replacement boundary without a conflict', () => {
    const result = applyThreeWayMerge({
      base: 'before\nchange\nafter',
      local: 'before\ninserted\nchange\nafter',
      remote: 'before\nupdated\nafter'
    });
    expect(result.conflicts).toBe(0);
    expect(result.merged).toBe('before\ninserted\nupdated\nafter');
  });
});
