//! Shared lexical validation for portable workspace-relative paths.
//! This does not establish filesystem containment or race-resistant access.

pub fn is_safe_portable_relative_path(path: &str) -> bool {
    if path.is_empty() || path.starts_with('/') || path.contains('\\') || path.contains(':') || path.contains('\\0') {
        return false;
    }
    path.split('/').all(is_safe_component)
}


fn is_safe_component(component: &str) -> bool {
    if component.is_empty() || component == "." || component == ".." || component.ends_with('.') || component.ends_with(' ') {
        return false;
    }
    if component.chars().any(|c| c.is_control() || matches!(c, '<' | '>' | '\"' | '|' | '?' | '*')) {
        return false;
    }
    let stem = component.split('.').next().unwrap_or(component).to_ascii_uppercase();
    !matches!(stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" |
        "COM1" | "COM2" | "COM3" | "COM4" | "COM5" | "COM6" | "COM7" | "COM8" | "COM9" | "COM¹" | "COM²" | "COM³" |
        "LPT1" | "LPT2" | "LPT3" | "LPT4" | "LPT5" | "LPT6" | "LPT7" | "LPT8" | "LPT9" | "LPT¹" | "LPT²" | "LPT³"
    )
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_components_across_portable_rules() {
        for path in ["", "/absolute", "C:/drive", "a\\\\b", "a//b", "./file", "../file", "src/CON.txt", "src/com².log", "src/name.", "src/name ", "src/bad?name", "src/tab\\tname", "src/new\\nline", "src/file:stream"] {
            assert!(!is_safe_portable_relative_path(path), "should reject: {path:?}");
        }
    }

    #[test]
    fn accepts_portable_relative_paths_and_dotfiles() {
        for path in ["src/main.rs", ".env", "src/.config", "docs/readme.md"] {
            assert!(is_safe_portable_relative_path(path), "should accept: {path:?}");
        }
    }
}
