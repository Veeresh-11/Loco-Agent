# macOS workspace snapshot adapter prototype

The macOS adapter pins an absolute, resolved workspace root by walking from `/` with descriptor-relative `openat` calls. Callers must provide the resolved path (for example, the workspace folder's canonical filesystem path); symlink components in that resolved path are rejected. Every path component is opened with `O_NOFOLLOW`; the final directory descriptor is retained as the workspace capability.

Capture walks each requested parent component relative to the retained workspace handle, rejecting symlinks and descendant mount/device crossings. The final file is opened with `O_NOFOLLOW | O_NONBLOCK`, checked as a regular file, rejected when its link count is not one, and read under both request and global byte limits. Device, inode, length, link count, mtime, and ctime are compared before and after the read. The shared snapshot helper creates and validates the content digest.

This is a read-only prototype. Metadata comparisons are best-effort mutation detection, not an atomic snapshot guarantee. It does not implement writes, approvals, authorization, or process execution. Runtime validation on supported macOS versions and filesystems remains required, including mount behavior, concurrent mutation, and filesystem-specific semantics.


## Firmlink containment: pre-open extended path attribute

The adapter now performs a parent-handle-relative preflight for every descendant path component before opening that component.

1. Obtain the parent's firmlink-free path from its retained descriptor with `F_GETPATH_NOFIRMLINK`.
2. Query the child entry with `getattrlistat(parent_fd, name, ...)`, requesting `ATTR_CMNEXT_NOFIRMLINKPATH` in the extended common-attribute namespace using `FSOPT_ATTR_CMN_EXTENDED | FSOPT_NOFOLLOW_ANY | FSOPT_RESOLVE_BENEATH`.
3. Construct the expected direct-child path as `parent_nofirmlink_path.join(name)`.
4. Require exact `PathBuf` equality between the returned path and the expected direct-child path. A mismatch returns `SnapshotPayloadError::FirmlinkBoundary` before `openat` is called for that child.
5. After opening, query `F_GETPATH_NOFIRMLINK` on the opened descriptor and require it to equal the preflight path, remain beneath the retained root, and pass the existing device/`f_fsid`, `O_NOFOLLOW`, file-type, hard-link, metadata-revalidation and bounded-read checks.

This permits a workspace root that the user deliberately selected through `/Users`: the root is established once, and its retained descriptor path is already canonicalized. It rejects a descendant entry that introduces a new firmlink redirection beneath that root.

### Native API evidence

The SDK exposes `ATTR_CMNEXT_NOFIRMLINKPATH` as an extended common attribute. XNU's `attrlist_build_path` routes this attribute through `build_path(..., BUILDPATH_NO_FIRMLINK)`; the path builder has explicit `BUILDPATH_NO_FIRMLINK` handling for vnode firmlink relationships. This reconstructs the path from vnode/name-cache metadata rather than reading file contents:

- [XNU vfs_attrlist.c — extended no-firmlink path attribute](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/vfs/vfs_attrlist.c#L90-L135)
- [XNU vfs_cache.c — build_path and BUILDPATH_NO_FIRMLINK](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/vfs/vfs_cache.c#L700-L915)
- [XNU sys/attr.h — ATTR_CMNEXT_NOFIRMLINKPATH and FSOPT_ATTR_CMN_EXTENDED](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/attr.h)

The native C probe on macOS 26.7.2 (build 25G332, arm64) verified both `getattrlistbulk` and `getattrlistat` behavior. The `getattrlistat` query used `FSOPT_NOFOLLOW_ANY | FSOPT_RESOLVE_BENEATH | FSOPT_ATTR_CMN_EXTENDED`; the bulk enumerator used `FSOPT_ATTR_CMN_EXTENDED`. Both returned:

| Entry | `ATTR_CMNEXT_NOFIRMLINKPATH` |
|---|---|
| `/Users` | `/System/Volumes/Data/Users` |
| `/Applications` | `/System/Volumes/Data/Applications` |
| `/Library` | `/System/Volumes/Data/Library` |
| `/private` | `/System/Volumes/Data/private` |
| `/System` | `/System` |
| `/usr` | `/usr` |

The probe asserts these exact mapped and ordinary-directory controls. The ordinary paths remain unchanged while the three firmlink paths resolve to their firmlink-free locations. The older `SF_FIRMLINK` flag remains absent on this runner; the extended path attribute is the signal used by the adapter.

### Rust tests and exact-head validation

PR #103 head `0841e76e6a0e0765b4ca15e3b16be6360c103c73` passed:
- Rust Core Tests [run #45342237513](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45342237513): Ubuntu, Windows and macOS.
- TypeScript CI [run #45342237538](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45342237538): Node 20 and Node 22.
- Contract Validation [run #45342237350](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45342237350).

The macOS run reported 118 passed, 0 failed, 1 ignored. New tests verify:
- Real `/Users`, `/Applications`, and `/Library` entries are rejected by preflight before target opening.
- Ordinary `/System` and `/usr` entries pass the same preflight.
- A workspace rooted under `/Users` remains usable.
- Malformed, truncated, missing-attribute, relative and unterminated path records fail closed.
- Six concurrent workers can perform repeated preflight checks against the same retained directory descriptor without shared directory-iteration state.

The sidecar advertises `macos_attr_cmnex_nofirmlinkpath` only when the extended attribute query works, and `macos_fcntl_getpath_nofirmlink` only when command 102 works. Under the CI Seatbelt profile that denies `system-fcntl`, the first capability remains available while the second is omitted. The adapter requires both and fails closed if either is unavailable.

### Scope and remaining review

The extended attribute query is a metadata/path query performed before the adapter opens the child descriptor; it does not acquire the target file descriptor or read target content. Kernel path resolution still evaluates the requested directory entry, so this is not a claim that the kernel performs no vnode/name-cache traversal.

Apple's WWDC19 filesystem session describes firmlinks as traversal points from a directory on the System volume to a directory on the Data volume, with one-to-one mappings that cannot cross a volume-group boundary; Apple also states that firmlinks are created by the installer. Apple Developer Forums guidance says end users cannot create firmlinks. A same-volume fixture is therefore not the right acceptance criterion. The hosted runner's `/usr/share/firmlinks` includes nested System/Data mappings below `/System/Library` (for example `Caches`, `Assets`, and `Speech`); the nested-preflight test now enumerates all OS-provided mappings beneath `/System/Library`, including deeper entries such as `/System/Library/CoreServices/CoreTypes.bundle/Contents/Library`, and requires rejection before `openat`. Supported macOS versions/filesystems, sandbox profiles, long-path limits and independent security review remain release gates. The path attribute supplements, and does not replace, handle-relative opens, post-open descriptor comparison, `f_fsid` checks, or metadata revalidation. Sources: [Apple WWDC19 session 710](https://developer.apple.com/videos/play/wwdc2019/710/) and [Apple Developer Forums filesystem lab notes](https://developer.apple.com/forums/thread/117521).
