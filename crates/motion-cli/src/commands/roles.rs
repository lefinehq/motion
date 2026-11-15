//! `motion roles`: what a master's layers and slots do (D50), with what
//! each role holds, which is where a brief's edits go.

use std::path::Path;
use std::process::ExitCode;

use motion_engine::{Content, Diagnostic, Kind, Result, Role, RoleTarget, Scene, Session};
use serde_json::{Value as Json, json};

/// Prints each role, what it names and what it holds; `json` prints them
/// as data.
pub fn run(path: &Path, json: bool) -> Result<ExitCode> {
    let session = Session::open(path)?;
    let scene = session.scene();
    if json {
        let data: Vec<Json> = scene.roles.iter().map(|role| data(scene, role)).collect();
        let text = serde_json::to_string_pretty(&data).map_err(|e| {
            Diagnostic::new(
                Kind::Invalid,
                format!("cannot write the roles as JSON: {e}"),
            )
        })?;
        println!("{text}");
    } else if scene.roles.is_empty() {
        println!("no roles");
    } else {
        let width =
            |f: fn(&Role) -> String| scene.roles.iter().map(|r| f(r).len()).max().unwrap_or(0);
        let (names, targets) = (width(|r| r.id.to_string()), width(|r| r.target.to_string()));
        for role in &scene.roles {
            let (kind, holds) = holds(scene, role);
            let holds = match holds {
                Json::Null => String::new(),
                value => format!(" {value}"),
            };
            println!(
                "{:names$}  {:targets$}  {kind}{holds}",
                role.id.to_string(),
                role.target.to_string()
            );
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// What `role` holds: its kind and its value, such as `text` and
/// `"Launch"`.
fn holds(scene: &Scene, role: &Role) -> (String, Json) {
    match &role.target {
        RoleTarget::Slot(id) => match scene.root.slots.iter().find(|s| &s.id == id) {
            Some(slot) => (slot.ty.to_string(), slot.value.clone()),
            None => ("slot".into(), Json::Null),
        },
        RoleTarget::Layer(address) => match scene.layer(&address.layers).map(|l| &l.content) {
            Some(Content::Text(text)) => ("text".into(), json!(text.text)),
            Some(Content::Image(image)) => ("image".into(), json!(image.path)),
            Some(Content::Video(video)) => ("video".into(), json!(video.path)),
            Some(Content::Comp(comp)) => ("comp".into(), json!(comp.id)),
            Some(Content::Rect(_)) => ("rect".into(), Json::Null),
            Some(Content::Shapes(_)) => ("shapes".into(), Json::Null),
            Some(Content::Pivot) => ("pivot".into(), Json::Null),
            Some(Content::Filter) => ("filter".into(), Json::Null),
            None => ("layer".into(), Json::Null),
        },
    }
}

fn data(scene: &Scene, role: &Role) -> Json {
    let (kind, value) = holds(scene, role);
    json!({
        "role": role.id.to_string(),
        "target": role.target.to_string(),
        "kind": kind,
        "value": value,
    })
}
