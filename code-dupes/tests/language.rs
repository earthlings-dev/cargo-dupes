mod common;

use common::{code_dupes, code_dupes_fixture_path, fixture_path};
use predicates::prelude::*;
use std::path::Path;

fn assert_path_command_contains(path: &Path, command_args: &[&str], expected: &str) {
    let mut args = vec!["--path", path.to_str().unwrap()];
    args.extend_from_slice(command_args);
    code_dupes()
        .args(args)
        .assert()
        .success()
        .stdout(predicate::str::contains(expected));
}

fn assert_stats_success(path: &Path) {
    assert_path_command_contains(path, &["stats"], "Total code units analyzed");
}

fn assert_language_stats_success(path: &Path, language: &str) {
    assert_path_command_contains(
        path,
        &["--language", language, "stats"],
        "Total code units analyzed",
    );
}

fn assert_stats_error(path: &Path, expected: &str) {
    code_dupes()
        .args(["--path", path.to_str().unwrap(), "stats"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(expected));
}

fn assert_language_stats_error(path: &Path, language: &str, expected: &str) {
    code_dupes()
        .args([
            "--path",
            path.to_str().unwrap(),
            "--language",
            language,
            "stats",
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(expected));
}

fn assert_python_dupes_report_contains(expected: &str) {
    assert_path_command_contains(
        &code_dupes_fixture_path("python_dupes"),
        &[
            "--language",
            "python",
            "--min-nodes",
            "1",
            "--min-lines",
            "1",
        ],
        expected,
    );
}

#[test]
fn explicit_language_rust() {
    assert_language_stats_success(&fixture_path("exact_dupes"), "rust");
}

#[test]
fn auto_detect_rust_from_rs_files() {
    // Fixture directories contain .rs files, so Rust should be auto-detected
    assert_stats_success(&fixture_path("exact_dupes"));
}

#[test]
fn error_on_empty_directory() {
    let tmp = tempfile::TempDir::new().unwrap();
    assert_stats_error(tmp.path(), "No recognized source files");
}

#[test]
fn error_on_directory_with_unknown_files_only() {
    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp.path().join("data.csv"), "a,b,c").unwrap();
    std::fs::write(tmp.path().join("blob.dat"), "hello").unwrap();
    assert_stats_error(tmp.path(), "No recognized source files");
}

#[test]
fn auto_detects_generic_text_duplicates() {
    assert_path_command_contains(
        &code_dupes_fixture_path("text_dupes"),
        &["--line-min-lines", "5", "report"],
        "Line Exact Duplicates",
    );
}

#[test]
fn invalid_language_shows_error() {
    code_dupes()
        .args([
            "--path",
            fixture_path("exact_dupes").to_str().unwrap(),
            "--language",
            "unknown",
            "stats",
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("invalid value"));
}

#[test]
fn no_cargo_subcommand_arg_needed() {
    // Unlike cargo-dupes, code-dupes should NOT require a hidden first arg
    code_dupes()
        .args([
            "--path",
            fixture_path("exact_dupes").to_str().unwrap(),
            "stats",
        ])
        .assert()
        .success();
}

#[test]
fn explicit_language_on_empty_dir_reports_no_source_files() {
    let tmp = tempfile::TempDir::new().unwrap();
    assert_language_stats_error(tmp.path(), "rust", "No source files");
}

#[test]
fn auto_detect_ignores_non_rust_files() {
    // Directory with .rs + other files should still auto-detect Rust
    let tmp = tempfile::TempDir::new().unwrap();
    let src = tmp.path().join("src");
    std::fs::create_dir(&src).unwrap();
    std::fs::write(
        src.join("lib.rs"),
        "pub fn hello() { println!(\"hello\"); }\npub fn world() { println!(\"world\"); }\n",
    )
    .unwrap();
    std::fs::write(tmp.path().join("readme.txt"), "some text").unwrap();
    std::fs::write(tmp.path().join("data.csv"), "a,b,c").unwrap();
    assert_stats_success(tmp.path());
}

#[test]
fn auto_detect_finds_deeply_nested_rs_files() {
    let tmp = tempfile::TempDir::new().unwrap();
    let deep = tmp.path().join("a").join("b").join("c");
    std::fs::create_dir_all(&deep).unwrap();
    std::fs::write(
        deep.join("lib.rs"),
        "pub fn deep() { println!(\"deep\"); }\n",
    )
    .unwrap();
    assert_stats_success(tmp.path());
}

#[test]
fn explicit_language_python() {
    assert_language_stats_success(&code_dupes_fixture_path("python_dupes"), "python");
}

#[test]
fn auto_detect_python_from_py_files() {
    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(
        tmp.path().join("example.py"),
        "def add(a, b):\n    return a + b\n\ndef sub(a, b):\n    return a - b\n",
    )
    .unwrap();
    assert_stats_success(tmp.path());
}

#[test]
fn python_detects_exact_duplicates() {
    assert_python_dupes_report_contains("Exact Duplicates");
}

// jscpd:ignore-start

#[test]
fn ambiguous_language_detection_errors() {
    // Directory with both .rs and .py files should report ambiguity
    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(
        tmp.path().join("lib.rs"),
        "pub fn hello() { println!(\"hello\"); }\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("example.py"),
        "def hello():\n    print('hello')\n",
    )
    .unwrap();
    code_dupes()
        .args(["--path", tmp.path().to_str().unwrap(), "stats"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("Multiple languages detected"));
}

#[test]
fn ambiguous_language_resolved_with_explicit_flag() {
    // When multiple languages are present, --language resolves the ambiguity
    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(
        tmp.path().join("lib.rs"),
        "pub fn hello() { println!(\"hello\"); }\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("example.py"),
        "def hello():\n    print('hello')\n",
    )
    .unwrap();
    assert_language_stats_success(tmp.path(), "python");
}

// jscpd:ignore-end

#[test]
fn python_detects_lambda_duplicates() {
    assert_python_dupes_report_contains("closure");
}

#[test]
fn python_detects_class_duplicates() {
    assert_python_dupes_report_contains("class");
}

#[test]
fn python_explicit_language_on_empty_dir() {
    let tmp = tempfile::TempDir::new().unwrap();
    assert_language_stats_error(tmp.path(), "python", "No source files");
}
