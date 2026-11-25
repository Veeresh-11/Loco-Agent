# Authority Attestation Verification — Readiness Assessment

**Status:** Research and release-gate assessment; no runtime verifier implementation  
**Date:** 2026-11-25  
**Related:** PRs #140–#145; runtime architecture decision in PR #142

## Executive finding

The current repository has a useful foundation for provenance generation and reproducible target packaging, but it is **not ready for runtime verification or sidecar launch**.

Verified from the current workflow and PR stack:

- Four native target artifacts are built: Linux x86_64, Windows x86_64, macOS arm64, and macOS Intel.
- The packaging job stages all four executables into the VSIX and checks that each expected resource path is present.
- Provenance attestation steps are scoped to `push` events on `refs/heads/main`.
- External workflow actions have been pinned to full commit SHAs and a dependency-free CI gate validates their syntax.
- The latest PR #145 exact-head run (45571797788) passed all six jobs: immutable-action validation, four native builds, and VSIX packaging.

Not demonstrated:

- A successful trusted-`main` attestation for each executable and the resulting VSIX.
- Independent verification of those subjects and their structured claims.
- A stable protected-branch/workflow trust policy and trusted builder.
- Authenticode signing for Windows or Developer ID signing/notarization for macOS.
- A cross-platform verifier that can be packaged and maintained.
- Exact file-object binding between verification and process creation.

The current build job uses a direct workflow rather than an isolated reusable trusted-builder workflow. GitHub's own documentation describes artifact attestations as provenance claims, recommends verifying actor identity and predicate type, and says the content of the predicate is workflow-controlled; a compromised workflow can falsify predicate fields. GitHub suggests a trusted reusable workflow to reduce that risk. Therefore the runtime policy should prioritize certificate-derived identity claims and should not rely on arbitrary self-reported predicate fields without a reviewed builder trust boundary.

## Workflow audit

The current workflow is `.github/workflows/authority-native-artifact-builds.yml`.

### Trigger and permissions

- Pull requests and pushes to `main` are selected by path filters.
- Workflow permissions include `contents: read`, `id-token: write`, and `attestations: write`.
- Attestation steps execute only when `github.event_name == 'push'` and `github.ref == 'refs/heads/main'`.

These conditions are useful but are not a substitute for repository protection. The branch rules, required reviews, workflow-change ownership, and bypass permissions must be inspected and recorded independently.

### Artifact subjects

The workflow attests each native executable at:

- `dist/authority/x86_64-unknown-linux-gnu/loco-authority`
- `dist/authority/x86_64-pc-windows-msvc/loco-authority.exe`
- `dist/authority/aarch64-apple-darwin/loco-authority`
- `dist/authority/x86_64-apple-darwin/loco-authority`

It also attests the packaged VSIX using `*.vsix`.

The package step validates path presence inside the VSIX, but path presence alone does not prove that the packaged bytes match the attested executable digest. The release verifier must extract the expected exact member, reject duplicates/traversal entries, hash those bytes, and verify the executable's own attestation subject. The VSIX attestation is a separate package-level claim.

### Immutable actions

PR #143 pins the workflow's external actions to immutable SHA references. PR #144 introduced a format gate, and PR #145 added a pinned checkout step so the validator can read the workflow source. The exact-head PR #145 run completed all six jobs successfully.

This proves those references passed the project's SHA-format check and the workflow jobs succeeded on that PR head. It does not establish that the referenced action code was reviewed, that runner images are immutable, or that the release trust policy is complete.

## Attestation verifier research

GitHub CLI exposes `gh attestation verify` for verifying an artifact against attestations and checking the producer identity and predicate type. Its documentation recommends constraining the repository and, ideally, the signer workflow path or certificate identity. It supports JSON output so a consumer can apply additional policy to structured results. It also warns that only selected signature-certificate and verified-timestamp properties are non-workflow-controlled; statement predicate fields may be falsified by a compromised workflow. A trusted builder/reusable workflow is one mitigation.

The CLI is a useful reference implementation and a candidate for controlled release verification, but the documentation does **not** establish that embedding or shelling out to the GitHub CLI is appropriate for a VS Code extension's cross-platform runtime. It is not selected as a runtime dependency here.

Offline verification is supported using an attestation bundle and Sigstore trusted-root material. The current GitHub documentation notes that trusted-root material should be refreshed as new signed material is imported; it has no built-in expiry and may not reveal revocations after it was captured. A static root embedded in the same updateable VSIX therefore needs an independently authenticated update/revocation process. This is a material lifecycle requirement, not a packaging detail.

Initial references:

- GitHub CLI verification guide: https://cli.github.com/manual/gh_attestation_verify
- GitHub artifact attestation concepts: https://docs.github.com/en/actions/concepts/security/artifact-attestations
- GitHub offline verification: https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/verify-attestations-offline
- GitHub trusted-builder / SLSA Build Level 3 guidance: https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/increase-security-rating

## Platform-signing readiness

### Windows

Windows exposes `WinVerifyTrust` with the generic Authenticode verification action for checking a signed file/object. This can be used as a platform-integrity layer, but LOCO still needs a signed release executable, a narrowly pinned publisher identity policy, timestamp/revocation behavior, and file replacement protections.

Current status: the native build pipeline compiles a Windows executable but contains no Authenticode signing step or certificate identity policy. Windows runtime launch must remain disabled.

Reference: https://learn.microsoft.com/en-us/windows/win32/api/wintrust/nf-wintrust-winverifytrust

### macOS

Apple Security provides `SecStaticCodeCheckValidity` for validating a static code signature and an optional code requirement. Apple's documentation explicitly warns that the result is only secure while code is not concurrently modified; mutable or dynamic filesystems require additional care. This makes exact-file binding and protected installation location mandatory parts of the design, not optional hardening.

Current status: the native build pipeline compiles arm64 and Intel macOS executables, but contains no Developer ID signing, notarization, or designated-requirement policy. macOS runtime launch must remain disabled.

References:

- https://developer.apple.com/documentation/security/secstaticcodecheckvalidity%28_%3A_%3A_%3A%29
- https://developer.apple.com/documentation/technotes/tn3127-inside-code-signing-requirements

### Linux

There is no universal OS-enforced code-signing trust mechanism for arbitrary desktop ELF executables analogous to Authenticode or macOS code signing. The selected design therefore requires a protected application-managed installation strategy and a provenance check bound to the exact bytes that will be launched. The same-user attacker model must be explicit; ordinary file modes do not protect against a process with the same user's authority.

Current status: the build pipeline produces Linux x86_64 but no signed artifact or protected install mechanism. Linux runtime launch must remain disabled until the threat model and installation guarantees are reviewed.

## Required next gates (ordered)

1. **Trusted release evidence:** produce one trusted-`main` run and verify all five attestation subjects (four executables plus VSIX) using a structured verifier. Record run ID, commit, workflow identity, certificate-derived repository identity/ID, signer workflow, subject names and digests. Do not accept PR-run artifacts.
2. **Repository governance:** inspect and record branch protection/rulesets for `main`, required review, workflow ownership, bypass roles, and tag/release controls. If protection is unavailable, record the residual risk rather than implying protection.
3. **Trusted builder:** evaluate extracting the build-and-attest job into a reusable workflow with minimal caller-controlled inputs. Review the threat model and pin the reusable workflow by immutable revision.
4. **Platform signing:** decide and provision Windows and macOS signing identities, signing/notarization steps, publisher/team identity pins, rotation and revocation. Validate on clean supported runners and verify the resulting signed artifacts.
5. **Verifier dependency:** evaluate a maintained verifier library or standalone binary for each target. Document licensing, update cadence, trust-root management, offline/online behavior, and whether it can enforce signer-workflow identity without parsing unstable human output.
6. **Exact file binding:** prototype verify-only operations on a staged artifact; test symlink/reparse, parent replacement, hard link, race windows, archive duplicate entries and target mismatch. Do not spawn.
7. **Independent review:** review the trust model, workflow, branch controls, verifier, signed artifacts, exact-file binding and same-user assumptions before any runtime activation.

## Explicit safety boundary

This assessment makes no runtime changes. Do not instantiate `RustSidecarClient`, resolve or spawn the sidecar during extension activation, or enable workspace writes, patch application, command execution, approval grants, or execution tokens. A future implementation milestone must first be a verification-only component that inspects a staged executable and produces a typed result without spawning it.
