# AGENTS.md — cargo-dupes

## Project Overview

`cargo-dupes` / `code-dupes` detects duplicate and near-duplicate code blocks across multiple languages. It works by normalizing AST into a custom representation where identifiers are replaced with positional placeholders and literal values are erased, then uses deterministic fingerprinting for exact duplicate detection and Dice coefficient tree comparison for near-duplicate detection. Generic token and line-window detection run alongside AST analysis so code/text duplication that does not fit a language AST is still visible.

Noise control has two layers: a global suppression-rule registry tags low-signal candidates at report time (never dropped; recoverable via `--show-suppressed`, attributed per rule under `-v` — the contract lives in `DETECTOR_REPORTABILITY.md`), and the per-project `.dupes-ignore.toml` registry filters adjudicated intentional duplicates after detection.

**Edition:** 2024, MSRV 1.96 (`rust-version` in the workspace `Cargo.toml`). Uses let chains natively.

## Workspace Structure

This is a Cargo workspace:

```
Cargo.toml                        # Workspace root (virtual manifest: members, shared deps, clippy pedantic/nursery lints)
dupes-core/                       # Language-agnostic core library (no syn dependency)
  src/
    lib.rs                        # AnalysisResult, analyze(), analyze_with_generic(), suppression partition + coverage
    analyzer.rs                   # LanguageAnalyzer trait
    node.rs                       # NormalizedNode struct { kind, children }, NodeKind, NormalizationContext
    fingerprint.rs                # Fingerprint (u64 blake3 prefix wrapper, hex serialization)
    similarity.rs                 # Dice coefficient tree comparison
    grouper.rs                    # Exact grouping (HashMap) + near-duplicate (union-find)
    extractor.rs                  # Sub-function extraction + trivial-shape suppression classifiers
    suppression.rs                # Suppression rule registry (RuleId, RULES, SuppressionPolicy)
    code_unit.rs                  # DetectionDimension, CodeUnit, CodeUnitKind (data types only)
    config.rs                     # Config + AnalysisConfig, loading from dupes.toml / Cargo.toml metadata
    cli.rs                        # Shared CLI types (CommonCliArgs, CliError, Command, CliOverrides, run_analysis)
    ignore.rs                     # .dupes-ignore.toml management
    scanner.rs                    # Gitignore-aware file discovery with glob excludes
    text_units.rs                 # Generic token / line-window units (quote profiles, window classifiers)
    runs.rs                       # Borrowed-slice run splitting shared by extraction and windowing
    error.rs                      # Error types via thiserror
    output/
      mod.rs                      # Reporter trait, ReportOptions, ReportSection
      text.rs                     # TextReporter
      json.rs                     # JsonReporter
      test_support.rs             # Test-only unit/group/stats/result builders
dupes-treesitter/                 # Tree-sitter normalization bridge (depends on dupes-core + tree-sitter)
  src/
    lib.rs                        # pub mod declarations
    analyzer.rs                   # TreeSitterAnalyzer implementing LanguageAnalyzer
    mapping.rs                    # NodeMapping table-driven config (builder pattern)
    normalizer.rs                 # normalize_ts_node(): tree-sitter CST → NormalizedNode
    extractor.rs                  # extract_code_units(): query-based CodeUnit extraction (functions, lambdas, classes)
  tests/
    python_integration.rs         # Test-only Python mapping for normalizer unit tests
dupes-python/                     # Python language analyzer (depends on dupes-treesitter + dupes-core)
  src/
    lib.rs                        # PythonAnalyzer wrapping TreeSitterAnalyzer, python_mapping(); extracts functions, lambdas, classes
  tests/
    integration.rs                # Python analyzer integration tests
    core_with_python_tests.rs     # Core pipeline tests using Python analyzer
dupes-rust/                       # Rust language analyzer (depends on syn + dupes-core)
  src/
    lib.rs                        # RustAnalyzer implementing LanguageAnalyzer
    normalizer/                   # syn AST → NormalizedNode conversion (mod.rs entry points, expr.rs, pat.rs, helpers.rs, tests.rs)
    parser.rs                     # CodeUnitExtractor + SubUnitExtractor via syn::visit::Visit
  tests/
    core_with_syn_tests.rs        # Core module tests that need syn to construct test data
cargo-dupes/                      # Cargo subcommand CLI (Rust only, depends on dupes-rust + dupes-core)
  src/
    main.rs                       # clap CLI with subcommands (cargo dupes ...)
  tests/
    check.rs, report.rs, options.rs, ignore.rs, sub_function.rs  # Shared CLI suite stamped from dupes-cli-test-support
    detector_coverage.rs          # Pins over the frozen detector_coverage fixture
    self_corpus.rs                # Self-corpus consolidation gate
    fixtures/                     # Minimal Rust fixture projects (incl. the frozen detector_coverage)
code-dupes/                       # Multi-language CLI (depends on dupes-rust + dupes-python + dupes-core)
  src/
    main.rs                       # clap CLI with --language flag and auto-detection
  tests/
    check.rs, report.rs, options.rs, ignore.rs, sub_function.rs  # Shared CLI suite stamped from dupes-cli-test-support
    language.rs                   # Language detection + Python CLI integration tests
    fixtures/                     # python_dupes, python_test_code, python_no_dupes, text_dupes
dupes-cli-test-support/           # Shared CLI test helpers (workspace-internal, publish = false)
  src/
    lib.rs                        # assert_cmd command factories, fixture paths, JSON report helpers, cli_support_tests! macro
```

## Build & Test

```sh
cargo build                       # Build all workspace members
cargo test                        # Run all workspace tests
cargo test -p dupes-core          # Core unit tests
cargo test -p dupes-treesitter    # Tree-sitter bridge tests (unit + Python-mapping integration)
cargo test -p dupes-python        # Python analyzer tests (integration + core pipeline + unit)
cargo test -p dupes-rust --lib    # Normalizer + parser + RustAnalyzer unit tests
cargo test -p dupes-rust --test core_with_syn_tests  # syn-dependent core tests
cargo test -p cargo-dupes --tests # CLI suite + detector-coverage pins + self-corpus gate
cargo test -p code-dupes --tests  # CLI suite + language detection
cargo clippy --workspace          # Lint (must be clean)
cargo fmt --all --check           # Format check
```

Pre-commit hooks (via cargo-husky) enforce `cargo clippy -- -D warnings` and `cargo fmt -- --check`.

## Architecture

### Analysis Pipeline

The CLIs (`cargo-dupes` for Rust, `code-dupes` for multi-language) orchestrate the pipeline:
1. Load config (`dupes_core::config`) and apply CLI overrides
2. Create language analyzer (`RustAnalyzer` or `PythonAnalyzer`)
3. Scan for source files (`dupes_core::scanner` — AST extensions from the analyzer, generic extensions for the token/line dimensions)
4. Analyze: parse + tag + group + partition + ignore-filter + stats (`dupes_core::analyze_with_generic()`)

```
scan_files → analyze_with_generic(analyzer, ast_files, generic_files, config) → AnalysisResult
               ├── analyzer.parse_file() / parse_sub_units() per AST file
               ├── text_units::extract() per generic file (token + line windows, pre-tagged)
               ├── filter test code (optional)
               └── analyze_units_with_generic()
                     ├── tag trivial top-level shapes (suppression rules)
                     ├── group exact + near duplicates per dimension
                     ├── classify sub-units, apply if-chain coverage
                     ├── merge shifted windows, overlap containment, AST coverage
                     ├── partition suppressed vs visible groups
                     ├── filter_ignored (visible and suppressed populations)
                     └── compute stats (incl. suppression + registry accounting)
```

Suppression is presentation policy, never extraction policy; the ordering guarantees (extract → tag → group → partition → ignore-filter → report) are specified in `DETECTOR_REPORTABILITY.md`.

### Module Map — dupes-core

| Module | File | Responsibility |
|--------|------|---------------|
| **analyzer** | `dupes-core/src/analyzer.rs` | `LanguageAnalyzer` trait (`Send + Sync`): `file_extensions()`, `parse_file()`, `is_test_code()`. Analyzers tag test code via `CodeUnit::is_test`; `analyze()` filters. |
| **node** | `dupes-core/src/node.rs` | `NormalizedNode` struct (data-driven `{ kind, children }`), `NodeKind` (incl. `Token`, `Yield`), `LiteralKind` (incl. `Null`), `BinOpKind` (incl. augmented assignments), `UnOpKind`, `NormalizationContext`, `count_nodes()`, `reindex_placeholders()`. |
| **fingerprint** | `dupes-core/src/fingerprint.rs` | `Fingerprint` struct wrapping a deterministic 64-bit `blake3` prefix. Supports hex serialization. |
| **code_unit** | `dupes-core/src/code_unit.rs` | `DetectionDimension`, `CodeUnit` (incl. `is_test`, `suppressed`, `parent_chain`), and `CodeUnitKind` (incl. `IfChain`, `TokenWindow`, `LineWindow`) — data types only, no parsing logic. |
| **similarity** | `dupes-core/src/similarity.rs` | Recursive tree comparison using Dice coefficient: `score = (2 * matching) / (nodes_a + nodes_b)`. |
| **grouper** | `dupes-core/src/grouper.rs` | Exact duplicate grouping via `HashMap<Fingerprint, Vec<CodeUnit>>`. Near-duplicate detection with size-bucket pre-filtering and union-find for transitive closure. Group fingerprints are scoped by dimension + match kind; `DuplicationStats` carries suppression and ignore-registry accounting. |
| **extractor** | `dupes-core/src/extractor.rs` | Sub-function extraction (`extract_sub_units`: if branches/chains, match arms, loop and closure bodies) plus the suppression classifiers (`classify_top_level_body`, `classify_closure_body`, `classify_sub_unit`) and the `TRIVIAL_BODY_MAX_NODES` size cap. |
| **suppression** | `dupes-core/src/suppression.rs` | Suppression rule registry: `RuleId`, the `RULES` table, and `SuppressionPolicy` with config/CLI toggles. Units and groups are tagged (never dropped), partitioned at report time, recoverable via `--show-suppressed`, attributed per rule under `-v`. |
| **scanner** | `dupes-core/src/scanner.rs` | Gitignore-aware file discovery. Skips `target` and hidden directories. Respects glob-style exclude patterns. Configurable file extensions. |
| **text_units** | `dupes-core/src/text_units.rs` | Generic normalized-token, raw-token, and line-window unit extraction: anchored one-window-per-segment token windows with per-extension quote lexing profiles (Rust `'` pairs only same-line char literals), sliding line windows with quote-aware block-comment stripping and declaration-stanza coalescing, and the window suppression/admission classifiers. |
| **config** | `dupes-core/src/config.rs` | `Config` loading: `dupes.toml` > `Cargo.toml [package.metadata.dupes]` > defaults, incl. the `[dimensions]`, `[token]`, `[line]`, and `[suppress]` tables. `AnalysisConfig` (min_nodes, min_lines) for the parsing-relevant subset. CLI overrides applied on top. |
| **cli** | `dupes-core/src/cli.rs` | Shared CLI surface: `CommonCliArgs` (global flags incl. `--show-suppressed`, `-v`, `--disable-rule`/`--enable-rule`), `CliError` (incl. `AmbiguousLanguage`), `Command`, `CliOverrides`, `OutputFormat`, `run_analysis()`, command implementations (`cmd_report`, `cmd_check`, etc.). |
| **ignore** | `dupes-core/src/ignore.rs` | TOML-based ignore file (`.dupes-ignore.toml`). Entries match by group fingerprint or by recorded member content fingerprints (drift-resilient). Stale entry detection and cleanup. |
| **error** | `dupes-core/src/error.rs` | Error types via `thiserror`. |
| **output** | `dupes-core/src/output/` | `Reporter` trait with `TextReporter` and `JsonReporter`. |

### Module Map — dupes-treesitter

| Module | File | Responsibility |
|--------|------|---------------|
| **TreeSitterAnalyzer** | `dupes-treesitter/src/analyzer.rs` | Generic `LanguageAnalyzer` impl using tree-sitter. Configured with `NodeMapping`, query, kind resolver (`with_kind_resolver`), test detector, extensions. |
| **mapping** | `dupes-treesitter/src/mapping.rs` | `NodeMapping` — table-driven config for tree-sitter normalization. Builder pattern with `identifiers()`, `literals()`, `binary_ops()`, `unary_ops()`, `node_kinds()`, etc. |
| **normalizer** | `dupes-treesitter/src/normalizer.rs` | `normalize_ts_node()`: tree-sitter CST → `NormalizedNode`. Handles identifiers, literals, binary/unary ops, node kinds, structural nodes. |
| **extractor** | `dupes-treesitter/src/extractor.rs` | `extract_code_units()`: query-based `CodeUnit` extraction from tree-sitter parse tree. `KindResolver` type alias for kind callback. |

### Module Map — dupes-python

| Module | File | Responsibility |
|--------|------|---------------|
| **PythonAnalyzer** | `dupes-python/src/lib.rs` | Thin wrapper around `TreeSitterAnalyzer` with Python-specific `NodeMapping`, tree-sitter query (functions, lambdas, classes), kind resolver, and `test_`/`Test` prefix test detection. `python_mapping()` is public for reuse. |

### Module Map — dupes-rust

| Module | File | Responsibility |
|--------|------|---------------|
| **RustAnalyzer** | `dupes-rust/src/lib.rs` | Implements `LanguageAnalyzer` for Rust. Delegates to `parser::parse_source()`. |
| **normalizer** | `dupes-rust/src/normalizer/` | syn AST → `NormalizedNode` conversion: `mod.rs` (entry points + signature normalization), `expr.rs` (expressions/statements/blocks), `pat.rs` (patterns/types), `helpers.rs` (placeholder/path/literal/operator/macro helpers). Method names are preserved as `Token` leaves (method-name preservation). |
| **parser** | `dupes-rust/src/parser.rs` | `CodeUnitExtractor` (top-level units) and `SubUnitExtractor` (sub-function units with precise spans, incl. if-chain units linked to their branches) using `syn::visit::Visit`; shared visitor behavior lives in stamped macros and `ImplNaming`. `parse_source()`/`parse_sub_units()` take `&str`, `parse_file()` reads from disk. |

### Module Map — cargo-dupes

| Module | File | Responsibility |
|--------|------|---------------|
| **CLI** | `cargo-dupes/src/main.rs` | `clap` derive CLI (Rust only). Subcommands: `stats`, `report` (default), `check`, `ignore`, `ignored`, `cleanup`. Uses `RustAnalyzer` + `dupes_core::cli::run_analysis()`. |

### Module Map — code-dupes

| Module | File | Responsibility |
|--------|------|---------------|
| **CLI** | `code-dupes/src/main.rs` | `clap` derive CLI (multi-language). `--language` flag with auto-detection from file extensions. Ambiguous detection (mixed languages) returns an error. Uses `dupes_core::cli::run_analysis()`. |

### Key Design: NormalizedNode

The `NormalizedNode` struct (`{ kind: NodeKind, children }`) provides a language-agnostic normalized AST:
- Replaces identifiers with `Placeholder(kind, positional_index)` — assigned by first-occurrence order — except method names, which stay opaque `Token` leaves (method-name preservation: `x.is_ascii_alphabetic()` never fingerprints equal to `x.is_ascii_alphanumeric()`)
- Preserves literal *kind* but erases *values* (`42` and `99` both become `Literal(Int)`)
- Preserves control flow structure exactly (if/match/loop/for)
- Maps language-specific constructs to shared `NodeKind` variants

This enables deterministic debug-form fingerprinting and recursive tree comparison for similarity scoring.

Two normalization backends exist:
- **syn-based** (dupes-rust): Direct Rust AST → `NormalizedNode` via syn's visitor pattern
- **tree-sitter-based** (dupes-treesitter): Generic CST → `NormalizedNode` via table-driven `NodeMapping`

### Key Types

- `LanguageAnalyzer` — Trait for language-specific parsing. Provides `file_extensions()`, `parse_file()`, `is_test_code()`.
- `AnalysisConfig` — Parsing-relevant config subset: `min_nodes`, `min_lines`.
- `CodeUnit` — A function, method, closure, class, impl block, token window, or line window extracted from source. Contains normalized signature + body, fingerprint, file location, line numbers, `is_test` flag, suppression tag (`suppressed`), and — for if-branch sub-units — the owning chain (`parent_chain`).
- `DuplicateGroup` — A group of code units with the same fingerprint (exact) or above the similarity threshold (near). Carries `dimension`, `match_kind`, a stable group fingerprint, an optional suppression tag, and `also_seen` cross-dimension coverage notes.
- `DuplicationStats` — Statistics including group/unit counts, duplicated line counts (exact and near), total lines, percentage helpers, and the suppression/ignored-group accounting. Totals cover the full extracted population, suppressed included.
- `Config` — All analysis parameters (min_nodes, min_lines, similarity_threshold, excludes, exclude_tests, dimension toggles, token/line window thresholds, the suppression policy, CI thresholds including percentage-based).
- `SuppressionPolicy` / `RuleId` — The active suppression/admission rule set: registry defaults plus `[suppress]` config and `--disable-rule`/`--enable-rule` CLI toggles.
- `KindResolver` — Type alias `Box<dyn Fn(&str) -> CodeUnitKind + Send + Sync>` for resolving code unit kind from tree-sitter node kind strings.
- `NodeMapping` — Table-driven configuration for tree-sitter normalization: identifier kinds, literal kinds, binary/unary op maps, node kinds, structural kinds.

## CLI Subcommands

Both `cargo-dupes` (Rust only) and `code-dupes` (multi-language) share the same subcommands:

- **`report`** (default) — Full report: stats + exact groups + near groups
- **`stats`** — Summary statistics only
- **`check`** — CI mode. Exits 1 if `--max-exact`, `--max-near`, `--max-exact-percent`, or `--max-near-percent` thresholds exceeded. Exits 0 on pass. Exits 2 on errors.
- **`ignore <fingerprint>`** — Add fingerprint to `.dupes-ignore.toml`
- **`ignored`** — List all ignored fingerprints
- **`cleanup`** — Remove stale entries from `.dupes-ignore.toml` (`--dry-run` lists them with possible successor groups)

Shared global flags cover the analysis thresholds, dimension toggles (`--dimension`/`--disable-dimension`), token/line window settings, and the suppression surface: `--show-suppressed` (render tagged groups), `-v` (per-rule attribution in stats), and `--disable-rule`/`--enable-rule` (toggle registry rules). `code-dupes` adds a `--language` flag (auto-detected from file extensions if omitted).

## Testing

- **dupes-core unit tests** — Colocated in each module (`#[cfg(test)] mod tests`). No syn dependency.
- **dupes-treesitter tests** — Unit tests plus integration tests (Python normalization).
- **dupes-python tests** — Integration tests, core pipeline tests, and unit tests.
- **dupes-rust unit tests** — Colocated in `normalizer/tests.rs`, `parser.rs`, and `lib.rs`. Use `syn::parse_str` to construct test data.
- **syn-dependent core tests** — In `dupes-rust/tests/core_with_syn_tests.rs`. Tests for grouper, extractor, similarity, fingerprint that require syn to build realistic test data.
- **dupes-cli-test-support** — Shared `assert_cmd` command factories, fixture paths, JSON report helpers (`fixture_json`, `groups_of_dimension`, `group_containing_member`, `suppressed_count_for_rule`, ...), and the `cli_support_tests!` macro that stamps the shared CLI suite into both binaries' test crates.
- **cargo-dupes CLI tests** — In `cargo-dupes/tests/`: the shared suite (`check`/`report`/`options`/`ignore`/`sub_function`) plus the detector-coverage and self-corpus gates below.
- **Detector-coverage harness** — `cargo-dupes/tests/detector_coverage.rs` over the frozen `tests/fixtures/detector_coverage/` project: pins stats totals, per-dimension visible groups, and the exact per-rule suppression attribution map; pins are updated together with any detector change.
- **Self-corpus gate** — `cargo-dupes/tests/self_corpus.rs` (`consolidated_sites_stay_consolidated`): the workspace's own code, excluding `refactor/` and both fixture trees, must never re-group the consolidated dupes-core sites.
- **code-dupes CLI tests** — In `code-dupes/tests/`: the shared suite plus `language.rs`. Covers language detection, Python analysis (functions, lambdas, classes), ambiguous detection, and generic scanning behavior.
- **Fixtures** — `cargo-dupes/tests/fixtures/` (minimal Rust projects, including the frozen `detector_coverage`), `code-dupes/tests/fixtures/` (`python_dupes`, `python_test_code`, `python_no_dupes`, plus the generic-text `text_dupes`).

## jscpd Cross-Check

The workspace is additionally kept clean under jscpd, an external copy/paste detector, as an independent check on the self-corpus. `.jscpd.json` configures it; reports land in the untracked `cov/` output directory, and both CLI fixture trees are excluded there rather than wrapped in markers.

Clone sites adjudicated as intentional are wrapped in `// jscpd:ignore-start` / `// jscpd:ignore-end` comment markers at the source. Placement rule: every marker line stays blank-line-isolated (one blank line above and below it). Markers are comments, so AST extraction never sees them, and an isolated marker forms its own one-line segment in this project's token/line windowing — wrapping a site leaves every neighboring window's fingerprint unchanged and cannot stale `.dupes-ignore.toml` entries. A marker placed flush against code would join that code's segment and re-cut its windows.

## syn 2 Gotchas

- `Pat::Lit` wraps `ExprLit` which has a `lit: Lit` field (not `expr: Expr`)
- `Member` does not implement `Display`; use a match to extract ident/index strings
- `ExprMatch` arm guards are `Option<(If, Box<Expr>)>` — a tuple, not just `Option<Box<Expr>>`
- `true`/`false` are parsed as path expressions (become `Placeholder`), not as `Lit::Bool`

## tree-sitter Gotchas

- tree-sitter 0.25 `QueryMatches` uses `StreamingIterator` not `Iterator`; import via `tree_sitter::StreamingIterator`
- Use `while let Some(m) = matches.next()` instead of `for m in matches`
- Python `for_statement` uses `left`/`right` fields (not `pattern`/`iterable`)
- Python `comparison_operator` lacks `left`/`right` field names — uses positional children instead

## Platform Notes

- macOS `TempDir` paths may have components that look like hidden directories; the scanner skips the hidden-directory filter for the root path to avoid false filtering in tests.

## Commit Messages

Subject line: `type(scope): structural imperative description` — conventional-commit style with a **required scope** (broad is fine: `workspace`, `detection`, `report`). Never use `chore` as the type; pick a descriptive type (`feat`, `fix`, `refactor`, `perf`, `build`, `ci`, `docs`, `test`, `style`, `revert`, ...) even for janitorial work. Append `!` after the scope for breaking changes. Describe the structural change in imperative mood, not the narrative.

Body: 1–5 sections matched to the size and breadth of the commit. Each section starts with a plain-text header line (no `#` headings, no bold), followed by 3–5 imperative bullets each describing a structural change (introduced, replaced, removed, renamed, rewired). Separate sections with exactly one blank line. Don't pad small commits with empty sections; don't compress a large change into one section.

Mechanics: pass the message via HEREDOC to `git commit -m` so blank lines and bullet spacing survive shell quoting.
