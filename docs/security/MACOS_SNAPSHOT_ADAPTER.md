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

