import * as cp from 'child_process';
import { randomUUID } from 'crypto';
import { isAbsolute } from 'path';
import { TextDecoder } from 'util';

const PROTOCOL_VERSION = '2.0';
const POLICY_CONTRACT_VERSION = '1.0';
const MAX_FRAME_BYTES = 12 * 1024 * 1024;
const MAX_STDERR_BYTES = 16 * 1024;
const MAX_STARTS = 3;
const MAX_REQUESTS_PER_PROCESS = 4096;
const MAX_CAPTURE_BYTES = 10 * 1024 * 1024;

type JsonObject = Record<string, unknown>;
type SpawnSidecar = (executable: string) => cp.ChildProcessWithoutNullStreams;

export interface SidecarClientOptions {
  /** Resolve only the extension-packaged executable; never accept a workspace/provider path. */
  resolveExecutable: () => string;
  extensionVersion: string;
  /** Opt into the additive 2.1 binding-challenge contract; default remains 2.0. */
  protocolVersion?: '2.0' | '2.1';
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

export interface WorkspaceCaptureAcknowledgement {
  captured: true;
  byteLength: number;
}

export type BindingAction =
  | { type: 'read_file'; path: string; maxBytes?: number }
  | { type: 'propose_patch'; path: string; patch: string; expectedTargetSha256: string };

export interface BindingChallenge {
  challengeId: string;
  actionDigest: string;
  snapshotDigest: string;
  expiresAtUnix: number;
}

/**
 * Parent-owned protocol v2 transport. A returned "allow" is a policy result
 * only; it is never an execution capability or workspace-write grant.
 *
 * Workspace captures are acknowledged only. File bytes and trusted snapshot
 * metadata remain inside Rust and are not exposed through this client.
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
  private requestsSent = 0;
  private seenRequestIds = new Set<string>();
  private serverCapabilities = new Set<string>();
  private stderrTextDecoder = new TextDecoder();

  private get protocolVersion(): '2.0' | '2.1' {
    return this.options.protocolVersion ?? PROTOCOL_VERSION;
  }

  constructor(private readonly options: SidecarClientOptions) {}

  /** Runtime capabilities report implemented protocol features, never authorization grants. */
  hasCapability(name: string): boolean {
    return this.serverCapabilities.has(name);
  }

  async evaluate(proposal: JsonObject, signal?: AbortSignal): Promise<SidecarDecision> {
    const response = await this.request({
      messageType: 'evaluate',
      proposal
    }, signal);
    if (response.messageType === 'error') throw protocolError(response);
    if (response.messageType !== 'decision' || !isObject(response.decision) ||
        !hasExactKeys(response, ['protocolVersion', 'messageType', 'requestId', 'decision'])) {
      await this.terminate();
      throw new Error('Unexpected sidecar decision response.');
    }
    const decision = response.decision;
    if (decision.contractVersion !== POLICY_CONTRACT_VERSION ||
        typeof decision.proposalId !== 'string' || decision.proposalId.length < 1 || decision.proposalId.length > 128 ||
        !['deny', 'approval_required', 'allow'].includes(String(decision.decision)) ||
        typeof decision.reasonCode !== 'string' || !/^[A-Z0-9_]{1,64}$/.test(decision.reasonCode) ||
        typeof decision.message !== 'string' || decision.message.length > 2000) {
      await this.terminate();
      throw new Error('Invalid sidecar decision payload.');
    }
    return decision as unknown as SidecarDecision;
  }

  async registerWorkspace(selectedRoot: string, displayName?: string, signal?: AbortSignal): Promise<string> {
    if (typeof selectedRoot !== 'string' || !isAbsolute(selectedRoot) ||
        Buffer.byteLength(selectedRoot, 'utf8') > 4096 || selectedRoot.includes('\0')) {
      throw new Error('Workspace selection must be a bounded absolute path.');
    }
    if (displayName !== undefined && (displayName.trim().length === 0 ||
        Buffer.byteLength(displayName, 'utf8') > 128 || displayName.includes('\0'))) {
      throw new Error('Workspace display name is invalid.');
    }
    const response = await this.request({
      messageType: 'registerWorkspace',
      selectedRoot,
      ...(displayName === undefined ? {} : { displayName })
    }, signal);
    if (response.messageType === 'error') throw protocolError(response);
    if (response.messageType !== 'workspaceRegistered' ||
        !hasExactKeys(response, ['protocolVersion', 'messageType', 'requestId', 'workspaceHandle']) ||
        typeof response.workspaceHandle !== 'string' || !/^[a-f0-9]{64}$/.test(response.workspaceHandle)) {
      await this.terminate();
      throw new Error('Invalid workspace registration response.');
    }
    return response.workspaceHandle;
  }

  async captureWorkspaceFile(
    workspaceHandle: string,
    relativePath: string,
    maxBytes: number,
    signal?: AbortSignal
  ): Promise<WorkspaceCaptureAcknowledgement> {
    if (!isWorkspaceHandle(workspaceHandle)) throw new Error('Workspace handle is invalid.');
    if (!isSafePortableRelativePath(relativePath)) throw new Error('Workspace relative path is invalid.');
    if (!Number.isSafeInteger(maxBytes) || maxBytes < 1 || maxBytes > MAX_CAPTURE_BYTES) {
      throw new Error('Workspace capture byte limit is invalid.');
    }
    const response = await this.request({
      messageType: 'captureWorkspaceFile',
      workspaceHandle,
      relativePath,
      maxBytes
    }, signal);
    if (response.messageType === 'error') throw protocolError(response);
    if (response.messageType !== 'workspaceFileCaptured' ||
        !hasExactKeys(response, ['protocolVersion', 'messageType', 'requestId', 'captured', 'byteLength']) ||
        response.captured !== true || !Number.isSafeInteger(response.byteLength) ||
        (response.byteLength as number) < 0 || (response.byteLength as number) > maxBytes) {
      await this.terminate();
      throw new Error('Invalid workspace capture acknowledgement.');
    }
    return { captured: true, byteLength: response.byteLength as number };
  }

  async issueBindingChallenge(
    workspaceHandle: string, action: BindingAction, ttlSeconds: number, signal?: AbortSignal
  ): Promise<BindingChallenge> {
    if (this.protocolVersion !== '2.1') throw new Error('Binding challenges require protocol 2.1.');
    if (!isWorkspaceHandle(workspaceHandle)) throw new Error('Workspace handle is invalid.');
    validateBindingAction(action);
    if (!Number.isSafeInteger(ttlSeconds) || ttlSeconds < 1 || ttlSeconds > 300) {
      throw new Error('Binding challenge TTL must be between 1 and 300 seconds.');
    }
    const response = await this.request({ messageType: 'issueBindingChallenge', workspaceHandle, action, ttlSeconds }, signal);
    if (response.messageType === 'error') throw protocolError(response);
    if (response.messageType !== 'bindingChallengeIssued' ||
        !hasExactKeys(response, ['protocolVersion', 'messageType', 'requestId', 'challengeId', 'actionDigest', 'snapshotDigest', 'expiresAtUnix']) ||
        typeof response.challengeId !== 'string' || !/^[a-f0-9]{64}$/.test(response.challengeId) ||
        typeof response.actionDigest !== 'string' || !/^[a-f0-9]{64}$/.test(response.actionDigest) ||
        typeof response.snapshotDigest !== 'string' || !/^[a-f0-9]{64}$/.test(response.snapshotDigest) ||
        !Number.isSafeInteger(response.expiresAtUnix) || (response.expiresAtUnix as number) <= 0) {
      await this.terminate();
      throw new Error('Invalid binding challenge response.');
    }
    return {
      challengeId: response.challengeId, actionDigest: response.actionDigest,
      snapshotDigest: response.snapshotDigest, expiresAtUnix: response.expiresAtUnix as number
    };
  }

  async consumeBindingChallenge(
    workspaceHandle: string, challengeId: string, action: BindingAction, signal?: AbortSignal
  ): Promise<{ bindingVerified: true }> {
    if (this.protocolVersion !== '2.1') throw new Error('Binding challenges require protocol 2.1.');
    if (!isWorkspaceHandle(workspaceHandle)) throw new Error('Workspace handle is invalid.');
    if (typeof challengeId !== 'string' || !/^[a-f0-9]{64}$/.test(challengeId)) {
      throw new Error('Binding challenge identifier is invalid.');
    }
    validateBindingAction(action);
    const response = await this.request({ messageType: 'consumeBindingChallenge', workspaceHandle, challengeId, action }, signal);
    if (response.messageType === 'error') throw protocolError(response);
    if (response.messageType !== 'bindingVerified' ||
        !hasExactKeys(response, ['protocolVersion', 'messageType', 'requestId', 'bindingVerified']) ||
        response.bindingVerified !== true) {
      await this.terminate();
      throw new Error('Invalid binding verification response.');
    }
    return { bindingVerified: true };
  }

  async revokeWorkspace(workspaceHandle: string, signal?: AbortSignal): Promise<void> {
    if (!isWorkspaceHandle(workspaceHandle)) throw new Error('Workspace handle is invalid.');
    const response = await this.request({ messageType: 'revokeWorkspace', workspaceHandle }, signal);
    if (response.messageType === 'error') throw protocolError(response);
    if (response.messageType !== 'workspaceRevoked' ||
        !hasExactKeys(response, ['protocolVersion', 'messageType', 'requestId', 'revoked']) ||
        response.revoked !== true) {
      await this.terminate();
      throw new Error('Invalid workspace revocation response.');
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
    this.requestsSent = 0;
    this.seenRequestIds.clear();
    child.stdout.on('data', (chunk: Buffer) => this.onStdout(Buffer.from(chunk)));
    child.stderr.on('data', (chunk: Buffer) => this.onStderr(Buffer.from(chunk)));
    child.on('error', error => this.fail(error));
    child.on('exit', (code, signal) => {
      this.started = false;
      this.fail(new Error(`Sidecar exited unexpectedly (code=${code ?? 'null'}, signal=${signal ?? 'null'}).`));
    });

    const requestId = randomUUID();
    let response: JsonObject;
    try {
      response = await this.exchange({
        protocolVersion: this.protocolVersion,
        messageType: 'hello',
        requestId,
        ...(this.protocolVersion === '2.1' ? { supportedProtocolVersions: ['2.0', '2.1'] } : {}),
        client: { name: 'loco-vscode-extension', version: this.options.extensionVersion }
      }, this.options.handshakeTimeoutMs ?? 5000, signal);
    } catch (error) {
      await this.terminate();
      throw error;
    }
    const expectedHelloKeys = this.protocolVersion === '2.1'
      ? ['protocolVersion', 'messageType', 'requestId', 'negotiatedProtocolVersion', 'supportedProtocolVersions', 'server']
      : ['protocolVersion', 'messageType', 'requestId', 'server'];
    const requiredCapabilities = this.protocolVersion === '2.1'
      ? ['proposal_evaluation', 'workspace_registration', 'workspace_capture', 'workspace_revocation', 'approval_binding_challenge']
      : ['proposal_evaluation', 'workspace_registration', 'workspace_capture', 'workspace_revocation'];
    if (response.protocolVersion !== this.protocolVersion ||
        response.messageType !== 'hello' || !isObject(response.server) ||
        !hasExactKeys(response, expectedHelloKeys) ||
        (this.protocolVersion === '2.1' &&
          (response.negotiatedProtocolVersion !== '2.1' || !Array.isArray(response.supportedProtocolVersions) ||
           !response.supportedProtocolVersions.includes('2.1'))) ||
        !hasExactKeys(response.server, ['name', 'version', 'capabilities']) ||
        response.server.name !== 'loco-rust-authority' ||
        typeof response.server.version !== 'string' || response.server.version.length < 1 ||
        response.server.version.length > 64 ||
        !Array.isArray(response.server.capabilities) ||
        response.server.capabilities.some(capability =>
          typeof capability !== 'string' || !/^[a-z0-9_]{1,64}$/.test(capability)) ||
        requiredCapabilities.some(capability => !((response.server as JsonObject).capabilities as unknown[]).includes(capability))) {
      await this.terminate();
      throw new Error('Rust sidecar handshake did not satisfy the v2 contract.');
    }
    this.serverCapabilities = new Set(response.server.capabilities as string[]);
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
        if (response.messageType !== 'shutdown' || response.accepted !== true ||
            !hasExactKeys(response, ['protocolVersion', 'messageType', 'requestId', 'accepted'])) {
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

  private async request(fields: JsonObject, signal?: AbortSignal): Promise<JsonObject> {
    if (this.busy) throw new Error('A sidecar request is already in progress.');
    this.busy = true;
    try {
      throwIfAborted(signal);
      await this.ensureStarted(signal);
      const response = await this.exchange({
        protocolVersion: this.protocolVersion,
        ...fields,
        requestId: randomUUID()
      }, this.options.requestTimeoutMs ?? 30_000, signal);
      if (response.messageType === 'error') {
        if (!hasExactKeys(response, ['protocolVersion', 'messageType', 'requestId', 'error']) ||
            !isObject(response.error) ||
            !hasExactKeys(response.error, ['code', 'message', 'retryable']) ||
            typeof response.error.code !== 'string' || !/^[A-Z0-9_]{1,64}$/.test(response.error.code) ||
            typeof response.error.message !== 'string' || response.error.message.length > 512 ||
            typeof response.error.retryable !== 'boolean') {
          throw new Error('Invalid sidecar error response.');
        }
      }
      return response;
    } catch (error) {
      await this.terminate();
      throw error;
    } finally {
      this.busy = false;
    }
  }

  private async ensureStarted(signal?: AbortSignal): Promise<void> {
    if (!this.started) await this.start(signal);
  }

  private exchange(message: JsonObject, timeoutMs: number, signal?: AbortSignal): Promise<JsonObject> {
    const child = this.child;
    if (!child || child.killed) return Promise.reject(new Error('Sidecar process is unavailable.'));
    if (this.pending) return Promise.reject(new Error('Sidecar protocol permits one outstanding request.'));
    const requestId = String(message.requestId);
    if (!isValidId(requestId) || this.seenRequestIds.has(requestId)) {
      return Promise.reject(new Error('Sidecar request identifier is invalid or duplicated.'));
    }
    if (this.requestsSent >= MAX_REQUESTS_PER_PROCESS) {
      return Promise.reject(new Error('Sidecar process request budget is exhausted.'));
    }
    if (signal?.aborted) return Promise.reject(new Error('Sidecar request cancelled.'));
    this.seenRequestIds.add(requestId);
    this.requestsSent += 1;
    return new Promise<JsonObject>((resolve, reject) => {
      const cleanup = () => {
        clearTimeout(timer);
        signal?.removeEventListener('abort', onAbort);
      };
      const onAbort = () => {
        if (this.pending?.requestId !== requestId) return;
        this.pending = undefined;
        cleanup();
        reject(new Error('Sidecar request cancelled.'));
        void this.terminate();
      };
      const timer = setTimeout(() => {
        if (this.pending?.requestId !== requestId) return;
        this.pending = undefined;
        cleanup();
        reject(new Error('Sidecar request timed out.'));
        void this.terminate();
      }, timeoutMs);
      signal?.addEventListener('abort', onAbort, { once: true });
      this.pending = {
        requestId,
        resolve: value => { cleanup(); this.pending = undefined; resolve(value); },
        reject: error => { cleanup(); this.pending = undefined; reject(error); }
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
      if (!isObject(parsed) || parsed.protocolVersion !== this.protocolVersion ||
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
  }

  private fail(error: Error): void {
    this.pending?.reject(error);
    this.pending = undefined;
    this.started = false;
    this.serverCapabilities.clear();
  }

  private async terminate(): Promise<void> {
    const child = this.child;
    this.child = undefined;
    this.started = false;
    this.serverCapabilities.clear();
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

function protocolError(response: JsonObject): Error {
  const error = response.error as JsonObject;
  return new Error(`Sidecar request denied (${String(error.code)}).`);
}

function hasExactKeys(value: JsonObject, expected: string[]): boolean {
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  return actual.length === wanted.length && actual.every((key, index) => key === wanted[index]);
}

function validateBindingAction(action: BindingAction): void {
  if (!action || typeof action !== 'object' || !isSafePortableRelativePath(action.path)) {
    throw new Error('Binding action path is invalid.');
  }
  if (action.type === 'read_file') {
    if (action.maxBytes !== undefined &&
        (!Number.isSafeInteger(action.maxBytes) || action.maxBytes < 1 || action.maxBytes > MAX_CAPTURE_BYTES)) {
      throw new Error('Binding read byte limit is invalid.');
    }
    return;
  }
  if (action.type === 'propose_patch') {
    if (typeof action.patch !== 'string' || action.patch.length < 1 || Buffer.byteLength(action.patch, 'utf8') > 1_000_000 ||
        typeof action.expectedTargetSha256 !== 'string' || !/^[a-f0-9]{64}$/.test(action.expectedTargetSha256)) {
      throw new Error('Binding patch action is invalid.');
    }
    return;
  }
  throw new Error('Binding action type is unsupported.');
}

function isWorkspaceHandle(value: string): boolean {
  return /^[a-f0-9]{64}$/.test(value);
}

function isSafePortableRelativePath(value: string): boolean {
  if (!value || value.startsWith('/') || value.includes('\\') || value.includes(':') || value.includes('\0')) return false;
  const reserved = new Set([
    'CON', 'PRN', 'AUX', 'NUL',
    'COM1', 'COM2', 'COM3', 'COM4', 'COM5', 'COM6', 'COM7', 'COM8', 'COM9',
    'COM¹', 'COM²', 'COM³', 'LPT1', 'LPT2', 'LPT3', 'LPT4', 'LPT5', 'LPT6', 'LPT7', 'LPT8', 'LPT9',
    'LPT¹', 'LPT²', 'LPT³'
  ]);
  return value.split('/').every(component => {
    if (!component || component === '.' || component === '..' || component.endsWith('.') ||
        component.endsWith(' ') || hasForbiddenPathCharacter(component)) return false;
    const stem = asciiUppercase(component.split('.')[0] ?? component);
    return !reserved.has(stem);
  });
}

function asciiUppercase(value: string): string {
  return value.replace(/[a-z]/g, character => String.fromCharCode(character.charCodeAt(0) - 0x20));
}

function hasForbiddenPathCharacter(value: string): boolean {
  for (const character of value) {
    const code = character.codePointAt(0) ?? 0;
    if (code <= 0x1f || (code >= 0x7f && code <= 0x9f) || '<>"|?*'.includes(character)) return true;
  }
  return false;
}

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
