# Changelog

## [0.3.0](https://github.com/earthlings-dev/cargo-dupes/compare/code-dupes-v0.2.1...code-dupes-v0.3.0) (2026-06-12)


### ⚠ BREAKING CHANGES

* method-name preservation redefines unit fingerprints — existing .dupes-ignore.toml entries go stale and need the migration procedure in dupes-core/AGENTS.md

### Features

* add token and line detection dimensions with per-dimension config ([b172cd0](https://github.com/earthlings-dev/cargo-dupes/commit/b172cd08ab009ae5338af4a2c24c7e2582729955))
* surface the suppression registry: --show-suppressed report sections, -v per-rule attribution, and repeatable --disable-rule/--enable-rule toggles
* lower the default near-duplicate similarity threshold from 0.9 to 0.8


### Tests

* stamp the shared CLI suite from the new workspace-internal dupes-cli-test-support crate and add the text_dupes generic-text fixture


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * dupes-core bumped from 0.2.1 to 0.3.0
    * dupes-python bumped from 0.2.1 to 0.3.0
    * dupes-rust bumped from 0.2.1 to 0.3.0

## [0.2.1](https://github.com/mpecan/cargo-dupes/compare/code-dupes-v0.2.0...code-dupes-v0.2.1) (2026-02-17)


### Miscellaneous

* **code-dupes:** Synchronize workspace versions


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * dupes-core bumped from 0.2.0 to 0.2.1
    * dupes-rust bumped from 0.2.0 to 0.2.1

## [0.2.0](https://github.com/mpecan/cargo-dupes/compare/code-dupes-v0.1.5...code-dupes-v0.2.0) (2026-02-17)


### Bug Fixes

* **ci:** inline crate versions for release-please compatibility ([#31](https://github.com/mpecan/cargo-dupes/issues/31)) ([e48c5da](https://github.com/mpecan/cargo-dupes/commit/e48c5daa381ec3427fe0ae8fa2cb3de4147ff94d))


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * dupes-core bumped from 0.1.5 to 0.2.0
    * dupes-rust bumped from 0.1.5 to 0.2.0
