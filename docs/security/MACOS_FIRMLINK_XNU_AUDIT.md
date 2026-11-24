# macOS firmlink XNU resolution and security-boundary audit

**Status:** source audit; no production workspace-confinement guarantee is approved.

**Scope:** public XNU/VFS behavior relevant to `getattrlistbulk`, `ATTR_CMN_FLAGS`, `ATTR_CMNEXT_NOFIRMLINKPATH`, path reconstruction, and the adapter's separate preflight/open sequence. This is not an audit of private APFS implementation code or a guarantee for every macOS release.

## Executive finding

The public sources establish three useful facts:

1. Apple's `getattrlistbulk(2)` manual says a firmlink directory entry should report `SF_FIRMLINK` in `ATTR_CMN_FLAGS`, and that the returned attributes describe the firmlink rather than its target.
2. XNU's VFS attribute tables define `ATTR_CMNEXT_NOFIRMLINKPATH` as an extended common attribute whose payload is an `attrreference`. Its table entry has no corresponding `vnode_attr` bit; the source comments explain that zero-bit attributes are synthesized at the VFS layer rather than expected from the filesystem's vnode-attribute response.
3. XNU path reconstruction has an explicit `BUILDPATH_NO_FIRMLINK` mode. Without that mode, `build_path_with_parent` can substitute a firmlink source vnode when the starting vnode is a firmlink target; with the mode, those substitutions are skipped.

These facts explain why a no-firmlink path can differ from the ordinary namespace path. They do **not** explain why hosted macOS 26.7.2 returns `SF_NOUNLINK` rather than the documented `SF_FIRMLINK`, nor do they establish that a userland metadata query and a later `openat` form one atomic authorization operation. The public source does not expose the APFS implementation needed to reconcile the observed flag discrepancy.

## Source findings

### 1. Bulk directory-entry contract

The XNU `getattrlistbulk(2)` manual states that the call returns attributes for directory entries, that symbolic-link metadata describes the link itself rather than its target, and specifically that a firmlink entry should have `SF_FIRMLINK` in `ATTR_CMN_FLAGS`. It further says the bulk attributes are those of the firmlink, not the firmlink target.

The same manual defines the record framing: each record begins with a 32-bit length; `ATTR_CMN_RETURNED_ATTRS` is required and identifies which attributes were actually returned; `ATTR_CMN_ERROR`, when requested and returned, follows the returned-attributes set. The per-entry error field is therefore conditional, not an unconditional word in every record.

Source: [XNU getattrlistbulk(2)](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/man/man2/getattrlistbulk.2).

### 2. Extended no-firmlink path attribute

XNU's `sys/attr.h` defines `ATTR_CMNEXT_NOFIRMLINKPATH` in the extended common-attribute bitset. Extended common attributes are carried in the `forkattr` field when `FSOPT_ATTR_CMN_EXTENDED` is supplied. The header also defines `attrreference_t` as a signed 32-bit data offset plus a 32-bit length.

In `vfs_attrlist.c`, the `ATTR_CMNEXT_NOFIRMLINKPATH` table entry has an `attrreference` payload, a zero `vnode_attr` bit mask, and a read-attributes authorization action. The surrounding source comment says a zero bit means VFS synthesizes the attribute rather than expecting the underlying filesystem to provide it in the ordinary vnode-attribute response. This is evidence about the VFS attribute contract, not proof that the returned string is an immutable identity or authorization token.

Sources:
- [XNU sys/attr.h](https://github.com/apple/darwin-xnu/blob/main/bsd/sys/attr.h)
- [XNU vfs_attrlist.c](https://github.com/apple/darwin-xnu/blob/main/bsd/vfs/vfs_attrlist.c)

### 3. Firmlink-aware path reconstruction

XNU's `vfs_cache.c` documents `build_path_with_parent` as reconstructing a path from a vnode. Under `CONFIG_FIRMLINKS`, the implementation checks whether the vnode is a firmlink target and, unless `BUILDPATH_NO_FIRMLINK` is set, switches to the associated firmlink source vnode. The same conditional substitution is repeated while walking the parent/mount chain. With `BUILDPATH_NO_FIRMLINK`, these source-vnode substitutions are skipped.

The function also documents that the name-cache lock stabilizes the vnode parent and mount-covered chains while path reconstruction is in progress. That lock is internal to this kernel operation; it is not held across the caller's later userland `openat`, read, or descriptor validation.

The source demonstrates a kernel path-building mode that preserves the target-side path representation. It does not, by itself, prove that the entire lookup was free of firmlink traversal, that the path is stable after the syscall returns, or that a later name-based open refers to the same vnode.

Source: [XNU vfs_cache.c](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/vfs/vfs_cache.c).

### 4. Lookup and no-follow semantics

Firmlinks are transparent namespace traversal points, not symbolic links. The `FSOPT_NOFOLLOW_ANY` documentation concerns symbolic links; it must not be treated as a general firmlink-denial flag. The public `getattrlistat(2)` contract does not provide an atomic "inspect this directory entry without firmlink resolution, then open that same entry" operation.

Source: [XNU getattrlist(2)](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/man/man2/getattrlist.2).

### 5. Platform design and fixture constraints

Apple's WWDC19 filesystem session describes firmlinks as one-to-one traversal points from the System volume to the Data volume that cannot cross a volume-group boundary, and says the installer creates them. Apple's Filesystems lab answer says end users cannot create firmlinks. A synthetic same-volume symlink or user-created APFS construct is therefore not an equivalent native firmlink fixture.

Sources:
- [Apple WWDC19 session 710](https://developer.apple.com/videos/play/wwdc2019/710/)
- [Apple Developer Forums Filesystems lab Q&A](https://developer.apple.com/forums/thread/117521)

## Reconciliation with native observations

Issue #74 and PR #106 record macOS 26.7.2 (build 25G332, arm64) observations from the hosted runner. For mapped entries, `ATTR_CMN_FLAGS` did not include the documented `SF_FIRMLINK` bit. The same bulk records contained no-firmlink paths under `/System/Volumes/Data` for `Users`, `Applications`, `Library`, and `private`; ordinary controls `System` and `usr` retained their namespace paths.

The returned-attribute mask and raw record bytes were captured, and PR #107 corrected the parser to handle `ATTR_CMN_ERROR` only when that bit is present. That ABI correction removes a record-offset ambiguity, but it does not reconcile the native flag mismatch.

The public VFS code helps explain the shape and purpose of a no-firmlink path. It does not show the private APFS code path that vends the observed `ATTR_CMN_FLAGS` value. No authoritative Apple statement found in this audit says that `SF_NOUNLINK` or `sunlnk` is an alias for `SF_FIRMLINK`; the implementation must not infer that equivalence.

## Security boundary and acceptance decision

### What the current adapter evidence supports

- A bounded `getattrlistbulk` scan can collect kernel-returned metadata/path observations for directory entries before the application calls `openat`.
- A fresh directory file description per scan avoids sharing the bulk-enumeration offset between concurrent captures.
- `O_NOFOLLOW` rejects symbolic-link final-component substitution; it does not independently reject firmlinks.
- Post-open descriptor-path checks and filesystem identity checks provide independent evidence about the object actually opened.
- Cross-volume `f_fsid` checks can reject the System-to-Data transition when the relevant volumes have distinct filesystem identities on the tested configuration.

### What is not established

- The no-firmlink path string is not an atomic capability, vnode handle, or proof that a subsequent name-based open addresses the exact entry observed.
- The VFS name-cache lock used during path reconstruction does not span the userland preflight-to-open interval.
- `SF_NOUNLINK`, `sunlnk`, path differences, and `/usr/share/firmlinks` membership are diagnostic evidence only; none may be promoted to an authorization signal.
- A distinct-volume check does not prove confinement for every volume-group layout, supported macOS version, filesystem, sandbox profile, or future APFS behavior.
- Public XNU sources do not explain the hosted runner's missing `SF_FIRMLINK` bit because the relevant APFS implementation is not available in this source audit.
- The nested System/Data regression improves coverage of OS-provided mappings, but it does not prove atomicity against an adversarial concurrent namespace change or establish a same-volume firmlink case.

### Decision

Keep the workspace-read capability **unverified for general firmlink confinement**. Preserve fail-closed behavior when any required metadata, path, descriptor, volume-identity, or revalidation check is unavailable or inconsistent. Do not claim production-grade firmlink confinement from the current hosted-runner evidence.

Issue #74 remains open. PRs #104, #106, #107, and the nested-fixture PR #108 must remain draft until:
1. Apple/XNU semantics for the observed flag discrepancy are authoritatively reconciled, or the implementation is redesigned so the disputed signal is not required;
2. the supported macOS/filesystem/version matrix and race model are explicitly bounded;
3. the complete stack receives independent security review; and
4. exact-head validation passes for the reviewed stack.

No PR in this stack should be merged solely because CI is green.
