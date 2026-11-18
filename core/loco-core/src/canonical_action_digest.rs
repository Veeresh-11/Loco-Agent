//! Cross-language canonical action digest encoding, version 1.
//! Encoding is length-prefixed UTF-8 with big-endian u64 lengths/integers.
//! Do not change this format without versioning the prefix and test vectors.

use sha2::{Digest, Sha256};
use crate::ProposedAction;

const PREFIX: &[u8] = b"LOCO-ACTION-DIGEST-V1\0";

fn put_string(out: &mut Vec<u8>, value: &str) {
    let bytes = value.as_bytes();
    out.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
    out.extend_from_slice(bytes);
}
fn put_optional_string(out: &mut Vec<u8>, value: Option<&str>) {
    match value {
        None => out.push(0),
        Some(value) => { out.push(1); put_string(out, value); }
    }
}
fn put_optional_u64(out: &mut Vec<u8>, value: Option<u64>) {
    match value {
        None => out.push(0),
        Some(value) => { out.push(1); out.extend_from_slice(&value.to_be_bytes()); }
    }
}

/// Returns the versioned byte representation. This is the canonical input to SHA-256.
pub fn canonical_action_bytes(action: &ProposedAction) -> Vec<u8> {
    let mut out = PREFIX.to_vec();
    match action {
        ProposedAction::ReadFile { path, max_bytes } => {
            out.push(1);
            put_string(&mut out, path);
            put_optional_u64(&mut out, *max_bytes);
        }
        ProposedAction::ProposePatch { path, patch, expected_target_sha256 } => {
            out.push(2);
            put_string(&mut out, path);
            put_string(&mut out, patch);
            put_optional_string(&mut out, expected_target_sha256.as_deref());
        }
        ProposedAction::RunCommand { executable, args, working_directory, timeout_ms } => {
            out.push(3);
            put_string(&mut out, executable);
            out.extend_from_slice(&(args.len() as u64).to_be_bytes());
            for arg in args { put_string(&mut out, arg); }
            put_optional_string(&mut out, working_directory.as_deref());
            put_optional_u64(&mut out, *timeout_ms);
        }
    }
    out
}

/// SHA-256 hex digest of canonical action bytes.
pub fn canonical_action_digest(action: &ProposedAction) -> String {
    let digest = Sha256::digest(canonical_action_bytes(action));
    digest.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_digest_vectors_match_v1_spec() {
        let read = ProposedAction::ReadFile { path: "src/main.rs".into(), max_bytes: Some(1024) };
        assert_eq!(canonical_action_digest(&read), "87cc9be51307092dd5c6d9203de172422cabdc506f78bcb224b07f39d15022a7");

        let patch = ProposedAction::ProposePatch {
            path: "src/lib.rs".into(),
            patch: "diff --git a b".into(),
            expected_target_sha256: Some("a".repeat(64)),
        };
        assert_eq!(canonical_action_digest(&patch), "d8ae37db445a5bf6b34239e8032fa0abb239e130c026ace019ecdaaf232de192");

        let command = ProposedAction::RunCommand {
            executable: "cargo".into(),
            args: vec!["test".into(), "--lib".into()],
            working_directory: Some("core".into()),
            timeout_ms: Some(30_000),
        };
        assert_eq!(canonical_action_digest(&command), "eb0ff76a3f5dd4cc6f1e9966d03140ab49e08700fc0a179505f0ae46c1b74292");
    }

    #[test]
    fn read_file_encoding_has_prefix_and_variant_tag() {
        let action = ProposedAction::ReadFile { path: "a".into(), max_bytes: None };
        let bytes = canonical_action_bytes(&action);
        assert!(bytes.starts_with(PREFIX));
        assert_eq!(bytes[PREFIX.len()], 1);
        assert_eq!(&bytes[PREFIX.len()+1..PREFIX.len()+9], &1u64.to_be_bytes());
    }

    #[test]
    fn option_presence_changes_digest() {
        let absent = ProposedAction::ReadFile { path: "a".into(), max_bytes: None };
        let present = ProposedAction::ReadFile { path: "a".into(), max_bytes: Some(0) };
        assert_ne!(canonical_action_digest(&absent), canonical_action_digest(&present));
    }

    #[test]
    fn unicode_uses_utf8_byte_lengths() {
        let action = ProposedAction::ReadFile { path: "資料/🦀.txt".into(), max_bytes: None };
        let bytes = canonical_action_bytes(&action);
        assert_eq!(bytes[PREFIX.len()+1..PREFIX.len()+9], (15u64).to_be_bytes());
    }

    #[test]
    fn different_argument_boundaries_are_distinct() {
        let one = ProposedAction::RunCommand { executable: "x".into(), args: vec!["ab".into()], working_directory: None, timeout_ms: None };
        let two = ProposedAction::RunCommand { executable: "x".into(), args: vec!["a".into(), "b".into()], working_directory: None, timeout_ms: None };
        assert_ne!(canonical_action_digest(&one), canonical_action_digest(&two));
    }
}
