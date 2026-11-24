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
  mode: 'normal' | 'wrong-id' | 'oversized' = 'normal';

  constructor() {
    super();
    this.stdin.on('data', (chunk: Buffer) => {
      const request = JSON.parse(chunk.toString('utf8').trim());
      let response: Record<string, unknown>;
      if (request.messageType === 'hello') {
        response = {
          protocolVersion: '1.0', messageType: 'hello', requestId: request.requestId,
          server: { name: 'loco-rust-authority', version: '0.1.0', capabilities: ['proposal_evaluation'] }
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

  it('rejects concurrent requests instead of multiplexing', async () => {
    // A deliberately stalled first request holds the single-request slot.
    const original = child.stdin.write.bind(child.stdin);
    let stallEvaluate = true;
    child.stdin.write = ((chunk: Buffer, ...args: unknown[]) => {
      const request = JSON.parse(chunk.toString('utf8').trim());
      if (request.messageType === 'evaluate' && stallEvaluate) return true;
      return (original as (...values: unknown[]) => boolean)(chunk, ...args);
    }) as typeof child.stdin.write;
    const first = client.evaluate({ contractVersion: '1.0', proposalId: 'p-1' });
    await Promise.resolve();
    await expect(client.evaluate({ contractVersion: '1.0', proposalId: 'p-2' }))
      .rejects.toThrow(/already in progress/i);
    stallEvaluate = false;
    await client.shutdown();
    await expect(first).rejects.toThrow();
  });
});
