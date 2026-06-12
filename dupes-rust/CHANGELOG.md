# Changelog

## [0.3.0](https://github.com/earthlings-dev/cargo-dupes/compare/dupes-rust-v0.2.1...dupes-rust-v0.3.0) (2026-06-12)


### ⚠ BREAKING CHANGES

* preserve method names as Token leaves in MethodCall normalization — unit fingerprints are redefined and existing .dupes-ignore.toml registries require the migration procedure in dupes-core/AGENTS.md

### Features

* add token and line detection dimensions with per-dimension config ([b172cd0](https://github.com/earthlings-dev/cargo-dupes/commit/b172cd08ab009ae5338af4a2c24c7e2582729955))
* preserve method names in normalized AST so differently named method calls never fingerprint as exact duplicates
* extract if-chain units with parent_chain branch linkage and emit trivial top-level shapes unconditionally for report-time suppression tagging


### Code Refactoring

* consolidate behavior shared between CodeUnitExtractor and SubUnitExtractor into stamped visitor macros and the ImplNaming helper
* consolidate the syn parse entry points behind a shared parse_syn_file helper


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * dupes-core bumped from 0.2.1 to 0.3.0

## [0.2.1](https://github.com/mpecan/cargo-dupes/compare/dupes-rust-v0.2.0...dupes-rust-v0.2.1) (2026-02-17)


### Miscellaneous

* **dupes-rust:** Synchronize workspace versions


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * dupes-core bumped from 0.2.0 to 0.2.1

## [0.2.0](https://github.com/mpecan/cargo-dupes/compare/dupes-rust-v0.1.5...dupes-rust-v0.2.0) (2026-02-17)


### Bug Fixes

* **ci:** inline crate versions for release-please compatibility ([#31](https://github.com/mpecan/cargo-dupes/issues/31)) ([e48c5da](https://github.com/mpecan/cargo-dupes/commit/e48c5daa381ec3427fe0ae8fa2cb3de4147ff94d))


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * dupes-core bumped from 0.1.5 to 0.2.0
