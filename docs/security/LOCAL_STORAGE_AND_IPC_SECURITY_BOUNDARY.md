# Local Storage and IPC Security Boundary

**Status:** Proposed design; no production implementation selected  
**Date:** 2026-11-24  
**Depends on:** approval-store prototype and authenticated-approval trust-boundary review

## Decision summary

SQLite page and WAL controls do not impose a hard quota on the complete database, WAL, shared-memory, temporary-file, backup, and filesystem-allocation footprint. A hard aggregate ceiling requires an OS/filesystem-enforced quota or dedicated bounded storage volume. There is no portable per-application quota primitive that can be assumed across supported desktop platforms.

For Rust IPC, prefer a parent-spawned sidecar over a discoverable local listener when lifecycle permits. If independently launched processes require a local endpoint, use platform-native local transports, kernel-provided peer credentials, and restrictive endpoint ACLs. Peer identity is an authorization input, not proof of human approval, executable integrity, or resistance to same-user malware.

This document is a design review, not an implementation. The approval challenge store remains prototype-only.

## 1. Local approval-store storage

### 1.1 Existing SQLite controls

The current prototype configures:
- SQLite max_page_count for a main-database page ceiling of 16 MiB, rounded down to a whole page.
- journal_size_limit=1048576 as a 1 MiB WAL retention target after a successful checkpoint/reset.
- wal_autocheckpoint=128 on each connection.

The main-database page limit is SQLite-enforced. The WAL limit is not a hard instantaneous file-size ceiling: long-lived readers can delay checkpoints and prevent WAL reset/truncation. These settings do not include -shm, temporary files, backups, filesystem metadata, or other application data. A pinned reader, full filesystem, or checkpoint failure can still cause resource growth or I/O errors.

Never manually remove -wal or -shm files while a database may be open. Do not disable journaling, weaken durability, or bypass the store after SQLITE_FULL, SQLITE_IOERR, SQLITE_BUSY, or corruption.

### 1.2 Required storage boundary

Before the store is used for security-sensitive state, implementation must:

1. Resolve a per-user application-data directory using the platform-supported location mechanism. Never accept an arbitrary directory from Python, a provider, or a workspace proposal.
2. Create the directory with restrictive permissions/ACLs before creating database files. POSIX target: user-owned directory equivalent to mode 0700, with umask and ACL verification. Windows target: explicit DACL limited to the intended user/logon identity and required system principals; do not rely on permissive default ACLs.
3. Open and retain a directory handle/descriptor where possible. Validate the opened object (directory type, owner/ACL, expected identity), not merely a prior pathname check.
4. Reject symlinks, reparse points, unexpected hard-link/reparse behavior, network paths, and unexpected filesystem locations under a documented platform policy. Path-string canonicalization alone is not a security boundary.
5. Ensure SQLite database and sidecar creation occurs inside that protected directory. SQLite opens WAL, SHM, journal, and temporary files by name. If the selected SQLite VFS cannot guarantee the required directory/handle behavior, use a reviewed custom VFS or a private directory whose ACL prevents untrusted peers racing names. Do not claim descriptor-relative safety SQLite does not provide.
6. Apply restrictive permissions/ACLs to database and sidecars and verify them after creation. Do not expose raw DB files through workspace sync, extension exports, or diagnostics.
7. Use SQLite's backup API or a documented consistent backup procedure. Treat DB, WAL, and SHM as one live database state; do not copy or restore only the main file while connections are active.
8. On corruption or failed migration, stop authorization use, preserve evidence in a private quarantine location, and require a safe recovery/reinitialization path. Never silently delete and recreate an empty approval store.
9. Surface storage-full, permission, busy, I/O, and corruption failures as hard denials. No alternate write path is allowed.

The private-directory boundary primarily protects against other local users and opportunistic pathname races. It does not protect against arbitrary code already executing as the same user when that code can modify the directory, inspect process memory, or invoke the same OS APIs.

### 1.3 Hard quota decision

Do not describe SQLite page limits as a complete disk quota. A hard aggregate cap must be enforced outside SQLite:

- **Linux:** user/group quotas or filesystem/project quotas may exist, but an app-specific project quota requires filesystem support, privileges, and safe project-ID lifecycle management. Do not assume an unprivileged extension can provision it.
- **macOS:** quota support depends on filesystem and system configuration. No portable per-app quota is assumed; APFS container free space is not a per-application quota.
- **Windows:** NTFS user quotas are user/volume scoped, not a portable per-app quota. File Server Resource Manager is not a general desktop-client dependency. A dedicated bounded virtual disk/volume is a separate packaging/lifecycle decision.

Cross-platform policy for now: retain SQLite's main-file page cap and WAL controls; bound row counts and individual payloads; monitor/report total directory usage; fail closed on storage errors; and leave a hard aggregate quota explicitly unimplemented until a supported deployment can provision and test a dedicated OS-enforced quota. Application-level file-length checks are not a hard quota because concurrent growth and sidecar creation can race the check.

### 1.4 Required tests

- Verify directory ownership and restrictive ACL/mode on each supported OS.
- Reject pre-existing symlink/reparse-point database paths and unsafe parent directories.
- Verify DB/WAL/SHM creation remains inside the private store directory.
- Exercise full-disk/SQLITE_FULL, permission-denied, busy-checkpoint, pinned-reader WAL growth, and I/O errors.
- Verify corruption and failed migration never silently reset approval state.
- Verify backup/restore with active and inactive WAL states using the supported SQLite backup mechanism.
- Verify cleanup never removes live sidecars or a database used by another authority instance.
- If an OS quota is selected, test enforcement on the exact filesystem and supported OS versions, including sidecar growth and crash recovery.

## 2. Rust IPC peer identity

### 2.1 Preferred topology

Where possible, the VS Code extension or trusted launcher should spawn Rust authority as a child and communicate over inherited stdin/stdout pipes. The launcher must:
- Resolve and launch a packaged, verified executable directly, without a shell or command-string interpolation.
- Use a controlled executable path; verify packaging/signature/integrity separately.
- Pass only required pipe handles/file descriptors and close unrelated inherited handles.
- Own child lifetime, restart, cancellation, and shutdown; reject unexpected child replacement.
- Bound protocol framing, payload sizes, request counts, deadlines, and concurrency.
- Treat every message as untrusted, even over inherited pipes. A compromised parent or child can forge messages within its process boundary.

This avoids a globally discoverable socket name and reduces endpoint-squatting exposure. It does not prove a UI click represents a human or protect against a compromised parent.

### 2.2 Independent local clients

If independent launch or multi-client access requires a listener, select a platform transport rather than exposing TCP or unauthenticated loopback HTTP.

| Platform | Candidate transport and peer evidence | Required endpoint controls | Limitations |
|---|---|---|---|
| Linux | Unix-domain stream socket; query SO_PEERCRED on the connected socket. It supplies peer PID/UID/GID associated with connection establishment. SO_PASSCRED/SCM_CREDENTIALS may be relevant for per-message credentials. | User-owned 0700 runtime directory; socket mode 0600; reject symlinked parent paths; filesystem namespace, not unauthenticated TCP; validate UID and, where appropriate, security label. | UID equality admits any process running as that user. PID can be reused and does not prove executable identity. Abstract sockets lack filesystem ACL protection. |
| macOS | Unix-domain stream socket; getpeereid supplies peer effective UID/GID. LOCAL_PEERPID may provide PID on supported macOS APIs, but must be validated on deployment targets. Consider XPC if signing-identity and entitlement model fits. | Private per-user directory and restrictive permissions/ACLs; no public TCP listener; validate peer credentials and endpoint ownership. | getpeereid is not executable identity or user presence. PID is not stable identity. XPC peer PID alone is not a code-signature decision. |
| Windows | Named pipe with explicit security descriptor/DACL; PIPE_REJECT_REMOTE_CLIENTS; retrieve client PID and impersonate the connected client only where required to inspect token/SID. | Restrict DACL to intended user/logon SID and required system principals; reject remote clients; validate client token identity; check every impersonation call and always revert; fail closed on token/ACL query errors. | Default named-pipe ACLs may grant read access to Everyone/anonymous. PID alone is not authorization. Same-user processes can generally satisfy same-user DACLs. |

Select exact native APIs and Rust crate/FFI only after minimum supported OS versions and packaging are fixed. A transport abstraction must not hide or silently omit peer-credential checks.

### 2.3 Peer identity is not approval

Peer-credential checks establish only kernel-reported properties of the connected process/session under platform semantics. They do not establish that the peer is the genuine LOCO binary, uncompromised, benign same-user code, that the user reviewed the action, or that action parameters/target state remain unchanged.

Do not authorize privileged actions solely because a request comes from expected UID, PID, pipe name, socket path, or process name. A shared secret stored in files readable by the same user does not solve same-user malware. If executable identity is required, evaluate OS code-signing/entitlement or process-token evidence as a separate platform-specific control. Bind requests to protocol version, fresh request identifier, operation/action digest, and bounded lifetime. Rust independently validates schema, capability, policy, target identity, and freshness.

### 2.4 Protocol and lifecycle requirements

- Version every message; reject unknown versions and unknown privileged operation types.
- Use length-delimited framing with strict maximum frame, field, nesting, and aggregate request sizes.
- Authenticate peer before processing application payloads; never trust client-supplied PID, UID, path, executable name, or role.
- Bind response correlation IDs to outstanding requests; reject duplicate, stale, and unsolicited responses.
- Apply connection/read/write/idle/operation deadlines; cap clients, queued work, and outstanding requests.
- Close on malformed framing, oversized messages, credential-query failure, protocol downgrade, or authorization error.
- Cancellation must never convert interrupted work into success.
- Prevent endpoint squatting and stale endpoint reuse. Do not blindly unlink an existing socket or pipe name without verifying ownership/type and prior-authority lifecycle.
- Log security denials without secrets, raw approval assertions, or unnecessary workspace content.
- Test process restart, PID-reuse assumptions, wrong-user and same-user clients, remote Windows clients, endpoint replacement, malformed frames, flooding, partial reads/writes, and authority crash/restart.

### 2.5 Relationship to authenticated approval

IPC peer authentication is separate from platform user-presence authentication. Rust remains the sole verifier/consumer of future authenticator assertions. TypeScript may display canonical proposal data and relay opaque evidence but cannot assert trusted approved=true. Python/provider messages remain proposals only. Approval binds the exact canonical action and target freshness; the writer revalidates immediately before mutation.

## 3. Implementation exit criteria

1. Select supported OS/version matrix and exact per-platform private storage directory/ACL strategy.
2. Decide whether Rust authority is always a spawned sidecar or needs independent local clients.
3. Select transport per platform and implement kernel peer-credential verification with fail-closed behavior.
4. Define same-user malware boundary and whether executable signing identity is required.
5. Prove DB/WAL/SHM creation and recovery remain inside protected storage boundary.
6. Either provision/test a real OS-enforced aggregate quota or keep it explicitly unsupported.
7. Add adversarial cross-platform tests for storage path races, ACLs, peer spoofing, endpoint squatting, protocol abuse, and resource exhaustion.
8. Complete independent security review before using IPC or approval store to authorize production writes or commands.

## References

- SQLite PRAGMA and WAL documentation: https://www.sqlite.org/pragma.html and https://www.sqlite.org/wal.html
- Linux unix(7) peer credentials: https://man7.org/linux/man-pages/man7/unix.7.html
- Apple getpeereid(3): https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man3/getpeereid.3.html
- Apple XPC peer PID caveats: https://developer.apple.com/documentation/xpc/xpc_connection_get_pid
- Microsoft Named Pipe Security: https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights
- Microsoft CreateNamedPipe: https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea
- Microsoft GetNamedPipeClientProcessId: https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeclientprocessid
- Microsoft ImpersonateNamedPipeClient: https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-impersonatenamedpipeclient

This records a design review, not an implementation or production security certification.
