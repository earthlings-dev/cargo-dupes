//! Self-corpus consolidation gate: duplication sites consolidated at the
//! source must never re-group on the workspace's own code.
//!
//! The pinned command excludes `refactor/` and both CLI fixture trees so the
//! corpus is the product code itself. The dupes-treesitter
//! `normalize_as_block` and dispatch-return consolidations are pinned by
//! unit tests next to their code, not here.
//!
//! Sub-units and windows carry generic names ("if-then branch", "line
//! window") and the JSON member surface does not include the parent, so the
//! fn-interior sites are pinned by line span against the current source; the
//! top-level units (the override helpers, closures) are pinned by name.

mod common;

use common::cargo_dupes;
use dupes_cli_test_support::assert_member_needles_absent;
use dupes_cli_test_support::command_for_path;
use dupes_cli_test_support::json_from_stdout;
use dupes_cli_test_support::workspace_root;

const SELF_CORPUS_ARGS: &[&str] = &[
  "--exclude", "refactor", "--exclude", "cargo-dupes/tests/fixtures", "--exclude", "code-dupes/tests/fixtures", "--sub-function",
  "--format", "json", "report",
];

/// The consolidated sites: (workspace-relative file, fn name).
const CONSOLIDATED_FN_SITES: &[(&str, &str)] = &[
  ("dupes-core/src/cli.rs", "cmd_check"),
  ("dupes-core/src/output/text.rs", "report_stats"),
  ("dupes-core/src/extractor.rs", "add_if_branches"),
  ("dupes-core/src/config.rs", "override_with"),
  ("dupes-core/src/config.rs", "override_option"),
];

#[test]
fn consolidated_sites_stay_consolidated() {
  let output = command_for_path(cargo_dupes, workspace_root())
    .args(SELF_CORPUS_ARGS)
    .assert()
    .success()
    .get_output()
    .stdout
    .clone();
  let report = json_from_stdout(&output);

  // Top-level units named after consolidated fns must not group anywhere
  // in dupes-core ("override_" covers override_with/override_option).
  assert_member_needles_absent(
    &report,
    &["cmd_check", "report_stats", "add_if_branches", "override_"],
    "dupes-core/src",
    "re-grouped: consolidated units must stay consolidated",
  );

  // No visible member of any dimension may lie inside a consolidated fn.
  for (file, fn_name) in CONSOLIDATED_FN_SITES {
    assert_fn_site_clean(&report, file, fn_name);
  }

  // The member-fingerprint projection consolidation: no visible group may
  // pair closures across grouper.rs and lib.rs again.
  for group in report["groups"].as_array().expect("report has groups") {
    assert!(
      !(group_has_closure_in(group, "dupes-core/src/grouper.rs") && group_has_closure_in(group, "dupes-core/src/lib.rs")),
      "closures re-grouped across grouper.rs and lib.rs"
    );
  }
}

/// True when the group has a closure-named member in the given file.
fn group_has_closure_in(group: &serde_json::Value, file_needle: &str) -> bool {
  group["members"].as_array().is_some_and(|members| {
    members.iter().any(|member| {
      member["name"].as_str().is_some_and(|name| name.contains("closure"))
        && member["file"].as_str().is_some_and(|file| file.ends_with(file_needle))
    })
  })
}

/// Assert no visible member of any dimension overlaps the named fn's span.
fn assert_fn_site_clean(report: &serde_json::Value, relative_path: &str, fn_name: &str) {
  let (start, end) = fn_span(relative_path, fn_name);
  for group in report["groups"].as_array().expect("report has groups") {
    for member in group["members"].as_array().expect("group has members") {
      let file = member["file"].as_str().unwrap_or_default();
      if !file.ends_with(relative_path) {
        continue;
      }
      let member_start = member["line_start"].as_u64().unwrap_or(0) as usize;
      let member_end = member["line_end"].as_u64().unwrap_or(0) as usize;
      assert!(
        member_end < start || member_start > end,
        "consolidated site {relative_path}::{fn_name} (lines {start}-{end}) re-grouped: member at lines {member_start}-{member_end}"
      );
    }
  }
}

/// True when the line declares the searched fn: the name is followed by its
/// parameter list or generics (`fn name(` / `fn name<`), so mentions in
/// comments or call sites do not match.
fn line_declares_fn(line: &str, needle: &str) -> bool {
  line
    .match_indices(needle)
    .any(|(at, _)| matches!(line[at + needle.len()..].chars().next(), Some('(' | '<')))
}

/// Line span (1-based, inclusive) of a named `fn` in a workspace source
/// file, located by scanning for the definition and balancing braces.
/// Format-string braces are balanced pairs, so they do not skew the count.
fn fn_span(relative_path: &str, fn_name: &str) -> (usize, usize) {
  let source = std::fs::read_to_string(workspace_root().join(relative_path)).expect("consolidated source file should exist");
  let needle = format!("fn {fn_name}");
  let mut start = None;
  let mut depth = 0usize;
  let mut opened = false;
  for (idx, line) in source.lines().enumerate() {
    if start.is_none() {
      if line_declares_fn(line, &needle) {
        start = Some(idx + 1);
      } else {
        continue;
      }
    }
    depth += line.matches('{').count();
    opened |= line.contains('{');
    depth = depth.saturating_sub(line.matches('}').count());
    if opened && depth == 0 {
      return (start.expect("start recorded before close"), idx + 1);
    }
  }
  panic!("fn {fn_name} not found in {relative_path}")
}
