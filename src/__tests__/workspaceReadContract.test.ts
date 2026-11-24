import { readFileSync } from 'fs';
import { join } from 'path';
import {
  isSafePortableRelativePath,
  MAX_WORKSPACE_READ_BYTES,
  validateWorkspaceReadRequest
} from '../core/workspaceReadContract';

describe('workspace read request v1 contract', () => {
  const valid = {
    contractVersion: '1.0',
    workspaceId: 'workspace-01',
    relativePath: 'src/main.rs',
    maxBytes: 1024
  };

  test('accepts the shared cross-language JSON fixture', () => {
    const fixturePath = join(process.cwd(), 'contracts', 'examples', 'workspace-read-request-v1.json');
    const fixture: unknown = JSON.parse(readFileSync(fixturePath, 'utf8'));
    expect(validateWorkspaceReadRequest(fixture)).toBe(true);
  });

  test('accepts a valid request and inclusive byte boundaries', () => {
    expect(validateWorkspaceReadRequest(valid)).toBe(true);
    expect(validateWorkspaceReadRequest({ ...valid, maxBytes: 1 })).toBe(true);
    expect(validateWorkspaceReadRequest({ ...valid, maxBytes: MAX_WORKSPACE_READ_BYTES })).toBe(true);
  });

  test('rejects unknown or missing fields and unsupported versions', () => {
    expect(validateWorkspaceReadRequest({ ...valid, execute: true })).toBe(false);
    const { workspaceId: _workspaceId, ...missing } = valid;
    expect(validateWorkspaceReadRequest(missing)).toBe(false);
    expect(validateWorkspaceReadRequest({ ...valid, contractVersion: '2.0' })).toBe(false);
  });

  test('rejects invalid workspace identifiers and limits', () => {
    for (const workspaceId of ['', '../workspace', '/tmp/ws', 'workspace id', 'x'.repeat(129)]) {
      expect(validateWorkspaceReadRequest({ ...valid, workspaceId })).toBe(false);
    }
    for (const maxBytes of [0, -1, 1.5, Number.MAX_SAFE_INTEGER + 1, MAX_WORKSPACE_READ_BYTES + 1]) {
      expect(validateWorkspaceReadRequest({ ...valid, maxBytes })).toBe(false);
    }
  });

  test('rejects ambiguous and escaping path forms', () => {
    for (const relativePath of ['', '../secret', 'src/../secret', '/etc/passwd', 'C:/secret', 'src\\secret', 'src//secret']) {
      expect(validateWorkspaceReadRequest({ ...valid, relativePath })).toBe(false);
    }
    expect(isSafePortableRelativePath('src/main.rs')).toBe(true);
  });

  test('rejects arrays and non-object values', () => {
    expect(validateWorkspaceReadRequest(null)).toBe(false);
    expect(validateWorkspaceReadRequest([])).toBe(false);
    expect(validateWorkspaceReadRequest('request')).toBe(false);
  });
});
