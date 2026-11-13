# Idiomatic parsing and errors: implementation plan

**Goal:** replace hand-written parsing and the per-module error helpers with standard Rust tools:
- **One diagnostic type** shared by every crate.
- **`serde` derive** for the scene document, with paths from `serde_path_to_error`.
- **`winnow` grammars** for the shorthands and the edit language.
- **One validation pass** for the rules that types cannot express.

**Why:**
- `parse.rs`, `anim.rs` and `preset.rs` walk `serde_json::Value` by hand, with if/else chains.
- Every shorthand has its own tokenizer, with local `err`/`bad` closures.
- The edit tokenizer is a 380-line char loop.
- There are four unrelated error types, and the CLI turns errors into `String`s.

This is hard to read and hard to extend, and each new field or shorthand repeats the same code.

**Architecture:**
- **`motion-diagnostic`** (new crate) holds `Diagnostic`, `Kind` and `Result<T>`. Every fallible public function in the workspace returns `motion_diagnostic::Result`. The CLI prints a diagnostic and takes its exit code from the `Kind`.
- **Shorthands:**
  - Each shorthand type implements `FromStr<Err = Diagnostic>`, using a `winnow` grammar in its own module.
  - One generic adapter lets serde deserialize any `FromStr` type, so the document structs can use the shorthand types directly as field types.
- **Document:** `doc.rs` holds `#[derive(Deserialize)] #[serde(deny_unknown_fields)]` structs that mirror the JSON. `serde_path_to_error` supplies paths such as `layers[1].fill`. A track (`anim.<prop>`) has a small custom `Deserialize` that accepts a string or a key list.
- **Validation:** `validate.rs` turns the document structs into the typed `Scene`. It holds every cross-field rule:
  - `motion` = 1 and `fps` > 0
  - exactly one of `text` or `rect`
  - `font` only on text
  - unique ids
  - presets exist
  - `in`/`anim` conflicts
  - value ranges
- **Edits:** a `winnow` grammar for the edit line, and one small module for setting and removing a path in a JSON value.

**Tech stack:**
- Rust 1.91, edition 2024
- `serde` with `derive`
- `serde_json` with `preserve_order`
- `serde_path_to_error`
- `winnow` 0.7
- `thiserror` 2
- `clap`
- dev-only: `proptest` and `insta`

## Global constraints

- **No hand-written parsing or validation sprawl** (user requirement):
  - Never walk `serde_json::Value` with if/else to read the document; use serde derive.
  - Never write a char-by-char tokenizer; use a `winnow` grammar.
  - No module-local error helpers (`err`, `bad`, `missing`, `opt_str`): every error is built with the one `Diagnostic` builder.
  - A reviewer flags any of these as Important.
- **One error type:** `motion_diagnostic::Diagnostic`. No other public error types. Its `Display` is `"{path}: {message}; expected {expected}"`, where the path and the expected part are each optional. Every user-facing diagnostic names what was wrong. Every syntax or unknown-name diagnostic has an expected form.
- **Names are data** (D27): names stay in their tables (`PROPERTIES`, `NAMED_CURVES`, `presets.json`). Unknown-name errors list the names from the table.
- **Error text may change** where serde or winnow phrase it better, but every error keeps a path and an expected form. Update the tests to the new exact text, and list each change in the task report.
- **Documentation, comments and tests:**
  - every module has a `//!` doc with its place in the pipeline;
  - every public item has a `///` doc;
  - non-obvious logic gets a short comment saying why;
  - unit tests live in `crates/<c>/tests/unit/<path>.rs`, attached by `#[cfg(test)] #[path = "…"] mod tests;`;
  - every parser has a never-panic proptest;
  - text output gets an `insta` snapshot where it helps.
- **Checks before every commit:**
  ```
  cargo fmt --all -- --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  ```
  `cargo deny check` must also pass whenever dependencies change.
- **Logging:** library crates use only `tracing` macros. There is no `println!`/`eprintln!` outside `motion-cli`'s output code, and stdout carries only command output.
- **Behaviour:** the scene format, the edit language, the `check`/`at` output and the CLI exit codes do not change, apart from the wording of error messages. `examples/launch.json` checks `OK 4`, and the README commands print what the README shows.
- **Commits:**
  - signed (`git commit -S`)
  - Conventional Commits, in English
  - no attribution trailers of any kind
  - only your own paths
  - no pushing

---

### Task 1: `motion-diagnostic`

**Files:**
- Create: `crates/motion-diagnostic/{Cargo.toml,src/lib.rs}` and its tests.
- Modify:
  - every crate's error module, to remove its own error type;
  - their call sites;
  - `crates/motion-cli/src/main.rs` and `commands/*`: no more `String` errors.

**Interfaces:**
- `enum Kind`. The CLI exit codes are 2 for every kind. `Kind` keeps them in one method, `exit_code()`, so a future kind can differ. The kinds:
  - `Syntax`: a shorthand or edit line that cannot be parsed
  - `UnknownName`: a key, preset, curve, property or layer id
  - `OutOfRange`
  - `Missing`
  - `Conflict`
  - `Invalid`: a semantic rule
  - `Io`
- `struct Diagnostic { kind, path: Option<String>, message: String, expected: Option<Cow<'static, str>> }`. It derives `thiserror::Error`.
  - Builders: `Diagnostic::new(kind, message)`, `.at(path)`, `.under(prefix)` (prefixes an existing path: `prefix.path`, or `prefix[i]` for an index segment), `.expected(form)`.
  - Shorthand constructors: `syntax(msg)`, `unknown(what, name, allowed: impl IntoIterator<Item = impl Display>)`, which writes `unknown {what} \`{name}\`; expected one of a, b, c`.
- `type Result<T> = std::result::Result<T, Diagnostic>`.
- **Replaces:**
  - `ShorthandError` and `FormatError` (motion-format)
  - `QueryError` (motion-query)
  - `EditError` (motion-edit)
  - the CLI's `String` errors: the CLI prints `error: {diagnostic}` and exits with `kind.exit_code()`.

- [ ] **Steps:**
  - Add the crate, with tests for the builders, `Display`, `under` with fields and indices, and `unknown`.
  - Move every crate onto it.
  - All tests keep passing with identical messages.
  - Commit: `refactor: share one diagnostic type across all crates`.

---

### Task 2: Shorthands as `winnow` grammars

**Files:** `crates/motion-format/src/shorthand/*`, `units.rs`, `curve.rs`.

**What to do:**
- `Time`, `Color`, `Size`, `Position`, `RectShape`, `Font`, `Behaviour` and `Curve` each implement `FromStr<Err = Diagnostic>` with a `winnow` grammar. Each module has:
  - one parser function per piece (`number`, `unit`, `coord`, …), combined declaratively;
  - errors mapped once, at the end of the grammar, into a `Diagnostic` of kind `Syntax` with the module's expected form.
- **Behaviour** stays order-free, using `winnow` `alt` over the token kinds in a repeat. "Given twice" is detected once, generically, for every kind.
- **Font** keeps the R5 rules:
  - read the size and weight from the end;
  - three trailing numbers mean a family that ends in a number;
  - two ambiguous trailing numbers give an error.
- **Tests:** keep every existing shorthand test, including the exact messages, unless the report lists a change. Keep the proptests and add a round-trip test where there is a `Display`.
- Commit: `refactor(format): parse shorthands with winnow grammars`.

---

### Task 3: The document via serde, plus one validation pass

**Files:**
- Create: `crates/motion-format/src/doc.rs` and `src/validate.rs`.
- Rewrite: `parse.rs`, so it only parses JSON, deserializes it and validates it.
- Fold into `doc.rs` and `validate.rs`: the `Value` walking in `anim.rs` and `preset.rs`.

**What to do:**
- **`doc.rs`:** `SceneDoc`, `LayerDoc`, `PresetDoc` and `TrackDoc`, all `#[serde(deny_unknown_fields)]`, with shorthand fields as `String` (parsed in `validate.rs`).
  - `TrackDoc` is either a `String` (parsed by a `winnow` grammar in `anim.rs`) or a `Vec<KeyDoc>`. It has a custom `Deserialize` with `visit_str` and `visit_seq`, not `untagged`, so its errors stay specific.
  - `KeyDoc` is `[time, value]` or `[time, value, curve]`, via a sequence visitor.
- **`parse_document`:** `serde_json::from_str` → `serde_path_to_error::deserialize` → `validate::scene(doc, raw)`.
  - Serde reports only structural errors (unknown field, wrong type, missing field). One function maps them to a `Diagnostic` with the path (`layers[1].fill`; the root path `.` becomes no path) and the right kind (`WrongType`, `UnknownName`, `Missing`).
  - Unknown fields give `UnknownName` and list the allowed keys. Serde's own message already lists them; the expected part must still be present.
- **`validate.rs`:** every cross-field rule, each a small named function called from one `scene()` function. Rules:
  - the version
  - fps
  - `text` xor `rect`
  - `font` only on text
  - the id rule
  - unique ids
  - preset existence
  - range checks
  - key order
  - the `in`/`anim` conflict, with the pasteable hint
  - preset start rules
- **Keep `scene_from_value`** for motion-edit, now implemented through the same path.
- **Keep `SCENE_KEYS` and `LAYER_KEYS`**, used by edit, and add a test that every listed key is accepted by the serde structs and an unlisted key is rejected.
- **Delete** `opt_str`, `missing`, `check_keys` and the hand-walking code.
- Commit: `refactor(format): read the scene with serde and validate in one pass`.

---

### Task 4: The edit language as a `winnow` grammar

**Files:**
- `crates/motion-edit/src/parse.rs`: rewrite.
- `src/apply.rs`.
- Create: `src/path.rs`, for setting and removing a path in a JSON value.

**What to do:**
- **Grammar:** `line = target ws change (ws change)*`; `change = path "=" value`; `path = segment ("." segment)*`.
  - A value is one of: a quoted string with escapes, JSON (a balanced `[` or `{`, parsed with serde_json), `none`, a number, `true`/`false`, or a bare word.
  - All the existing behaviours stay:
    - operators other than `=` are rejected with "not supported yet";
    - stray-quote errors keep their pasteable suggestion;
    - overlapping keys are rejected;
    - duplicate keys are rejected;
    - errors for an unclosed quote or bracket.
- **`path.rs`:** `set(&mut Value, &[&str], Value) -> Result<()>` and `remove(&mut Value, &[&str]) -> bool`, with empty-parent cleanup.
- **`apply.rs`:** it shrinks to: check the first segments, then set or remove, then `scene_from_value`, then the quoting hint.
- Commit: `refactor(edit): parse edit lines with a winnow grammar`.

---

### Task 5: Documents

- **`CONTRIBUTING.md`:**
  - **"How errors work":** the `Diagnostic` builder, the kinds, and paths via `under`.
  - **"Adding a shorthand":** a grammar plus `FromStr`, with the serde adapter for free.
  - **"Adding a field":** a `doc.rs` field plus a rule in `validate.rs`.
- **`ARCHITECTURE.md`:** the new crate, and where parsing and validation live.
- **`CHANGELOG.md`:** an `[Unreleased]` entry.
- **D28** in `docs/design/decisions.md`: standard parsing tools and one diagnostic type.
- Commit: `docs: explain diagnostics, grammars and validation (D28)`.

---

### Task 6: One engine entry point with error and telemetry handling, plus logs via `tracing`

**Why:**
- Every command repeats the same steps (read, parse, validate, compile, evaluate, check), each with its own error handling. The maintainer asked for one base function that runs these steps end to end, with the error and telemetry handling in one place.
- Nobody can see what the engine does or how long each step takes without a debugger.
- The render-time targets (see Overview) will need per-step timings.
- Agents need clean stdout.

**Files:**
- Create: `crates/motion-telemetry/{Cargo.toml,src/lib.rs,src/init.rs,src/timing.rs}` and its tests.
- Create: `crates/motion-engine/{Cargo.toml,src/lib.rs,src/stage.rs,src/session.rs}` and its tests.
- Modify: `crates/motion-cli/src/commands/*`, so they call only `motion-engine`.
- Modify:
  - the library crates, to add instrumentation;
  - `crates/motion-cli/src/main.rs`, to initialise logging and add the flags;
  - `Cargo.toml`, for the workspace deps: `tracing`, and `tracing-subscriber` with `env-filter`, `fmt` and `json`;
  - `deny.toml` if needed.

**Interfaces:**
- **`motion-engine`: the base function.**
  - `stage<T>(name: &'static str, f: impl FnOnce() -> Result<T>) -> Result<T>` is the one wrapper every pipeline step runs through. It:
    - opens a `tracing` span named after the step;
    - times it;
    - on error, logs one `error` event with the diagnostic's kind and path;
    - returns the diagnostic unchanged.
    - No step does its own logging or error mapping.
  - `Session` runs the pipeline end to end and caches each step's result:
    - `Session::open(path)` and `Session::from_str(text)`, for reading and parsing;
    - `scene()`, `graph()`, `frame(t)`, `check()` and `at(time, ids)`;
    - `edit(line)`, which applies, re-validates, re-runs `check` and returns the new report;
    - `save()`, an atomic write.
    - Every method goes through `stage`.
  - The CLI, and later the MCP server and the Node binding, call only `Session`. They never call format, scene, eval or query directly.
- **Library crates** depend only on `tracing`. They emit events and spans, and they never print. These are the instrumentation points:
  - **`motion-format`:** a `parse_document` span, with fields for the byte length and layer count. A `debug` event per validation rule that rejects, with the diagnostic kind and path.
  - **`motion-scene`:** a `compile` span, with node and track counts.
  - **`motion-eval`:** a `trace`-level span per `evaluate` call, with the frame. It is cheap when disabled.
  - **`motion-query`:** a `check` span, with frames sampled, rules run, violations and passed. An `at` span.
  - **`motion-edit`:** an `apply_edit` span, with the target, the change keys and the outcome.
- **`motion-telemetry`** is used only by binaries and tests.
  - `init(Options { level, format: Text | Json, timings: bool }) -> Guard`. It installs a `tracing-subscriber` that writes to **stderr only**.
    - The filter comes from `MOTION_LOG`, using `EnvFilter` syntax such as `motion_query=debug`, falling back to `level`.
    - `Json` writes one JSON object per line, for machines and agents.
    - It is safe to call twice: the second call is a no-op returning a guard.
  - `timing` holds a `Timings` layer that records each closed span's name and duration. `report()` returns a compact line such as `timings parse=0.4ms compile=0.1ms check=2.3ms`, which the CLI prints to stderr when `--timings` is on.
  - `#[track_caller]`-free helpers only; no macros of our own. The `tracing` macros are the API.
- **CLI:**
  - global flags `--log <level>` (default `warn`), `--log-format text|json` and `--timings`;
  - stdout is unchanged; everything else goes to stderr;
  - a diagnostic error is also logged as an `error` event with its kind and path, before the `error:` line.
- **Tests:**
  - `init` is idempotent;
  - the JSON format emits valid JSON lines, captured with a test writer;
  - the timings layer records nested spans;
  - a CLI test shows that `--log debug --log-format json` leaves stdout byte-identical to a run without it, and that stderr has JSON lines;
  - a CLI test that `--timings` prints a `timings` line to stderr.

- [ ] Steps:
  - Add the crate and the instrumentation.
  - Add the CLI flags.
  - Write the tests.
  - Add a CONTRIBUTING section, "Logs and timings": how to enable them, field naming (`snake_case`; ids as `id`), and levels:
    - `error`: failures shown to the user
    - `warn`: suspicious input
    - `info`: one line per command
    - `debug`: per step
    - `trace`: per frame
  - Run `cargo deny check`.
  - Commit: `feat: add tracing-based logs and step timings`.
