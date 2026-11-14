# Contributing

Thank you for helping. This project aims to be a codebase people enjoy reading, so small, clear changes with tests are the best kind.

## Before you commit

All three must pass:

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs the same commands on Linux and macOS.

## Where tests live

There is no test code in `src/`. Unit tests for `crates/<c>/src/<path>.rs` live in `crates/<c>/tests/unit/<path>.rs`. The source file ends with a child module that points at them, so the tests can still reach private items:

```rust
#[cfg(test)]
#[path = "../tests/unit/curve.rs"]
mod tests;
```

The path is relative to the source file's directory. There is no `tests/unit/main.rs`, so Cargo does not build the folder as a test target. Tests that use only the public API (such as the CLI tests) stay directly in `tests/`. `include_str!` paths are relative to the test file.

## Tools

- **Coverage:** `cargo install cargo-llvm-cov`, then `cargo llvm-cov --workspace --fail-under-lines 85`. CI fails below 85% line coverage.
- **Licences and advisories:** `cargo install cargo-deny --locked`, then `cargo deny check`. The policy is in `deny.toml`: permissive licences only.
- **Property tests:** `proptest` checks that every parser of user text never panics on arbitrary strings, and that printing then parsing gives back the same value.
- **Snapshot tests:** `insta` is for tests of text output. Run `cargo insta review` (from `cargo install cargo-insta`) to accept or reject changed snapshots, and commit the `.snap` files.

## Logs and timings

Logs go to stderr, never stdout, so command output stays clean for agents.

- **Enable them** with `--log <level>` (default `warn`), or with `MOTION_LOG`, which takes `EnvFilter` directives such as `MOTION_LOG=warn,motion_query=debug` and wins over `--log`. Add `--log-format json` for one JSON object per line.
- **Time the steps** with `--timings`, which prints a line such as `timings read=0.05ms parse=0.40ms compile=0.02ms check=0.45ms` to stderr at the end.
- **Emit them** with the `tracing` macros only. Library crates never print and never install a subscriber; `motion-telemetry` does that for binaries and tests. Every engine step runs through `motion_engine::stage`, which opens the step's span and logs its failure, so a step never logs its own errors.
- **Name fields** in `snake_case`, with counts as plain names (`layers`, `violations`) and a layer id as `id`.
- **Pick the level** by who needs it:
  - `error`: failures shown to the user
  - `warn`: suspicious input
  - `info`: one line per command
  - `debug`: per step
  - `trace`: per frame

## Rules

- **Every change comes with tests.** New behaviour gets a test that fails without it; a bug fix gets a test that reproduces the bug. Tests ship in the same commit as the code.
- **Document the code.** Every crate and module starts with a `//!` doc saying what it is for and where it sits in the pipeline. Every public item has a `///` doc. Non-obvious logic gets a short comment saying why.
- **Use our own vocabulary.** We do not use other products' names or trademarks in code, identifiers, docs or examples. Other tools may be named only in factual, reproducible benchmark comparisons. See [D18](docs/design/decisions.md#d18-naming-rule).
- **Write docs the way we write docs.** See [How we write docs](docs/README.md#how-we-write-docs).
- **Print numbers with `motion_format::fmt_num`** so output is stable.

## Commits

- Use [Conventional Commits](https://www.conventionalcommits.org/) in English, for example `feat: add colour parsing` or `fix: reject negative times`.
- Sign your commits (`git commit -S`).
- Keep each commit to one task, with its tests.
