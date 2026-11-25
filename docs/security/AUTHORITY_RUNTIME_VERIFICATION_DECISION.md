# Authority Runtime Verification — Architecture Decision

**Status:** Proposed decision; implementation remains blocked pending independent review  
**Date:** 2026-11-25  
**Scope:** Verify the packaged Rust authority executable before any extension activation launches it

## Decision

Adopt a **hybrid, platform-native executable-integrity plus signed build-provenance model** as the target architecture:

1. Use platform code-signing and OS verification for executable integrity on macOS and Windows.
2. Use a separately maintained, pinned provenance verifier and an authenticated trust policy to verify the build identity and exact artifact digest on all supported platforms.
3. Require both checks. Platform signing alone does not prove the expected repository/workflow built the executable; provenance alone does not prevent a local executable from being replaced between verification and process creation.
4. Do not implement an ad-hoc signature format, trust a trust root shipped only inside the package it authenticates, or fall back to PATH/workspace/user configuration.

This is a **design selection for implementation evaluation**, not approval to launch. If a platform cannot provide equivalent executable-integrity verification and exact-file binding, that target remains unsupported and activation fails closed.

## Why this model

- **Online-only provenance verification** introduces network availability and privacy dependencies into activation, and still needs a robust same-file launch binding.
- **Offline-only verification bundles** require a separately authenticated trust-root update/revocation channel. A bundle and root shipped together in the same replaceable VSIX cannot authenticate one another.
- **Platform signing only** gives useful OS-level integrity and publisher checks on supported platforms, but does not by itself constrain the GitHub repository, protected source ref, workflow path, builder identity, or expected attestation subject.
- **Hybrid verification** separates two claims: the OS authenticates the installed executable's publisher/integrity, while provenance policy authenticates the expected build origin and subject digest. It also makes platform gaps explicit rather than silently weakening verification.

The hybrid model is selected as the design target because it composes independent controls and can fail closed. Its feasibility and exact implementation still require platform-specific validation and review.

## Attestation trust policy

The verifier must consume structured attestation claims, not human-readable CLI output. The approved policy must require:

- Repository: `Veeresh-11/Loco-Agent`.
- Source ref: `refs/heads/main`, protected by reviewed repository rules.
- Workflow: the exact reviewed path `.github/workflows/authority-native-artifact-builds.yml`.
- Workflow source revision and builder identity consistent with the trusted release workflow.
- Subject name/path and digest matching the selected target executable.
- Target tuple exactly matching the extension host OS and architecture.
- A successful release event, not a pull-request workflow run.
- A valid signature chain rooted in the verifier's explicitly pinned trust configuration.

Do not accept a matching digest alone. Do not accept an attestation for the VSIX as a substitute for the executable subject unless the package contents are independently extracted, verified, and bound to the exact executable digest selected for launch.

The repository, workflow, ref, and target values above are initial policy values; before implementation they must be checked against protected-branch configuration, workflow identity, release process, and actual attestation claims from a trusted `main` push.

## Platform integrity requirements

### Windows

- Verify Authenticode signature using supported Windows trust APIs and enforce an exact publisher identity policy (certificate chain, subject/public-key identity, timestamp and revocation policy).
- Reject unsigned, invalid, expired without valid timestamp, revoked, wrong-publisher, wrong-architecture, or untrusted binaries.
- Determine whether the current build pipeline produces signed Windows executables. If not, Windows runtime launch remains unsupported until signing and certificate lifecycle are established.
- Bind verification and process creation to the same file object as far as supported Win32 APIs permit; document residual replacement races and mitigate using an ACL-protected immutable install location or a verified private copy.

### macOS

- Verify code signature, designated requirement/team identity, hardened runtime/notarization policy as appropriate to distribution, and architecture.
- Reject ad-hoc/unsigned signatures and unexpected team identifiers.
- Determine whether the current artifacts are Developer ID signed and notarized. If not, macOS runtime launch remains unsupported until signing/notarization and update/revocation behavior are established.
- Bind verification to the exact executable bytes/object used for launch; document filesystem and quarantine behavior.

### Linux

- Linux has no universal OS-enforced code-signing trust service equivalent to Authenticode or macOS code signing for arbitrary desktop executables.
- Therefore provenance verification is mandatory, plus an application-controlled immutable install strategy: verify the packaged binary digest, copy it into a private extension-managed directory with restrictive ownership/mode, re-open without symlink following, hash the opened file, and launch only the verified installed object/path with no untrusted write access to its parent.
- The implementation must state its threat model. It cannot claim protection against same-user malware that can modify the extension process or its private files; such an attacker may also interfere with launch.
- If same-user tampering cannot be excluded or the exact-file launch binding cannot be demonstrated, Linux launch remains disabled.

## Exact-file binding and launch sequence

1. Determine the exact platform/architecture from the extension host. Reject unknown targets; do not infer from the UI/webview or workspace.
2. Resolve exactly one compiled-in package resource path. Do not read executable paths from settings, environment variables, PATH, workspace files, temporary directories, or user-controlled inputs.
3. Validate the expected file type and reject symlinks/reparse points. Apply platform-appropriate ACL/mode protections.
4. Establish a private installation location if required by the platform, with no untrusted write access to the executable or containing directory.
5. Verify platform signature/integrity and publisher policy where available.
6. Compute SHA-256 from the exact file object to be launched, then verify the matching attestation subject and all policy claims.
7. Prevent replacement between verification and launch. Prefer handle-based launch where supported; otherwise use an OS-protected immutable file location and revalidate identity/digest immediately before spawn. Document the residual TOCTOU assumptions and test replacement attempts.
8. Spawn the exact absolute executable path directly, never through a shell. Use a minimal environment, bounded stdio, explicit cwd, no inherited secrets, and an activation-scoped lifecycle.
9. Do not fall back to another path or launch mode after any verification, policy, copy, or spawn error.
10. On deactivation, terminate the process, close channels, and revoke process-local workspace registrations.

No TypeScript code may instantiate the production sidecar client before this sequence is implemented and accepted.

## Release and trust-root lifecycle

- Run provenance attestation only for trusted pushes/releases from the protected source ref; PR artifacts remain validation-only.
- Protect workflow changes and action pin updates with review. Immutable action pins are necessary but do not replace reviewing pinned action code, runner images, toolchains, dependencies, or workflow permissions.
- Maintain a separate, versioned trust-policy update process. Policy changes require review and must not be controlled by untrusted VSIX contents.
- Define certificate rotation, revocation, verifier updates, provenance schema compatibility, expiry, offline behavior, and emergency disablement before release.
- Do not cache a positive verification result across executable replacement or package update; cache keys must include the exact digest and policy version.
- Record only non-sensitive verification outcome, policy version, digest, and reason code. Do not log executable contents, secrets, or absolute workspace paths.

## Required adversarial tests

- Wrong repository, source ref, workflow path, workflow revision, builder, event type, subject name, subject digest, and target tuple.
- PR attestation presented as release evidence.
- Missing, malformed, expired, revoked, or untrusted attestation and trust policy.
- Unsigned, invalidly signed, wrong-publisher, revoked-certificate, and wrong-architecture platform binaries.
- Executable replaced before verification, during verification, after verification, and immediately before process creation.
- Symlink/reparse-point resource, directory/file type confusion, hard-link substitution where applicable, and parent-directory replacement.
- Missing verifier, offline network, timeout, corrupt cache, stale trust policy, and update interruption.
- VSIX containing stale or mismatched executable bytes, missing target artifact, duplicate target resources, or package traversal entries.
- Spawn failure, malformed protocol greeting, unexpected child process exit, activation/deactivation races, and repeated activation.
- No PATH, workspace, environment, user-setting, temporary-file, or shell fallback on every failure path.

## Current evidence and unresolved prerequisites

- PR #140 established native builds for Linux x86_64, Windows x86_64, macOS arm64, and macOS Intel, plus VSIX inclusion checks. It did not establish signed executables.
- PR #141 added GitHub artifact provenance steps for trusted pushes to `main`; the documented PR run skipped attestations. A real trusted-main attestation has not yet been independently verified in this review.
- PR #143 pins external actions to immutable SHAs; PR #144 adds a SHA-format CI gate; PR #145 fixes checkout before that validator. PR #145's exact-head run `45571797788` passed all six jobs: immutable-pin validation, four native builds, and VSIX packaging.
- PR #142 defines the runtime gate but has no independent review submission.
- The current build workflow does not show executable code-signing steps. Thus Authenticode and Developer ID/notarization requirements are not yet satisfied by the workflow as inspected.
- The supported same-user attacker model, protected-main configuration, signing identity, trust-policy distribution/update channel, verifier implementation, and handle-to-launch guarantees remain unresolved.

## Explicitly disabled

Until the exit criteria below are accepted, do not launch the Rust sidecar from extension activation. Keep workspace writes, patch application, command execution, approval grants, and execution tokens disabled.

## Exit criteria

1. Independent security review accepts the hybrid model or records a better-supported alternative.
2. Protected source-ref and workflow identity are verified against repository configuration and a real trusted release attestation.
3. Windows and macOS signing identities, signing/notarization, rotation and revocation are implemented and tested; Linux installation-integrity assumptions are documented and tested.
4. A maintained verifier is selected, pinned, tested and packaged for every enabled target.
5. The executable digest and attestation are bound to the exact file object/path that is launched, with replacement-race tests.
6. Package tests prove exact target selection and no fallback.
7. Lifecycle, IPC, environment, permissions and failure behavior pass cross-platform tests.
8. Independent review explicitly approves enabling launch on each supported target. A platform may remain disabled independently.

## Follow-up task after this decision

Implement a **verification-only prototype** that can inspect a staged executable and return a typed verification result without spawning it. Start with the verification policy and test fixtures; do not wire it into extension activation. Only after that prototype passes adversarial tests and independent review should launch integration be considered.
