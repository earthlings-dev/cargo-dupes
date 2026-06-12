# Changelog

## [0.3.0](https://github.com/earthlings-dev/cargo-dupes/compare/dupes-core-v0.2.1...dupes-core-v0.3.0) (2026-06-12)


### ⚠ BREAKING CHANGES

* reshape the analysis API around report-time suppression: CodeUnit, DuplicateGroup, DuplicationStats, AnalysisResult, and Config gain suppression surfaces, and stats totals now count the full extracted population, suppressed included

### Features

* add token and line detection dimensions with per-dimension config ([b172cd0](https://github.com/earthlings-dev/cargo-dupes/commit/b172cd08ab009ae5338af4a2c24c7e2582729955))
* add the suppression rule registry: 24 suppress/admit rules tag low-signal units and groups at report time instead of dropping them at extraction, recoverable via --show-suppressed, attributed per rule under -v, toggled via [suppress] config and --disable-rule/--enable-rule
* extract if-chain units linked to their branches and release branch groups to full visibility when the owning chains do not group as wholes
* admit declaration-stanza and builder-chain-run line windows, coalescing blank-separated declaration stanzas into one window space
* anchor one token window per blank-line-separated segment so identical duplicated segments always produce identical windows
* record member content fingerprints on ignore entries for drift resilience and suggest possible successor groups in cleanup --dry-run
* lower the default near-duplicate similarity threshold from 0.9 to 0.8 so structurally close pairs surface without flag tuning


### Bug Fixes

* lex Rust ticks as same-line char literals only so lifetimes, labels, and comment apostrophes cannot blind token segmentation
* strip line-window block comments quote-aware so quoted or line-commented markers cannot blind line segmentation


### Code Refactoring

* consolidate self-corpus duplication at the source: shared run splitting (runs.rs), config override helpers, and reporter test builders (output/test_support.rs)
* consolidate the behavior-keyword tables behind one BEHAVIOR_KEYWORDS constant shared by the token scorer and both line classifiers

## [0.2.1](https://github.com/mpecan/cargo-dupes/compare/dupes-core-v0.2.0...dupes-core-v0.2.1) (2026-02-17)


### Bug Fixes

* use is_excluded helper in scan_files instead of inline duplicate ([#34](https://github.com/mpecan/cargo-dupes/issues/34)) ([c812c2d](https://github.com/mpecan/cargo-dupes/commit/c812c2db37dc43abf82f332447ab1fbb3d7de4d0))

## [0.2.0](https://github.com/mpecan/cargo-dupes/compare/dupes-core-v0.1.5...dupes-core-v0.2.0) (2026-02-17)


### ⚠ BREAKING CHANGES

* add code-dupes multi-language CLI and extract shared CLI module ([#29](https://github.com/mpecan/cargo-dupes/issues/29))

### Features

* add code-dupes multi-language CLI and extract shared CLI module ([#29](https://github.com/mpecan/cargo-dupes/issues/29)) ([949001d](https://github.com/mpecan/cargo-dupes/commit/949001d47d73de0f93bc2682f7217168dd3be806))


### Bug Fixes

* **ci:** inline crate versions for release-please compatibility ([#31](https://github.com/mpecan/cargo-dupes/issues/31)) ([e48c5da](https://github.com/mpecan/cargo-dupes/commit/e48c5daa381ec3427fe0ae8fa2cb3de4147ff94d))
