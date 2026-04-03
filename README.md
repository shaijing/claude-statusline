# claude-statusline (Rust) — Configurable statusbar

A fast, configurable status line for Claude Code with minimal allocations and LTO-optimized binary.

```
Opus 4.6  [██████░░░░] 27%/200k  │  $1.40
i:5k/o:340  │  9m49s  │  +5/-2  │  project
```

## Build

```bash
cargo build --release
cp target/release/claude-statusline ~/.local/bin/
```

Binary is ~1.8MB (LTO + stripped).

## Configure

### Claude Code integration

`~/.claude/settings.json`:
```json
{
  "statusLine": {
    "type": "command",
    "command": "claude-statusline"
  }
}
```

### Layout customization

`~/.claude/statusline_layout.json`:
```json
{
  "rows": [
    ["model", "context_bar", "cost"],
    [{"guard": "session_usage", "members": ["session_reset", "burn_rate"]}],
    ["tokens", "duration", "git_diff", "dir"]
  ]
}
```

**Available blocks:**

| Block | Content |
|-------|---------|
| `model` | Model name (Opus 4.6) |
| `context_bar` | Progress bar + percentage |
| `cost` | Session cost ($1.40) |
| `session_usage` | 5-hour usage % (guard) |
| `session_reset` | Reset countdown |
| `daily_spend` | Daily spend/average |
| `burn_rate` | Cost per hour |
| `extra_credits` | Extra credits used |
| `tokens` | i/o token counts |
| `duration` | Session duration |
| `git_diff` | +/- lines |
| `dir` | Directory + branch |

**Conditional groups:** `{"guard": "X", "members": ["A", "B"]}` — members only show when guard has data.

## Test without Claude Code

```bash
echo '{
  "model": {"display_name": "Opus 4.6"},
  "workspace": {"current_dir": "/Users/me/myproject"},
  "context_window": {
    "used_percentage": 27,
    "context_window_size": 200000,
    "current_usage": {"input_tokens": 5000, "output_tokens": 340}
  },
  "cost": {"total_cost_usd": 1.40, "total_duration_ms": 589000, "total_lines_added": 5, "total_lines_removed": 2}
}' | ./target/release/claude-statusline
```

Enable debug: `STATUSLINE_DEBUG=1` logs to `~/.claude/statusline_debug.log`.

## Performance

- LTO + `opt-level="z"` for small binary
- Buffer-based string building (avoid intermediate allocations)
- Integer math for token formatting
- Background cache refresh (non-blocking HTTP)
- Single-pass git info extraction

## Dependencies

- `serde` + `serde_json` — JSON parsing
- `chrono` — Timestamp handling
- `ureq 3` — HTTP client (sync, lightweight)
- `dirs` — Cross-platform home dir
- `owo-colors` — True-color terminal output
