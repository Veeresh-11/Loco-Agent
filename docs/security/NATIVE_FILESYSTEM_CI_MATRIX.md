# Native filesystem CI matrix

**Status:** this is a record of the hosted CI environments and native filesystem fixtures exercised by the Rust Core Tests workflow. It is not a declaration that every filesystem or operating-system release is supported in production.

## Exact CI run

Rust Core Tests [run #45353310385](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45353310385) completed successfully on all six jobs for the PR #113 environment-capture change. The explicit Windows/macOS matrix [run #45354176701](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45354176701) completed successfully on all seven jobs. The expanded Linux/Windows/macOS matrix [run #45354964372](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45354964372) completed successfully on all nine jobs: Ubuntu 22.04/24.04/26.04, Windows Server 2022/2025, macOS 15 arm64/Intel, and macOS 26 arm64/Intel. The native Windows SDK ABI probe also passed on both Windows runner generations in [run #45358148067](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45358148067). The tmpfs workspace-root case and the complete nine-job matrix passed in [run #45358956133](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45358956133): Ubuntu 22.04/24.04/26.04, Windows Server 2022/2025, and macOS 15/26 arm64/Intel. Contract Validation [run #45358956095](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45358956095) also passed.

## Runner environments observed

| Runner | OS / build | Architecture | Workspace temporary filesystem |
|---|---|---|---|
| `ubuntu-22.04` | Ubuntu 22.04.5 LTS; kernel `6.8.0-1070-azure` | `x86_64` | `findmnt` reports `ext4` on `/dev/sda1`; `stat -f` reports the generic `ext2/ext3` statfs family; block size 4096 |
| `ubuntu-24.04` | Ubuntu 24.04.5 LTS; kernel `7.0.0-1014-aws` | `x86_64` | `findmnt` reports `ext4` on `/dev/mapper/cow-root`; `stat -f` reports the generic `ext2/ext3` statfs family; block size 4096 |
| `ubuntu-26.04` | Ubuntu 26.04.1 LTS; kernel `7.0.0-1020-azure` | `x86_64` | `findmnt` reports `ext4` on `/dev/nvme0n1p1`; `stat -f` reports the generic `ext2/ext3` statfs family; block size 4096 |
| `windows-2022` | Microsoft Windows Server 2022 Datacenter, version `10.0.20348`, build `20348`; PowerShell `7.6.10` | `X64` | Drive `D:`, NTFS, label `Temporary Storage`, allocation unit 4096 bytes |
| `windows-2025` | Microsoft Windows Server 2025 Datacenter, version `10.0.26100`, build `26100`; PowerShell `7.6.10` | `X64` | Drive `D:`, NTFS, label `Temporary Storage`, allocation unit 4096 bytes |
| `macos-15` | macOS `15.8.2`, build `24H232` | `arm64` | APFS native probes and nested APFS fixture passed; the workspace volume is hosted runner-managed |
| `macos-15-intel` | macOS `15.7.9`, build `24G830` | `x86_64` | APFS native probes and nested APFS fixture passed; the workspace volume is hosted runner-managed |
| `macos-26` | macOS `26.7.2`, build `25G332` | `arm64` | APFS native probes and nested APFS fixture passed; the workspace volume is hosted runner-managed |
| `macos-26-intel` | macOS `26.6.1`, build `25G76` | `x86_64` | APFS native probes and nested APFS fixture passed; the workspace volume is hosted runner-managed |

The Linux `findmnt` filesystem type (`ext4`) is the specific mount type. The `stat -f` value (`ext2/ext3`) is the generic statfs family string and must not be read as evidence that the mount is ext2 or ext3.

## Native filesystem fixtures exercised

| Platform | Fixture | Result in run #45353310385 |
|---|---|---|
| Ubuntu 22.04/24.04/26.04 | Bind-mounted directory placed beneath a workspace, with a sentinel outside the workspace | Setup, boundary verification, and cleanup passed on all three explicit runners in run #45354964372 |
| Ubuntu 22.04/24.04/26.04 | 128 MiB ext4 filesystem image loop-mounted beneath a workspace, with a sentinel on the mounted filesystem | Setup, distinct-filesystem boundary verification, and cleanup passed on all three explicit runners in run #45354964372 |
| Ubuntu 22.04/24.04/26.04 | 16 MiB tmpfs mounted beneath the ext4 workspace, with a sentinel on the tmpfs mount | Setup, `RESOLVE_NO_XDEV` rejection, and cleanup passed on all three runners in Rust Core Tests run #45356622561 |
| Ubuntu 22.04/24.04/26.04 | 16 MiB tmpfs mounted as the workspace root, with a direct-child sentinel | Root admission, exact-byte snapshot capture, payload validation, and unmount cleanup passed on all three runners in Rust Core Tests run #45358956133 |
| Windows Server 2022 and 2025 | Secondary NTFS VHDX volume mounted at a directory beneath a workspace | Setup, volume-boundary verification, and cleanup passed on both explicit runner labels in run #45354176701 |
| macOS 15 and 26, arm64 and Intel | Native bulk firmlink metadata and `O_RESOLVE_BENEATH` probes | Passed on all four listed runner builds |
| macOS 15 and 26, arm64 and Intel | Separate APFS volume mounted beneath a workspace | Setup, nested-volume boundary verification, and detach passed on all four runners |
| macOS 15 and 26, arm64 and Intel | Restrictive sandbox capability-loss probe and opened-handle System/Data identity regression | Passed; the latter is separately documented in [the macOS support matrix](MACOS_SNAPSHOT_SUPPORT_MATRIX.md) |

## Interpretation and limits

- These runs prove the workflow diagnostics and listed fixtures execute successfully on these exact hosted runner images. Linux now uses explicit `ubuntu-22.04`, `ubuntu-24.04`, and `ubuntu-26.04` labels; Windows uses explicit `windows-2022` and `windows-2025` labels. Do not infer an immutable OS support promise beyond the recorded builds.
- Linux's three hosted images reported kernels 6.8.0-1070-azure, 7.0.0-1014-aws, and 7.0.0-1020-azure respectively; Ubuntu 22.04's runner did not use the distribution's 5.15 GA kernel. The loop-mounted ext4 and tmpfs fixtures both verify that `RESOLVE_NO_XDEV` rejects mount crossings beneath the workspace. This does not establish root-workspace support for every filesystem or cover all kernel versions, mount options, or namespace configurations. The tmpfs root positive test demonstrates capture only on the three listed hosted runner builds.
- Windows validation covers Windows Server 2022 build 20348 and Windows Server 2025 build 26100, both x64 with NTFS VHDX mountpoint fixtures. ReFS, FAT variants, network filesystems, other Windows builds, and other architectures remain untested here.
- macOS evidence is limited to the exact OS builds and architectures listed above. The System/Data firmlink conclusions remain scoped as stated in [the macOS support matrix](MACOS_SNAPSHOT_SUPPORT_MATRIX.md); this document does not broaden those security claims.
- Passing CI is not independent security review and does not authorize production enablement or merging the security stack.
