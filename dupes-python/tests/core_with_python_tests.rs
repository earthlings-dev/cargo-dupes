//! `dupes-core` grouping and similarity behavior exercised through units
//! parsed by the Python analyzer.

use std::path::PathBuf;

use dupes_core::analyzer::LanguageAnalyzer;
use dupes_core::code_unit::CodeUnit;
use dupes_core::config::AnalysisConfig;
use dupes_core::grouper::DuplicateGroup;
use dupes_core::grouper::compute_stats;
use dupes_core::grouper::find_near_duplicates;
use dupes_core::grouper::group_exact_duplicates;
use dupes_core::similarity::similarity_score;
use dupes_python::PythonAnalyzer;

const fn default_config() -> AnalysisConfig {
  AnalysisConfig {
    min_nodes: 1,
    min_lines: 1,
  }
}

fn parse(source: &str) -> Vec<CodeUnit> {
  let analyzer = PythonAnalyzer::new();
  analyzer
    .parse_file(&PathBuf::from("test.py"), source, &default_config())
    .expect("parse should succeed")
}

fn group_fingerprints(groups: &[DuplicateGroup]) -> Vec<dupes_core::fingerprint::Fingerprint> {
  groups.iter().map(|g| g.fingerprint).collect()
}

#[test]
fn exact_duplicates_grouped() {
  let units = parse(
    r"
def add(a, b):
    result = a + b
    return result

def add2(x, y):
    result = x + y
    return result

def mul(a, b):
    result = a * b
    return result
",
  );
  assert_eq!(units.len(), 3);
  let groups = group_exact_duplicates(&units);
  // add and add2 are exact duplicates
  assert_eq!(groups.len(), 1);
  assert_eq!(groups[0].members.len(), 2);
}

#[test]
fn no_exact_duplicates_when_all_different() {
  let units = parse(
    r"
def add(a, b):
    return a + b

def mul(a, b):
    return a * b

def div(a, b):
    return a / b
",
  );
  assert_eq!(units.len(), 3);
  let groups = group_exact_duplicates(&units);
  assert!(groups.is_empty());
}

#[test]
fn near_duplicates_found() {
  let units = parse(
    r"
def process_add(a, b):
    result = a + b
    x = result * 2
    return x

def process_mul(a, b):
    result = a * b
    x = result * 2
    return x
",
  );
  assert_eq!(units.len(), 2);
  let exact = group_exact_duplicates(&units);
  assert!(exact.is_empty(), "these should not be exact duplicates");

  let exact_fps = group_fingerprints(&exact);
  let near = find_near_duplicates(&units, 0.5, &exact_fps);
  assert_eq!(near.len(), 1, "should find one near-duplicate group");
}

#[test]
fn similarity_score_identical_bodies() {
  let units = parse(
    r"
def foo(a, b):
    return a + b

def bar(x, y):
    return x + y
",
  );
  assert_eq!(units.len(), 2);
  let score = similarity_score(&units[0].body, &units[1].body);
  assert!(
    (score - 1.0).abs() < f64::EPSILON,
    "identical bodies should have score 1.0, got {score}"
  );
}

#[test]
fn similarity_score_different_bodies() {
  let units = parse(
    r"
def simple(a):
    return a

def complex(a, b, c):
    x = a + b
    y = x * c
    z = y - a
    return z
",
  );
  assert_eq!(units.len(), 2);
  let score = similarity_score(&units[0].body, &units[1].body);
  assert!(score < 0.5, "very different bodies should have low score, got {score}");
}

// jscpd:ignore-start

#[test]
fn compute_stats_with_exact_duplicates() {
  let units = parse(
    r"
def add(a, b):
    result = a + b
    return result

def add2(x, y):
    result = x + y
    return result
",
  );
  let exact = group_exact_duplicates(&units);
  let exact_fps = group_fingerprints(&exact);
  let near = find_near_duplicates(&units, 0.8, &exact_fps);
  let stats = compute_stats(&units, &exact, &near);
  assert!(stats.exact_duplicate_groups > 0);
  assert!(stats.exact_duplicate_units > 0);
  assert!(stats.exact_duplicate_lines > 0);
}

#[test]
fn compute_stats_no_duplicates() {
  let units = parse(
    r"
def add(a, b):
    return a + b

def mul(a, b):
    return a * b
",
  );
  let exact = group_exact_duplicates(&units);
  let exact_fps = group_fingerprints(&exact);
  let near = find_near_duplicates(&units, 0.8, &exact_fps);
  let stats = compute_stats(&units, &exact, &near);
  assert_eq!(stats.exact_duplicate_groups, 0);
  assert_eq!(stats.exact_duplicate_units, 0);
  assert_eq!(stats.exact_duplicate_lines, 0);
}

// jscpd:ignore-end

#[test]
fn is_test_code_through_trait() {
  let analyzer = PythonAnalyzer::new();
  let units = parse(
    r"
def test_something():
    assert 1 == 1

def regular():
    return 42
",
  );
  assert!(analyzer.is_test_code(&units[0]));
  assert!(!analyzer.is_test_code(&units[1]));
}

#[test]
fn analyze_end_to_end_with_exclude_tests() {
  let analyzer = PythonAnalyzer::new();
  let tmp = tempfile::TempDir::new().unwrap();
  let py_file = tmp.path().join("example.py");
  std::fs::write(
    &py_file,
    r"
def add(a, b):
    result = a + b
    return result

def add2(x, y):
    result = x + y
    return result

def test_add():
    assert add(1, 2) == 3

def test_add2():
    assert add2(1, 2) == 3
",
  )
  .unwrap();

  let files = vec![py_file];

  // Without excluding tests — use low thresholds so test functions are included
  let config_with_tests = dupes_core::config::Config {
    exclude_tests: false,
    min_nodes: 1,
    min_lines: 1,
    ..Default::default()
  };
  let result_with = dupes_core::analyze(&analyzer, &files, &config_with_tests).expect("analyze should succeed");
  let total_with = result_with.stats.total_code_units;

  // With excluding tests
  let config_no_tests = dupes_core::config::Config {
    exclude_tests: true,
    min_nodes: 1,
    min_lines: 1,
    ..Default::default()
  };
  let result_without = dupes_core::analyze(&analyzer, &files, &config_no_tests).expect("analyze should succeed");
  let total_without = result_without.stats.total_code_units;

  assert!(
    total_with > total_without,
    "excluding tests should reduce unit count: {total_with} vs {total_without}"
  );
}
