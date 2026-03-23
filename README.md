# claude-statusline (Rust) — 4-line statusbar

Matches the layout from the screenshot:

```
Opus 4.6 · Max  [██████░░░░]  0%/200k  │  #3/35 $1.40
~27%  │  28m→02:00  │  $9.6/$1.4D  │  ● $2.6/h
$0.00  │  i:0/o:0  │  9m49s  │  +0/-0  │  myproject (main)
Effort set to high for this turn
```

## Build

```bash
cargo build --release
cp target/release/claude-statusline ~/.local/bin/
```

## Configure

`~/.claude/settings.json`:
```json
{
  "statusLine": {
    "type": "command",
    "command": "claude-statusline"
  }
}
```

## Test without Claude Code

```bash
echo '{
  "model": {"display_name": "Opus 4.6", "id": "claude-opus-4-6"},
  "workspace": {"current_dir": "/Users/me/myproject"},
  "context_window": {
    "used_percentage": 27,
    "remaining_percentage": 73,
    "context_window_size": 200000,
    "current_usage": {"input_tokens": 0, "output_tokens": 0}
  },
  "cost": {"total_cost_usd": 1.40, "total_lines_added": 0, "total_lines_removed": 0},
  "session": {"turns": 3, "total_turns": 35, "duration_ms": 589000, "thinking_effort": "high"}
}' | ./target/release/claude-statusline
```

## Lines breakdown

| Line | Content |
|------|---------|
| 1 | Model name · context bar · ctx%/size · turn counter · session cost |
| 2 | 5h usage% · reset countdown · daily spend · $/h rate |
| 3 | Extra credits · i/o tokens · duration · git +/- · dir(branch) |
| 4 | Thinking effort hint (only shown when set) |

## Dependencies

- `serde` + `serde_json` — JSON parsing
- `chrono` — ISO timestamp → epoch
- `ureq 3` — Usage API HTTP call
- `dirs` — cross-platform home dir
