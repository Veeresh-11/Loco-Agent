# Native filesystem CI matrix

**Status:** this is a record of the hosted CI environments and native filesystem fixtures exercised by the Rust Core Tests workflow. It is not a declaration that every filesystem or operating-system release is supported in production.

## Exact CI run

Rust Core Tests [run #45353310385](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45353310385) completed successfully on all six jobs for the PR #113 environment-capture change. The subsequent explicit OS matrix [run #45354176701](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45354176701) completed successfully on all seven jobs (Ubuntu, Windows Server 2022, Windows Server 2025, macOS 15 arm64/Intel, and macOS 26 arm64/Intel). The latter tested PR #114 head `4848f5affe1c2a0e6871b9f6f69ce47fae379ab5`. All native diagnostic steps, platform-specific fixtures, core unit tests, and fixture cleanup steps completed successfully.

## Runner environments observed

| Runner | OS / build | Architecture | Workspace temporary filesystem |
|---|---|---|---|
| `ubuntu-latest` | Ubuntu 26.04.1 LTS; kernel `7.0.0-1020-azure` | `x86_64` | `findmnt` reports `ext4` on `/dev/nvme0n1p1`; `stat -f` reports the generic `ext2/ext3` statfs family; block size 4096 |
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
| Linux | Bind-mounted directory placed beneath a workspace, with a sentinel outside the workspace | Setup, boundary verification, and cleanup passed |
| Linux | 128 MiB ext4 filesystem image loop-mounted beneath a workspace, with a sentinel on the mounted filesystem | Setup, distinct-filesystem boundary verification, and cleanup passed |
| Windows Server 2022 and 2025 | Secondary NTFS VHDX volume mounted at a directory beneath a workspace | Setup, volume-boundary verification, and cleanup passed on both explicit runner labels in run #45354176701 |
| macOS 15 and 26, arm64 and Intel | Native bulk firmlink metadata and `O_RESOLVE_BENEATH` probes | Passed on all four listed runner builds |
| macOS 15 and 26, arm64 and Intel | Separate APFS volume mounted beneath a workspace | Setup, nested-volume boundary verification, and detach passed on all four runners |
| macOS 15 and 26, arm64 and Intel | Restrictive sandbox capability-loss probe and opened-handle System/Data identity regression | Passed; the latter is separately documented in [the macOS support matrix](MACOS_SNAPSHOT_SUPPORT_MATRIX.md) |

## Interpretation and limits

- These runs prove the workflow diagnostics and listed fixtures execute successfully on these exact hosted runner images. Linux remains on `ubuntu-latest`; Windows now uses explicit `windows-2022` and `windows-2025` labels. Do not infer an immutable OS support promise beyond the recorded builds.
- Linux's loop-mounted ext4 fixture is a separate filesystem boundary test. It does not constitute a broad matrix of Linux filesystem types, kernel versions, mount options, or namespace configurations.
- Windows validation covers Windows Server 2022 build 20348 and Windows Server 2025 build 26100, both x64 with NTFS VHDX mountpoint fixtures. ReFS, FAT variants, network filesystems, other Windows builds, and other architectures remain untested here.
- macOS evidence is limited to the exact OS builds and architectures listed above. The System/Data firmlink conclusions remain scoped as stated in [the macOS support matrix](MACOS_SNAPSHOT_SUPPORT_MATRIX.md); this document does not broaden those security claims.
- Passing CI is not independent security review and does not authorize production enablement or merging the security stack.
