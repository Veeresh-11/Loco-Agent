//! Strict, bounded unified-diff envelope validation only. Not an apply/write API.
//! No authorization, approval, or race-resistant filesystem behavior is provided.

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
    MultiFilePatchUnsupported,
    InvalidHunkCounts,
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn range_count(range: &str) -> Option<usize> {
    let raw = range.strip_prefix(['-', '+'])?;
    if let Some((_, count)) = raw.split_once(',') {
        count.parse().ok()
    } else {
        Some(1)
    }
}

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
    if relative_target.is_empty() || relative_target.starts_with('/')
        || relative_target.contains('\\') || relative_target.contains(':')
        || relative_target.split('/').any(|s| s.is_empty() || s == "." || s == "..") {
        return Err(PatchValidationError::InvalidTargetHeader);
    }

    let lines: Vec<&str> = patch.lines().collect();
    if lines.iter().filter(|l| l.starts_with("diff --git ")).count() != 1
        || lines.iter().filter(|l| l.starts_with("--- ")).count() != 1
        || lines.iter().filter(|l| l.starts_with("+++ ")).count() != 1
        || lines.iter().filter(|l| l.starts_with("@@")).count() != 1 {
        return Err(PatchValidationError::MultiFilePatchUnsupported);
    }
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
    if old_name != format!("a/{relative_target}") || new_name != format!("b/{relative_target}") {
        return Err(PatchValidationError::InvalidTargetHeader);
    }

    let hunk_idx = lines.iter().position(|l| l.starts_with("@@"))
        .ok_or(PatchValidationError::InvalidUnifiedDiff)?;
    let ranges = lines[hunk_idx].split("@@").nth(1)
        .ok_or(PatchValidationError::InvalidUnifiedDiff)?;
    let mut parts = ranges.trim().split_whitespace();
    let old_range = parts.next().ok_or(PatchValidationError::InvalidUnifiedDiff)?;
    let new_range = parts.next().ok_or(PatchValidationError::InvalidUnifiedDiff)?;
    let expected_old = range_count(old_range).ok_or(PatchValidationError::InvalidUnifiedDiff)?;
    let expected_new = range_count(new_range).ok_or(PatchValidationError::InvalidUnifiedDiff)?;

    let (mut actual_old, mut actual_new) = (0usize, 0usize);
    for line in lines.iter().skip(hunk_idx + 1) {
        if line.starts_with("@@") || line.starts_with("diff --git ") {
            return Err(PatchValidationError::MultiFilePatchUnsupported);
        }
        if line.starts_with(' ') { actual_old += 1; actual_new += 1; }
        else if line.starts_with('-') { actual_old += 1; }
        else if line.starts_with('+') { actual_new += 1; }
        else if line.starts_with("\\") { continue; }
        else { return Err(PatchValidationError::InvalidHunkCounts); }
    }
    if actual_old != expected_old || actual_new != expected_new {
        return Err(PatchValidationError::InvalidHunkCounts);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATCH: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n";
    fn digest() -> String { sha256_hex(b"old\n") }

    #[test]
    fn accepts_valid_patch() {
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
    fn rejects_invalid_digest() {
        assert_eq!(validate_patch_proposal(PATCH, b"old\n", Some("bad"), "src/lib.rs"), Err(PatchValidationError::InvalidExpectedDigest));
    }
    #[test]
    fn rejects_binary_patch() {
        let p = PATCH.to_owned() + "GIT binary patch\n";
        assert_eq!(validate_patch_proposal(&p, b"old\n", Some(&digest()), "src/lib.rs"), Err(PatchValidationError::BinaryPatchUnsupported));
    }
    #[test]
    fn rejects_wrong_target() {
        let p = PATCH.replace("src/lib.rs", "wrong.rs");
        assert_eq!(validate_patch_proposal(&p, b"old\n", Some(&digest()), "src/lib.rs"), Err(PatchValidationError::InvalidTargetHeader));
    }
    #[test]
    fn rejects_traversal_target() {
        assert_eq!(validate_patch_proposal(PATCH, b"old\n", Some(&digest()), "../bad"), Err(PatchValidationError::InvalidTargetHeader));
    }
    #[test]
    fn rejects_empty_patch() {
        assert_eq!(validate_patch_proposal("", b"old\n", Some(&digest()), "src/lib.rs"), Err(PatchValidationError::EmptyPatch));
    }
    #[test]
    fn rejects_oversized_patch() {
        let p = "x".repeat(MAX_PATCH_BYTES + 1);
        assert_eq!(validate_patch_proposal(&p, b"old\n", Some(&digest()), "src/lib.rs"), Err(PatchValidationError::PatchTooLarge));
    }
    #[test]
    fn rejects_hunk_count_mismatch() {
        let p = PATCH.replace("@@ -1 +1 @@", "@@ -1,2 +1 @@");
        assert_eq!(validate_patch_proposal(&p, b"old\n", Some(&digest()), "src/lib.rs"), Err(PatchValidationError::InvalidHunkCounts));
    }
    #[test]
    fn rejects_multi_file_patch() {
        let p = PATCH.to_owned() + "diff --git a/other.rs b/other.rs\n--- a/other.rs\n+++ b/other.rs\n@@ -1 +1 @@\n-x\n+y\n";
        assert_eq!(validate_patch_proposal(&p, b"old\n", Some(&digest()), "src/lib.rs"), Err(PatchValidationError::MultiFilePatchUnsupported));
    }
    #[test]
    fn rejects_invalid_hunk_line() {
        let p = PATCH.replace("-old\n+new", "?old\n?new");
        assert_eq!(validate_patch_proposal(&p, b"old\n", Some(&digest()), "src/lib.rs"), Err(PatchValidationError::InvalidHunkCounts));
    }
}
