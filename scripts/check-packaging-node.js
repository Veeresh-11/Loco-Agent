'use strict';

function assertSupportedPackagingNode(version = process.versions.node) {
  const match = /^v?(\d+)\./.exec(String(version));
  if (!match) {
    throw new Error(`Unable to determine the Node.js version for VSIX packaging: ${version}`);
  }

  const major = Number(match[1]);
  if (major < 22) {
    throw new Error(
      `VSIX packaging requires Node.js >=22.0.0 because @vscode/vsce@4.1.0 declares that engine requirement. ` +
      `Current runtime: ${version}. Switch to Node.js 22 or newer and rerun "npm run package".`
    );
  }
}

if (require.main === module) {
  try {
    assertSupportedPackagingNode();
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}

module.exports = { assertSupportedPackagingNode };
