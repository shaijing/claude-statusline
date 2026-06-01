//! Debug-mode logging to `~/.claude/statusline_debug.log`.
//!
//! Activated by setting `STATUSLINE_DEBUG=1` in the environment. The env-var
//! check is evaluated at most once per process (cached in a `OnceLock`).

use crate::format::now_epoch;
use std::io::Write as _;
use std::sync::OnceLock;

static DEBUG_MODE: OnceLock<bool> = OnceLock::new();

pub(crate) fn is_debug() -> bool {
    *DEBUG_MODE.get_or_init(|| std::env::var("STATUSLINE_DEBUG").is_ok())
}

pub(crate) fn debug_log(msg: &str) {
    if !is_debug() {
        return;
    }
    if let Some(path) = dirs::home_dir().map(|h| h.join(".claude/statusline_debug.log")) {
        let line = format!("[{}] {}\n", now_epoch(), msg);
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = f.write_all(line.as_bytes());
        }
    }
}
