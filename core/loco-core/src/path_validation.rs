//! Workspace-aware path validation prototype.
//! Canonicalizes existing path prefixes and permits missing suffixes.
//! This is not race-free authorization: later operations must revalidate and
//! use race-resistant filesystem primitives.

use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathValidationError {
    InvalidWorkspaceRoot, EmptyPath, AbsolutePath, ParentTraversal,
    CanonicalizationFailed, OutsideWorkspace,
}

fn has_parent_component(path: &Path) -> bool {
    path.components().any(|c| matches!(c, Component::ParentDir))
}

fn canonicalize_with_missing_tail(path: &Path) -> Result<PathBuf, PathValidationError> {
    let mut cursor = path.to_path_buf();
    let mut missing: Vec<std::ffi::OsString> = Vec::new();
    while !cursor.exists() {
        missing.push(cursor.file_name().ok_or(PathValidationError::CanonicalizationFailed)?.to_os_string());
        cursor = cursor.parent().ok_or(PathValidationError::CanonicalizationFailed)?.to_path_buf();
    }
    let mut resolved = cursor.canonicalize().map_err(|_| PathValidationError::CanonicalizationFailed)?;
    for part in missing.iter().rev() { resolved.push(part); }
    Ok(resolved)
}

/// Returns the resolved target if it remains strictly beneath the canonical
/// workspace root. Rejects absolute paths and any parent traversal component.
/// This is a validation helper, not a secure read/write operation.
pub fn validate_workspace_path(root: &Path, candidate: &Path) -> Result<PathBuf, PathValidationError> {
    if root.as_os_str().is_empty() { return Err(PathValidationError::InvalidWorkspaceRoot); }
    if candidate.as_os_str().is_empty() { return Err(PathValidationError::EmptyPath); }
    if candidate.is_absolute() { return Err(PathValidationError::AbsolutePath); }
    if has_parent_component(candidate) { return Err(PathValidationError::ParentTraversal); }

    let canonical_root = root.canonicalize().map_err(|_| PathValidationError::InvalidWorkspaceRoot)?;
    let target = canonicalize_with_missing_tail(&canonical_root.join(candidate))?;
    if target != canonical_root && target.starts_with(&canonical_root) {
        Ok(target)
    } else {
        Err(PathValidationError::OutsideWorkspace)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn accepts_existing_file() {
        let t = tempdir().unwrap(); let root = t.path().join("ws");
        fs::create_dir_all(root.join("src")).unwrap(); fs::write(root.join("src/a.rs"), "x").unwrap();
        assert!(validate_workspace_path(&root, Path::new("src/a.rs")).unwrap().ends_with("src/a.rs"));
    }
    #[test]
    fn accepts_missing_target_within_workspace() {
        let t = tempdir().unwrap(); let root = t.path().join("ws");
        fs::create_dir_all(root.join("src")).unwrap();
        let p = validate_workspace_path(&root, Path::new("src/new.rs")).unwrap();
        assert!(p.ends_with("src/new.rs")); assert!(!p.exists());
    }
    #[test]
    fn rejects_parent_traversal() {
        let t = tempdir().unwrap(); let root = t.path().join("ws"); fs::create_dir_all(&root).unwrap();
        assert_eq!(validate_workspace_path(&root, Path::new("../outside")), Err(PathValidationError::ParentTraversal));
    }
    #[test]
    fn rejects_absolute_path() {
        let t = tempdir().unwrap(); let root = t.path().join("ws"); fs::create_dir_all(&root).unwrap();
        assert_eq!(validate_workspace_path(&root, &root.join("file")), Err(PathValidationError::AbsolutePath));
    }
    #[test]
    fn rejects_root_as_target() {
        let t = tempdir().unwrap(); let root = t.path().join("ws"); fs::create_dir_all(&root).unwrap();
        assert_eq!(validate_workspace_path(&root, Path::new(".")), Err(PathValidationError::OutsideWorkspace));
    }
    #[test]
    fn rejects_symlink_escape_if_supported() {
        let t = tempdir().unwrap(); let root = t.path().join("ws"); let outside = t.path().join("outside");
        fs::create_dir_all(&root).unwrap(); fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret"), "x").unwrap();
        #[cfg(unix)] let result = std::os::unix::fs::symlink(&outside, root.join("link"));
        #[cfg(windows)] let result = std::os::windows::fs::symlink_dir(&outside, root.join("link"));
        if result.is_err() { return; }
        assert_eq!(validate_workspace_path(&root, Path::new("link/secret")), Err(PathValidationError::OutsideWorkspace));
    }
    #[test]
    fn rejects_new_target_under_symlink_escape_if_supported() {
        let t = tempdir().unwrap(); let root = t.path().join("ws"); let outside = t.path().join("outside");
        fs::create_dir_all(&root).unwrap(); fs::create_dir_all(&outside).unwrap();
        #[cfg(unix)] let result = std::os::unix::fs::symlink(&outside, root.join("link"));
        #[cfg(windows)] let result = std::os::windows::fs::symlink_dir(&outside, root.join("link"));
        if result.is_err() { return; }
        assert_eq!(validate_workspace_path(&root, Path::new("link/new")), Err(PathValidationError::OutsideWorkspace));
    }
}
