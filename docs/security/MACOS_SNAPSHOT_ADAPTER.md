# macOS workspace snapshot adapter prototype

The macOS adapter pins an absolute, resolved workspace root by walking from `/` with descriptor-relative `openat` calls. Callers must provide the resolved path (for example, the workspace folder's canonical filesystem path); symlink components in that resolved path are rejected. Every path component is opened with `O_NOFOLLOW`; the final directory descriptor is retained as the workspace capability.

Capture walks each requested parent component relative to the retained workspace handle, rejecting symlinks and descendant mount/device crossings. The final file is opened with `O_NOFOLLOW | O_NONBLOCK`, checked as a regular file, rejected when its link count is not one, and read under both request and global byte limits. Device, inode, length, link count, mtime, and ctime are compared before and after the read. The shared snapshot helper creates and validates the content digest.

This is a read-only prototype. Metadata comparisons are best-effort mutation detection, not an atomic snapshot guarantee. It does not implement writes, approvals, authorization, or process execution. Runtime validation on supported macOS versions and filesystems remains required, including mount behavior, concurrent mutation, and filesystem-specific semantics.
