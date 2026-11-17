import { applyThreeWayMerge } from '../core/threeWayMerge.apply';

describe('Three-Way Merge', () => {
  test('clean merge', () => {
    const result = applyThreeWayMerge({
      base: 'a\nb\nc',
      local: 'a\nx\nc',
      remote: 'a\nb\ny',
    });
    expect(result.conflicts).toBe(0);
    expect(result.merged).toBe('a\nx\ny');
  });

  test('applies local-only changes', () => {
    const result = applyThreeWayMerge({ base: 'a\nb', local: 'a\nlocal', remote: 'a\nb' });
    expect(result).toEqual({ merged: 'a\nlocal', conflicts: 0 });
  });

  test('applies remote-only changes', () => {
    const result = applyThreeWayMerge({ base: 'a\nb', local: 'a\nb', remote: 'a\nremote' });
    expect(result).toEqual({ merged: 'a\nremote', conflicts: 0 });
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

  test('merges additions and removals on both sides', () => {
    const result = applyThreeWayMerge({
      base: 'keep\nremove-local\nremove-remote\nend',
      local: 'keep\nlocal-add\nremove-remote\nend',
      remote: 'keep\nremove-local\nremote-add\nend'
    });
    expect(result.conflicts).toBe(0);
    expect(result.merged).toBe('keep\nlocal-add\nremote-add\nend');
  });

  test('merges independent additions from an empty base', () => {
    const result = applyThreeWayMerge({ base: '', local: 'local line', remote: 'remote line' });
    expect(result.conflicts).toBe(1);
    expect(result.merged).toContain('<<<<<<< LOCAL');
    expect(result.merged).toContain('local line');
    expect(result.merged).toContain('remote line');
  });
});
