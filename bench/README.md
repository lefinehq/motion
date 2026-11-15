# The brief set

Thirty briefs for agents writing videos from scratch, grouped by use case: `ads`, `launch` (launch and feature videos), `explainers` and `social`. Each asks for one video the way a client would: its size and length, what it must say and show, and how it should feel. No footage, photos or logos come with them, so the agent draws everything, and every name in them is made up.

These are the briefs the [public scoreboard](../docs/design/quality.md#the-public-scoreboard) uses ([craft lint plan](../docs/plans/2026-11-22-craft-lint.md), step 6).

## Running them

`motion bench` runs an agent on every brief, one at a time, each in a folder of its own:

```sh
motion bench bench/briefs --out runs/2026-11-15 --agent \
  'claude -p --output-format json "Write the video this brief asks for as scene.json in this folder, with the motion CLI. $(cat "$MOTION_BRIEF")"'
```

- The agent is any shell command. It runs in the brief's folder, with the brief on stdin and its path in `MOTION_BRIEF`, and writes `scene.json` there.
- Its output goes to `agent.log` and `agent.err`. When its last line is a JSON object, the bench reads its cost from it: `tokens` and `steps`, or `usage` and `num_turns`, as agents that print a JSON result write them.
- `--timeout` stops an agent after that many seconds; the default is 1800.

For each brief, the bench then opens the scene, runs `motion check` on it, rules and pixel gates, and renders it to `video.mp4`. It writes one JSON object per brief to `results.jsonl`, and prints one line per brief as it ends:

| Field | What it holds |
|---|---|
| `brief`, `group` | The brief, as `group/name`, and its use case |
| `agent_seconds`, `agent_exit`, `timed_out` | How long the agent ran, how it ended |
| `tokens`, `steps` | What the agent reported it cost, or `null` |
| `scene`, `scene_error` | Whether it wrote a scene, and why that scene did not open or render |
| `render_seconds`, `video_seconds` | How long the render took, and how long the video is |
| `errors`, `warnings`, `passed`, `findings` | The check report: its counts, and the lines of each rule |

The bench scores nothing. Whether a video is good is for people to judge ([D22](../docs/design/decisions.md#d22-no-ai-judge)); the counts say what each run cost and what the check caught.

For a fair comparison, keep the agent, the model and the prompt the same, and change only the engine and its own guidance.
