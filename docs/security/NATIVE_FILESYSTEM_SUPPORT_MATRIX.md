# Native filesystem snapshot adapter — CI environment matrix

**Status:** evidence inventory for the exact hosted CI environments recorded below. This is not a production support declaration and does not imply compatibility with every OS release, architecture, filesystem, mount namespace, or APFS layout.

## Latest full matrix run

Rust Core Tests [run #45353310385](https://github.com/Veeresh-11/Loco-Agent/actions/runs/45353310385) passed all six jobs on PR #113 head `4a695087636ea16a055c2403d23588588c114a9f`:

- Ubuntu Linux
- Windows Server
- macOS 15 arm64
- macOS 15 Intel
- macOS 26 arm64
- macOS 26 Intel

The workflow records native OS, kernel/architecture, runner-image version, and the filesystem containing `RUNNER_TEMP` for Linux and Windows. The macOS jobs record `sw_vers` and `uname -m`, and run the native APFS/firmlink probes.

## Recorded environments

| OS / runner | OS build | Architecture | Hosted image | `RUNNER_TEMP` filesystem / native fixture | Rust core unit tests |
|---|---|---|---|---|---:|
| Ubuntu `ubuntu-latest` | Ubuntu 26.04.1 LTS; kernel `7.0.0-1020-azure` | x86_64 | `ubuntu-26.04`, image `20261116.202.1` | `findmnt` and `df` report ext4 on `/dev/nvme0n1p1`; `statfs` reports generic family `ext2/ext3`. Real bind-mount and distinct ext4 loop-mount boundary fixtures passed. | 120 passed, 0 failed, 2 ignored |
| Windows `windows-latest` | Windows Server 2025 Datacenter, version `10.0.26100`, build `26100` | X64 | `windows-2025-vs2026`, image `20261116.315.1` | Runner temp drive `D:` is NTFS (label `Temporary Storage`, allocation unit 4096 bytes). Real secondary NTFS VHDX mountpoint boundary test and teardown passed. | 121 passed, 0 failed, 1 ignored |
| macOS 15 `macos-15` | macOS 15.7.9, build `24G830` | x86_64 | `macos-15`, image `20260824.0482.1` | Native APFS root admission, nested APFS sparse-image mount fixture, bulk firmlink metadata, and opened-handle System/Data identity regression passed. | 130 passed, 0 failed, 1 ignored |
| macOS 15 `macos-15` arm64 | macOS 15.8.2, build `24H232` | arm64 | `macos-15-arm64`, image `20261118.0550.1` | Native APFS root admission, nested APFS sparse-image mount fixture, bulk firmlink metadata, and opened-handle System/Data identity regression passed. | 130 passed, 0 failed, 1 ignored |
| macOS 26 `macos-26` | macOS 26.6.1, build `25G76` | x86_64 | `macos-26`, image `20260824.0517.1` | Native APFS root admission, nested APFS sparse-image mount fixture, bulk firmlink metadata, and opened-handle System/Data identity regression passed. | 130 passed, 0 failed, 1 ignored |
| macOS 26 `macos-26` arm64 | macOS 26.7.2, build `25G332` | arm64 | `macos-26-arm64`, image `20261117.0516.1` | Native APFS root admission, nested APFS sparse-image mount fixture, bulk firmlink metadata, and opened-handle System/Data identity regression passed. | 130 passed, 0 failed, 1 ignored |

The two macOS 15 runner builds and two macOS 26 runner builds are distinct observations; do not generalize from one architecture's OS build to the other architecture.

## What this evidence establishes

- The Linux test environment exercised Ubuntu 26.04.1 x86_64, kernel 7.0.0-1020-azure, ext4 temporary storage, a real bind mount, and a separate ext4 loop-mounted filesystem. Both mount-boundary tests passed.
- The Windows test environment exercised Windows Server 2025 x64 with NTFS temporary storage and a real secondary NTFS VHDX mountpoint. The mount-boundary test passed.
- The macOS matrix exercised the four exact OS/build/architecture combinations listed above, including nested APFS mount rejection and the System/Data firmlink/opened-handle regression.
- The workflow now emits Linux and Windows environment details directly in CI logs rather than inferring them from runner labels.

## Limits and unsupported configurations

- Linux: other distributions, kernels, architectures, filesystems, mount namespaces, and container configurations are not validated by this run.
- Windows: x86/ARM64 and filesystems other than the tested NTFS configuration remain unsupported or unverified.
- macOS: earlier releases, future releases, non-APFS filesystems, external APFS volumes, snapshots, sealed-system variants, and unusual volume-group layouts remain unverified beyond the explicitly tested cases.
- Hosted labels `ubuntu-latest` and `windows-latest` move over time. The image version and OS/kernel/filesystem observations must be refreshed for each CI run; they are not a stable minimum-version contract.
- Passing CI does not replace independent security review. Keep the workspace-read capability fail closed and do not claim production support from this matrix alone.

## Related evidence

- [macOS firmlink-specific support matrix](MACOS_SNAPSHOT_SUPPORT_MATRIX.md)
- [macOS firmlink XNU audit](MACOS_FIRMLINK_XNU_AUDIT.md)
- [PR #113 — Linux and Windows environment capture](https://github.com/Veeresh-11/Loco-Agent/pull/113)
