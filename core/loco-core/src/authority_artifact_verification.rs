//! Verification-only policy types for authority release artifacts.
//!
//! This module deliberately does not read files, invoke a verifier, access the
//! network, or spawn a process. It validates normalized evidence supplied by
//! a future platform verifier. It is not a runtime launch authorization API.

use std::collections::HashSet;

pub const EXPECTED_REPOSITORY: &str = "Veeresh-11/Loco-Agent";
pub const EXPECTED_SOURCE_REF: &str = "refs/heads/main";
pub const EXPECTED_WORKFLOW: &str = ".github/workflows/authority-native-artifact-builds.yml";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorityTarget {
    LinuxX64,
    WindowsX64,
    MacosArm64,
    MacosX64,
}

impl AuthorityTarget {
    pub fn package_member(self) -> &'static str {
        match self {
            Self::LinuxX64 => "extension/resources/authority/x86_64-unknown-linux-gnu/loco-authority",
            Self::WindowsX64 => "extension/resources/authority/x86_64-pc-windows-msvc/loco-authority.exe",
            Self::MacosArm64 => "extension/resources/authority/aarch64-apple-darwin/loco-authority",
            Self::MacosX64 => "extension/resources/authority/x86_64-apple-darwin/loco-authority",
        }
    }

    pub fn attestation_subject_path(self) -> &'static str {
        match self {
            Self::LinuxX64 => "dist/authority/x86_64-unknown-linux-gnu/loco-authority",
            Self::WindowsX64 => "dist/authority/x86_64-pc-windows-msvc/loco-authority.exe",
            Self::MacosArm64 => "dist/authority/aarch64-apple-darwin/loco-authority",
            Self::MacosX64 => "dist/authority/x86_64-apple-darwin/loco-authority",
        }
    }
}

/// The caller must construct this only from successfully verified structured
/// verifier output. Strings are data, never parsed from CLI prose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedProvenance {
    pub repository: String,
    pub source_ref: String,
    pub workflow_path: String,
    pub workflow_revision: String,
    pub event_name: String,
    pub subject_name: String,
    pub subject_sha256: String,
    pub target: AuthorityTarget,
    pub cryptographic_verification_succeeded: bool,
    pub trusted_identity_verified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationFailure {
    CryptographicVerificationFailed,
    UntrustedIdentity,
    WrongRepository,
    WrongSourceRef,
    WrongWorkflow,
    InvalidWorkflowRevision,
    UntrustedEvent,
    WrongSubject,
    InvalidDigest,
    WrongTarget,
}

/// Pure policy only. It does not prove that the evidence is authentic; that
/// remains the responsibility of a separately reviewed verifier adapter.
pub fn validate_provenance(
    evidence: &VerifiedProvenance,
    expected_target: AuthorityTarget,
    expected_digest: &str,
) -> Result<(), VerificationFailure> {
    if !evidence.cryptographic_verification_succeeded {
        return Err(VerificationFailure::CryptographicVerificationFailed);
    }
    if !evidence.trusted_identity_verified {
        return Err(VerificationFailure::UntrustedIdentity);
    }
    if evidence.repository != EXPECTED_REPOSITORY {
        return Err(VerificationFailure::WrongRepository);
    }
    if evidence.source_ref != EXPECTED_SOURCE_REF {
        return Err(VerificationFailure::WrongSourceRef);
    }
    if evidence.workflow_path != EXPECTED_WORKFLOW {
        return Err(VerificationFailure::WrongWorkflow);
    }
    if !is_full_git_sha(&evidence.workflow_revision) {
        return Err(VerificationFailure::InvalidWorkflowRevision);
    }
    if evidence.event_name != "push" {
        return Err(VerificationFailure::UntrustedEvent);
    }
    if evidence.subject_name != expected_target.attestation_subject_path() {
        return Err(VerificationFailure::WrongSubject);
    }
    if !is_sha256_hex(&evidence.subject_sha256) || !is_sha256_hex(expected_digest) {
        return Err(VerificationFailure::InvalidDigest);
    }
    // Compare normalized lowercase hexadecimal digests without early exit.
    if !constant_time_eq(
        evidence.subject_sha256.to_ascii_lowercase().as_bytes(),
        expected_digest.to_ascii_lowercase().as_bytes(),
    ) {
        return Err(VerificationFailure::InvalidDigest);
    }
    if evidence.target != expected_target {
        return Err(VerificationFailure::WrongTarget);
    }
    Ok(())
}

pub fn validate_package_member_set(
    members: &[String],
    expected_target: AuthorityTarget,
) -> Result<(), VerificationFailure> {
    let expected = expected_target.package_member();
    let mut exact_count = 0usize;
    let mut seen = HashSet::new();

    for member in members {
        // Reject path traversal, absolute paths, and backslash variants before
        // considering the target's expected archive member.
        if member.starts_with('/')
            || member.starts_with('\\')
            || member.contains('\\')
            // Colons reject Windows drive-qualified/drive-relative paths and ADS.
            || member.contains(':')
            || member.split('/').any(|part| part == "." || part == "..")
            || member.contains('\0')
            || member.bytes().any(|byte| byte.is_ascii_control())
        {
            return Err(VerificationFailure::WrongSubject);
        }
        if !seen.insert(member.as_str()) {
            return Err(VerificationFailure::WrongSubject);
        }
        if member == expected {
            exact_count += 1;
        }
    }

    if exact_count != 1 {
        return Err(VerificationFailure::WrongSubject);
    }
    Ok(())
}

pub fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn is_full_git_sha(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0u8;
    for (a, b) in left.iter().zip(right.iter()) {
        difference |= a ^ b;
    }
    difference == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_evidence(target: AuthorityTarget, digest: &str) -> VerifiedProvenance {
        VerifiedProvenance {
            repository: EXPECTED_REPOSITORY.into(),
            source_ref: EXPECTED_SOURCE_REF.into(),
            workflow_path: EXPECTED_WORKFLOW.into(),
            workflow_revision: "0123456789abcdef0123456789abcdef01234567".into(),
            event_name: "push".into(),
            subject_name: target.attestation_subject_path().into(),
            subject_sha256: digest.into(),
            target,
            cryptographic_verification_succeeded: true,
            trusted_identity_verified: true,
        }
    }

    #[test]
    fn accepts_exact_trusted_main_subject_and_digest() {
        let digest = "a".repeat(64);
        let evidence = valid_evidence(AuthorityTarget::LinuxX64, &digest);
        assert_eq!(validate_provenance(&evidence, AuthorityTarget::LinuxX64, &digest), Ok(()));
    }

    #[test]
    fn rejects_each_identity_or_event_mismatch() {
        let digest = "b".repeat(64);
        let mut e = valid_evidence(AuthorityTarget::LinuxX64, &digest);
        e.repository = "attacker/Loco-Agent".into();
        assert_eq!(validate_provenance(&e, AuthorityTarget::LinuxX64, &digest), Err(VerificationFailure::WrongRepository));

        let mut e = valid_evidence(AuthorityTarget::LinuxX64, &digest);
        e.source_ref = "refs/pull/5/merge".into();
        assert_eq!(validate_provenance(&e, AuthorityTarget::LinuxX64, &digest), Err(VerificationFailure::WrongSourceRef));

        let mut e = valid_evidence(AuthorityTarget::LinuxX64, &digest);
        e.workflow_path = ".github/workflows/untrusted.yml".into();
        assert_eq!(validate_provenance(&e, AuthorityTarget::LinuxX64, &digest), Err(VerificationFailure::WrongWorkflow));

        let mut e = valid_evidence(AuthorityTarget::LinuxX64, &digest);
        e.workflow_revision = "main".into();
        assert_eq!(validate_provenance(&e, AuthorityTarget::LinuxX64, &digest), Err(VerificationFailure::InvalidWorkflowRevision));

        let mut e = valid_evidence(AuthorityTarget::LinuxX64, &digest);
        e.event_name = "pull_request".into();
        assert_eq!(validate_provenance(&e, AuthorityTarget::LinuxX64, &digest), Err(VerificationFailure::UntrustedEvent));
    }

    #[test]
    fn rejects_unverified_or_untrusted_verifier_results() {
        let digest = "c".repeat(64);
        let mut e = valid_evidence(AuthorityTarget::LinuxX64, &digest);
        e.cryptographic_verification_succeeded = false;
        assert_eq!(validate_provenance(&e, AuthorityTarget::LinuxX64, &digest), Err(VerificationFailure::CryptographicVerificationFailed));

        let mut e = valid_evidence(AuthorityTarget::LinuxX64, &digest);
        e.trusted_identity_verified = false;
        assert_eq!(validate_provenance(&e, AuthorityTarget::LinuxX64, &digest), Err(VerificationFailure::UntrustedIdentity));
    }

    #[test]
    fn rejects_wrong_subject_digest_and_target() {
        let digest = "d".repeat(64);
        let mut e = valid_evidence(AuthorityTarget::LinuxX64, &digest);
        e.subject_name = AuthorityTarget::WindowsX64.attestation_subject_path().into();
        assert_eq!(validate_provenance(&e, AuthorityTarget::LinuxX64, &digest), Err(VerificationFailure::WrongSubject));

        let mut e = valid_evidence(AuthorityTarget::LinuxX64, &digest);
        e.subject_sha256 = "e".repeat(64);
        assert_eq!(validate_provenance(&e, AuthorityTarget::LinuxX64, &digest), Err(VerificationFailure::InvalidDigest));

        let e = valid_evidence(AuthorityTarget::WindowsX64, &digest);
        assert_eq!(validate_provenance(&e, AuthorityTarget::LinuxX64, &digest), Err(VerificationFailure::WrongSubject));
    }

    #[test]
    fn digest_and_git_sha_formats_are_strict() {
        assert!(is_sha256_hex(&"f".repeat(64)));
        assert!(!is_sha256_hex(&"f".repeat(63)));
        assert!(!is_sha256_hex(&"g".repeat(64)));
        assert!(is_full_git_sha(&"0".repeat(40)));
        assert!(!is_full_git_sha("abcdef"));
    }

    #[test]
    fn package_member_validation_rejects_missing_duplicate_and_traversal() {
        let target = AuthorityTarget::MacosArm64;
        let expected = target.package_member().to_string();
        assert_eq!(validate_package_member_set(&[expected.clone()], target), Ok(()));
        assert_eq!(validate_package_member_set(&[], target), Err(VerificationFailure::WrongSubject));
        assert_eq!(validate_package_member_set(&[expected.clone(), expected.clone()], target), Err(VerificationFailure::WrongSubject));
        assert_eq!(validate_package_member_set(&["extension/../evil".into(), expected.clone()], target), Err(VerificationFailure::WrongSubject));
        assert_eq!(validate_package_member_set(&["/absolute".into(), expected], target), Err(VerificationFailure::WrongSubject));
    }

    #[test]
    fn package_member_validation_rejects_windows_drive_paths_and_ads() {
        let target = AuthorityTarget::WindowsX64;
        let expected = target.package_member().to_string();
        for hostile in ["C:/Windows/system32/evil.exe", "C:relative\\evil.exe", "extension/resources/file.exe:stream"] {
            assert_eq!(
                validate_package_member_set(&[hostile.into(), expected.clone()], target),
                Err(VerificationFailure::WrongSubject),
                "must reject Windows path form: {hostile}"
            );
        }
    }

    #[test]
    fn package_member_validation_rejects_control_characters() {
        let target = AuthorityTarget::LinuxX64;
        let expected = target.package_member().to_string();
        let hostile = "extension/resources/authority/evil\u{7f}name";
        assert_eq!(
            validate_package_member_set(&[hostile.into(), expected], target),
            Err(VerificationFailure::WrongSubject)
        );
    }

    #[test]
    fn provenance_rejects_target_enum_mismatch_even_when_subject_path_matches() {
        let digest = "a".repeat(64);
        let mut evidence = valid_evidence(AuthorityTarget::LinuxX64, &digest);
        evidence.target = AuthorityTarget::WindowsX64;
        assert_eq!(
            validate_provenance(&evidence, AuthorityTarget::LinuxX64, &digest),
            Err(VerificationFailure::WrongTarget)
        );
    }

    #[test]
    fn package_member_validation_rejects_windows_separator_and_dot_components() {
        let target = AuthorityTarget::WindowsX64;
        let expected = target.package_member().to_string();
        assert_eq!(validate_package_member_set(&[expected.clone()], target), Ok(()));
        assert_eq!(validate_package_member_set(&["extension\\resources\\evil".into(), expected], target), Err(VerificationFailure::WrongSubject));
        assert_eq!(validate_package_member_set(&["extension/./evil".into()], target), Err(VerificationFailure::WrongSubject));
    }
}
