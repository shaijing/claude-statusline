# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

A Rust CLI tool that renders a multi-line status bar for Claude Code's statusline hook. It reads JSON from stdin, fetches usage data from Anthropic's API (cached), and outputs colored terminal text.

## Build Commands

```bash
cargo build --release          # Build optimized binary
cargo build                    # Debug build
```

The binary is at `target/release/claude-statusline`.

## Testing

Test with sample input:
```bash
echo '{
  "model": {"display_name": "Opus 4.6"},
  "workspace": {"current_dir": "/Users/me/project"},
  "context_window": {"used_percentage": 27, "context_window_size": 200000},
  "cost": {"total_cost_usd": 1.40},
  "session": {"turns": 3, "total_turns": 35, "duration_ms": 589000}
}' | cargo run --release
```

## Architecture

Single-file application (`src/main.rs`) with these components:

1. **Input parsing** - `ClaudeInput` struct deserializes JSON from stdin containing model, workspace, context window, cost, and session data

2. **OAuth token resolution** - Falls through: env var `CLAUDE_CODE_OAUTH_TOKEN` → macOS Keychain → Linux `~/.claude/.credentials.json` → GNOME Keyring via `secret-tool`

3. **Usage API** - Fetches from `https://api.anthropic.com/api/oauth/usage` with 5-minute cache in `~/.claude/statusline_cache.json`

4. **Output rendering** - 3 lines of formatted, colored status using Catppuccin Mocha palette via `owo-colors`

## Key Dependencies

- `owo-colors` - True-color terminal output
- `ureq` - HTTP client for usage API
- `serde_json` - JSON parsing
- `chrono` - Timestamp handling
- `dirs` - Cross-platform home directory