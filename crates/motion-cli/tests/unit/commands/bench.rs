//! Unit tests for `src/commands/bench.rs`.

use super::*;
use pretty_assertions::assert_eq;
use rstest::rstest;

#[rstest]
#[case::plain(r#"{"tokens":1200,"steps":7}"#, (Some(1200), Some(7)))]
#[case::usage_and_turns(
    r#"{"type":"result","num_turns":12,"usage":{"input_tokens":10,"output_tokens":5,"cache_read_input_tokens":100}}"#,
    (Some(115), Some(12))
)]
#[case::last_line_only("{\"tokens\":1}\ndone\n", (None, None))]
#[case::blank_lines_after("{\"steps\":3}\n\n", (None, Some(3)))]
#[case::no_json("wrote scene.json", (None, None))]
fn an_agent_reports_its_cost_on_its_last_line(
    #[case] log: &str,
    #[case] cost_of: (Option<u64>, Option<u64>),
) {
    assert_eq!(cost(log), cost_of);
}

#[test]
fn briefs_are_listed_by_folder_then_name() {
    let root = std::env::temp_dir().join(format!("motion-bench-list-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("ads")).unwrap();
    fs::create_dir_all(root.join("social")).unwrap();
    for file in [
        "ads/02-b.md",
        "ads/01-a.md",
        "social/01-c.md",
        "top.md",
        "ads/notes.txt",
    ] {
        fs::write(root.join(file), "# brief").unwrap();
    }
    let ids: Vec<String> = list(&root).unwrap().iter().map(Brief::id).collect();
    assert_eq!(ids, ["ads/01-a", "ads/02-b", "social/01-c", "top"]);
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_summary_names_what_happened() {
    let record = json!({"agent_seconds": 12.5, "timed_out": false, "steps": 4, "tokens": 900,
        "scene": true, "errors": 1, "warnings": 2, "render_seconds": 3.25});
    assert_eq!(
        summary("ads/01-a", &record),
        "ads/01-a  agent 12.5s  4 steps  900 tokens  E1 W2  render 3.25s"
    );
    let missing = json!({"agent_seconds": 1, "timed_out": true, "scene": false});
    assert_eq!(
        summary("ads/01-a", &missing),
        "ads/01-a  agent 1s  timed out  no scene"
    );
}
