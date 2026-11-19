//! `nakkash ml`: learned models ([D80]). `models` fetches, imports, lists
//! and verifies the files the model lock pins ([D82]); the only commands of
//! the set that reach the network are `models fetch`. `preview`, `bake`
//! and `track` run a scene's requests ([`scene`]).
//!
//! [D80]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d80-learned-models-run-in-a-bake-never-in-a-render
//! [D82]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d82-a-model-lock-and-a-content-addressed-model-store

use std::num::NonZero;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Subcommand;
use nakkash_engine::Result;
use nakkash_fonts::Cache;
use nakkash_format::Address;
use nakkash_ml::fetch::{Fetched, Fetcher};
use nakkash_ml::lock::LicenceClass;
use nakkash_ml::{IMPORT_COMMAND, Imported, ModelLock, ModelStore};

mod scene;

/// Threads a graph runs on when `--threads` is not given.
pub const DEFAULT_THREADS: usize = 4;

/// The `nakkash ml` commands.
#[derive(Subcommand)]
pub enum MlCommand {
    /// The model files: fetch, import, list and verify them.
    Models {
        #[command(subcommand)]
        command: ModelsCommand,
    },
    /// Segment the objects the scene's video layers ask for, follow a
    /// track's points, and draw a contact sheet of each, to check the
    /// prompts before a long bake.
    Preview {
        /// The scene file.
        scene: PathBuf,
        /// Where the sheet goes; with several requests, one sheet each,
        /// named after the layer and the request.
        #[arg(long, default_value = "sheet.png")]
        out: PathBuf,
        /// Only the video layer at this address.
        #[arg(long)]
        layer: Option<Address>,
        /// Frames on a sheet, spread over the clip; the prompted ones are
        /// always among them.
        #[arg(long, default_value_t = 12)]
        count: usize,
        /// A frame's width on the sheet, in pixels.
        #[arg(long, default_value_t = 480)]
        tile: usize,
    },
    /// Bake every cutout and track the scene's video layers ask for whose
    /// bake is missing or stale, and write their digests into the scene.
    Bake {
        /// The scene file.
        scene: PathBuf,
        /// Only the video layer at this address.
        #[arg(long)]
        layer: Option<Address>,
        /// Bake again even what is baked already.
        #[arg(long)]
        again: bool,
    },
    /// Write a baked track as keys on a pivot parented to its video layer.
    Track {
        /// The scene file.
        scene: PathBuf,
        /// The track, as `<layer>.tracks[<id>]`.
        track: Address,
        /// The pivot's id, made when there is none.
        #[arg(long = "to", value_name = "PIVOT")]
        pivot: String,
        /// Also write the four corner pivots `<PIVOT>-tl`, `-tr`, `-br`
        /// and `-bl`.
        #[arg(long)]
        corners: bool,
    },
}

/// The `nakkash ml models` commands.
#[derive(Subcommand)]
pub enum ModelsCommand {
    /// List every model of the lock: what it does, its licence, and which
    /// of its files this machine has.
    List,
    /// Download the inference runtime for this platform and every model
    /// released as a file, each checked against the lock before it is kept.
    Fetch {
        /// Only these models, by key, such as `matte.edges`.
        #[arg(long = "model", value_name = "KEY")]
        models: Vec<String>,
    },
    /// Take in the exported graphs and constants in a folder: those whose
    /// hash the lock pins, graphs once they pass the check. Others are left
    /// out and listed.
    Import {
        /// The folder that holds them.
        folder: PathBuf,
    },
    /// Hash every kept file again and check every graph again.
    Verify,
}

impl MlCommand {
    /// The file or folder the command works on, for the log.
    pub fn file(&self) -> &Path {
        match self {
            MlCommand::Models {
                command: ModelsCommand::Import { folder },
            } => folder,
            MlCommand::Models { .. } => Path::new(""),
            MlCommand::Preview { scene, .. }
            | MlCommand::Bake { scene, .. }
            | MlCommand::Track { scene, .. } => scene,
        }
    }

    /// Runs it, graphs on `threads` threads when it runs any.
    pub fn run(self, threads: Option<NonZero<usize>>) -> Result<ExitCode> {
        let threads = threads.map_or(DEFAULT_THREADS, NonZero::get);
        let store = store()?;
        let lock = ModelLock::builtin();
        match self {
            MlCommand::Models { command } => models(command, &store, &lock),
            MlCommand::Preview {
                scene,
                out,
                layer,
                count,
                tile,
            } => {
                let models = scene::Models::load(store, lock, threads)?;
                scene::preview(&models, &scene, layer.as_ref(), &out, count, tile)
            }
            MlCommand::Bake {
                scene,
                layer,
                again,
            } => {
                let models = scene::Models::load(store, lock, threads)?;
                scene::bake(&models, &scene, layer.as_ref(), again)
            }
            MlCommand::Track {
                scene,
                track,
                pivot,
                corners,
            } => scene::apply(&scene, &track, &pivot, corners),
        }
    }
}

/// The model store in this user's nakkash cache.
pub fn store() -> Result<ModelStore> {
    Ok(ModelStore::new(Cache::user()?.root()))
}

/// Runs a `nakkash ml models` command.
fn models(command: ModelsCommand, store: &ModelStore, lock: &ModelLock) -> Result<ExitCode> {
    match command {
        ModelsCommand::List => {
            list(store, lock)?;
            Ok(ExitCode::SUCCESS)
        }
        ModelsCommand::Fetch { models } => {
            let fetcher = Fetcher::new(store, |line| eprintln!("  ml: {line}"));
            let mut done = vec![fetcher.runtime(lock)?];
            done.extend(fetcher.models(lock, &models)?);
            let mut exported = 0;
            for fetched in &done {
                match fetched {
                    Fetched::Kept { name, size } => println!("  fetched {name} ({size} bytes)"),
                    Fetched::Present { name } => println!("  present {name}"),
                    Fetched::Exported { name } => {
                        exported += usize::from(!store_has(store, lock, name));
                        println!("  export  {name}")
                    }
                }
            }
            if exported > 0 {
                println!(
                    "  {exported} exported files are not fetched: export them, then run `{IMPORT_COMMAND} <folder>`"
                );
            }
            Ok(ExitCode::SUCCESS)
        }
        ModelsCommand::Import { folder } => {
            let imported = store.import(lock, &folder)?;
            for entry in &imported {
                match entry {
                    Imported::Kept { path, name } => {
                        println!("  kept    {name} from {}", path.display())
                    }
                    Imported::Unknown { path } => println!(
                        "  unknown {} (the lock pins no file with its hash)",
                        path.display()
                    ),
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        ModelsCommand::Verify => {
            let states = store.verify(lock)?;
            let missing = states.iter().filter(|s| !s.present).count();
            for state in &states {
                let mark = if state.present { "ok     " } else { "missing" };
                println!("  {mark} {} ({} bytes)", state.name, state.size);
            }
            println!(
                "  {} of {} files present and checked",
                states.len() - missing,
                states.len()
            );
            Ok(match missing {
                0 => ExitCode::SUCCESS,
                _ => ExitCode::from(1),
            })
        }
    }
}

/// Whether the store has the file the lock calls `name` (`key part`).
fn store_has(store: &ModelStore, lock: &ModelLock, name: &str) -> bool {
    lock.files()
        .iter()
        .find(|f| format!("{} {}", f.name, f.part) == name)
        .is_some_and(|f| store.files().has(f.sha256))
}

/// Prints every model of the lock.
fn list(store: &ModelStore, lock: &ModelLock) -> Result<()> {
    match lock.library_here() {
        Ok(library) => println!(
            "runtime {} ({}): {}",
            lock.runtime.version,
            library.path,
            if store.files().has(&library.sha256) {
                "present"
            } else {
                "missing"
            }
        ),
        Err(e) => println!("runtime {}: {}", lock.runtime.version, e.message),
    }
    for (key, model) in &lock.models {
        let licence = &model.licence;
        println!("{key}: {}", model.does);
        println!(
            "  licence {} ({}), {}",
            licence.spdx, licence.class, licence.url
        );
        if let (LicenceClass::Conditions, Some(notice)) = (licence.class, &licence.notice) {
            println!("  notice {notice}");
        }
        for (name, graph) in &model.graphs {
            let state = if store.files().has(&graph.sha256) {
                "present"
            } else {
                "missing"
            };
            println!(
                "  graph {name}: {state}, {} bytes, on {}",
                graph.size,
                graph.placement.here()
            );
        }
        for (name, constant) in &model.constants {
            let state = if store.files().has(&constant.sha256) {
                "present"
            } else {
                "missing"
            };
            println!("  constant {name}: {state}, {} bytes", constant.size);
        }
    }
    for (key, why) in &lock.refused {
        println!("refused {key}: {why}");
    }
    Ok(())
}
