use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;

/// Configuration for scanning the filesystem for source files.
pub struct ScanConfig {
    /// Root directory to scan.
    pub root: PathBuf,
    /// Glob-like patterns to exclude.
    pub exclude_patterns: Vec<String>,
    /// File extensions to include (without the leading dot). Defaults to `["rs"]`.
    pub extensions: Vec<String>,
}

impl ScanConfig {
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            exclude_patterns: Vec::new(),
            extensions: vec!["rs".to_string()],
        }
    }

    #[must_use]
    pub fn with_excludes(mut self, patterns: Vec<String>) -> Self {
        self.exclude_patterns = patterns;
        self
    }

    #[must_use]
    pub fn with_extensions(mut self, extensions: Vec<String>) -> Self {
        self.extensions = extensions;
        self
    }
}

/// Scan for source files under the given config.
/// Always skips `target/` directories.
#[must_use]
pub fn scan_files(config: &ScanConfig) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let exclude_set = build_exclude_set(&config.exclude_patterns);

    for entry in WalkBuilder::new(&config.root)
        .hidden(true)
        .git_ignore(true)
        .git_exclude(true)
        .parents(true)
        .build()
        .flatten()
    {
        let path = entry.path();
        if path.is_file()
            && path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| {
                    config
                        .extensions
                        .iter()
                        .any(|e| e.eq_ignore_ascii_case(ext))
                })
            && !is_excluded_with_set(path, exclude_set.as_ref(), &config.exclude_patterns)
        {
            files.push(path.to_path_buf());
        }
    }

    files
}

/// Check if a path should be excluded based on exclusion patterns.
#[must_use]
pub fn is_excluded(path: &Path, patterns: &[String]) -> bool {
    let exclude_set = build_exclude_set(patterns);
    is_excluded_with_set(path, exclude_set.as_ref(), patterns)
}

/// Check if a path should be excluded with a precompiled glob set.
fn is_excluded_with_set(path: &Path, exclude_set: Option<&GlobSet>, patterns: &[String]) -> bool {
    let path_str = path.to_string_lossy();
    if path
        .components()
        .any(|component| component.as_os_str().to_string_lossy() == "target")
    {
        return true;
    }
    if exclude_set.is_some_and(|set| set.is_match(path)) {
        return true;
    }
    patterns
        .iter()
        .any(|pattern| path_str.contains(pattern.as_str()))
}

/// Build a glob set from configured exclude patterns.
fn build_exclude_set(patterns: &[String]) -> Option<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    let mut added = false;
    for pattern in patterns {
        for candidate in exclude_variants(pattern) {
            if let Ok(glob) = Glob::new(&candidate) {
                builder.add(glob);
                added = true;
            }
        }
    }
    if added { builder.build().ok() } else { None }
}

/// Expand a user pattern into useful path-oriented glob variants.
fn exclude_variants(pattern: &str) -> Vec<String> {
    if pattern.contains('*') || pattern.contains('?') || pattern.contains('[') {
        vec![pattern.to_string()]
    } else {
        vec![
            pattern.to_string(),
            format!("**/{pattern}"),
            format!("**/{pattern}/**"),
            format!("**/{pattern}/"),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_tree(dir: &Path) {
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::create_dir_all(dir.join("src/utils")).unwrap();
        fs::create_dir_all(dir.join("target/debug")).unwrap();
        fs::create_dir_all(dir.join(".hidden")).unwrap();
        fs::write(dir.join("src/main.rs"), "fn main() {}").unwrap();
        fs::write(dir.join("src/lib.rs"), "pub mod utils;").unwrap();
        fs::write(dir.join("src/utils/helper.rs"), "pub fn help() {}").unwrap();
        fs::write(dir.join("target/debug/build.rs"), "fn build() {}").unwrap();
        fs::write(dir.join(".hidden/secret.rs"), "fn secret() {}").unwrap();
        fs::write(dir.join("src/readme.md"), "# README").unwrap();
    }

    #[test]
    fn scan_finds_rust_files() {
        let tmp = TempDir::new().unwrap();
        create_test_tree(tmp.path());
        let config = ScanConfig::new(tmp.path().to_path_buf());
        let files = scan_files(&config);
        assert_eq!(files.len(), 3);
        assert!(files.iter().all(|f| f.extension().unwrap() == "rs"));
    }

    #[test]
    fn scan_skips_target_directory() {
        let tmp = TempDir::new().unwrap();
        create_test_tree(tmp.path());
        let config = ScanConfig::new(tmp.path().to_path_buf());
        let files = scan_files(&config);
        assert!(!files.iter().any(|f| f.to_string_lossy().contains("target")));
    }

    #[test]
    fn scan_skips_hidden_directories() {
        let tmp = TempDir::new().unwrap();
        create_test_tree(tmp.path());
        let config = ScanConfig::new(tmp.path().to_path_buf());
        let files = scan_files(&config);
        assert!(
            !files
                .iter()
                .any(|f| f.to_string_lossy().contains(".hidden"))
        );
    }

    #[test]
    fn scan_respects_exclude_patterns() {
        let tmp = TempDir::new().unwrap();
        create_test_tree(tmp.path());
        let config =
            ScanConfig::new(tmp.path().to_path_buf()).with_excludes(vec!["utils".to_string()]);
        let files = scan_files(&config);
        assert!(!files.iter().any(|f| f.to_string_lossy().contains("utils")));
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn scan_empty_directory() {
        let tmp = TempDir::new().unwrap();
        let config = ScanConfig::new(tmp.path().to_path_buf());
        let files = scan_files(&config);
        assert!(files.is_empty());
    }

    #[test]
    fn is_excluded_works() {
        let path = Path::new("/foo/bar/tests/test.rs");
        assert!(is_excluded(path, &["tests".to_string()]));
        assert!(!is_excluded(path, &["benches".to_string()]));
    }
}
