# macOS firmlink native evidence reconciliation

**Status:** investigation only; no production authorization guarantee is added by this document.

## Documented API contract

The public XNU `getattrlistbulk(2)` manual says:

- the call returns attributes for directory entries;
- a firmlink directory entry has `SF_FIRMLINK` in `ATTR_CMN_FLAGS`; and
- the attributes returned for that entry describe the firmlink, not its target.

Source: [Apple XNU getattrlistbulk(2)](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/man/man2/getattrlistbulk.2).

Apple's `FileFlags.firmlink` documentation identifies the corresponding C constant as `SF_FIRMLINK`: [Apple Developer Documentation](https://developer.apple.com/documentation/system/fileflags/firmlink).

## What the hosted runner has actually shown

The issue #74 investigation records exact-head runs on macOS 26.7.2 (build 25G332, arm64). For the known root mappings `/Users`, `/Applications`, and `/Library`, the BSD metadata surfaces returned `0x00100000` (`SF_NOUNLINK`), not `0x00800000` (`SF_FIRMLINK`). The diagnostic compared bulk attributes, `getattrlistat`, `fgetattrlist`, no-follow/follow `fstatat`, and `fstat` on opened handles. The recorded probe also reported the returned attribute bitset and raw record bytes, so the recorded result was not explained by the flag bit being omitted from the returned set.

Sources and exact runner records:
- [Issue #74 native investigation](https://github.com/Veeresh-11/Loco-Agent/issues/74)
- [PR #100 BSD stat diagnostic](https://github.com/Veeresh-11/Loco-Agent/pull/100)
- [Run 45334961812](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45334961812)

The separate `ATTR_CMNEXT_NOFIRMLINKPATH` experiment recorded these bulk results on the same hosted macOS image:

| Entry | `ATTR_CMN_FLAGS` | no-firmlink path |
|---|---:|---|
| `Users` | `0x00100000` | `/System/Volumes/Data/Users` |
| `Applications` | `0x00100000` | `/System/Volumes/Data/Applications` |
| `Library` | `0x00100000` | `/System/Volumes/Data/Library` |
| `private` | `0x00108000` | `/System/Volumes/Data/private` |
| `System` | `0x00080000` | `/System` |
| `usr` | `0x00088000` | `/usr` |

Source: [Issue #74 bulk extended-path result](https://github.com/Veeresh-11/Loco-Agent/issues/74#issuecomment-7124124696).

These observations distinguish the mapped entries from the ordinary controls by their no-firmlink path values, but they do not explain why `SF_FIRMLINK` is absent or prove the complete kernel traversal/race property required for a general authorization guarantee.

## Fixture limitation

Apple's Filesystems lab answer states that firmlinks cannot be created by end users and are currently reserved to the system. A user-created APFS image or an ordinary symlink is therefore not a valid substitute for an OS-provided firmlink fixture.

Source: [Apple Developer Forums — Filesystems lab Q&A](https://developer.apple.com/forums/thread/117521).

Apple's WWDC19 explanation describes firmlinks as one-to-one mappings used to connect the system and data volumes, and says they cannot cross a volume-group boundary: [What's New in Apple File Systems](https://developer.apple.com/videos/play/wwdc2019/710/).

## Probe and CI changes in this PR

The native C probe now requests `ATTR_CMNEXT_NOFIRMLINKPATH` through the extended common-attribute mode of `getattrlistbulk`, reports both the raw common flags and returned no-firmlink path, and includes `/private` and ordinary `/System` and `/usr` controls. It remains diagnostic: it does not turn `SF_NOUNLINK`, a path string, or any other marker into `SF_FIRMLINK`.

The Rust Core Tests workflow now triggers when `scripts/macos-*.c` changes. Previously the workflow's path filters omitted the probe directory, so a probe-only edit could fail to schedule the native macOS job.

## Acceptance decision

- Do not infer `SF_FIRMLINK` from `SF_NOUNLINK`, `sunlnk`, or a path difference.
- Do not create a fake firmlink fixture or treat a user-created APFS image as equivalent.
- Keep issue #74 open until the documented flag discrepancy is explained by authoritative Apple guidance or a supported OS-provided fixture is available.
- Keep the workspace-read capability unsupported/unverified for the unresolved property. Preserve existing fail-closed behavior and the separately tested distinct-volume boundary.
- Require independent security review before merging the workspace-read stack.


## Review follow-up — 2026-11-27

A scoped review of the complete PR #106 patch found that the diagnostic C record parser checked that the no-firmlink path payload was inside the record and NUL-terminated, but did not require its start to follow the attrreference_t itself. It also did not require an absolute path or prevent the name payload from overlapping the path payload. These checks matter even in a diagnostic probe: malformed offsets could otherwise be interpreted as plausible path evidence.

The probe is hardened in commit `fbf79969c4a2ad73eef9fbc371e6602d70534d4e`:

- require the name payload to start after the name reference and the subsequent fixed fields;
- require the no-firmlink path payload to start after its own reference;
- require an absolute, exactly NUL-terminated path with no embedded NUL;
- reject overlap between the name and no-firmlink path payloads.

This changes only diagnostic record validation; it does not alter adapter authorization or reinterpret any filesystem flag. Exact-head CI for the new commit must complete before the probe change is considered validated. The previous run #45344892209 applies to the earlier head only.


### CI finding and correction

The first exact-head run after the parser hardening exposed a compile error in the diagnostic C probe: the name-range variables were scoped inside the optional name-attribute block but referenced later during the path-overlap check. The macOS job failed at the clang compile step before running the native probe. Corrected in code commit `e4fa7d51a0ab56040d7caa3d13fd4afe3820d306` by retaining the name start/length in the enclosing record scope and applying the absolute-path check. The failed run is [#45984098897](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45984098897); its failure is a real compile regression in the review fix, not a runner issue. A new exact-head run is required.


## Diagnostic parser follow-up — 2026-11-27

A subsequent source review found that the name payload lower bound reserved the flags word and extended-path reference unconditionally, although the parser has branches for records where those attributes are omitted. It also accepted an embedded NUL before the final name terminator. Both conditions are now handled in commit `b9361b2e1c4303e6b9f8bf9edfac98e4dca0776f`: the fixed-field reservation follows the returned-attribute bitmap, and the name must be exactly NUL terminated. This is parser robustness only; it does not change the diagnostic meaning of flags or paths. Exact-head CI is pending.
