# macOS `O_RESOLVE_BENEATH` and firmlink probe

**Status:** native experiment pending exact-head macOS CI; no adapter authorization change is made by this probe.

## Question

Can `openat(parent_fd, child_name, ... | O_RESOLVE_BENEATH)` reject a firmlink transition as an escape from the directory represented by `parent_fd`? If it does, the kernel's single open operation could close the preflight-to-open gap for this boundary. If it does not, the flag is not a firmlink control and the adapter must continue relying on opened-handle checks and a narrower platform guarantee.

Do not infer the answer from the flag's name or from its behavior on `..` and symbolic links.

## Public source contract

- The macOS `open(2)` manual says `O_RESOLVE_BENEATH` causes `openat` to fail when path resolution escapes the directory associated with the supplied descriptor. It describes the result in terms of directory hierarchy, but does not explicitly discuss firmlink transitions.
- XNU's `vn_open_auth` maps `O_RESOLVE_BENEATH` to `NAMEI_RESOLVE_BENEATH` and documents `ENOTCAPABLE` when resolution escapes the starting directory.
- XNU's name-cache lookup code also has explicit firmlink handling: when it encounters a firmlink source vnode, it can switch to the associated target vnode during name lookup. The public snippets reviewed do not, by themselves, settle whether that transition is treated as an escape by the beneath policy.
- Apple's `file_cmds` uses `O_RESOLVE_BENEATH` on `openat`, confirming that this is a real Darwin open flag used by system tooling. That usage does not establish firmlink-specific semantics.

Sources:
- [macOS open(2) manual](https://man.freebsd.org/cgi/man.cgi?apropos=0&manpath=macOS+26.7&query=open&sektion=2)
- [XNU vfs_vnops.c](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/vfs/vfs_vnops.c)
- [XNU vfs_cache.c](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/vfs/vfs_cache.c)
- [Apple file_cmds openat usage](https://github.com/apple-oss-distributions/file_cmds/blob/main/cp/utils.c)

## Native CI probe

`scripts/macos-openat-resolve-beneath-firmlink-probe.c` runs only on the macOS CI runner and:

1. Verifies that the flag is active by requiring `openat(root_fd, "..", ... | O_RESOLVE_BENEATH)` to fail with `ENOTCAPABLE`.
2. Opens ordinary controls `/System` and `/usr` relative to their parent descriptors.
3. Attempts the same descriptor-relative open for root firmlink mappings `/Users`, `/Applications`, `/Library`, and `/private`.
4. Enumerates all installed `/System/Library/` mappings from `/usr/share/firmlinks` and attempts each nested entry.
5. For successful opens, records `F_GETPATH_NOFIRMLINK` so the observed target-side path is visible beside the outcome.

The probe is diagnostic by design: mapped entries may be reported as either `blocked` or `opened`. It must not turn a surprising outcome into a false green authorization claim. The control case ensures the tested flag is actually enforced for a lexical parent escape. The native macOS CI log is the evidence used to decide whether this primitive contributes any firmlink-specific protection.

## Interpretation and acceptance

- If any real mapped entry opens successfully while its no-firmlink descriptor path points to the Data volume, `O_RESOLVE_BENEATH` is **not sufficient** to enforce the firmlink boundary on that runner. Do not add it as an authorization substitute.
- If all mapped entries are blocked, treat that as runner-specific empirical evidence only. Confirm the behavior against the supported macOS matrix and authoritative API semantics before relying on it.
- Ordinary controls must continue to open successfully; otherwise the probe is not discriminating.
- The `O_RESOLVE_BENEATH` result is separate from `O_NOFOLLOW`, which is documented for symbolic links and does not itself reject firmlinks.
- No production code changes are authorized by this probe. The existing adapter must keep checking the actual opened descriptor's `f_fsid`, firmlink-free descriptor path, root containment, object type, link count, and read-time metadata. Any missing or inconsistent check remains a hard denial.
