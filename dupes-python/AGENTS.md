# `dupes-python` Agent Notes

This crate owns the Python analyzer built on top of `dupes-treesitter`. Read the workspace-level `AGENTS.md` first, then use these crate-specific constraints when editing files under `dupes-python/`.

## Scope

- Keep this crate as the Python-specific wrapper: `PythonAnalyzer`, `python_mapping()`, the tree-sitter query, kind resolution, and Python test-code detection.
- Put generic tree-sitter behavior in `dupes-treesitter`; put language-agnostic grouping, config, and reporting in `dupes-core`.
- Keep Python fixture expectations in `tests/integration.rs` and cross-pipeline behavior in `tests/core_with_python_tests.rs`.

## Design Constraints

- Preserve normalization equivalence for renamed variables while distinguishing Python constructs that change behavior, such as augmented assignments, `None`, containers, decorators, and comparison operators.
- Keep function, lambda, class, and method extraction expectations explicit; tests should say whether they assert a unit count, a fingerprint relation, or test-code tagging.
- Treat `test_` functions and `Test` classes as test code consistently with `PythonAnalyzer::is_test_code`.
- Prefer extending `python_mapping()` over special-casing Python syntax in the tree-sitter bridge.

## Testing

- Run `cargo test -p dupes-python` for Python analyzer changes.
- Run `cargo test -p code-dupes --tests` when Python behavior changes multi-language CLI output.
- Run `cargo test -p dupes-treesitter` if a Python fix requires generic bridge changes.
