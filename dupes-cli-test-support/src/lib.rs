//! Shared integration-test support for the `cargo-dupes` and `code-dupes`
//! CLIs: `assert_cmd` command factories, fixture path resolution, JSON
//! report helpers, and the [`cli_support_tests!`] macro that stamps the
//! shared CLI test suite into both binaries' test crates.

use std::path::{Path, PathBuf};

use predicates::prelude::*;

pub type CommandFactory = fn() -> assert_cmd::Command;

#[macro_export]
macro_rules! cli_support_test {
    ($name:ident, $command:path, $helper:path $(, $arg:expr)* $(,)?) => {
        #[test]
        fn $name() {
            $helper($command $(, $arg)*);
        }
    };
}

#[macro_export]
macro_rules! cli_support_tests {
    ($command:path; $($name:ident => $helper:ident $(::$segment:ident)* $(($($arg:expr),* $(,)?))?;)+) => {
        $(
            #[test]
            fn $name() {
                $helper $(::$segment)*($command $(, $($arg),*)?);
            }
        )+
    };
}

#[derive(Clone, Copy)]
struct StdoutCase {
    fixture: &'static str,
    args: &'static [&'static str],
    code: Option<i32>,
    needles: &'static [&'static str],
}

/// Path to the workspace root.
#[must_use]
pub fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("test-support crate should live under the workspace root")
        .to_path_buf()
}

fn fixture_path(crate_name: &str, fixture: &str) -> PathBuf {
    workspace_root()
        .join(crate_name)
        .join("tests")
        .join("fixtures")
        .join(fixture)
}

/// Path to a shared Rust fixture used by both CLI crates.
#[must_use]
pub fn rust_fixture_path(name: &str) -> PathBuf {
    fixture_path("cargo-dupes", name)
}

/// Path to a `code-dupes`-local fixture.
#[must_use]
pub fn code_fixture_path(name: &str) -> PathBuf {
    fixture_path("code-dupes", name)
}

fn compiled_binary(name: &str) -> assert_cmd::Command {
    assert_cmd::Command::cargo_bin(name).expect("CLI binary should be built")
}

/// Command for running the `cargo-dupes` binary in integration tests.
///
/// # Panics
///
/// Panics if Cargo did not provide the compiled binary path to the test.
#[must_use]
pub fn cargo_dupes() -> assert_cmd::Command {
    compiled_binary("cargo-dupes")
}

/// Command for running the `code-dupes` binary in integration tests.
///
/// # Panics
///
/// Panics if Cargo did not provide the compiled binary path to the test.
#[must_use]
pub fn code_dupes() -> assert_cmd::Command {
    compiled_binary("code-dupes")
}

/// Command for running a CLI against an arbitrary path.
#[must_use]
pub fn command_for_path(command: CommandFactory, path: impl AsRef<Path>) -> assert_cmd::Command {
    let mut cmd = command();
    cmd.arg("--path").arg(path.as_ref());
    cmd
}

/// Command for running a CLI against a shared Rust fixture.
#[must_use]
pub fn command_for_fixture(command: CommandFactory, fixture: &str) -> assert_cmd::Command {
    command_for_path(command, rust_fixture_path(fixture))
}

pub fn assert_stdout_contains(mut assertion: assert_cmd::assert::Assert, needles: &[&str]) {
    for needle in needles {
        assertion = assertion.stdout(predicate::str::contains(*needle));
    }
}

pub fn assert_fixture_stdout(
    command: CommandFactory,
    fixture: &str,
    args: &[&str],
    status: impl FnOnce(assert_cmd::assert::Assert) -> assert_cmd::assert::Assert,
    needles: &[&str],
) {
    let mut cmd = command_for_fixture(command, fixture);
    cmd.args(args);
    assert_stdout_contains(status(cmd.assert()), needles);
}

pub fn assert_fixture_success_stdout(
    command: CommandFactory,
    fixture: &str,
    args: &[&str],
    needles: &[&str],
) {
    assert_fixture_stdout(
        command,
        fixture,
        args,
        assert_cmd::assert::Assert::success,
        needles,
    );
}

pub fn assert_fixture_code_stdout(
    command: CommandFactory,
    fixture: &str,
    args: &[&str],
    code: i32,
    needles: &[&str],
) {
    assert_fixture_stdout(
        command,
        fixture,
        args,
        |assertion| assertion.code(code),
        needles,
    );
}

fn assert_case(command: CommandFactory, case: StdoutCase) {
    match case.code {
        Some(code) => {
            assert_fixture_code_stdout(command, case.fixture, case.args, code, case.needles);
        }
        None => {
            assert_fixture_success_stdout(command, case.fixture, case.args, case.needles);
        }
    }
}

macro_rules! stdout_case_helpers {
    ($($name:ident => $case:ident;)+) => {
        $(
            pub fn $name(command: CommandFactory) {
                assert_case(command, $case);
            }
        )+
    };
}

#[must_use]
pub fn stdout_from_output(output: &[u8]) -> String {
    String::from_utf8(output.to_vec()).expect("stdout should be UTF-8")
}

#[must_use]
pub fn json_from_text(text: &str) -> serde_json::Value {
    serde_json::from_str(text).expect("stdout should be valid JSON")
}

#[must_use]
pub fn json_from_stdout(output: &[u8]) -> serde_json::Value {
    let text = std::str::from_utf8(output).expect("stdout should be UTF-8");
    json_from_text(text)
}

fn successful_stdout(mut cmd: assert_cmd::Command, args: &[&str]) -> String {
    let output = cmd
        .args(args)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    stdout_from_output(&output)
}

pub fn fixture_stdout(command: CommandFactory, fixture: &str, args: &[&str]) -> String {
    successful_stdout(command_for_fixture(command, fixture), args)
}

pub fn fixture_json(command: CommandFactory, fixture: &str, args: &[&str]) -> serde_json::Value {
    json_from_text(&fixture_stdout(command, fixture, args))
}

/// All groups of one detection dimension from a JSON `report` document.
#[must_use]
pub fn groups_of_dimension<'a>(
    report: &'a serde_json::Value,
    dimension: &str,
) -> Vec<&'a serde_json::Value> {
    report["groups"]
        .as_array()
        .map(|groups| {
            groups
                .iter()
                .filter(|group| group["dimension"] == dimension)
                .collect()
        })
        .unwrap_or_default()
}

/// First group with a member matching both name and file needles.
#[must_use]
pub fn group_containing_member<'a>(
    report: &'a serde_json::Value,
    name_needle: &str,
    file_needle: &str,
) -> Option<&'a serde_json::Value> {
    report["groups"].as_array()?.iter().find(|group| {
        group["members"].as_array().is_some_and(|members| {
            members.iter().any(|member| {
                member["name"]
                    .as_str()
                    .is_some_and(|n| n.contains(name_needle))
                    && member["file"]
                        .as_str()
                        .is_some_and(|f| f.contains(file_needle))
            })
        })
    })
}

/// Assert no visible group has a member matching any needle in files
/// matching `file_needle`; `why` finishes the failure message after the
/// offending needle.
pub fn assert_member_needles_absent(
    report: &serde_json::Value,
    needles: &[&str],
    file_needle: &str,
    why: &str,
) {
    for needle in needles {
        assert!(
            group_containing_member(report, needle, file_needle).is_none(),
            "{needle} {why}"
        );
    }
}

/// Suppressed count attributed to one rule id in a stats document, zero when
/// the stats document carries no suppression surface.
#[must_use]
pub fn suppressed_count_for_rule(stats: &serde_json::Value, rule_id: &str) -> u64 {
    stats["suppressed_by_rule"][rule_id].as_u64().unwrap_or(0)
}

pub fn path_stdout(command: CommandFactory, path: impl AsRef<Path>, args: &[&str]) -> String {
    successful_stdout(command_for_path(command, path), args)
}

pub fn assert_path_success_stdout(
    command: CommandFactory,
    path: impl AsRef<Path>,
    args: &[&str],
    needles: &[&str],
) {
    let mut cmd = command_for_path(command, path);
    cmd.args(args);
    assert_stdout_contains(cmd.assert().success(), needles);
}

#[must_use]
pub fn report_fingerprint(text: &str, require_similarity: bool) -> String {
    text.lines()
        .find(|line| {
            line.contains("fingerprint:") && (!require_similarity || line.contains("similarity:"))
        })
        .and_then(|line| {
            let start = line.find("fingerprint: ")? + 13;
            let end = line[start..].find(',')?;
            Some(line[start..start + end].to_string())
        })
        .expect("report should include a matching fingerprint")
}

#[must_use]
pub fn temp_copy_fixture(fixture: &str) -> tempfile::TempDir {
    let tmp = tempfile::TempDir::new().expect("temp dir should be created");
    std::fs::create_dir_all(tmp.path().join("src")).expect("src dir should be created");
    std::fs::copy(
        rust_fixture_path(fixture).join("src/lib.rs"),
        tmp.path().join("src/lib.rs"),
    )
    .expect("fixture should be copied");
    tmp
}

const CHECK_NO_THRESHOLDS_PASSES_WITH_DUPLICATES: StdoutCase = StdoutCase {
    fixture: "exact_dupes",
    args: &["check"],
    code: None,
    needles: &["Check passed"],
};

const CHECK_FAILS_WITH_DUPLICATES: StdoutCase = StdoutCase {
    fixture: "exact_dupes",
    args: &["check", "--max-exact", "0"],
    code: Some(1),
    needles: &["Check FAILED"],
};

const CHECK_PASSES_WITH_HIGH_THRESHOLD: StdoutCase = StdoutCase {
    fixture: "exact_dupes",
    args: &["check", "--max-exact", "100"],
    code: None,
    needles: &["Check passed"],
};

const CHECK_NO_DUPES_PASSES: StdoutCase = StdoutCase {
    fixture: "no_dupes",
    args: &["check", "--max-exact", "0"],
    code: None,
    needles: &["Check passed"],
};

const CHECK_FAILS_WITH_PERCENTAGE_THRESHOLD_EXCEEDED: StdoutCase = StdoutCase {
    fixture: "exact_dupes",
    args: &["check", "--max-exact", "100", "--max-exact-percent", "0.0"],
    code: Some(1),
    needles: &["Check FAILED", "exact duplicate lines"],
};

const CHECK_PASSES_WITH_GENEROUS_PERCENTAGE_THRESHOLD: StdoutCase = StdoutCase {
    fixture: "exact_dupes",
    args: &[
        "check",
        "--max-exact",
        "100",
        "--max-exact-percent",
        "100.0",
    ],
    code: None,
    needles: &["Check passed"],
};

const CHECK_ABSOLUTE_PASSES_PERCENTAGE_FAILS: StdoutCase = StdoutCase {
    fixture: "exact_dupes",
    args: &["check", "--max-exact", "100", "--max-exact-percent", "0.0"],
    code: Some(1),
    needles: &["Check FAILED"],
};

const REPORT_EXACT_DUPES_FIXTURE: StdoutCase = StdoutCase {
    fixture: "exact_dupes",
    args: &["report"],
    code: None,
    needles: &["Exact Duplicates", "Group 1"],
};

const REPORT_NO_DUPES_FIXTURE: StdoutCase = StdoutCase {
    fixture: "no_dupes",
    args: &["report"],
    code: None,
    needles: &["No exact duplicates"],
};

const REPORT_MIXED_FIXTURE: StdoutCase = StdoutCase {
    fixture: "mixed",
    args: &["report"],
    code: None,
    needles: &["Exact Duplicates", "Group 1"],
};

const STATS_SHOWS_SUMMARY: StdoutCase = StdoutCase {
    fixture: "exact_dupes",
    args: &["stats"],
    code: None,
    needles: &["Total code units analyzed", "Exact duplicates"],
};

const STATS_SHOWS_DUPLICATE_LINES: StdoutCase = StdoutCase {
    fixture: "exact_dupes",
    args: &["stats"],
    code: None,
    needles: &["Duplicated lines (exact):", "Duplicated lines (near):"],
};

const DEFAULT_COMMAND_IS_REPORT: StdoutCase = StdoutCase {
    fixture: "exact_dupes",
    args: &[],
    code: None,
    needles: &["Duplication Statistics", "Exact Duplicates"],
};

const NEAR_DUPES_DETECTED: StdoutCase = StdoutCase {
    fixture: "near_dupes",
    args: &["--threshold", "0.7", "report"],
    code: None,
    needles: &["Near Duplicates", "Group 1", "similarity:"],
};

const MIN_NODES_OPTION: StdoutCase = StdoutCase {
    fixture: "exact_dupes",
    args: &["--min-nodes", "1000", "stats"],
    code: None,
    needles: &["Exact duplicates: 0 groups"],
};

const MIN_LINES_OPTION: StdoutCase = StdoutCase {
    fixture: "exact_dupes",
    args: &["--min-lines", "1000", "stats"],
    code: None,
    needles: &["Exact duplicates: 0 groups"],
};

const EXCLUDE_TESTS_TEXT_REPORT: StdoutCase = StdoutCase {
    fixture: "test_code",
    args: &["--exclude-tests", "report"],
    code: None,
    needles: &["Exact Duplicates", "Group 1"],
};

const SUB_FUNCTION_DETECTS_DUPLICATE_BRANCHES: StdoutCase = StdoutCase {
    fixture: "sub_function_dupes",
    args: &["--sub-function", "report"],
    code: None,
    needles: &[
        "Sub-function Exact Duplicates",
        "if-then branch",
        "match arm",
        "for body",
    ],
};

const SUB_FUNCTION_SHOWS_PARENT_NAMES: StdoutCase = StdoutCase {
    fixture: "sub_function_dupes",
    args: &["--sub-function", "report"],
    code: None,
    needles: &[
        "in handle_positive",
        "in process_value",
        "in classify_number",
        "in describe_value",
    ],
};

const SUB_FUNCTION_STATS_SHOWN: StdoutCase = StdoutCase {
    fixture: "sub_function_dupes",
    args: &["--sub-function", "stats"],
    code: None,
    needles: &["Sub-function exact: 3 groups"],
};

stdout_case_helpers! {
    check_no_thresholds_passes_with_duplicates => CHECK_NO_THRESHOLDS_PASSES_WITH_DUPLICATES;
    check_fails_with_duplicates => CHECK_FAILS_WITH_DUPLICATES;
    check_passes_with_high_threshold => CHECK_PASSES_WITH_HIGH_THRESHOLD;
    check_no_dupes_passes => CHECK_NO_DUPES_PASSES;
    check_fails_with_percentage_threshold_exceeded => CHECK_FAILS_WITH_PERCENTAGE_THRESHOLD_EXCEEDED;
    check_passes_with_generous_percentage_threshold => CHECK_PASSES_WITH_GENEROUS_PERCENTAGE_THRESHOLD;
    check_absolute_passes_percentage_fails => CHECK_ABSOLUTE_PASSES_PERCENTAGE_FAILS;
    report_exact_dupes_fixture => REPORT_EXACT_DUPES_FIXTURE;
    report_no_dupes_fixture => REPORT_NO_DUPES_FIXTURE;
    report_mixed_fixture => REPORT_MIXED_FIXTURE;
    stats_shows_summary => STATS_SHOWS_SUMMARY;
    stats_shows_duplicate_lines => STATS_SHOWS_DUPLICATE_LINES;
    default_command_is_report => DEFAULT_COMMAND_IS_REPORT;
    near_dupes_detected => NEAR_DUPES_DETECTED;
}

pub fn json_format_stats(command: CommandFactory) {
    let parsed = fixture_json(command, "exact_dupes", &["--format", "json", "stats"]);
    assert!(parsed["total_code_units"].as_u64().unwrap() > 0);
}

pub fn json_format_report(command: CommandFactory) {
    let report = fixture_json(command, "exact_dupes", &["--format", "json", "report"]);
    let stats = &report["stats"];
    assert!(stats["total_code_units"].as_u64().unwrap() > 0);
    assert!(stats["exact_duplicate_groups"].as_u64().unwrap() > 0);
    let groups = &report["groups"];
    assert!(!groups.as_array().unwrap().is_empty());
    assert!(groups[0]["fingerprint"].is_string());
    assert!(groups[0]["dimension"].is_string());
    assert!(groups[0]["match_kind"].is_string());
    assert!(groups[0]["members"].is_array());
}

pub fn json_stats_includes_line_counts(command: CommandFactory) {
    let parsed = fixture_json(command, "exact_dupes", &["--format", "json", "stats"]);
    assert!(parsed["exact_duplicate_lines"].is_u64());
    assert!(parsed["near_duplicate_lines"].is_u64());
}

stdout_case_helpers! {
    min_nodes_option => MIN_NODES_OPTION;
    min_lines_option => MIN_LINES_OPTION;
}

pub fn exclude_option(
    command: CommandFactory,
    disable_generic_dimensions: bool,
    expected_stderr: &str,
) {
    let mut cmd = command_for_fixture(command, "exact_dupes");
    cmd.args(["--exclude", "lib.rs"]);
    if disable_generic_dimensions {
        cmd.args([
            "--disable-dimension",
            "token-normalized",
            "--disable-dimension",
            "token-raw",
            "--disable-dimension",
            "line",
        ]);
    }
    cmd.arg("stats")
        .assert()
        .code(2)
        .stderr(predicate::str::contains(expected_stderr));
}

pub fn exclude_tests_flag_reduces_duplicates(command: CommandFactory) {
    let all = fixture_json(command, "test_code", &["--format", "json", "stats"]);
    assert_eq!(all["exact_duplicate_units"].as_u64().unwrap(), 3);

    let excl = fixture_json(
        command,
        "test_code",
        &["--exclude-tests", "--format", "json", "stats"],
    );
    assert_eq!(excl["exact_duplicate_units"].as_u64().unwrap(), 2);
    assert_eq!(excl["total_code_units"].as_u64().unwrap(), 2);
}

stdout_case_helpers! {
    exclude_tests_text_report => EXCLUDE_TESTS_TEXT_REPORT;
}

pub fn dimension_option_limits_reported_dimensions(command: CommandFactory) {
    let all = fixture_json(
        command,
        "sub_function_dupes",
        &["--format", "json", "stats"],
    );
    assert!(
        all["exact_duplicate_groups"].as_u64().unwrap() > 0,
        "baseline fixture should have exact duplicate groups before dimension filtering"
    );

    let line_only = fixture_json(
        command,
        "sub_function_dupes",
        &[
            "--dimension",
            "line",
            "--line-min-lines",
            "3",
            "--format",
            "json",
            "report",
        ],
    );
    let groups = line_only["groups"].as_array().unwrap();
    assert!(
        !groups.is_empty(),
        "line-only analysis should report line duplicate groups"
    );
    assert!(
        groups.iter().all(|group| group["dimension"] == "line"),
        "line-only analysis should only report line groups: {groups:?}"
    );
    assert_eq!(
        line_only["stats"]["exact_duplicate_groups"]
            .as_u64()
            .unwrap(),
        0,
        "line-only analysis should not report the fixture's AST exact duplicate group"
    );
}

pub fn error_on_nonexistent_path(command: CommandFactory, expected_stderr: &str) {
    command()
        .args(["--path", "/nonexistent/path/that/does/not/exist", "stats"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(expected_stderr));
}

pub fn help_works(command: CommandFactory) {
    command()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Detect duplicate code"));
}

stdout_case_helpers! {
    sub_function_detects_duplicate_branches => SUB_FUNCTION_DETECTS_DUPLICATE_BRANCHES;
    sub_function_shows_parent_names => SUB_FUNCTION_SHOWS_PARENT_NAMES;
    sub_function_stats_shown => SUB_FUNCTION_STATS_SHOWN;
}

pub fn sub_function_json_stats(command: CommandFactory) {
    let parsed = fixture_json(
        command,
        "sub_function_dupes",
        &["--sub-function", "--format", "json", "stats"],
    );
    assert_eq!(parsed["sub_exact_groups"].as_u64().unwrap(), 3);
    assert_eq!(parsed["sub_exact_units"].as_u64().unwrap(), 6);
}

pub fn without_sub_function_flag_no_sub_sections(command: CommandFactory) {
    let mut cmd = command_for_fixture(command, "sub_function_dupes");
    cmd.arg("report")
        .assert()
        .success()
        .stdout(predicate::str::contains("Exact Duplicates"))
        .stdout(predicate::str::contains("Sub-function").not());
}

pub fn without_sub_function_json_no_sub_fields(command: CommandFactory) {
    let parsed = fixture_json(
        command,
        "sub_function_dupes",
        &["--format", "json", "stats"],
    );
    assert!(parsed.get("sub_exact_groups").is_none());
    assert!(parsed.get("sub_near_groups").is_none());
}

pub fn sub_function_min_sub_nodes_filters(command: CommandFactory) {
    let mut cmd = command_for_fixture(command, "sub_function_dupes");
    cmd.args(["--sub-function", "--min-sub-nodes", "1000", "stats"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Sub-function").not());
}

fn exact_fingerprint_for_path(command: CommandFactory, path: &Path) -> String {
    let text = path_stdout(command, path, &["report"]);
    report_fingerprint(&text, false)
}

fn near_fingerprint_for_path(command: CommandFactory, path: &Path) -> String {
    let text = path_stdout(command, path, &["--threshold", "0.7", "report"]);
    report_fingerprint(&text, true)
}

fn add_ignore(command: CommandFactory, path: &Path, fingerprint: &str, reason: Option<&str>) {
    let mut cmd = command_for_path(command, path);
    cmd.arg("ignore").arg(fingerprint);
    if let Some(reason) = reason {
        cmd.args(["--reason", reason]);
    }
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Added"));
}

fn append_stale_ignore(path: &Path, reason: &str) {
    let content = std::fs::read_to_string(path).expect("ignore file should exist");
    let new_content = format!(
        "{content}\n[[ignore]]\nfingerprint = \"deadbeefdeadbeef\"\nreason = \"{reason}\"\n"
    );
    std::fs::write(path, new_content).expect("ignore file should be writable");
}

fn write_stale_ignore(path: &Path) {
    std::fs::write(
        path,
        "[[ignore]]\nfingerprint = \"deadbeefdeadbeef\"\nreason = \"stale\"\n",
    )
    .expect("ignore file should be writable");
}

pub fn ignore_workflow(command: CommandFactory) {
    let tmp = temp_copy_fixture("exact_dupes");
    let fp = exact_fingerprint_for_path(command, tmp.path());

    add_ignore(command, tmp.path(), &fp, Some("test ignore"));

    assert_path_success_stdout(
        command,
        tmp.path(),
        &["ignored"],
        &[fp.as_str(), "test ignore"],
    );

    let text_after = path_stdout(command, tmp.path(), &["stats"]);
    assert!(text_after.contains("Exact duplicates: 0 groups"));
}

pub fn ignore_near_duplicate_workflow(command: CommandFactory) {
    let tmp = temp_copy_fixture("near_dupes");
    let fp = near_fingerprint_for_path(command, tmp.path());

    add_ignore(command, tmp.path(), &fp, Some("near dupe ignore test"));

    let text_after = path_stdout(command, tmp.path(), &["--threshold", "0.7", "stats"]);
    assert!(text_after.contains("Near duplicates:  0 groups"));
}

pub fn cleanup_removes_stale_entries(command: CommandFactory) {
    let tmp = temp_copy_fixture("exact_dupes");
    let real_fp = exact_fingerprint_for_path(command, tmp.path());
    add_ignore(command, tmp.path(), &real_fp, None);

    let ignore_path = tmp.path().join(".dupes-ignore.toml");
    append_stale_ignore(&ignore_path, "stale entry");

    assert_path_success_stdout(
        command,
        tmp.path(),
        &["cleanup"],
        &[
            "Removed stale entries",
            "deadbeefdeadbeef",
            "Removed 1 stale entries",
        ],
    );

    assert_path_success_stdout(command, tmp.path(), &["ignored"], &[real_fp.as_str()]);

    let final_content = std::fs::read_to_string(&ignore_path).expect("ignore file should exist");
    assert!(!final_content.contains("deadbeefdeadbeef"));
}

pub fn cleanup_dry_run(command: CommandFactory) {
    let tmp = temp_copy_fixture("exact_dupes");
    let ignore_path = tmp.path().join(".dupes-ignore.toml");
    write_stale_ignore(&ignore_path);

    assert_path_success_stdout(
        command,
        tmp.path(),
        &["cleanup", "--dry-run"],
        &[
            "Stale entries (dry run)",
            "deadbeefdeadbeef",
            "would be removed",
        ],
    );

    let content = std::fs::read_to_string(&ignore_path).expect("ignore file should exist");
    assert!(content.contains("deadbeefdeadbeef"));
}
