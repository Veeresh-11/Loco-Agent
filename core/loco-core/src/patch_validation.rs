//! Conservative validation for proposed unified-diff patches (not an apply/write API).
//! Requires an expected SHA-256 for the current target to prevent silent stale-target proposals.
//! This validator does not apply patches or close filesystem TOCTOU races.

use sha2::{Digest, Sha256};

pub const MAX_PATCH_BYTES: usize = 1_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchValidationError {
    EmptyPatch,
    PatchTooLarge,
    MissingExpectedDigest,
    InvalidExpectedDigest,
    TargetDigestMismatch,
    InvalidUnifiedDiff,
    BinaryPatchUnsupported,
    InvalidTargetHeader,
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Validate patch envelope and target-state binding only.
/// No patch application, filesystem mutation, or authorization is performed.
pub fn validate_patch_proposal(
    patch: &str,
    target_bytes: &[u8],
    expected_target_sha256: Option<&str>,
    relative_target: &str,
) -> Result<(), PatchValidationError> {
    if patch.is_empty() { return Err(PatchValidationError::EmptyPatch); }
    if patch.len() > MAX_PATCH_BYTES { return Err(PatchValidationError::PatchTooLarge); }
    let expected = expected_target_sha256.ok_or(PatchValidationError::MissingExpectedDigest)?;
    if expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(PatchValidationError::InvalidExpectedDigest);
    }
    if !sha256_hex(target_bytes).eq_ignore_ascii_case(expected) {
        return Err(PatchValidationError::TargetDigestMismatch);
    }
    if patch.contains("GIT binary patch") || patch.contains("Binary files ") {
        return Err(PatchValidationError::BinaryPatchUnsupported);
    }
    if relative_target.is_empty() || relative_target.starts_with('/') || relative_target.contains('\\')
        || relative_target.split('/').any(|s| s.is_empty() || s == "." || s == "..")
        || relative_target.contains(':')
    {
        return Err(PatchValidationError::InvalidTargetHeader);
    }
    let lines: Vec<&str> = patch.lines().collect();
    if !lines.iter().any(|l| l.starts_with("--- "))
        || !lines.iter().any(|l| l.starts_with("+++ "))
        || !lines.iter().any(|l| l.starts_with("@@"))
        || !lines.iter().any(|l| l.starts_with("diff --git "))
    {
        return Err(PatchValidationError::InvalidUnifiedDiff);
    }
    // Require the git header and the two unified-diff file headers to bind to this target.
    let expected_git_header = format!("diff --git a/{relative_target} b/{relative_target}");
    if lines.first().copied() != Some(expected_git_header.as_str()) {
        return Err(PatchValidationError::InvalidTargetHeader);
    }
    let old_header = lines.iter().find(|l| l.starts_with("--- "))
        .ok_or(PatchValidationError::InvalidUnifiedDiff)?;
    let new_header = lines.iter().find(|l| l.starts_with("+++ "))
        .ok_or(PatchValidationError::InvalidUnifiedDiff)?;
    let old_name = old_header[4..].split_whitespace().next().unwrap_or("");
    let new_name = new_header[4..].split_whitespace().next().unwrap_or("");
    let old_matches = old_name == "/dev/null" || old_name == format!("a/{relative_target}");
    let new_matches = new_name == "/dev/null" || new_name == format!("b/{relative_target}");
    if !old_matches || !new_matches || (old_name == "/dev/null" && new_name == "/dev/null") {
        return Err(PatchValidationError::InvalidTargetHeader);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATCH: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n";

    fn digest() -> String { sha256_hex(b"old\n") }

    #[test]
    fn accepts_valid_bound_patch_envelope() {
        assert_eq!(validate_patch_proposal(PATCH, b"old\n", Some(&digest()), "src/lib.rs"), Ok(()));
    }
    #[test]
    fn rejects_missing_digest() {
        assert_eq!(validate_patch_proposal(PATCH, b"old\n", None, "src/lib.rs"), Err(PatchValidationError::MissingExpectedDigest));
    }
    #[test]
    fn rejects_stale_target() {
        assert_eq!(validate_patch_proposal(PATCH, b"changed", Some(&digest()), "src/lib.rs"), Err(PatchValidationError::TargetDigestMismatch));
    }
    #[test]
    fn rejects_invalid_digest_format() {
        assert_eq!(validate_patch_proposal(PATCH, b"old\n", Some("bad"), "src/lib.rs"), Err(PatchValidationError::InvalidExpectedDigest));
    }
    #[test]
    fn rejects_binary_patch() {
        let patch = PATCH.to_owned() + "GIT binary patch\n";
        assert_eq!(validate_patch_proposal(&patch, b"old\n", Some(&digest()), "src/lib.rs"), Err(PatchValidationError::BinaryPatchUnsupported));
    }
    #[test]
    fn rejects_patch_with_wrong_target_header() {
        let patch = PATCH.replace("src/lib.rs", "other.rs");
        assert_eq!(validate_patch_proposal(&patch, b"old\n", Some(&digest()), "src/lib.rs"), Err(PatchValidationError::InvalidTargetHeader));
    }
    #[test]
    fn rejects_path_traversal_in_target() {
        assert_eq!(validate_patch_proposal(PATCH, b"old\n", Some(&digest()), "../outside"), Err(PatchValidationError::InvalidTargetHeader));
    }
    #[test]
    fn rejects_empty_patch() {
        assert_eq!(validate_patch_proposal("", b"old\n", Some(&digest()), "src/lib.rs"), Err(PatchValidationError::EmptyPatch));
    }
    #[test]
    fn rejects_oversized_patch() {
        let patch = "x".repeat(MAX_PATCH_BYTES + 1);
        assert_eq!(validate_patch_proposal(&patch, b"old\n", Some(&digest()), "src/lib.rs"), Err(PatchValidationError::PatchTooLarge));
    }
}
