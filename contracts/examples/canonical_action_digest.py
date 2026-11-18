"""Canonical LOCO action digest v1 helper. See contracts/action-digest-v1.md."""
import hashlib
import struct
from typing import Any

PREFIX = b"LOCO-ACTION-DIGEST-V1\0"

def _string(value: str) -> bytes:
    encoded = value.encode("utf-8")
    return struct.pack(">Q", len(encoded)) + encoded

def _optional_string(value: str | None) -> bytes:
    return b"\x00" if value is None else b"\x01" + _string(value)

def _optional_u64(value: int | None) -> bytes:
    if value is None:
        return b"\x00"
    if not (0 <= value <= 0xFFFFFFFFFFFFFFFF):
        raise ValueError("u64 out of range")
    return b"\x01" + struct.pack(">Q", value)

def canonical_action_bytes(action: dict[str, Any]) -> bytes:
    kind = action.get("type")
    out = bytearray(PREFIX)
    if kind == "read_file":
        out.append(1)
        out.extend(_string(action["path"]))
        out.extend(_optional_u64(action.get("max_bytes")))
    elif kind == "propose_patch":
        out.append(2)
        out.extend(_string(action["path"]))
        out.extend(_string(action["patch"]))
        out.extend(_optional_string(action.get("expected_target_sha256")))
    elif kind == "run_command":
        out.append(3)
        out.extend(_string(action["executable"]))
        args = action["args"]
        out.extend(struct.pack(">Q", len(args)))
        for arg in args:
            out.extend(_string(arg))
        out.extend(_optional_string(action.get("working_directory")))
        out.extend(_optional_u64(action.get("timeout_ms")))
    else:
        raise ValueError("unsupported action type")
    return bytes(out)

def canonical_action_digest(action: dict[str, Any]) -> str:
    return hashlib.sha256(canonical_action_bytes(action)).hexdigest()
