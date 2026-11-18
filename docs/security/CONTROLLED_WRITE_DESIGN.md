
## Platform research notes (initial, documentation-backed)

These notes are an initial evidence pass, not a finalized implementation decision. The research must be validated against the actual Rust crate/platform support matrix and filesystem behavior before implementation.


### Linux — candidate approach, not yet selected

- Linux `openat2()` offers path-resolution controls including `RESOLVE_BENEATH`, `RESOLVE_IN_ROOT`, `RESOLVE_NO_MAGICLINKS`, and `RESOLVE_NO_SYMLINKS`. These are designed for restricting untrusted path resolution; no single flag should be treated as the complete workspace boundary.
- `RESOLVE_NO_SYMLINKS` rejects symlink traversal across path components and implies `RESOLVE_NO_MAGICLINKS`; the implementation should still explicitly consider magic-link behavior and supported kernel versions.
- Directory-fd-relative operations should be considered to reduce dependence on ambient current-directory state.
- `rename()` replacement semantics are documented as atomic with respect to the destination name on Linux, but atomic namespace replacement is not equivalent to durable data or race-free authorization. Durability requires separate flush policy and filesystem consideration.
- Candidate Linux primitives: `openat2`, `O_NOFOLLOW`, `fstat` identity/type checks, and `renameat2` where its flags are appropriate. Kernel/filesystem support and safe fallback policy remain to be decided.

References:
- https://man7.org/linux/man-pages/man2/openat2.2.html
- https://man7.org/linux/man-pages/man2/rename.2.html

### Windows — candidate approach, not yet selected

- Windows reparse points include symbolic links and other filesystem redirection mechanisms; a safe path policy must inspect reparse-point attributes and handle behavior rather than just normalize strings.
- Microsoft documents `FILE_FLAG_OPEN_REPARSE_POINT` / `FILE_FLAG_OPEN_REPARSE_POINT` behavior through `CreateFile`, allowing the caller to open the reparse point itself rather than following normal processing. This is a building block, not a complete traversal defense.
- `ReplaceFile` is a candidate replacement primitive and Microsoft documents preservation of original-file attributes such as creation time, ACLs, and encryption attributes. The desired metadata policy must be explicit; blindly preserving metadata may also preserve unwanted properties.
- File identity, sharing modes, directory-handle traversal, and race behavior require additional research and dedicated Windows tests before a design decision.

References:
- https://learn.microsoft.com/windows/win32/fileio/reparse-point-operations
- https://learn.microsoft.com/windows/win32/api/fileapi/nf-fileapi-createfilew
- https://learn.microsoft.com/windows/win32/fileio/moving-and-replacing-files

### macOS — candidate approach, not yet selected

- Apple's Secure Coding guidance recommends exclusive temporary-file creation using `O_CREAT` and `O_EXCL`, followed by descriptor-based operations.
- The guidance also describes using `O_NOFOLLOW` when opening existing files and checking descriptor-based metadata such as ownership, type, and permissions.
- These references provide useful building blocks, but they do not establish a complete directory-relative, atomic replace, durability, or TOCTOU strategy for LOCO Agent. Those details require focused research and tests.

References:
- https://developer.apple.com/library/archive/documentation/Security/Conceptual/SecureCodingGuide/Articles/RaceConditions.html
- https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/open.2.html

### Research status and guardrails

Linux has a stronger initial candidate set of path-resolution controls documented in the sources above. Windows and macOS references identify useful reparse/no-follow and exclusive-creation primitives but do not yet justify choosing an end-to-end write design. Do not claim identical guarantees across platforms. Next research should compare exact handle-relative traversal, file identity, atomic create/replace, flush/durability, filesystem support, and failure semantics before implementation.
