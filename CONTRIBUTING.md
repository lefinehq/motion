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
