# Changelog
All notable changes to this project will be documented in this file.
See [conventional commits](https://www.conventionalcommits.org/) for commit guidelines.

---
## [0.3.0](https://github.com/earthlings-dev/cargo-dupes/compare/cargo-dupes-v0.2.1...cargo-dupes-v0.3.0) (2026-06-12)


### ⚠ BREAKING CHANGES

* preserve method names as Token leaves in MethodCall normalization — unit fingerprints are redefined and existing .dupes-ignore.toml registries require the migration procedure in dupes-core/AGENTS.md
* reshape the analysis API around report-time suppression: CodeUnit, DuplicateGroup, DuplicationStats, AnalysisResult, and Config gain suppression surfaces, and stats totals now count the full extracted population, suppressed included

### Features

* add dupes-treesitter crate for tree-sitter language support ([#36](https://github.com/mpecan/cargo-dupes/issues/36)) ([1c21c37](https://github.com/mpecan/cargo-dupes/commit/1c21c37f7821147a7b65e3828539650e1de1b279))
* add dupes-python crate for Python duplicate detection ([#39](https://github.com/mpecan/cargo-dupes/issues/39)) ([056f9b9](https://github.com/mpecan/cargo-dupes/commit/056f9b9137880b64dcbf5ef7d9ae3956495480ff))
* extend Python extraction to cover lambdas and class bodies ([#42](https://github.com/mpecan/cargo-dupes/issues/42)) ([6f5a8c8](https://github.com/mpecan/cargo-dupes/commit/6f5a8c89364c3f70aea0bd2aabbb4c266151cf53))
* add token and line detection dimensions with per-dimension config ([b172cd0](https://github.com/earthlings-dev/cargo-dupes/commit/b172cd08ab009ae5338af4a2c24c7e2582729955))
* add the suppression rule registry: 24 suppress/admit rules tag low-signal units and groups at report time instead of dropping them at extraction, recoverable via --show-suppressed, attributed per rule under -v, toggled via [suppress] config and --disable-rule/--enable-rule
* preserve method names in normalized AST so differently named method calls never fingerprint as exact duplicates
* extract if-chain units linked to their branches and release branch groups to full visibility when the owning chains do not group as wholes
* admit declaration-stanza and builder-chain-run line windows, coalescing blank-separated declaration stanzas into one window space
* anchor one token window per blank-line-separated segment so identical duplicated segments always produce identical windows
* record member content fingerprints on ignore entries for drift resilience and suggest possible successor groups in cleanup --dry-run
* lower the default near-duplicate similarity threshold from 0.9 to 0.8 so structurally close pairs surface without flag tuning


### Bug Fixes

* lex Rust ticks as same-line char literals only so lifetimes, labels, and comment apostrophes cannot blind token segmentation
* strip line-window block comments quote-aware so quoted or line-commented markers cannot blind line segmentation


### Code Refactoring

* consolidate self-corpus duplication at the source: shared extractor visitor macros, run splitting, config override helpers, and reporter test builders
* consolidate the behavior-keyword tables behind one shared constant and the syn parse entry points behind a shared parse helper


### Tests

* add the frozen detector_coverage fixture with per-rule pins, the self-corpus consolidation gate, and the shared dupes-cli-test-support CLI suite

## [0.2.1](https://github.com/mpecan/cargo-dupes/compare/cargo-dupes-v0.2.0...cargo-dupes-v0.2.1) (2026-02-17)


### Bug Fixes

* **ci:** only trigger publish on dupes-core tag, fix crates.io API calls ([#33](https://github.com/mpecan/cargo-dupes/issues/33)) ([e5c802f](https://github.com/mpecan/cargo-dupes/commit/e5c802fb8a7aeb1e35dfc5ebd0089bb0319273e9))
* use is_excluded helper in scan_files instead of inline duplicate ([#34](https://github.com/mpecan/cargo-dupes/issues/34)) ([c812c2d](https://github.com/mpecan/cargo-dupes/commit/c812c2db37dc43abf82f332447ab1fbb3d7de4d0))

## [0.2.0](https://github.com/mpecan/cargo-dupes/compare/cargo-dupes-v0.1.5...cargo-dupes-v0.2.0) (2026-02-17)


### ⚠ BREAKING CHANGES

* add code-dupes multi-language CLI and extract shared CLI module ([#29](https://github.com/mpecan/cargo-dupes/issues/29))
* enable pedantic lints, split large files, add file size CI gate ([#28](https://github.com/mpecan/cargo-dupes/issues/28))
* flatten NormalizedNode into data-driven { kind, children } struct ([#26](https://github.com/mpecan/cargo-dupes/issues/26))

### Features

* add code-dupes multi-language CLI and extract shared CLI module ([#29](https://github.com/mpecan/cargo-dupes/issues/29)) ([949001d](https://github.com/mpecan/cargo-dupes/commit/949001d47d73de0f93bc2682f7217168dd3be806))
* add sub-function duplicate detection ([#14](https://github.com/mpecan/cargo-dupes/issues/14)) ([502e2c7](https://github.com/mpecan/cargo-dupes/commit/502e2c7243d2c7c8d6039b69fe7aa1a664325955))
* introduce LanguageAnalyzer trait and extract dupes-rust crate ([#27](https://github.com/mpecan/cargo-dupes/issues/27)) ([953d8dd](https://github.com/mpecan/cargo-dupes/commit/953d8ddff7e2e0dd94426d62e68a0429b11066d0))
* replace opaque macro handling with MacroCall variant ([#13](https://github.com/mpecan/cargo-dupes/issues/13)) ([c396d64](https://github.com/mpecan/cargo-dupes/commit/c396d64cf6d6737f3437f5b5796dad516bd67c1f))


### Bug Fixes

* **ci:** add all workspace crates to release-please config ([#30](https://github.com/mpecan/cargo-dupes/issues/30)) ([525484f](https://github.com/mpecan/cargo-dupes/commit/525484f6343c86d7c78a7c939a834c7cac36a4c2))
* **ci:** inline crate versions for release-please compatibility ([#31](https://github.com/mpecan/cargo-dupes/issues/31)) ([e48c5da](https://github.com/mpecan/cargo-dupes/commit/e48c5daa381ec3427fe0ae8fa2cb3de4147ff94d))
* enable pedantic lints, split large files, add file size CI gate ([#28](https://github.com/mpecan/cargo-dupes/issues/28)) ([327fdca](https://github.com/mpecan/cargo-dupes/commit/327fdca544aff77a3d042ebcaf31e982615f354e))


### Code Refactoring

* flatten NormalizedNode into data-driven { kind, children } struct ([#26](https://github.com/mpecan/cargo-dupes/issues/26)) ([e871f36](https://github.com/mpecan/cargo-dupes/commit/e871f36cfd6c9c23c183222035f67208c6eb324e))
* split into workspace with dupes-core and cargo-dupes crates ([#24](https://github.com/mpecan/cargo-dupes/issues/24)) ([475c3f1](https://github.com/mpecan/cargo-dupes/commit/475c3f143b42aa8974f092799000342543fb6329))

## [0.1.5](https://github.com/mpecan/cargo-dupes/compare/cargo-dupes-v0.1.4...cargo-dupes-v0.1.5) (2026-02-13)


### Features

* composite fingerprints for near-duplicate groups + cleanup command ([#11](https://github.com/mpecan/cargo-dupes/issues/11)) ([2e49add](https://github.com/mpecan/cargo-dupes/commit/2e49add086a412c6debfb34f77bc1dbe5087c519))

## [0.1.4](https://github.com/mpecan/cargo-dupes/compare/cargo-dupes-v0.1.3...cargo-dupes-v0.1.4) (2026-02-11)


### Features

* add percentage-based duplication thresholds to check ([#8](https://github.com/mpecan/cargo-dupes/issues/8)) ([f528953](https://github.com/mpecan/cargo-dupes/commit/f528953f0ebb1aeb1d67773094ca1012ebf45657))


### Bug Fixes

* make all check thresholds default to disabled when not set ([#10](https://github.com/mpecan/cargo-dupes/issues/10)) ([de3743f](https://github.com/mpecan/cargo-dupes/commit/de3743f2615b9d4bc3a1f3bd077a3bbaa49efeae))

## [0.1.3](https://github.com/mpecan/cargo-dupes/compare/cargo-dupes-v0.1.2...cargo-dupes-v0.1.3) (2026-02-11)


### Features

* add --exclude-tests flag to filter out test code ([#2](https://github.com/mpecan/cargo-dupes/issues/2)) ([c116c95](https://github.com/mpecan/cargo-dupes/commit/c116c959197373664c470ea6edb00dba407c1f04))

## [0.1.2](https://github.com/mpecan/cargo-dupes/compare/cargo-dupes-v0.1.1...cargo-dupes-v0.1.2) (2026-02-11)


### Bug Fixes

* **ci:** trigger release on GitHub release instead of tag push ([#5](https://github.com/mpecan/cargo-dupes/issues/5)) ([2bc5059](https://github.com/mpecan/cargo-dupes/commit/2bc505956df8ce4b8a78e461aa038e7c1c07fc06))

## [0.1.1](https://github.com/mpecan/cargo-dupes/compare/cargo-dupes-v0.1.0...cargo-dupes-v0.1.1) (2026-02-11)


### Features

* add --min-lines filter and duplicated line statistics ([7de772b](https://github.com/mpecan/cargo-dupes/commit/7de772bda588c92d71341be0b50edfeddd26ccb5))
* add --min-lines filter and duplicated line statistics ([c47dd33](https://github.com/mpecan/cargo-dupes/commit/c47dd3348799a9f735122b29de2af92c49c7b02c))
* initial implementation of cargo-dupes ([5c8b4c1](https://github.com/mpecan/cargo-dupes/commit/5c8b4c1c47eb1a1c89a380e44a7adb53d24cb905))


### Bug Fixes

* **ci:** upgrade cocogitto-action from v3 to v4 ([b5621a6](https://github.com/mpecan/cargo-dupes/commit/b5621a66042da23cc8936d4070b4b15a975c491f))


### Documentation

* update CLAUDE.md for --min-lines and line stats ([faac27b](https://github.com/mpecan/cargo-dupes/commit/faac27b1fb913511d33a3249ff912478799d6c3b))
* update README with --min-lines flag and line stats output ([c21ec7c](https://github.com/mpecan/cargo-dupes/commit/c21ec7c9daeccfef6bc8734f6dc7b06b157ddd68))

## [0.1.0] - 2026-02-10

### Initial Release
- AST-based duplicate and near-duplicate code detection
- Text and JSON output formats
- CLI with report, stats, check, ignore, and ignored subcommands
- Configuration via dupes.toml or Cargo.toml metadata
