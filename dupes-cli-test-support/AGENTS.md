# `dupes-cli-test-support` Agent Notes

This workspace-internal crate (`publish = false`) owns the shared integration-test support for the `cargo-dupes` and `code-dupes` binaries. Read the workspace-level `AGENTS.md` first, then use these crate-specific constraints when editing files under `dupes-cli-test-support/`.

## Scope

- Keep every helper binary-agnostic: assertions take a `CommandFactory` (`fn() -> assert_cmd::Command`) so the same test body runs against both binaries.
- Own the command factories (`cargo_dupes`, `code_dupes`, `command_for_path`, `command_for_fixture`), fixture path resolution (`workspace_root`, `rust_fixture_path`, `code_fixture_path`), the stdout/JSON report helpers (`fixture_stdout`, `fixture_json`, `groups_of_dimension`, `group_containing_member`, `assert_member_needles_absent`, `suppressed_count_for_rule`, ...), and the `cli_support_tests!` macro that stamps the shared suite into both binaries' test crates.
- Keep binary-specific behavior out: language detection lives in `code-dupes/tests/language.rs`, and the detector-coverage pins and self-corpus gate live in `cargo-dupes/tests/`.

## Design Constraints

- Shared cases are data: `StdoutCase` consts stamped into helper fns via `stdout_case_helpers!`. Adding a case means adding the const, its stamped helper, and one line in each binary's `cli_support_tests!` block, so both binaries stay pinned to the same suite.
- Fixture paths resolve from the workspace root: shared Rust fixtures live under `cargo-dupes/tests/fixtures/`, `code-dupes`-local fixtures under `code-dupes/tests/fixtures/`. Use `temp_copy_fixture` for ignore-file workflows so tests never write into the repo's own fixtures.
- Prefer the JSON helpers for structural assertions; `report_fingerprint` scrapes the text report only because the ignore workflow consumes the fingerprint a user would copy from that output.

## Testing

- This crate has no tests of its own; both binaries' suites exercise it. Run `cargo test -p cargo-dupes --tests` and `cargo test -p code-dupes --tests` after any change here.
