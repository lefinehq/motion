//! `motion`, the command's name before the project was renamed ([D73]): it
//! runs the `nakkash` beside it with the same arguments, and `nakkash` then
//! prints a deprecation note. It goes away at 1.0.
//!
//! [D73]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d73-the-project-is-named-nakkash

use std::env;
use std::path::Path;
use std::process::{Command, ExitCode};

/// The variable that tells `nakkash` it was run as `motion`; `main.rs` reads it.
const CALLED_AS: &str = "NAKKASH_CALLED_AS";

fn main() -> ExitCode {
    let nakkash = match env::current_exe() {
        Ok(this) => this.with_file_name(format!("nakkash{}", env::consts::EXE_SUFFIX)),
        Err(e) => {
            eprintln!(
                "error: `motion` is now `nakkash` (D73), and this command cannot find itself: {e}"
            );
            return ExitCode::from(2);
        }
    };
    let mut command = Command::new(&nakkash);
    command
        .args(env::args_os().skip(1))
        .env(CALLED_AS, "motion");
    run(command, &nakkash)
}

/// Replaces this process with `command`, so signals and the exit code are
/// `nakkash`'s own.
#[cfg(unix)]
fn run(mut command: Command, nakkash: &Path) -> ExitCode {
    use std::os::unix::process::CommandExt;
    let e = command.exec();
    cannot_run(nakkash, &e)
}

/// Runs `command` and exits as it does.
#[cfg(not(unix))]
fn run(mut command: Command, nakkash: &Path) -> ExitCode {
    match command.status() {
        Ok(status) => ExitCode::from(
            status
                .code()
                .and_then(|c| u8::try_from(c).ok())
                .unwrap_or(1),
        ),
        Err(e) => cannot_run(nakkash, &e),
    }
}

/// The failure when there is no `nakkash` to run.
fn cannot_run(nakkash: &Path, e: &std::io::Error) -> ExitCode {
    eprintln!(
        "error: `motion` is now `nakkash` (D73), and `{}` cannot run: {e}",
        nakkash.display()
    );
    ExitCode::from(2)
}
