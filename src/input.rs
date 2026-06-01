//! JSON input deserialized from stdin (per Claude Code's statusline hook contract).

use crate::debug::debug_log;
use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
pub(crate) struct ClaudeInput {
    pub(crate) model: ModelInfo,
    pub(crate) workspace: Workspace,
    pub(crate) cost: Cost,
    pub(crate) context_window: ContextWindow,
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct ModelInfo {
    pub(crate) display_name: String,
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct Workspace {
    pub(crate) current_dir: String,
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct ContextWindow {
    pub(crate) context_window_size: u64,
    pub(crate) used_percentage: Option<u32>,
    pub(crate) current_usage: Option<CurrentUsage>,
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct CurrentUsage {
    pub(crate) input_tokens: Option<u64>,
    pub(crate) output_tokens: Option<u64>,
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct Cost {
    pub(crate) total_cost_usd: f64,
    pub(crate) total_duration_ms: u64,
    pub(crate) total_lines_added: i64,
    pub(crate) total_lines_removed: i64,
}

pub(crate) fn parse_input(buf: &str) -> ClaudeInput {
    match serde_json::from_str(buf) {
        Ok(v) => v,
        Err(e) => {
            debug_log(&format!("parse_input error: {e}\n---\n{buf}"));
            ClaudeInput::default()
        }
    }
}
