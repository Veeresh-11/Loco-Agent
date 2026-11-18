# Canonical Action Digest v1

This specification defines the byte encoding used to calculate action digests across TypeScript, Python, and Rust. It replaces language-specific JSON serialization for approval-binding digest calculation.

**Security scope:** A matching digest identifies the same encoded action bytes. It is not an approval signature, proof of identity, or authorization by itself.

## Hash

SHA-256 of the exact canonical byte sequence. Render the digest as 64 lowercase hexadecimal characters.

## Encoding

All action encodings start with the 22-byte ASCII/UTF-8 prefix:

`LOCO-ACTION-DIGEST-V1` followed by one NUL byte.

Strings are encoded as:
- 8-byte unsigned length in big-endian order (length is UTF-8 byte count, not character count)
- raw UTF-8 bytes, with no normalization or Unicode escaping

Unsigned integers are encoded as 8-byte big-endian values.

Optional values use:
- `0x00` for absent
- `0x01` followed by the encoded value for present

Variant tags:
- `0x01` read_file
- `0x02` propose_patch
- `0x03` run_command

### Action fields (strict order)

**read_file**: tag, path string, max_bytes optional u64.

**propose_patch**: tag, path string, patch string, expected_target_sha256 optional string.

**run_command**: tag, executable string, argument count u64, each argument string in array order, working_directory optional string, timeout_ms optional u64.

The order is mandatory. Absent optional values are encoded with the absent marker; they are not omitted from the byte stream. Empty strings are distinct from absent. No additional fields are allowed in this v1 encoding.

## Test vectors

These vectors are normative. All three implementations must reproduce the hashes exactly.

| Action | Values | SHA-256 |
|---|---|---|
| read_file | path=`src/main.rs`, max_bytes=1024 | `87cc9be51307092dd5c6d9203de172422cabdc506f78bcb224b07f39d15022a7` |
| propose_patch | path=`src/lib.rs`, patch=`diff --git a b`, expected_target_sha256=`a` repeated 64 times | `d8ae37db445a5bf6b34239e8032fa0abb239e130c026ace019ecdaaf232de192` |
| run_command | executable=`cargo`, args=[`test`,`--lib`], working_directory=`core`, timeout_ms=30000 | `eb0ff76a3f5dd4cc6f1e9966d03140ab49e08700fc0a179505f0ae46c1b74292` |

## Implementation notes

- Python: encode strings as `value.encode("utf-8")`; use `struct.pack(">Q", length)` for lengths and integers; append explicit option markers.
- TypeScript: use `TextEncoder().encode(value)`; construct 8-byte big-endian lengths using `DataView.setBigUint64(0, value, false)`; do not use JavaScript string length for encoded length.
- Rust: the implementation is `core/loco-core/src/canonical_action_digest.rs`.
- Digest output is lowercase hexadecimal.
- Do not trim, case-fold, normalize, or otherwise transform string content before encoding.

## Required cross-language tests

Each language must test the three vectors above and at least these boundaries:
- Unicode strings whose character count differs from UTF-8 byte length
- Missing optional value versus present empty string
- Empty argument list versus one empty argument
- One argument containing spaces versus multiple arguments
- Field changes, reordered arguments, and variant changes must produce different digests

## Versioning

Any change to prefix, tags, field order, integer encoding, optional markers, or string encoding requires a new digest version (e.g. v2). Never silently change v1. Contract versioning and digest-format versioning are related but independent.

