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

## How errors work

Every failure in every crate is one type, `motion_diagnostic::Diagnostic`. It prints as `path: message; expected form`, where the path and the expected form are each optional:

```
layers[1].rect: `300xx` is not a valid rectangle; expected `<width>x<height> [r<radius>]`, e.g. `300x400 r24`
```

Build one with the builder, never with `format!` or a helper of your own:

```rust
use motion_diagnostic::{Diagnostic, Kind};

Diagnostic::syntax("`300xx` is not a valid rectangle").expected("`<width>x<height>`, e.g. `300x400`");
Diagnostic::unknown("curve", "fast", ["linear", "ease-out"]); // lists the allowed names
Diagnostic::new(Kind::Conflict, "`font` only applies to text layers").at("font");
```

- **`syntax`, `unknown`, `new(kind, …)`, `io`** start a diagnostic. `unknown` writes the list of allowed names itself, so pass the names from their table (names are data, [D27](docs/design/decisions.md#d27-one-generic-property-model)).
- **`expected`** sets the accepted form, with an example. `or_expected` adds another form to it.
- **`at`** sets the path where the error happened. **`under`** puts an outer path in front of it, so a nested parser reports its own place and the caller adds the key: `parse().map_err(|e| e.under("rect"))`.
- **Every `Syntax` and `UnknownName` error has an expected form.** A test for a new error should check it.
- **`Kind`** says what went wrong: `Syntax`, `UnknownName`, `OutOfRange`, `Missing`, `Conflict`, `WrongType`, `Invalid` or `Io`. `Kind::exit_code()` is the one place that maps it to the command's exit code, which is 2 for every kind today. (`check` exits 1 when it finds violations, which are results, not errors.)

## Adding a shorthand

A shorthand is a short string such as `"300x400 r24"`. Write it as a small `winnow` grammar, a `FromStr` impl that runs it, and a `Form` impl. Never read it character by character.

```rust
impl FromStr for RectShape {
    type Err = Diagnostic;
    fn from_str(text: &str) -> Result<RectShape> {
        parse_whole(text, rect, "rectangle", EXPECTED)
    }
}
impl Form for RectShape {
    const FORM: &str = EXPECTED; // shown when the value is not a string
}
```

- Build the grammar from the pieces in `motion-grammar` (`number`, `word`, `blank0`, `blank1`, `ends_with`). They read numbers and spaces the same way everywhere.
- `parse_whole` fits a grammar with one kind of failure. When failures differ, give the grammar one private reason enum, raise a reason where it fails, and turn it into a `Diagnostic` once with `parse_all` (see `shorthand/font.rs`).
- Use the type as `Shorthand<T>` in `doc.rs`. serde reads it from the document for free, and a value that is not a string is told `T::FORM`.
- Add unit tests with the exact error texts, and a never-panic `proptest`.

## Adding a field

1. **Document:** add the field to `SceneDoc` or `LayerDoc` in `crates/motion-format/src/doc.rs`, as `Option<Shorthand<T>>` (or a plain type), with a `///` doc. It stays `Option` so that `validate` can report a missing key, with an example.
2. **Key:** add the key's spelling to `parse::key` and to `SCENE_KEYS` or `LAYER_KEYS`, in the same order as the fields. A unit test keeps them in step.
3. **Rule:** read it in `crates/motion-format/src/validate.rs`, the one place that checks values and rules that span several keys. Parse a shorthand with `parse_at(key)`, which puts the key in the error path, and return a `Diagnostic` for any rule that fails:

   ```rust
   fn font_only_on_text(doc: &LayerDoc) -> Result<()> {
       match doc.font {
           None => Ok(()),
           Some(_) => Err(Diagnostic::new(Kind::Conflict, "`font` only applies to text layers")
               .at(FONT)
               .expected("`font` only next to `text`")),
       }
   }
   ```

4. **Test:** add a test in `crates/motion-format/tests/unit/validate.rs` for the good value and for each error, with the exact text.

Edits already reach the new key: `card fill=#FF0000` and nested keys such as `card anim.opacity="0 -> 1 @0-12f"` write into the document, and `validate` checks the result.

## Logs and timings

Logs go to stderr, never stdout, so command output stays clean for agents. They are off by default.

- **Enable them** with `--log <level>` (default `off`, so a plain failing run prints only its `error:` line), or with `MOTION_LOG`, which takes `EnvFilter` directives such as `MOTION_LOG=warn,motion_query=debug` and wins over `--log`. An invalid `MOTION_LOG` is ignored with one `WARN` log line. Add `--log-format json` for one JSON object per line.
- **Time the steps** with `--timings`, which prints a line such as `timings read=0.05ms parse=0.40ms compile=0.02ms check=0.45ms` to stderr at the end. With `--log-format json` the timings are one `info` event instead (target `motion::timings`, a field per step in milliseconds).
- **Emit them** with the `tracing` macros only. Library crates never print and never install a subscriber; `motion-telemetry` does that for binaries and tests. Every engine step runs through the engine's private `stage` wrapper (`crates/motion-engine/src/stage.rs`), which opens the step's span and logs its failure, so a step never logs its own errors. The timings recognise a step by its span's target and name, not by a field.
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
