//! `nakkash brief <scene|instance> [--json]`: everything an agent may set in
//! a template, on one page ([D91]).
//!
//! [D91]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d91-an-agent-fills-a-template-through-a-pinned-instance-a-brief-and-one-merge-point

use std::path::Path;
use std::process::ExitCode;

use nakkash_engine::{Result, Session, instance};

/// Prints the brief of the scene or instance at `path`.
pub fn run(path: &Path, json: bool) -> Result<ExitCode> {
    let file = path.display().to_string();
    let brief = match super::instance::is_instance(path) {
        true => {
            let opened = instance::open(path)?;
            let Some(session) = &opened.session else {
                return Err(instance::diagnostic(&opened));
            };
            let i = opened
                .instance
                .as_ref()
                .expect("an instance that merged was read");
            let template = opened.template_path.display().to_string();
            nakkash_engine::brief::brief(
                session,
                &file,
                Some(nakkash_engine::brief::Pinned {
                    template: &template,
                    hash: &opened.hash,
                    look: i.look.as_deref(),
                    sections: &i.sections,
                }),
            )?
        }
        false => nakkash_engine::brief::brief(&Session::open(path)?, &file, None)?,
    };
    match json {
        true => println!(
            "{}",
            serde_json::to_string_pretty(&brief).expect("JSON writes")
        ),
        false => print!("{brief}"),
    }
    Ok(ExitCode::SUCCESS)
}
