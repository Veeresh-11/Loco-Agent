import { EventEmitter } from 'events';
import { PassThrough } from 'stream';
import type * as cp from 'child_process';
import { RustSidecarClient } from '../core/rustSidecarClient';

class FakeChild extends EventEmitter {
  stdin = new PassThrough();
  stdout = new PassThrough();
  stderr = new PassThrough();
  exitCode: number | null = null;
  signalCode: NodeJS.Signals | null = null;
  killed = false;
  mode: 'normal' | 'wrong-id' | 'oversized' | 'stall-evaluate' | 'stall-hello' | 'legacy' | 'bad-capture' = 'normal';
  capabilities = ['proposal_evaluation', 'workspace_registration', 'workspace_capture', 'workspace_revocation', 'approval_binding_challenge'];
  workspaceHandle = 'a'.repeat(64);
  lastShutdownProtocolVersion?: string;

  constructor() {
    super();
    this.stdin.on('data', (chunk: Buffer) => {
      const request = JSON.parse(chunk.toString('utf8').trim());
      if ((this.mode === 'stall-evaluate' && request.messageType === 'evaluate') ||
          (this.mode === 'stall-hello' && request.messageType === 'hello')) return;
      const protocolVersion = this.mode === 'legacy' ? '1.0' : String(request.protocolVersion ?? '2.0');
      let response: Record<string, unknown>;
      if (request.messageType === 'hello') {
        response = protocolVersion === '2.1'
          ? { protocolVersion, messageType: 'hello', requestId: request.requestId,
              negotiatedProtocolVersion: '2.1', supportedProtocolVersions: ['2.0', '2.1'],
              server: { name: 'loco-rust-authority', version: '0.1.0', capabilities: this.capabilities } }
          : { protocolVersion, messageType: 'hello', requestId: request.requestId,
              server: { name: 'loco-rust-authority', version: '0.1.0', capabilities: this.capabilities } };
      } else if (request.messageType === 'evaluate') {
        response = {
          protocolVersion, messageType: 'decision',
          requestId: this.mode === 'wrong-id' ? 'wrong' : request.requestId,
          decision: { contractVersion: '1.0', proposalId: 'p-1', decision: 'allow', reasonCode: 'POLICY_ALLOWED', message: 'Policy result only.' }
        };
      } else if (request.messageType === 'registerWorkspace') {
        response = { protocolVersion, messageType: 'workspaceRegistered', requestId: request.requestId, workspaceHandle: this.workspaceHandle };
      } else if (request.messageType === 'captureWorkspaceFile') {
        response = {
          protocolVersion, messageType: 'workspaceFileCaptured', requestId: request.requestId,
          captured: true, byteLength: this.mode === 'bad-capture' ? request.maxBytes + 1 : 17
        };
      } else if (request.messageType === 'revokeWorkspace') {
        response = { protocolVersion, messageType: 'workspaceRevoked', requestId: request.requestId, revoked: true };
      } else if (request.messageType === 'issueBindingChallenge') {
        response = { protocolVersion, messageType: 'bindingChallengeIssued', requestId: request.requestId,
          challengeId: 'b'.repeat(64), actionDigest: 'c'.repeat(64), snapshotDigest: 'd'.repeat(64), expiresAtUnix: 2_000_000_000 };
      } else if (request.messageType === 'consumeBindingChallenge') {
        response = { protocolVersion, messageType: 'bindingVerified', requestId: request.requestId, bindingVerified: true };
      } else {
        if (request.messageType === 'shutdown') this.lastShutdownProtocolVersion = protocolVersion;
        response = { protocolVersion, messageType: 'shutdown', requestId: request.requestId, accepted: true };
      }
      const frame = this.mode === 'oversized' && request.messageType === 'evaluate'
        ? Buffer.concat([Buffer.alloc(12 * 1024 * 1024 + 1, 0x61), Buffer.from('\n')])
        : Buffer.from(JSON.stringify(response) + '\n');
      this.stdout.write(frame);
    });
  }

  kill(signal?: NodeJS.Signals): boolean {
    this.killed = true;
    this.exitCode = 0;
    this.signalCode = signal ?? null;
    this.emit('exit', this.exitCode, this.signalCode);
    return true;
  }
}

describe('RustSidecarClient protocol v2', () => {
  let child: FakeChild;
  let client: RustSidecarClient;

  beforeEach(() => {
    child = new FakeChild();
    client = new RustSidecarClient({
      resolveExecutable: () => '/extension/bin/loco-authority',
      extensionVersion: '0.1.0',
      spawnProcess: () => child as unknown as cp.ChildProcessWithoutNullStreams,
      sleep: async () => undefined
    });
  });

  afterEach(async () => client.shutdown());

  it('negotiates protocol 2.1 and issues/consumes a binding verification result', async () => {
    client = new RustSidecarClient({
      resolveExecutable: () => '/extension/bin/loco-authority', extensionVersion: '0.1.0', protocolVersion: '2.1',
      spawnProcess: () => child as unknown as cp.ChildProcessWithoutNullStreams, sleep: async () => undefined
    });
    const handle = await client.registerWorkspace('/workspace/project');
    const action = { type: 'read_file' as const, path: 'src/main.rs', maxBytes: 1024 };
    const challenge = await client.issueBindingChallenge(handle, action, 60);
    expect(challenge.challengeId).toBe('b'.repeat(64));
    await expect(client.consumeBindingChallenge(handle, challenge.challengeId, action))
      .resolves.toEqual({ bindingVerified: true });
  });

  it('uses the negotiated protocol version for graceful shutdown', async () => {
    client = new RustSidecarClient({
      resolveExecutable: () => '/extension/bin/loco-authority', extensionVersion: '0.1.0', protocolVersion: '2.1',
      spawnProcess: () => child as unknown as cp.ChildProcessWithoutNullStreams, sleep: async () => undefined
    });
    await client.start();
    await client.shutdown();
    expect(child.lastShutdownProtocolVersion).toBe('2.1');
  });

  it('keeps binding challenge methods unavailable to protocol 2.0 clients', async () => {
    await expect(client.issueBindingChallenge('a'.repeat(64), { type: 'read_file', path: 'src/main.rs' }, 60))
      .rejects.toThrow(/protocol 2.1/i);
  });

  it('validates binding action and TTL before sending a request', async () => {
    client = new RustSidecarClient({
      resolveExecutable: () => '/extension/bin/loco-authority', extensionVersion: '0.1.0', protocolVersion: '2.1',
      spawnProcess: () => child as unknown as cp.ChildProcessWithoutNullStreams, sleep: async () => undefined
    });
    await expect(client.issueBindingChallenge('a'.repeat(64), { type: 'read_file', path: '../secret' }, 60))
      .rejects.toThrow(/path/i);
    await expect(client.issueBindingChallenge('a'.repeat(64), { type: 'read_file', path: 'src/main.rs' }, 301))
      .rejects.toThrow(/TTL/i);
    expect(child.killed).toBe(false);
  });

  it('requires a v2 handshake and correlates a policy decision', async () => {
    const decision = await client.evaluate({ contractVersion: '1.0', proposalId: 'p-1' });
    expect(decision.decision).toBe('allow');
    expect(decision.proposalId).toBe('p-1');
    expect(client.hasCapability('workspace_capture')).toBe(true);
  });

  it('registers, captures and revokes a workspace using opaque handles', async () => {
    const handle = await client.registerWorkspace('/workspace/project', 'project');
    expect(handle).toBe(child.workspaceHandle);
    await expect(client.captureWorkspaceFile(handle, 'src/main.rs', 1024))
      .resolves.toEqual({ captured: true, byteLength: 17 });
    await expect(client.revokeWorkspace(handle)).resolves.toBeUndefined();
  });

  it('sends a strict v2 handshake and does not downgrade to v1', async () => {
    child.mode = 'legacy';
    await expect(client.start()).rejects.toThrow(/protocol envelope/i);
    expect(child.killed).toBe(true);
  });

  it('exposes runtime capabilities and clears them on shutdown', async () => {
    child.capabilities.push('macos_fcntl_getpath_nofirmlink', 'macos_attr_cmnex_nofirmlinkpath');
    expect(client.hasCapability('macos_fcntl_getpath_nofirmlink')).toBe(false);
    await client.start();
    expect(client.hasCapability('proposal_evaluation')).toBe(true);
    expect(client.hasCapability('macos_fcntl_getpath_nofirmlink')).toBe(true);
    expect(client.hasCapability('macos_attr_cmnex_nofirmlinkpath')).toBe(true);
    expect(client.hasCapability('unknown_capability')).toBe(false);
    await client.shutdown();
    expect(client.hasCapability('macos_fcntl_getpath_nofirmlink')).toBe(false);
  });

  it('fails closed when the sidecar advertises malformed capabilities', async () => {
    child.capabilities = ['proposal_evaluation', 'bad-capability'];
    await expect(client.start()).rejects.toThrow(/handshake/i);
  });

  it('clears runtime capabilities when the sidecar exits unexpectedly', async () => {
    await client.start();
    expect(client.hasCapability('workspace_registration')).toBe(true);
    child.exitCode = 2;
    child.emit('exit', 2, null);
    expect(client.hasCapability('workspace_registration')).toBe(false);
  });

  it('fails closed on a mismatched response ID', async () => {
    child.mode = 'wrong-id';
    await expect(client.evaluate({ contractVersion: '1.0', proposalId: 'p-1' }))
      .rejects.toThrow(/response ID/i);
  });

  it('rejects oversized stdout frames', async () => {
    child.mode = 'oversized';
    await expect(client.evaluate({ contractVersion: '1.0', proposalId: 'p-1' }))
      .rejects.toThrow(/frame limit/i);
  });

  it('validates workspace handles, portable paths and capture limits before transport', async () => {
    await expect(client.captureWorkspaceFile('not-a-handle', 'src/main.rs', 1024))
      .rejects.toThrow(/handle/i);
    await expect(client.captureWorkspaceFile('a'.repeat(64), '../secret', 1024))
      .rejects.toThrow(/path/i);
    await expect(client.captureWorkspaceFile('a'.repeat(64), 'src/CON.txt', 1024))
      .rejects.toThrow(/path/i);
    await expect(client.captureWorkspaceFile('a'.repeat(64), 'src/\u0085name', 1024))
      .rejects.toThrow(/path/i);
    await expect(client.captureWorkspaceFile('a'.repeat(64), 'src/\u009fname', 1024))
      .rejects.toThrow(/path/i);
    await expect(client.captureWorkspaceFile('a'.repeat(64), 'src/ſon.txt', 1024))
      .resolves.toEqual({ captured: true, byteLength: 17 });
    await expect(client.captureWorkspaceFile('a'.repeat(64), 'src/main.rs', 10 * 1024 * 1024 + 1))
      .rejects.toThrow(/byte limit/i);
    expect(child.killed).toBe(false);
  });

  it('rejects malformed capture acknowledgements and never exposes content', async () => {
    child.mode = 'bad-capture';
    const handle = await client.registerWorkspace('/workspace/project');
    await expect(client.captureWorkspaceFile(handle, 'src/main.rs', 10))
      .rejects.toThrow(/acknowledgement/i);
  });

  it('rejects non-absolute and oversized workspace selections locally', async () => {
    await expect(client.registerWorkspace('relative/project')).rejects.toThrow(/absolute path/i);
    await expect(client.registerWorkspace('/' + 'x'.repeat(4096))).rejects.toThrow(/absolute path/i);
    expect(child.killed).toBe(false);
  });

  it('terminates the child when an evaluation is cancelled', async () => {
    child.mode = 'stall-evaluate';
    const controller = new AbortController();
    const pending = client.evaluate({ contractVersion: '1.0', proposalId: 'p-1' }, controller.signal);
    await new Promise(resolve => setImmediate(resolve));
    controller.abort();
    await expect(pending).rejects.toThrow(/cancelled/i);
    expect(child.killed).toBe(true);
  });

  it('fails an in-flight request when the child exits unexpectedly', async () => {
    child.mode = 'stall-evaluate';
    const pending = client.evaluate({ contractVersion: '1.0', proposalId: 'p-1' });
    await new Promise(resolve => setImmediate(resolve));
    child.exitCode = 2;
    child.emit('exit', 2, null);
    await expect(pending).rejects.toThrow(/exited unexpectedly/i);
  });

  it('times out and terminates a child that never completes the handshake', async () => {
    child.mode = 'stall-hello';
    client = new RustSidecarClient({
      resolveExecutable: () => '/extension/bin/loco-authority',
      extensionVersion: '0.1.0',
      handshakeTimeoutMs: 20,
      spawnProcess: () => child as unknown as cp.ChildProcessWithoutNullStreams,
      sleep: async () => undefined
    });
    await expect(client.evaluate({ contractVersion: '1.0', proposalId: 'p-1' }))
      .rejects.toThrow(/timed out/i);
    expect(child.killed).toBe(true);
  });

  it('times out an evaluation and does not replay it', async () => {
    child.mode = 'stall-evaluate';
    client = new RustSidecarClient({
      resolveExecutable: () => '/extension/bin/loco-authority',
      extensionVersion: '0.1.0',
      requestTimeoutMs: 20,
      spawnProcess: () => child as unknown as cp.ChildProcessWithoutNullStreams,
      sleep: async () => undefined
    });
    await expect(client.evaluate({ contractVersion: '1.0', proposalId: 'p-1' }))
      .rejects.toThrow(/timed out/i);
    expect(child.killed).toBe(true);
  });

  it('acknowledges graceful shutdown', async () => {
    await client.start();
    await client.shutdown();
    expect(child.killed).toBe(true);
  });

  it('rejects concurrent requests instead of multiplexing', async () => {
    const first = client.evaluate({ contractVersion: '1.0', proposalId: 'p-1' });
    await expect(client.evaluate({ contractVersion: '1.0', proposalId: 'p-2' }))
      .rejects.toThrow(/already in progress/i);
    await expect(first).resolves.toMatchObject({ proposalId: 'p-1' });
  });
});
