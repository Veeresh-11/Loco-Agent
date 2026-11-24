import * as cp from 'child_process';
import { randomUUID } from 'crypto';
import { TextDecoder } from 'util';

const PROTOCOL_VERSION = '1.0';
const MAX_FRAME_BYTES = 12 * 1024 * 1024;
const MAX_STDERR_BYTES = 16 * 1024;
const MAX_STARTS = 3;

type JsonObject = Record<string, unknown>;
type SpawnSidecar = (executable: string) => cp.ChildProcessWithoutNullStreams;

export interface SidecarClientOptions {
  /** Resolve only the extension-packaged executable; never accept a workspace/provider path. */
  resolveExecutable: () => string;
  extensionVersion: string;
  handshakeTimeoutMs?: number;
  requestTimeoutMs?: number;
  shutdownTimeoutMs?: number;
  spawnProcess?: SpawnSidecar;
  sleep?: (milliseconds: number) => Promise<void>;
}

export interface SidecarDecision {
  contractVersion: '1.0';
  proposalId: string;
  decision: 'deny' | 'approval_required' | 'allow';
  reasonCode: string;
  message: string;
}

/**
 * Parent-owned proposal-evaluation transport. A returned "allow" is a policy
 * result only; it is never an execution capability or workspace-write grant.
 */
export class RustSidecarClient implements vscodeDisposable {
  private child?: cp.ChildProcessWithoutNullStreams;
  private stdoutBuffer = Buffer.alloc(0);
  private stderrTail = Buffer.alloc(0);
  private pending?: { requestId: string; resolve: (message: JsonObject) => void; reject: (error: Error) => void };
  private busy = false;
  private started = false;
  private closed = false;
  private starts = 0;
  private stderrTextDecoder = new TextDecoder();

  constructor(private readonly options: SidecarClientOptions) {}

  async evaluate(proposal: JsonObject, signal?: AbortSignal): Promise<SidecarDecision> {
    if (this.busy) throw new Error('A sidecar request is already in progress.');
    this.busy = true;
    try {
      throwIfAborted(signal);
      await this.ensureStarted(signal);
      const requestId = randomUUID();
      const response = await this.exchange({
        protocolVersion: PROTOCOL_VERSION,
        messageType: 'evaluate',
        requestId,
        proposal
      }, this.options.requestTimeoutMs ?? 30_000, signal);
      if (response.messageType !== 'decision' || !isObject(response.decision)) {
        throw new Error('Unexpected sidecar response.');
      }
      const decision = response.decision;
      if (decision.contractVersion !== PROTOCOL_VERSION ||
          typeof decision.proposalId !== 'string' ||
          !['deny', 'approval_required', 'allow'].includes(String(decision.decision)) ||
          typeof decision.reasonCode !== 'string' || !/^[A-Z0-9_]{1,64}$/.test(decision.reasonCode) ||
          typeof decision.message !== 'string' || decision.message.length > 2000) {
        throw new Error('Invalid sidecar decision payload.');
      }
      return decision as unknown as SidecarDecision;
    } catch (error) {
      await this.terminate();
      throw error;
    } finally {
      this.busy = false;
    }
  }

  async start(signal?: AbortSignal): Promise<void> {
    if (this.closed) throw new Error('Sidecar client has been disposed.');
    if (this.started && this.child) return;
    if (this.starts >= MAX_STARTS) throw new Error('Sidecar restart budget exhausted.');
    if (this.starts > 0) await (this.options.sleep ?? delay)(250 * (2 ** (this.starts - 1)));
    this.starts += 1;
    throwIfAborted(signal);
    const executable = this.options.resolveExecutable();
    if (!executable || executable.includes('\0')) throw new Error('Packaged sidecar executable is unavailable.');
    const spawnProcess = this.options.spawnProcess ?? ((path: string) =>
      cp.spawn(path, [], { shell: false, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] }) as cp.ChildProcessWithoutNullStreams);
    const child = spawnProcess(executable);
    this.child = child;
    this.stdoutBuffer = Buffer.alloc(0);
    this.stderrTail = Buffer.alloc(0);
    child.stdout.on('data', (chunk: Buffer) => this.onStdout(Buffer.from(chunk)));
    child.stderr.on('data', (chunk: Buffer) => this.onStderr(Buffer.from(chunk)));
    child.on('error', error => this.fail(error));
    child.on('exit', (code, signal) => {
      this.started = false;
      this.fail(new Error(`Sidecar exited unexpectedly (code=${code ?? 'null'}, signal=${signal ?? 'null'}).`));
    });

    const requestId = randomUUID();
    const response = await this.exchange({
      protocolVersion: PROTOCOL_VERSION,
      messageType: 'hello',
      requestId,
      client: { name: 'loco-vscode-extension', version: this.options.extensionVersion }
    }, this.options.handshakeTimeoutMs ?? 5000, signal);
    if (response.messageType !== 'hello' || !isObject(response.server) ||
        response.server.name !== 'loco-rust-authority' ||
        typeof response.server.version !== 'string' || response.server.version.length < 1 ||
        response.server.version.length > 64 ||
        !Array.isArray(response.server.capabilities) ||
        !response.server.capabilities.includes('proposal_evaluation')) {
      await this.terminate();
      throw new Error('Rust sidecar handshake did not satisfy the v1 contract.');
    }
    this.started = true;
  }

  async shutdown(): Promise<void> {
    if (!this.child) return;
    if (this.started && !this.busy) {
      try {
        const response = await this.exchange({
          protocolVersion: PROTOCOL_VERSION,
          messageType: 'shutdown',
          requestId: randomUUID()
        }, this.options.shutdownTimeoutMs ?? 2000);
        if (response.messageType !== 'shutdown' || response.accepted !== true) {
          throw new Error('Sidecar shutdown was not acknowledged.');
        }
      } catch {
        // Termination below is the bounded fail-closed fallback.
      }
    }
    await this.terminate();
  }

  dispose(): void {
    this.closed = true;
    void this.terminate();
  }

  private async ensureStarted(): Promise<void> {
    if (!this.started) await this.start();
  }

  private exchange(message: JsonObject, timeoutMs: number, signal?: AbortSignal): Promise<JsonObject> {
    const child = this.child;
    if (!child || child.killed) return Promise.reject(new Error('Sidecar process is unavailable.'));
    if (this.pending) return Promise.reject(new Error('Sidecar protocol permits one outstanding request.'));
    const requestId = String(message.requestId);
    return new Promise<JsonObject>((resolve, reject) => {
      const timer = setTimeout(() => {
        if (this.pending?.requestId === requestId) {
          this.pending = undefined;
          reject(new Error('Sidecar request timed out.'));
          void this.terminate();
        }
      }, timeoutMs);
      this.pending = {
        requestId,
        resolve: value => { clearTimeout(timer); this.pending = undefined; resolve(value); },
        reject: error => { clearTimeout(timer); this.pending = undefined; reject(error); }
      };
      const frame = Buffer.from(JSON.stringify(message) + '\n', 'utf8');
      if (frame.length > MAX_FRAME_BYTES) {
        this.pending.reject(new Error('Sidecar request exceeds the frame limit.'));
        return;
      }
      child.stdin.write(frame, error => {
        if (error) this.fail(error);
      });
    });
  }

  private onStdout(chunk: Buffer): void {
    this.stdoutBuffer = Buffer.concat([this.stdoutBuffer, chunk]);
    if (this.stdoutBuffer.length > MAX_FRAME_BYTES && !this.stdoutBuffer.includes(0x0a)) {
      this.fail(new Error('Sidecar response exceeded the frame limit.'));
      return;
    }
    let newline: number;
    while ((newline = this.stdoutBuffer.indexOf(0x0a)) !== -1) {
      if (newline + 1 > MAX_FRAME_BYTES) { this.fail(new Error('Sidecar response exceeded the frame limit.')); return; }
      let frame = this.stdoutBuffer.subarray(0, newline);
      this.stdoutBuffer = this.stdoutBuffer.subarray(newline + 1);
      if (frame.length && frame[frame.length - 1] === 0x0d) frame = frame.subarray(0, -1);
      if (!frame.length) { this.fail(new Error('Sidecar emitted a blank protocol frame.')); return; }
      let parsed: unknown;
      try {
        const text = new TextDecoder('utf-8', { fatal: true }).decode(frame);
        parsed = JSON.parse(text);
      } catch {
        this.fail(new Error('Sidecar emitted an invalid UTF-8 or JSON frame.'));
        return;
      }
      if (!isObject(parsed) || parsed.protocolVersion !== PROTOCOL_VERSION ||
          typeof parsed.requestId !== 'string' || !isValidId(parsed.requestId)) {
        this.fail(new Error('Sidecar emitted an invalid protocol envelope.'));
        return;
      }
      const pending = this.pending;
      if (!pending || parsed.requestId !== pending.requestId) {
        this.fail(new Error('Sidecar response ID did not match the outstanding request.'));
        return;
      }
      pending.resolve(parsed);
    }
    if (this.stdoutBuffer.length > MAX_FRAME_BYTES) this.fail(new Error('Sidecar response exceeded the frame limit.'));
  }

  private onStderr(chunk: Buffer): void {
    this.stderrTail = Buffer.concat([this.stderrTail, chunk]).subarray(-MAX_STDERR_BYTES);
    // Retain only a bounded diagnostic tail. Never forward raw child output to UI/logs.
  }

  private fail(error: Error): void {
    this.pending?.reject(error);
    this.pending = undefined;
    this.started = false;
  }

  private async terminate(): Promise<void> {
    const child = this.child;
    this.child = undefined;
    this.started = false;
    this.pending?.reject(new Error('Sidecar process terminated.'));
    this.pending = undefined;
    if (!child) return;
    if (child.exitCode === null && child.signalCode === null) {
      child.kill();
      await Promise.race([
        new Promise<void>(resolve => child.once('exit', () => resolve())),
        delay(1000)
      ]);
      if (child.exitCode === null && child.signalCode === null) child.kill('SIGKILL');
    }
  }
}

interface vscodeDisposable { dispose(): void; }

function throwIfAborted(signal?: AbortSignal): void {
  if (signal?.aborted) throw new Error('Sidecar request cancelled.');
}

function isObject(value: unknown): value is JsonObject {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function isValidId(value: string): boolean {
  return value.length >= 1 && value.length <= 128 && /^[A-Za-z0-9._:-]+$/.test(value);
}

function delay(milliseconds: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, milliseconds));
}
