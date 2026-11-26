const { assertSupportedPackagingNode } = require('../../scripts/check-packaging-node');

describe('VSIX packaging Node.js requirement', () => {
  test.each(['22.0.0', '22.1.0', '24.0.0'])('accepts Node.js %s', (version) => {
    expect(() => assertSupportedPackagingNode(version)).not.toThrow();
  });

  test.each(['16.20.2', '18.20.8', '20.20.2', '21.7.3'])('rejects Node.js %s with an actionable message', (version) => {
    expect(() => assertSupportedPackagingNode(version)).toThrow(
      /VSIX packaging requires Node\.js >=22\.0\.0.*Current runtime/
    );
  });

  test('rejects an unparseable runtime version', () => {
    expect(() => assertSupportedPackagingNode('unknown')).toThrow(
      /Unable to determine the Node\.js version/
    );
  });
});
