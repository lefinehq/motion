//! What a scene means, written compactly for snapshots: shared by the
//! integration tests that hold scenes to what they drew before.

use motion_engine::{RenderOptions, Result, Session};
use motion_format::{Value, fmt_num};

/// What `session` means at its first, middle and last frames.
///
/// # Errors
/// A scene that cannot be compiled or drawn.
pub fn meaning(session: &Session) -> Result<String> {
    let last = (session.graph()?.duration_frames - 1.0).max(0.0);
    let frames = [0.0, (last / 2.0).floor(), last]
        .iter()
        .map(|&frame| at(session, frame))
        .collect::<Result<Vec<String>>>()?;
    Ok(frames.join("\n"))
}

/// What `session` means at `frame`: a hash of the frame as drawn, then each
/// layer's state, so a change shows which layer it moved.
fn at(session: &Session, frame: f64) -> Result<String> {
    let image = session.render_with(frame, RenderOptions::GOLDEN)?;
    let mut out = format!("frame {frame}: {:016x}\n", fnv(image.pixels()));
    for node in session.frame(frame)?.nodes {
        let [a, b, c, d, e, f] = node.transform.as_coeffs();
        let values: Vec<String> = node.values.iter().map(value).collect();
        out.push_str(&format!(
            "  {}{}{} [{}] {}\n",
            node.id,
            if node.shown { "" } else { " (not shown)" },
            if node.hidden { " (hidden)" } else { "" },
            [a, b, c, d, e, f].map(fmt_num).join(" "),
            values.join(" | "),
        ));
    }
    Ok(out)
}

/// FNV-1a over `bytes`: stable across platforms and Rust versions, unlike
/// the standard hasher.
fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// A property value, to two decimals.
fn value(value: &Value) -> String {
    let list = |numbers: &[f64]| {
        numbers
            .iter()
            .map(|n| fmt_num(*n))
            .collect::<Vec<_>>()
            .join(",")
    };
    match value {
        Value::Number(n) => fmt_num(*n),
        Value::Point(p) => list(p),
        Value::Color(c) => list(c),
        Value::Path(path) => path.to_string(),
    }
}
