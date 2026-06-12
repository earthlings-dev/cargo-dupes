# `code-dupes` Agent Notes

This crate owns the multi-language CLI binary. Read the workspace-level `AGENTS.md` first, then use these crate-specific constraints when editing files under `code-dupes/`.

## Scope

- Keep this binary focused on CLI argument parsing, language detection, analyzer selection, and wiring into `dupes_core::cli::run_analysis`.
- Put shared command behavior, output formats, thresholds, ignore-file commands, and reporter logic in `dupes-core`.
- Keep Rust-only Cargo subcommand behavior in `cargo-dupes`.

## Design Constraints

- Preserve language auto-detection semantics: a single detected language should run automatically, mixed known languages should produce the ambiguous-language error, and generic files should still flow through generic detection where supported.
- Keep `--language` behavior explicit and predictable; analyzer construction should remain easy to audit when adding languages.
- Add or update fixtures under `tests/fixtures/` when behavior depends on file extensions, mixed-language trees, Python extraction, or generic scanning.

## Testing

- Run `cargo test -p code-dupes --tests` for CLI and language-detection changes.
- Run analyzer-specific tests, such as `cargo test -p dupes-python`, when changing language behavior surfaced by this binary.
- Run `cargo clippy --workspace --all-targets -- -D warnings` before handing off lint-sensitive CLI work.
