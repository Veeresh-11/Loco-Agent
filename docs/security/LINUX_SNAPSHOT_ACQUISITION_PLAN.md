# Linux Workspace Snapshot Acquisition Prototype Plan

Status: implementation plan only. This document does not claim secure acquisition exists.

## Scope
First concrete adapter will be Linux-only and fail closed on unsupported kernels/syscalls. Other platforms return explicit unsupported until separately reviewed implementations exist.

## Proposed mechanism
Use a workspace directory descriptor as trust anchor. Acquire files relative to that descriptor with Linux openat2 and strict resolution:
- RESOLVE_BENEATH: prohibit resolution escaping the supplied directory descriptor.
- RESOLVE_NO_SYMLINKS: reject symlink components.
- RESOLVE_NO_MAGICLINKS: reject proc-style magic links.
- Consider RESOLVE_NO_XDEV as an explicit policy choice; it rejects mount crossings (including bind mounts) and may break legitimate workspace layouts.

Open read-only and close-on-exec. Never fall back to path-based std::fs::File::open if openat2 is unavailable. Treat ENOSYS, unsupported flags, EAGAIN, and resolution failures as explicit errors; retries must be bounded.

## Post-open validation
1. Query metadata from the opened descriptor, not a separate path.
2. Require a regular file; reject directories, devices, sockets, FIFOs.
3. Enforce the 10 MiB hard ceiling and request limit.
4. Read at most limit + 1 to detect oversize/growth; fail if exceeded.
5. Derive identity from descriptor metadata (device/inode plus relevant fields); document that it is a snapshot identifier, not cryptographic identity.
6. Hash captured bytes and construct metadata using existing builder.
7. Validate metadata and bytes before returning.

## Threat-oriented test matrix
- Normal/empty file, exact cap, cap + 1.
- Absolute path, parent traversal, repeated separators, reserved names.
- Symlink at leaf/intermediate component; magic-link attempt.
- Directory, FIFO, socket, device where safely testable.
- Rename/unlink after open; replacement before open.
- Concurrent path swap stress test.
- Hard-link policy documented and tested.
- Kernel lacking openat2 or syscall blocked: fail closed.
- Mount-crossing behavior if NO_XDEV selected.
- Identity from opened descriptor; digest from captured bytes.

## Implementation gate
Pin the exact rustix version and API, review signatures, and compile/test on Linux before claiming implementation. API research indicates rustix exposes openat2, OpenHow, ResolveFlags, fstat, and descriptor-oriented I/O. Confirm against the chosen version.

## References
- rustix filesystem API: https://docs.rs/rustix/latest/rustix/fs/index.html
- ResolveFlags: https://docs.rs/rustix/latest/rustix/fs/struct.ResolveFlags.html
- Linux openat2 manual: https://man7.org/linux/man-pages/man2/openat2.2.html

## Caveats
- Lexical validation is defense-in-depth, not the boundary.
- No acquisition implementation is included here.
- Open-handle metadata does not make file contents immutable; digest describes captured bytes, and consistency guarantees need testing.
- Hard links are not path escapes but may expose content linked from elsewhere; define provenance policy if required.
- NO_XDEV is not mandatory until compatibility tradeoffs are assessed.
