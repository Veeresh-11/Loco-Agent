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
  });
});
