# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

A Rust CLI tool that renders a multi-line status bar for Claude Code's statusline hook. It reads JSON from stdin, fetches usage data from Anthropic's API (cached), and outputs colored terminal text.

## Build Commands

```bash
cargo build --release          # Build optimized binary (LTO + stripped)
cargo build                    # Debug build
```

The binary is at `target/release/claude-statusline`.

## Testing

Test with sample input:
```bash
echo '{
  "model": {"display_name": "Opus 4.6"},
  "workspace": {"current_dir": "/Users/me/project"},
  "context_window": {"used_percentage": 27, "context_window_size": 200000, "current_usage": {"input_tokens": 5000, "output_tokens": 340}},
  "cost": {"total_cost_usd": 1.40, "total_duration_ms": 589000, "total_lines_added": 5, "total_lines_removed": 2}
}' | cargo run --release
```

Enable debug logging with `STATUSLINE_DEBUG=1` to write to `~/.claude/statusline_debug.log`.

## Layout Configuration

Config file: `~/.claude/statusline_layout.json`

### Simple format (rows of block names)
```json
{
  "rows": [
    ["model", "cost"],
    ["context_bar", "tokens", "git_diff"],
    ["duration", "dir"]
  ]
}
```

### Group format (conditional blocks)
```json
{
  "rows": [
    [{"guard": "session_usage", "members": ["session_reset", "daily_spend", "burn_rate"]}]
  ]
}
```
The `guard` block must return data for `members` to render.

### Available blocks

| Name | Content |
|------|---------|
| `model` | Model display name (bold cyan) |
| `context_bar` | Progress bar + percentage/size |
| `cost` | Session cost in USD |
| `session_usage` | 5-hour usage percentage |
| `session_reset` | Reset countdown (28m→02:00) |
| `daily_spend` | Daily spend vs daily average |
| `burn_rate` | Cost per hour ($/h) |
| `extra_credits` | Extra credits spent |
| `tokens` | Input/output token counts |
| `duration` | Session wall-clock duration |
| `git_diff` | Lines added/removed (+5/-2) |
| `dir` | Directory basename + git branch |

### Default layout (when config missing)
```json
{
  "rows": [
    ["model", "context_bar", "cost"],
    [{"guard": "session_usage", "members": ["session_reset", "daily_spend", "burn_rate"]}],
    ["extra_credits", "tokens", "duration", "git_diff", "dir"]
  ]
}
```

## Architecture

Single-file application (`src/main.rs`) with these components:

1. **Block registry** - Maps string names to constructors via `block_registry()`. New blocks: add entry + implement `Block` trait.

2. **Layout config** - `LayoutConfig` parses JSON, `build_layout()` constructs `Row`s. Supports simple strings or conditional groups.

3. **Block system** - Each display element implements `Block` trait. Renders to `Option<String>` (None = skip).

4. **Input parsing** - `ClaudeInput` deserializes JSON from stdin. `DerivedCtx::build()` computes shared data (dir name, git branch, diff stats).

5. **OAuth token resolution** - Falls through: env var `CLAUDE_CODE_OAUTH_TOKEN` → macOS Keychain → Linux `~/.claude/.credentials.json` → GNOME Keyring via `secret-tool`

6. **Usage API** - Fetches from `https://api.anthropic.com/api/oauth/usage` with 5-minute cache. Background thread refreshes; current invocation uses cached data.

7. **Output rendering** - Catppuccin Mocha palette via `owo-colors`, buffer-based string building for minimal allocations.

## Adding a new block

1. Implement `Block` trait:
```rust
struct MyBlock;
impl Block for MyBlock {
    fn render(&self, ctx: &RenderCtx) -> Option<String> {
        // Return Some(String) to show, None to skip
    }
}
```

2. Register in `block_registry()`:
```rust
m.insert("my_block", || Box::new(MyBlock));
```

3. Use in config: `"my_block"` or add to default layout.

## Key Dependencies

- `owo-colors` - True-color terminal output
- `ureq` - HTTP client for usage API
- `serde_json` - JSON parsing
- `chrono` - Timestamp handling
- `dirs` - Cross-platform home directory