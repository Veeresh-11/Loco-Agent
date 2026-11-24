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
  mode: 'normal' | 'wrong-id' | 'oversized' | 'stall-evaluate' | 'stall-hello' = 'normal';
  capabilities = ['proposal_evaluation'];

  constructor() {
    super();
    this.stdin.on('data', (chunk: Buffer) => {
      const request = JSON.parse(chunk.toString('utf8').trim());
      if ((this.mode === 'stall-evaluate' && request.messageType === 'evaluate') ||
          (this.mode === 'stall-hello' && request.messageType === 'hello')) return;
      let response: Record<string, unknown>;
      if (request.messageType === 'hello') {
        response = {
          protocolVersion: '1.0', messageType: 'hello', requestId: request.requestId,
          server: { name: 'loco-rust-authority', version: '0.1.0', capabilities: this.capabilities }
        };
      } else if (request.messageType === 'evaluate') {
        response = {
          protocolVersion: '1.0', messageType: 'decision',
          requestId: this.mode === 'wrong-id' ? 'wrong' : request.requestId,
          decision: { contractVersion: '1.0', proposalId: 'p-1', decision: 'allow', reasonCode: 'POLICY_ALLOWED', message: 'Proposal accepted for evaluation only.' }
        };
      } else {
        response = { protocolVersion: '1.0', messageType: 'shutdown', requestId: request.requestId, accepted: true };
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

describe('RustSidecarClient', () => {
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

  it('requires handshake and correlates a proposal decision', async () => {
    const decision = await client.evaluate({ contractVersion: '1.0', proposalId: 'p-1' });
    expect(decision.decision).toBe('allow');
    expect(decision.proposalId).toBe('p-1');
  });

  it('exposes optional runtime capabilities and clears them on shutdown', async () => {
    child.capabilities.push('macos_fcntl_getpath_nofirmlink');
    expect(client.hasCapability('macos_fcntl_getpath_nofirmlink')).toBe(false);
    await client.start();
    expect(client.hasCapability('proposal_evaluation')).toBe(true);
    expect(client.hasCapability('macos_fcntl_getpath_nofirmlink')).toBe(true);
    expect(client.hasCapability('unknown_capability')).toBe(false);
    await client.shutdown();
    expect(client.hasCapability('macos_fcntl_getpath_nofirmlink')).toBe(false);
  });

  it('fails closed when the sidecar advertises malformed capabilities', async () => {
    child.capabilities = ['proposal_evaluation', 'bad-capability'];
    await expect(client.start()).rejects.toThrow(/handshake/i);
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
