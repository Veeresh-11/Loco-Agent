# Authority artifact runtime verification contract

**Status:** design gate; no runtime verification or sidecar launch is enabled by this document  
**Assessment date:** 2026-11-25

## Purpose

The native artifact workflow can build and package the Rust authority executable for the declared desktop targets. A packaged path and a CI-generated digest are not, by themselves, proof that an executable selected at runtime is the artifact produced by a trusted build.

This document defines the minimum decision points that must be resolved before extension activation may launch the authority executable.

## Trust model

The extension must treat the executable as untrusted until all required checks succeed. Verification must bind the exact bytes about to be launched to an attestation whose identity is authorized by a narrowly scoped policy.

The policy must constrain, at minimum:

- Repository identity: `Veeresh-11/Loco-Agent`.
- Trusted source ref: the protected release source ref (currently intended to be `refs/heads/main`).
- Expected workflow identity: the repository's authority native artifact build workflow, at its reviewed path.
- Expected workflow source revision and builder identity, using the verifier's structured attestation claims rather than parsing human-readable output.
- Subject identity: the exact platform executable or VSIX subject expected by the packaging/release process.
- Digest: a cryptographic digest computed from the exact executable bytes that will be launched.
- Platform/architecture: the executable target must match the current VS Code extension host platform and architecture.

A PR workflow run is validation only. It must not satisfy the release trust policy. A build provenance attestation from an unexpected repository, ref, workflow, subject, or digest must be rejected.

## Verification architecture decision required

Before implementation, choose and document one supported verification architecture:

1. **Online verification:** use a maintained verifier and an explicitly defined trust root to verify the artifact's attestation at install/activation time. Specify network failure behavior, caching, revocation/update behavior, and privacy implications.
2. **Offline verification:** ship a verifier and authenticated attestation bundle with the extension, with a separately authenticated trust root and a clear update/revocation mechanism. The trust root must not be replaceable by the same untrusted package being checked.
3. **Platform-native signing plus provenance:** require OS-native code-signing verification for executable integrity and use build provenance as an additional source/build identity check. Specify certificate identity, timestamp, revocation, notarization, and per-platform parity.

Do not silently substitute one model for another. GitHub artifact attestation verification is not automatically available as a lightweight, offline runtime API on every supported OS. Do not invent a custom signature format or trust the executable's own self-reported version.

## Required runtime sequence

1. Determine the exact supported target from the extension host OS and architecture; reject unknown or unsupported targets.
2. Resolve only the expected executable under the installed extension's immutable package resources. Do not search PATH, workspace folders, user settings, temporary directories, or arbitrary environment overrides.
3. Reject symlinks/reparse points and unexpected file types where the platform permits safe inspection; apply restrictive permissions where applicable.
4. Read the executable bytes or use a platform mechanism that securely binds verification to the same file object later launched. Avoid a verify-then-replace race between path verification and process launch.
5. Verify the digest and attestation claims against the approved trust policy. Any missing verifier, unavailable trust data, malformed attestation, mismatch, unsupported platform, or verification timeout must fail closed.
6. Launch the exact verified file without a shell, with a minimal environment, bounded stdio, and no inherited secrets.
7. Do not fall back to a different executable if verification or launch fails. Surface a generic actionable error without logging absolute workspace paths, executable contents, secrets, or raw stderr.
8. Tie the process to one extension activation lifecycle; dispose it on deactivation and revoke registered workspace handles on folder removal/shutdown.

## Packaging and release requirements

- Build and attest release artifacts only from the trusted source ref and reviewed workflow identity.
- Pin third-party GitHub Actions to reviewed immutable commit SHAs before treating the workflow as a release supply-chain control; version tags alone are mutable references.
- Retain artifact digests and attestation verification evidence for release review.
- Verify the VSIX subject and the selected executable subject. Verifying only the VSIX does not prove the runtime-selected executable matches the intended target unless the package contents are independently bound and checked.
- Ensure release packaging does not silently include stale binaries from the checkout or a previous build.
- Keep pull-request packaging useful for tests, but never accept its artifacts as trusted release inputs.
- Test missing, replaced, truncated, wrong-architecture, unsigned/unattested, wrong-ref, wrong-workflow, wrong-repository, and expired/revoked trust material cases.

## Explicit non-goals

This design does not authorize workspace writes, patch application, command execution, or approval grants. A valid artifact attestation establishes a build-provenance claim under a trust policy; it does not establish that the program is bug-free, that a workspace is safe, or that a user approved an operation.

The Rust protocol's capture acknowledgement still does not deliver task-consumable file content. The separate Rust-owned content-consumption design remains required before task-path migration.

## Exit criteria

Do not instantiate `RustSidecarClient` in production activation until:

- The verification architecture and trust policy are approved.
- The exact launched file is cryptographically bound to the verified digest.
- Per-platform verification behavior and failure modes are tested on supported OS/architectures.
- Package-level tests prove there is no PATH/workspace/user-setting fallback.
- Independent security review accepts the trust root, verifier, update/revocation, and TOCTOU design.
