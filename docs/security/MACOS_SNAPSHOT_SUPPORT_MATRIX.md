# macOS workspace snapshot adapter support matrix

**Status:** conservative validation inventory, not a production support declaration. Native evidence is recorded for macOS 26.7.2 (build 25G332, arm64) and macOS 26.6.1 (build 25G76, x86_64), each on its exact hosted runner build.

## Validated configuration

| Dimension | Evidence | Current status |
|---|---|---|
| OS version | macOS 26.7.2 (build 25G332, arm64); macOS 26.6.1 (build 25G76, x86_64) | Tested on the two exact hosted runner builds |
| Architecture | arm64 and x86_64 hosted runners | Both tested on the exact macOS 26 builds recorded above |
| System/Data volume group | System and Data are separate APFS volumes with distinct descriptor-derived `f_fsid` values | Tested |
| `st_dev` behavior | System and Data returned the same `st_dev` value on this runner | Tested; do not use as the only volume boundary |
| `f_fsid` behavior | `fstatfs` on retained/opened descriptors distinguished System and Data | Tested |
| Firmlink preflight | `getattrlistbulk` with `ATTR_CMNEXT_NOFIRMLINKPATH` distinguished root and nested mappings from ordinary controls | Tested |
| Firmlink fallback | Direct `openat` bypassing preflight followed by descriptor `f_fsid` comparison rejected real System/Data targets before content read | Tested |
| `F_GETPATH_NOFIRMLINK` | Returned target-side paths for opened firmlink descriptors | Tested |
| Nested APFS volume | Separate APFS volume mounted below the workspace was rejected | Tested |
| Sandbox capability loss | Denying the relevant `fcntl` command caused the sidecar to omit that capability; acquisition fails closed when required descriptor evidence is unavailable | Tested |

Exact-head evidence for the opened-firmlink identity regression:
- Rust Core Tests [run #45350706796](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45350706796), code head `7a86ae80a2b0147fc4c058792abd44c5751219b3): Ubuntu, Windows, and macOS passed; macOS 130 passed, 0 failed, 1 ignored.
- Contract Validation [run #45350706807](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45350706807): passed.

## Not yet validated

| Dimension | Status |
|---|---|
| Earlier macOS releases (including 15.x, 14.x, 13.x, 12.x, 11.x, 10.15) | Unverified; no minimum supported version is declared |
| Future macOS releases | Unverified until CI evidence is added |
| Intel/x86_64 macOS | macOS 26.6.1 / build 25G76 hosted runner tested; other Intel macOS releases/builds remain unverified |
| APFS configurations outside the hosted System/Data volume group | Unverified |
| APFS snapshots, sealed-system variants, external APFS volumes, and unusual volume-group layouts | Unverified beyond the explicit nested-APFS mount rejection test |
| HFS+, exFAT, network filesystems, removable non-APFS filesystems | Not supported by this adapter's current APFS admission check; fail closed |
| Custom sandbox profiles beyond the tested restrictive profile | Unverified |
| Long paths near `PATH_MAX` and larger directory populations near the bulk-scan cap | Separate coverage/release gates |
| Concurrent content mutation that attempts to preserve metadata | Not an atomic snapshot guarantee; the adapter detects ordinary metadata changes but does not claim a filesystem snapshot |

## Runtime admission versus platform support

The runtime capability probe is a necessary fail-closed gate, not a substitute for a tested support matrix. A host must provide the required bulk no-firmlink attribute and descriptor-path evidence, and the workspace root must pass APFS admission. Missing or denied capabilities must not trigger a weaker fallback.

The authorization boundary for the tested System/Data firmlink model is the descriptor-derived `f_fsid` comparison performed after each open and before content acquisition. The bulk no-firmlink path is an early-denial observation. `SF_FIRMLINK`, `SF_NOUNLINK`, `sunlnk`, `O_RESOLVE_BENEATH`, and `st_dev` alone are not authorization signals.

Apple's public design describes firmlinks as System-to-Data mappings within a volume group. The hosted runner's System and Data volumes have distinct `f_fsid` values, even though `st_dev` is equal. This is strong evidence for that tested configuration, not proof for every APFS layout or OS release.

## Release policy

Until the unverified rows are tested and independently reviewed:

1. Do not publish a minimum macOS version based solely on header presence or a successful capability probe.
2. Treat macOS 26.7.2 arm64 (25G332) and macOS 26.6.1 Intel (25G76) as validated only for these exact hosted runner configurations.
3. Keep all other macOS versions/architectures and non-APFS filesystems unsupported or unverified; fail closed when the runtime contract cannot be met.
4. Require exact-head CI on every newly claimed OS/architecture/filesystem combination.
5. Require independent security review before enabling the workspace-read capability in a production release.
