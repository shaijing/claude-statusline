//! JSON layout configuration and row/layout construction.

use crate::block::Layout;
use crate::blocks::{make_block, make_group, make_row};
use crate::debug::debug_log;
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub(crate) enum BlockSpec {
    /// Simple block by name: "model"
    Simple(String),
    /// Group with guard: `{"guard": "session_usage", "members": ["reset", "spend"]}`
    Group { guard: String, members: Vec<String> },
}

#[derive(Debug, Deserialize, Clone)]
pub(crate) struct LayoutConfig {
    /// Each row is a list of block specs.
    pub(crate) rows: Vec<Vec<BlockSpec>>,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        LayoutConfig {
            rows: vec![
                // Row 1: model · context bar · cost
                vec![
                    BlockSpec::Simple("model".into()),
                    BlockSpec::Simple("context_bar".into()),
                    BlockSpec::Simple("cost".into()),
                ],
                // Row 2: session group
                vec![BlockSpec::Group {
                    guard: "session_usage".into(),
                    members: vec![
                        "session_reset".into(),
                        "daily_spend".into(),
                        "burn_rate".into(),
                    ],
                }],
                // Row 3: extra credits · tokens · duration · git diff · dir
                vec![
                    BlockSpec::Simple("extra_credits".into()),
                    BlockSpec::Simple("tokens".into()),
                    BlockSpec::Simple("duration".into()),
                    BlockSpec::Simple("git_diff".into()),
                    BlockSpec::Simple("dir".into()),
                ],
            ],
        }
    }
}

fn config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude/statusline_layout.json"))
}

/// Cached config with expiration (5 seconds).
///
/// `OnceLock` cannot model "expire and refresh", so use a `Mutex<Option<…>>`.
/// This is a single-threaded CLI in practice; the lock never contends.
static CONFIG_CACHE: Mutex<Option<(LayoutConfig, Instant)>> = Mutex::new(None);
const CONFIG_TTL_SECS: u64 = 5;

pub(crate) fn load_config() -> LayoutConfig {
    let now = Instant::now();

    // Fast path: cache hit and still fresh.
    if let Some((config, ts)) = CONFIG_CACHE.lock().unwrap().as_ref()
        && now.duration_since(*ts).as_secs() < CONFIG_TTL_SECS
    {
        return config.clone();
    }

    // Slow path: re-read from disk.
    let config: LayoutConfig = config_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| {
            let result: Result<LayoutConfig, _> = serde_json::from_str(&s);
            if let Err(ref e) = result {
                debug_log(&format!("config parse error: {e}"));
            }
            result.ok()
        })
        .unwrap_or_default();

    *CONFIG_CACHE.lock().unwrap() = Some((config.clone(), now));
    config
}

/// Build a `Row` from a list of `BlockSpec`s. Returns `None` if no blocks
/// resolved (every spec was unknown).
fn build_row(specs: Vec<BlockSpec>) -> Option<crate::block::Row> {
    let mut blocks: Vec<Box<dyn crate::block::Block>> = Vec::new();

    for spec in specs {
        match spec {
            BlockSpec::Simple(name) => {
                if let Some(b) = make_block(&name) {
                    blocks.push(b);
                } else {
                    debug_log(&format!("unknown block: {name}"));
                }
            }
            BlockSpec::Group { guard, members } => {
                let Some(guard_block) = make_block(&guard) else {
                    debug_log(&format!("unknown guard block: {guard}"));
                    continue;
                };
                let member_blocks: Vec<Box<dyn crate::block::Block>> =
                    members.iter().filter_map(|m| make_block(m)).collect();
                blocks.push(Box::new(make_group(guard_block, member_blocks)));
            }
        }
    }

    if blocks.is_empty() {
        None
    } else {
        Some(make_row(blocks))
    }
}

/// Build a `Layout` from the config.
pub(crate) fn build_layout(config: &LayoutConfig) -> Layout {
    let rows = config
        .rows
        .iter()
        .filter_map(|specs| build_row(specs.clone()))
        .collect();
    Layout::new(rows)
}
