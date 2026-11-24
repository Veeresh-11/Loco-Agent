# macOS workspace snapshot adapter prototype

The macOS adapter pins an absolute, resolved workspace root by walking from `/` with descriptor-relative `openat` calls. Callers must provide the resolved path (for example, the workspace folder's canonical filesystem path); symlink components in that resolved path are rejected. Every path component is opened with `O_NOFOLLOW`; the final directory descriptor is retained as the workspace capability.

Capture walks each requested parent component relative to the retained workspace handle, rejecting symlinks and descendant mount/device crossings. The final file is opened with `O_NOFOLLOW | O_NONBLOCK`, checked as a regular file, rejected when its link count is not one, and read under both request and global byte limits. Device, inode, length, link count, mtime, and ctime are compared before and after the read. The shared snapshot helper creates and validates the content digest.

This is a read-only prototype. Metadata comparisons are best-effort mutation detection, not an atomic snapshot guarantee. It does not implement writes, approvals, authorization, or process execution. Runtime validation on supported macOS versions and filesystems remains required, including mount behavior, concurrent mutation, and filesystem-specific semantics.

## Follow-up — APFS volume identity boundary (2026-11-24)

The adapter now records the filesystem identity returned by `fstatfs` on the retained workspace-root descriptor. After each descriptor-relative intermediate-directory open and final-file open, it queries `f_fsid` on that opened descriptor and requires the value to match the root identity. The existing `st_dev` check remains in place. The filesystem identity is handled as an opaque fixed-width Darwin `fsid_t` representation; the code does not inspect libc's private fields.

### Native evidence

PR #77 exact-head commit `cfe0510ea6deff4f74476b622ceffd9d5d6da82b` passed:
- Rust Core Tests run [45322641807](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45322641807): Ubuntu, Windows, and macOS.
- Contract Validation run [45322641628](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45322641628).
- GitGuardian Security Checks on the exact commit.

The macOS 26.7.2 (build 25G332, arm64) job successfully ran the native firmlink metadata diagnostic, core unit tests, and nested APFS volume fixture. The nested-volume test asserted distinct handle-derived `f_fsid` values as well as distinct `st_dev`, then verified capture failed closed. Ubuntu's real bind-mount and distinct ext4 mount tests passed; Windows' real secondary NTFS mountpoint test passed.

The separate System/Data diagnostic previously observed the harder case: `/` and `/Users` shared `st_dev` but returned distinct `f_fsid` values. PR #77's test-only adapter rooted at `/` verifies capture through the actual `/Users` mapping fails before content acquisition.

### Security scope and limitations

This is a handle-derived **volume identity boundary**, not a universal firmlink detector. It rejects traversal when an opened descendant resolves to a different filesystem identity than the retained root, including the observed System-to-Data mapping on the tested runner. It does not prove that every firmlink crosses a distinct `f_fsid`, and it cannot distinguish a hypothetical same-volume firmlink from an ordinary directory by volume identity alone.

The hosted macOS runner returned `SF_NOUNLINK`, not `SF_FIRMLINK`, for mapped entries through the tested metadata APIs. The implementation does not reinterpret `SF_NOUNLINK`, `sunlnk`, or pathname maps as firmlink evidence. The native firmlink-specific signal remains unresolved for general cases. Do not expand the claim to other macOS releases, APFS arrangements, HFS+, network/removable filesystems, or untested storage configurations.

This remains a read-only snapshot prototype. Metadata checks are not an atomic snapshot guarantee and do not implement approvals, controlled writes, or process execution. PR #77 remains draft pending the broader independent security review and completion of the approval/write/integration gates.



## Follow-up diagnostic — BSD stat flag surfaces (2026-11-24)

The existing native probe established that `getattrlistbulk`, `getattrlistat`, and `fgetattrlist` returned `SF_NOUNLINK` rather than `SF_FIRMLINK` for the hosted macOS runner's mapped directories. A further diagnostic now records `st_flags` from:
- `fstatat(..., AT_SYMLINK_NOFOLLOW)` on the parent-relative entry;
- `fstatat(..., 0)` on the followed entry; and
- `fstat()` on the opened directory handle.

It records `st_dev` and `st_ino` alongside the flags so the results can be compared with the existing `getattrlist*` output. `AT_SYMLINK_NOFOLLOW` is diagnostic only: firmlinks are not POSIX symbolic links, and this option must not be treated as a firmlink denial primitive.

The Rust Core Tests workflow now runs when `scripts/macos-*.c` changes, so this probe is independently exercised by the macOS runner. This experiment does not change production adapter behavior. Until the exact-head macOS output establishes a reliable distinction, the adapter continues to enforce only the tested handle-derived volume identity boundary and must not claim universal firmlink detection.


### Exact-head BSD stat diagnostic result (PR #100, 2026-11-24)

Rust Core Tests run [45334961812](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45334961812) passed on Ubuntu, Windows, and macOS. On the macOS 26.7.2 (build 25G332, arm64) runner, the new probe compiled with `clang -Wall -Wextra -Werror` and executed successfully. For `/Users`, `/Applications`, and `/Library`, all six queried surfaces reported `0x00100000` (`SF_NOUNLINK`): `getattrlistbulk`, `getattrlistat` with `FSOPT_NOFOLLOW_ANY | FSOPT_RESOLVE_BENEATH`, `fgetattrlist`, `fstatat` with `AT_SYMLINK_NOFOLLOW`, followed `fstatat`, and `fstat` on the opened handle. None reported `SF_FIRMLINK` (`0x00800000`). The raw `getattrlistbulk` records showed the requested/returned attribute layout and the `ATTR_CMN_FLAGS` value at the parsed offset, so this result is not explained by a missing returned-attribute bit or an unparsed field.

This closes the current diagnostic comparison of the available BSD `stat` and getattrlist surfaces on this hosted image, but it does not explain the discrepancy with Apple's documented contract. Apple's `getattrlistbulk(2)` manual states that a firmlink directory entry has `SF_FIRMLINK` in `ATTR_CMN_FLAGS`, and Apple's public `FileFlags.firmlink` documentation identifies the corresponding C constant. See [XNU getattrlistbulk(2)](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/man/man2/getattrlistbulk.2) and [Apple FileFlags.firmlink](https://developer.apple.com/documentation/system/fileflags/firmlink).

**Next evidence gate:** obtain an authoritative Apple DTS / Developer Forums explanation for why the documented flag is absent on this live runner, or obtain a reproducible Apple-provided OS firmlink fixture. A user-created APFS volume image is not accepted as a substitute. Until then, keep issue #74 open and treat general firmlink containment as unsupported/unverified. The tested `f_fsid` distinct-volume boundary remains a narrower, independently validated control.


### Follow-up candidate — descriptor path without firmlink resolution (PR #102)

The BSD stat-surface probe did not expose `SF_FIRMLINK`, so a separate native diagnostic now compares `fcntl(F_GETPATH)` with `fcntl(F_GETPATH_NOFIRMLINK)` on the retained root descriptor and each opened mapped-directory descriptor. Apple's XNU public `sys/fcntl.h` defines `F_GETPATH_NOFIRMLINK` as command 102 with the comment “return the full path without firmlinks of the fd”: https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/fcntl.h. An Apple Developer Forums discussion reports that, for an open descriptor reached via `/Users/<user>`, `F_GETPATH` returned `/Users/<user>` while `F_GETPATH_NOFIRMLINK` returned `/System/Volumes/Data/Users/<user>`: https://developer.apple.com/forums/thread/120665.

This is a promising handle-associated diagnostic surface and is being tested on the actual hosted runner. It is not yet accepted as a security boundary: the public behavioral guarantees, sandbox behavior, path-length handling, rename/concurrency semantics, and whether its output can safely establish descendant containment still require authoritative confirmation and adversarial tests. Do not use a path-string comparison as production authorization. PR #102 is stacked on PR #100 and remains draft pending exact-head native results.


### Exact-head descriptor-path result (PR #102, 2026-11-24)

The exact-head macOS job in Rust Core Tests run [45335594962](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45335594962) successfully compiled the probe with `clang -Wall -Wextra -Werror` and executed it on macOS 26.7.2 (build 25G332, arm64). For the retained descriptors opened at the root entries, the observed values were:

| Entry | `F_GETPATH` | `F_GETPATH_NOFIRMLINK` |
|---|---|---|
| `/` | `/` | `/` |
| `/Users` | `/Users` | `/System/Volumes/Data/Users` |
| `/Applications` | `/Applications` | `/System/Volumes/Data/Applications` |
| `/Library` | `/Library` | `/System/Volumes/Data/Library` |

This is direct evidence that the API exposes a non-firmlinked path for these opened firmlink directory descriptors on this runner, unlike the tested `st_flags` and getattrlist flag surfaces. Apple's macOS `fcntl(2)` manual describes `F_GETPATH_NOFIRMLINK` as returning the non-firmlinked path and requires a buffer of at least `MAXPATHLEN`: https://man.freebsd.org/cgi/man.cgi?manpath=macOS+26.7&query=fcntl&sektion=2. The XNU header defines command 102: https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/fcntl.h.

The Apple Developer Forums discussion that identified this command also notes that it only works on open files and that paths longer than `MAXPATHLEN` are a limitation; the author explicitly asks whether it works in a sandbox, without a confirmed answer in that discussion: https://developer.apple.com/forums/thread/120665. Therefore sandbox availability and error behavior remain open compatibility questions.

**Security interpretation:** the descriptor-associated output is materially stronger diagnostic evidence than a path-based lookup, but the result is still a pathname string. It is not an atomic ancestry proof and must not replace handle-relative operations or mount identity checks. A string prefix comparison alone is not accepted for authorization.

### Rename and replacement experiment

The probe now also opens a temporary directory, records both descriptor paths, renames that directory while keeping the descriptor open, creates a replacement directory at the original name, and queries the original and replacement descriptors independently. It records whether the original descriptor retains its device/inode identity and whether the replacement has a distinct identity. This experiment is intended to establish whether the path strings track the opened vnode across rename/replacement; it does not claim concurrent-race safety.

The rename experiment was added after run #45335594962, so it still requires a fresh exact-head macOS run. Keep PR #102 and issue #74 open until that evidence is reviewed and sandbox/compatibility and security semantics are resolved.


### Descriptor-path rename/replacement fixture (PR #102 follow-up)

A new native CI experiment now opens a temporary directory by descriptor, captures both path forms, renames the directory while retaining the original descriptor, creates a different directory at the old pathname, and then queries both descriptors. The probe asserts:
- the original descriptor's device/inode identity remains unchanged across the rename;
- both path APIs report the moved name for the retained descriptor;
- both path APIs report the original name for the replacement descriptor; and
- the replacement descriptor has a distinct device/inode identity.

The assertions deliberately require a NUL-terminated result in a `MAXPATHLEN`-sized buffer. This follows the macOS `fcntl(2)` manual's documented buffer requirement. This fixture is a sequential rename/replacement test, not a concurrent-race proof. The latest assertion-bearing source commit is `b63b8ca309850ffb615704ccfde728e0f2dd09d3`; exact-head macOS CI is still required before recording its outcome.
