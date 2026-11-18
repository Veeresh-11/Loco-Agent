/** Canonical LOCO action digest v1 helper. See contracts/action-digest-v1.md */
const PREFIX = new TextEncoder().encode("LOCO-ACTION-DIGEST-V1\0");
const encoder = new TextEncoder();

function join(...parts: Uint8Array[]): Uint8Array {
  const size = parts.reduce((n, p) => n + p.length, 0);
  const out = new Uint8Array(size);
  let offset = 0;
  for (const part of parts) { out.set(part, offset); offset += part.length; }
  return out;
}
function u64(value: number | bigint): Uint8Array {
  const out = new Uint8Array(8);
  new DataView(out.buffer).setBigUint64(0, BigInt(value), false);
  return out;
}
function stringBytes(value: string): Uint8Array {
  const bytes = encoder.encode(value);
  return join(u64(bytes.length), bytes);
}
function optionalString(value?: string | null): Uint8Array {
  return value == null ? Uint8Array.of(0) : join(Uint8Array.of(1), stringBytes(value));
}
function optionalU64(value?: number | null): Uint8Array {
  return value == null ? Uint8Array.of(0) : join(Uint8Array.of(1), u64(value));
}

export type CanonicalAction =
  | { type: "read_file"; path: string; max_bytes?: number | null }
  | { type: "propose_patch"; path: string; patch: string; expected_target_sha256?: string | null }
  | { type: "run_command"; executable: string; args: string[]; working_directory?: string | null; timeout_ms?: number | null };

export function canonicalActionBytes(action: CanonicalAction): Uint8Array {
  let body: Uint8Array;
  switch (action.type) {
    case "read_file":
      body = join(Uint8Array.of(1), stringBytes(action.path), optionalU64(action.max_bytes));
      break;
    case "propose_patch":
      body = join(Uint8Array.of(2), stringBytes(action.path), stringBytes(action.patch), optionalString(action.expected_target_sha256));
      break;
    case "run_command":
      body = join(Uint8Array.of(3), stringBytes(action.executable), u64(action.args.length),
        ...action.args.map(stringBytes), optionalString(action.working_directory), optionalU64(action.timeout_ms));
      break;
    default: {
      const unreachable: never = action;
      throw new Error(`Unsupported action: ${unreachable}`);
    }
  }
  return join(PREFIX, body);
}

export async function canonicalActionDigest(action: CanonicalAction): Promise<string> {
  const hash = await crypto.subtle.digest("SHA-256", canonicalActionBytes(action));
  return Array.from(new Uint8Array(hash), b => b.toString(16).padStart(2, "0")).join("");
}
