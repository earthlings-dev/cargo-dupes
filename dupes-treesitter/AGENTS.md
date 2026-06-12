# `dupes-treesitter` Agent Notes

This crate owns the language-neutral tree-sitter bridge used by non-`syn` analyzers. Read the workspace-level `AGENTS.md` first, then use these crate-specific constraints when editing files under `dupes-treesitter/`.

## Scope

- Keep this crate generic. Language behavior should come from `NodeMapping`, extraction queries, kind resolvers, and test detectors supplied by downstream crates.
- Use `normalizer.rs` for CST-to-`NormalizedNode` conversion, `mapping.rs` for table-driven configuration, and `extractor.rs` for query-based `CodeUnit` extraction.
- Python examples in tests are fixtures for the bridge; production Python mapping belongs in `dupes-python`.

## Design Constraints

- Preserve `NodeMapping` as the extension point. Prefer adding mapping capabilities over hard-coding language-specific node names.
- Keep error and unknown-node handling deterministic; malformed syntax should normalize to `Opaque` where the structure cannot be trusted.
- Respect tree-sitter 0.25+ query iteration details. `QueryMatches` uses `StreamingIterator`; use `while let Some(m) = matches.next()` where query streams are consumed.
- Be precise with source byte slices and line ranges; extraction uses tree-sitter byte and point positions directly.

## Testing

- Run `cargo test -p dupes-treesitter` for bridge changes.
- Run `cargo test -p dupes-python` when mapping or extraction behavior can affect Python analyzer output.
- Run `cargo clippy --workspace --all-targets -- -D warnings` before handoff when touching tests, mappings, or normalizer helpers.
