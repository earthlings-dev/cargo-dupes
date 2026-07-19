//! Detection-coverage regression harness over the frozen `detector_coverage`
//! fixture.
//!
//! Every pin asserts the CURRENT detector behavior and is updated together
//! with the change that flips it. All numeric values are measured actuals;
//! the qualitative expectations around them restate the contract in
//! `DETECTOR_REPORTABILITY.md`, so a pin that can only be satisfied by
//! contradicting that contract is a detector regression to investigate, not
//! a number to adjust.

mod common;

use common::cargo_dupes;
use dupes_cli_test_support::assert_member_needles_absent;
use dupes_cli_test_support::fixture_json;
use dupes_cli_test_support::fixture_stdout;
use dupes_cli_test_support::group_containing_member;
use dupes_cli_test_support::groups_of_dimension;
use dupes_cli_test_support::suppressed_count_for_rule;

const FIXTURE: &str = "detector_coverage";
const REPORT_ARGS: &[&str] = &["--sub-function", "--format", "json", "report"];
const STATS_ARGS: &[&str] = &["--sub-function", "--format", "json", "stats"];
const SHOW_SUPPRESSED_REPORT_ARGS: &[&str] = &["--sub-function", "--show-suppressed", "--format", "json", "report"];

/// Line spans of every line-dimension member in files ending with `suffix`.
fn line_member_spans(report: &serde_json::Value, suffix: &str) -> Vec<(u64, u64)> {
  groups_of_dimension(report, "line")
    .into_iter()
    .flat_map(|group| group["members"].as_array().unwrap().iter())
    .filter(|member| member["file"].as_str().unwrap().ends_with(suffix))
    .map(|member| (member["line_start"].as_u64().unwrap(), member["line_end"].as_u64().unwrap()))
    .collect()
}

#[test]
fn stats_pins() {
  let stats = fixture_json(cargo_dupes, FIXTURE, STATS_ARGS);
  // The total covers the full extracted population, suppressed included.
  assert_eq!(stats["total_code_units"].as_u64().unwrap(), 42);
  // ranges_overlap/spans_collide (size-cap release) and chain_x/chain_y.
  assert_eq!(stats["exact_duplicate_groups"].as_u64().unwrap(), 2);
  assert_eq!(stats["exact_duplicate_units"].as_u64().unwrap(), 4);
  // collect/rank pairs at >= 0.9 plus the 0.8-threshold captures: the
  // *_total quartet, both touch/order pairs, the dispatch pair, and the
  // build_one/build_two runs (method-name preservation keeps them near).
  assert_eq!(stats["near_duplicate_groups"].as_u64().unwrap(), 7);
  assert_eq!(stats["near_duplicate_units"].as_u64().unwrap(), 16);
  // The released 14-member apply branch group and the chain_x/chain_y
  // IfChain pair; the 6-member chain branch group is covered and hidden.
  assert_eq!(stats["sub_exact_groups"].as_u64().unwrap(), 2);
  assert_eq!(stats["token_normalized_exact_groups"].as_u64().unwrap(), 2);
  assert_eq!(stats["token_normalized_near_groups"].as_u64().unwrap(), 1);
  assert_eq!(stats["token_raw_exact_groups"].as_u64().unwrap(), 1);
  // The Dense pair, the Renderer trio, the CliA/CliB stanza windows, and
  // at least one SpreadA/SpreadB table group admitted by the structural
  // stanza coalescer; the builder-run pair sits under group.covered-by-ast
  // because build_one/build_two form an ast near pair at the 0.8 threshold.
  assert_eq!(stats["line_exact_groups"].as_u64().unwrap(), 8);
  // The Rust quote profile lexes the doc-comment apostrophes ("chain's")
  // as punctuation, so the overrides spans yield four signature-prefix
  // token windows over chain_x/chain_y and one suppressed group (the
  // identical normalized pair).
  assert_eq!(stats["suppressed_unit_count"].as_u64().unwrap(), 100);
  assert_eq!(stats["suppressed_group_count"].as_u64().unwrap(), 27);
}

#[test]
fn suppressed_rule_attribution_pins() {
  let stats = fixture_json(cargo_dupes, FIXTURE, STATS_ARGS);
  // Every (rule, count) pair the fixture produces; the pins flip together
  // with any rule change. Note that simple field comparator closures
  // attribute to ast.forwarding-accessor (its check precedes the
  // comparator-adapter shape); call-based adapters reach the comparator
  // rule.
  let expected = [
    // Size-cap release: ranges_overlap/spans_collide (>= 24-node bodies)
    // are visible; only is_word_start/is_word_part stay suppressed.
    ("ast.boolean-projection", 2),
    ("ast.forwarding-accessor", 6),
    // Assignment setters and method-mutation setters both tag here.
    ("ast.setter-returning-self", 4),
    // Two covered groups: the chain_x/chain_y body windows shadowed by
    // the AST chain pair, and the admitted builder-run windows shadowed
    // by the build_one/build_two ast near pair at the 0.8 threshold.
    ("group.covered-by-ast", 2),
    // Six narrower stanza-window groups contained (>= 0.8) within wider
    // admitted groups over the same coalesced declaration blocks.
    ("group.overlap-contained", 6),
    ("line.chain-tail", 8),
    ("line.declaration-signature-prefix", 1),
    ("line.import-scaffold", 7),
    // Punct/header-crossing windows inside coalesced declaration blocks
    // fail stanza admission and fall through to low-signal.
    ("line.low-signal", 36),
    // The six chain_x/chain_y branches, fully covered by the duplicated
    // IfChain pair; the 14 apply branches stay released and visible.
    ("sub.covered-by-chain", 6),
    ("sub.empty-default-return", 2),
    ("sub.message-only-macro", 2),
    ("sub.value-plumbing", 10),
    ("token.match-table-prefix", 2),
    // Includes the four chain_x/chain_y fn-head windows that the Rust
    // quote profile keeps segmented (both token modes, both files).
    ("token.signature-prefix", 14),
  ];
  let map = stats["suppressed_by_rule"].as_object().unwrap();
  for (rule, count) in expected {
    assert_eq!(suppressed_count_for_rule(&stats, rule), count, "rule {rule}");
  }
  // The comparator rule must NOT fire here: the fixture's field-projection
  // comparators attribute to ast.forwarding-accessor (shadowing note above).
  assert!(!map.contains_key("ast.comparator-adapter"));
  assert_eq!(map.len(), expected.len(), "no unexpected rules fire");
}

#[test]
fn show_suppressed_exposes_tagged_groups() {
  let report = fixture_json(cargo_dupes, FIXTURE, SHOW_SUPPRESSED_REPORT_ARGS);
  let suppressed = report["suppressed_groups"].as_array().unwrap();
  assert!(!suppressed.is_empty());
  assert!(
    suppressed.iter().all(|group| group["suppressed"].is_string()),
    "every suppressed group carries its rule"
  );
  // The chain-covered chain_x branch members surface here with their rule.
  assert!(suppressed.iter().any(|group| {
    group["suppressed"] == "sub.covered-by-chain"
      && group["members"]
        .as_array()
        .unwrap()
        .iter()
        .any(|member| member["file"].as_str().unwrap().ends_with("overrides_a.rs"))
  }));
  // Without the flag the array is absent from the document.
  let default_report = fixture_json(cargo_dupes, FIXTURE, REPORT_ARGS);
  assert!(default_report.get("suppressed_groups").is_none());
}

#[test]
fn text_report_renders_suppression_surface() {
  let stats = fixture_stdout(cargo_dupes, FIXTURE, &["--sub-function", "stats"]);
  assert!(stats.contains("Suppressed: 100 units, 27 groups (--show-suppressed to list)"));
  assert!(!stats.contains("Suppressed by rule:"));

  let verbose = fixture_stdout(cargo_dupes, FIXTURE, &["--sub-function", "-v", "stats"]);
  assert!(verbose.contains("Suppressed by rule:"));
  assert!(verbose.contains("  sub.covered-by-chain: 6 units"));
  assert!(verbose.contains("  group.covered-by-ast: 2 groups"));

  let default_report = fixture_stdout(cargo_dupes, FIXTURE, &["--sub-function", "report"]);
  assert!(!default_report.contains("Suppressed Sub-function Exact Duplicates"));

  let shown = fixture_stdout(cargo_dupes, FIXTURE, &["--sub-function", "--show-suppressed", "report"]);
  assert!(shown.contains("Suppressed Sub-function Exact Duplicates"));
  assert!(shown.contains("[rule: sub.covered-by-chain]"));
}

#[test]
fn disabling_a_rule_unhides_its_findings() {
  // sub.value-plumbing hides the dispatch-arm group; disabling the rule
  // makes the arms a visible sub_ast group and removes the attribution.
  let run = |command| {
    let args = [
      "--sub-function", "--disable-rule", "sub.value-plumbing", "--format", "json", command,
    ];
    fixture_json(cargo_dupes, FIXTURE, &args)
  };
  let report = run("report");
  let arms = group_containing_member(&report, "match arm", "shapes.rs").expect("dispatch arms become visible");
  assert_eq!(arms["dimension"], "sub_ast");
  let stats = run("stats");
  assert_eq!(suppressed_count_for_rule(&stats, "sub.value-plumbing"), 0);
}

#[test]
fn identical_if_chains_group_as_whole_chains() {
  let report = fixture_json(cargo_dupes, FIXTURE, REPORT_ARGS);
  let chain_fns = group_containing_member(&report, "chain_x", "overrides_a.rs").expect("chain_x/chain_y function group");
  assert_eq!(chain_fns["dimension"], "ast");
  assert!(group_containing_member(&report, "chain_y", "overrides_b.rs").is_some());
  let chain_subs = group_containing_member(&report, "if chain (3 branches)", "overrides_a.rs").expect("chain-level sub group");
  assert_eq!(chain_subs["dimension"], "sub_ast");
}

#[test]
fn override_branch_duplication_is_released() {
  // The apply_a/apply_b rows share one branch shape across 14 regions and
  // their chains never group, so release-on-no-match keeps the cross-file
  // IfBranch group visible; the chain_x/chain_y branches stay hidden under
  // sub.covered-by-chain.
  let report = fixture_json(cargo_dupes, FIXTURE, REPORT_ARGS);
  assert_eq!(groups_of_dimension(&report, "sub_ast").len(), 2);
  let released = group_containing_member(&report, "if-then branch", "overrides_a.rs").expect("released apply-branch group");
  assert_eq!(released["dimension"], "sub_ast");
  let members = released["members"].as_array().unwrap();
  assert_eq!(members.len(), 14);
  assert!(
    members
      .iter()
      .any(|member| member["file"].as_str().unwrap().ends_with("overrides_b.rs")),
    "the released group spans both files"
  );
  // Only apply rows may appear: apply_a spans lines 6-34 of overrides_a.rs
  // and apply_b spans lines 6-22 of overrides_b.rs; the chain branches
  // (lines 39+ / 27+) belong to the covered group.
  for member in members {
    let file = member["file"].as_str().unwrap();
    let start = member["line_start"].as_u64().unwrap();
    if file.ends_with("overrides_a.rs") {
      assert!(start <= 34, "chain branch leaked into the released group");
    } else {
      assert!(start <= 22, "chain branch leaked into the released group");
    }
  }
}

#[test]
fn declaration_stanzas_are_admitted() {
  // The structural stanza coalescer joins blank-separated doc/attr/field
  // stanzas (including the derive-headed table and the final stanza that
  // carries the closing brace), and line.declaration-stanza admits their
  // windows.
  let report = fixture_json(cargo_dupes, FIXTURE, REPORT_ARGS);
  let decl_members = line_member_spans(&report, "decl_a.rs");
  // CliA stanza rows span lines 12-34: admitted windows must reach both the
  // first stanza (start <= 14) and the final stanza (end >= 32).
  assert!(
    decl_members.iter().any(|(start, _)| *start <= 14),
    "no admitted window reaches the first CliA stanza"
  );
  assert!(
    decl_members.iter().any(|(_, end)| *end >= 32),
    "no admitted window reaches the final CliA stanza"
  );
  // At least one admitted window lies inside the SpreadA row span (40-50).
  assert!(
    decl_members.iter().any(|(start, end)| *start >= 40 && *end <= 50),
    "no admitted window covers the SpreadA/SpreadB table"
  );
}

#[test]
fn contiguous_derive_table_is_visible() {
  let report = fixture_json(cargo_dupes, FIXTURE, REPORT_ARGS);
  // DenseA's rows live at decl_a.rs lines 57-62; the pair with DenseB is
  // visible without any admission carve-out.
  let dense = groups_of_dimension(&report, "line")
    .into_iter()
    .find(|group| {
      group["members"]
        .as_array()
        .unwrap()
        .iter()
        .any(|member| member["file"].as_str().unwrap().ends_with("decl_a.rs") && member["line_start"].as_u64().unwrap() >= 57)
    })
    .expect("DenseA/DenseB table group");
  let member_files: Vec<&str> = dense["members"]
    .as_array()
    .unwrap()
    .iter()
    .filter_map(|member| member["file"].as_str())
    .collect();
  assert!(member_files.iter().any(|file| file.ends_with("decl_b.rs")));
}

#[test]
fn builder_runs_are_admitted() {
  // The identical six-step runs in build_one (lines 18-23) and build_two
  // (lines 29-34) are admitted via line.builder-chain-run. At the 0.8 near
  // threshold the whole fns also pair in the ast dimension, so the admitted
  // line shadow carries group.covered-by-ast instead of rendering twice;
  // the tail_one/tail_two fragments (lines 39+) stay chain-tail suppressed.
  let report = fixture_json(cargo_dupes, FIXTURE, SHOW_SUPPRESSED_REPORT_ARGS);
  let builders = group_containing_member(&report, "build_one", "builders.rs").expect("build_one/build_two ast near pair");
  assert_eq!(builders["dimension"], "ast");
  assert_eq!(builders["match_kind"], "near");
  let suppressed = report["suppressed_groups"].as_array().unwrap();
  assert!(
    suppressed.iter().any(|group| {
      group["suppressed"] == "group.covered-by-ast"
        && group["members"].as_array().unwrap().iter().any(|member| {
          member["file"].as_str().unwrap().ends_with("builders.rs")
            && member["line_start"].as_u64().unwrap() >= 18
            && member["line_end"].as_u64().unwrap() <= 23
        })
    }),
    "the admitted builder run is covered by the ast pair, not low-signal"
  );
  let default_report = fixture_json(cargo_dupes, FIXTURE, REPORT_ARGS);
  assert!(
    line_member_spans(&default_report, "builders.rs").is_empty(),
    "no builder line window is visible once the ast pair covers the runs"
  );
}

#[test]
fn capped_boolean_projections_surface_while_trivial_shapes_stay_suppressed() {
  // TRIVIAL_BODY_MAX_NODES releases the >= 24-node ranges_overlap and
  // spans_collide pair into a visible ast group; genuinely trivial shapes
  // (setters, accessors, small projections, comparator closures) stay
  // rule-suppressed.
  let report = fixture_json(cargo_dupes, FIXTURE, REPORT_ARGS);
  let released = group_containing_member(&report, "ranges_overlap", "shapes.rs").expect("size-cap released boolean-projection pair");
  assert_eq!(released["dimension"], "ast");
  assert_eq!(released["match_kind"], "exact");
  assert!(group_containing_member(&report, "spans_collide", "shapes.rs").is_some());
  assert_member_needles_absent(
    &report,
    &[
      "with_level", "with_scale", "push_item", "push_mark", "level_for", "mark_for", "is_word_start", "is_word_part", "closure at",
    ],
    "shapes.rs",
    "must stay suppressed",
  );
}

#[test]
fn mutation_setters_are_suppressed_with_attribution() {
  // Method-mutation setters join ast.setter-returning-self; the pair
  // stays detected and recoverable via --show-suppressed.
  let report = fixture_json(cargo_dupes, FIXTURE, SHOW_SUPPRESSED_REPORT_ARGS);
  let suppressed = report["suppressed_groups"].as_array().unwrap();
  let setters = suppressed
    .iter()
    .find(|group| {
      group["suppressed"] == "ast.setter-returning-self"
        && group["members"]
          .as_array()
          .unwrap()
          .iter()
          .any(|member| member["name"].as_str().unwrap_or_default() == "Gauge::push_item")
    })
    .expect("push_item/push_mark suppressed setter group");
  let member_names: Vec<&str> = setters["members"]
    .as_array()
    .unwrap()
    .iter()
    .filter_map(|member| member["name"].as_str())
    .collect();
  assert_eq!(
    member_names,
    ["Gauge::push_item", "Gauge::push_mark"],
    "the mutation-setter pair groups alone"
  );
}

#[test]
fn sub_unit_noise_shapes_stay_suppressed() {
  // Message-only writeln branches, empty-default guards, and dispatch arms
  // are tagged sub units (sub.message-only-macro, sub.empty-default-return,
  // sub.value-plumbing) and never surface in the default report.
  let report = fixture_json(cargo_dupes, FIXTURE, REPORT_ARGS);
  let branch = group_containing_member(&report, "if-then branch", "shapes.rs");
  assert!(branch.is_none(), "guard branches stay suppressed");
  let arm = group_containing_member(&report, "match arm", "shapes.rs");
  assert!(arm.is_none(), "dispatch arms stay suppressed");
}

#[test]
fn impl_signature_parity_stays_visible() {
  // The Renderer parameter rows pair across the trait and both impls; the
  // fn-led declaration-side window is rejected, the parameter-row windows
  // survive.
  let report = fixture_json(cargo_dupes, FIXTURE, REPORT_ARGS);
  let parity = groups_of_dimension(&report, "line")
    .into_iter()
    .find(|group| {
      let members = group["members"].as_array().unwrap();
      members.len() == 3
        && members
          .iter()
          .all(|member| member["file"].as_str().unwrap().ends_with("shapes.rs"))
    })
    .expect("Renderer signature parity line group");
  assert_eq!(parity["match_kind"], "exact");
}

#[test]
fn import_scaffolding_line_windows_stay_invisible() {
  // The shared decl_a/decl_b header + import block surfaces as token
  // windows today (pinned via stats), but the line dimension keeps
  // rejecting import scaffolds: no line window may start in the import
  // block region (lines 1-7).
  let report = fixture_json(cargo_dupes, FIXTURE, REPORT_ARGS);
  for group in groups_of_dimension(&report, "line") {
    for member in group["members"].as_array().unwrap() {
      let file = member["file"].as_str().unwrap();
      if file.ends_with("decl_a.rs") || file.ends_with("decl_b.rs") {
        assert!(member["line_start"].as_u64().unwrap() > 7);
      }
    }
  }
}
