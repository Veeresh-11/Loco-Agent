'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const fixture = fs.mkdtempSync(path.join(os.tmpdir(), 'loco-vsce-ignore-'));
const output = path.join(os.tmpdir(), `loco-vsce-ignore-${process.pid}.vsix`);
const vsce = path.join(__dirname, 'node_modules', '.bin', process.platform === 'win32' ? 'vsce.cmd' : 'vsce');

function write(relativePath, content = 'fixture') {
  const target = path.join(fixture, relativePath);
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, content);
}

try {
  fs.writeFileSync(path.join(fixture, 'package.json'), JSON.stringify({
    name: 'loco-minimatch-fixture',
    version: '1.0.0',
    publisher: 'loco-test',
    engines: { vscode: '^1.85.0' }
  }, null, 2) + '\n');

  fs.writeFileSync(path.join(fixture, '.vscodeignore'), [
    'ignored.txt',
    '**/nested-ignore.txt',
    '.secret/**',
    'docs/**',
    '!docs/keep.md',
    'src/**/skip.txt',
    ''
  ].join('\n'));

  write('keep.txt');
  write('ignored.txt');
  write('deep/nested-ignore.txt');
  write('.secret/token.txt');
  write('docs/drop.md');
  write('docs/keep.md');
  write('src/sub/skip.txt');
  write('src/sub/keep.txt');

  const packageResult = spawnSync(vsce, [
    'package',
    '--out', output,
    '--no-dependencies',
    '--allow-missing-repository'
  ], { cwd: fixture, encoding: 'utf8' });

  if (packageResult.status !== 0) {
    throw new Error(
      `VSCE fixture packaging failed (exit ${packageResult.status}).\n` +
      `${packageResult.stdout ?? ''}\n${packageResult.stderr ?? ''}`
    );
  }

  const listing = spawnSync('unzip', ['-Z1', output], { encoding: 'utf8' });
  if (listing.status !== 0) {
    throw new Error(`Unable to list VSIX archive:\n${listing.stderr ?? ''}`);
  }

  const members = new Set(listing.stdout.split(/\r?\n/).filter(Boolean));
  const included = [
    'extension/keep.txt',
    'extension/docs/keep.md',
    'extension/src/sub/keep.txt'
  ];
  const excluded = [
    'extension/ignored.txt',
    'extension/deep/nested-ignore.txt',
    'extension/.secret/token.txt',
    'extension/docs/drop.md',
    'extension/src/sub/skip.txt'
  ];

  for (const member of included) {
    assert(members.has(member), `Expected VSIX to include ${member}`);
  }
  for (const member of excluded) {
    assert(!members.has(member), `Expected VSIX to exclude ${member}`);
  }

  console.log(JSON.stringify({
    result: 'passed',
    included,
    excluded,
    archiveMembers: members.size
  }, null, 2));
} finally {
  fs.rmSync(fixture, { recursive: true, force: true });
  fs.rmSync(output, { force: true });
}
