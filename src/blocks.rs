//! All concrete block implementations and the registry that maps names to constructors.

use crate::block::{Block, BlockGroup, RenderCtx, Row};
use crate::color::{
    col, col_bold_to, col_to, make_bar, usage_color, CYAN, GRAY, GREEN, RED, WHITE, YELLOW,
};
use crate::format::{fmt_duration_ms, fmt_token, now_epoch};
use chrono::{TimeZone, Utc};
use std::collections::HashMap;
use std::sync::OnceLock;

// ─── Block implementations ────────────────────────────────────────────────────

/// Model name (bold cyan)
struct ModelBlock;
impl Block for ModelBlock {
    fn render(&self, ctx: &RenderCtx) -> Option<String> {
        let name = &ctx.input.model.display_name;
        if name.is_empty() {
            return None;
        }
        let mut s = String::with_capacity(name.len() + 20);
        col_bold_to(&mut s, name, CYAN);
        Some(s)
    }
}

/// Context window bar + percentage/size label. `width` controls the bar char count.
struct ContextBarBlock {
    width: usize,
}
impl Default for ContextBarBlock {
    fn default() -> Self {
        ContextBarBlock { width: 10 }
    }
}
impl Block for ContextBarBlock {
    fn render(&self, ctx: &RenderCtx) -> Option<String> {
        let cw = &ctx.input.context_window;
        let used_pct = cw.used_percentage.unwrap_or(0);
        let size_k = cw.context_window_size / 1000;
        let bar = make_bar(used_pct, self.width);
        let mut s = String::with_capacity(bar.len() + 20);
        s.push_str(&bar);
        s.push(' ');
        col_to(&mut s, format_args!("{used_pct}%/{size_k}k"), WHITE);
        Some(s)
    }
}

/// Session cost (yellow bold)
struct CostBlock;
impl Block for CostBlock {
    fn render(&self, ctx: &RenderCtx) -> Option<String> {
        let cost = ctx.input.cost.total_cost_usd;
        let mut s = String::with_capacity(16);
        col_bold_to(&mut s, format_args!("${cost:.2}"), YELLOW);
        Some(s)
    }
}

/// 5-hour session usage percentage. Acts as the guard for `SessionGroup` — if
/// this returns None, reset/spend blocks are suppressed too.
struct SessionUsageBlock;
impl Block for SessionUsageBlock {
    fn render(&self, ctx: &RenderCtx) -> Option<String> {
        let pct = ctx.cache.session_pct?;
        Some(col(format_args!("~{pct}%"), usage_color(pct)))
    }
}

/// Countdown to 5-hour window reset: "28m→02:00"
struct SessionResetBlock;
impl Block for SessionResetBlock {
    fn render(&self, ctx: &RenderCtx) -> Option<String> {
        let reset_secs = ctx.cache.session_reset_secs?;
        if reset_secs == 0 {
            return Some(col("resetting…", GREEN));
        }
        let m = reset_secs / 60;
        let reset_epoch = now_epoch() + reset_secs;
        let dt = Utc
            .timestamp_opt(reset_epoch as i64, 0)
            .single()
            .unwrap_or_default();
        let hhmm = dt.format("%H:%M").to_string();
        Some(col(format_args!("{m}m→{hhmm}"), GRAY))
    }
}

/// Daily spend + daily average from extra credits: "$9.6/$1.4D"
struct DailySpendBlock;
impl Block for DailySpendBlock {
    fn render(&self, ctx: &RenderCtx) -> Option<String> {
        let (Some(used_c), Some(limit_c)) = (ctx.cache.extra_used_cents, ctx.cache.extra_limit_cents)
        else {
            return None;
        };
        if limit_c <= 0.0 {
            return None;
        }
        let spent = used_c / 100.0;
        let daily_avg = spent / 30.0;
        Some(col(format_args!("${spent:.1}/${daily_avg:.1}D"), GRAY))
    }
}

/// Cost burn rate: "● $2.6/h". `min_session_ms` controls how long a session
/// must run before the rate appears.
struct BurnRateBlock {
    min_session_ms: u64,
}
impl Default for BurnRateBlock {
    fn default() -> Self {
        BurnRateBlock {
            min_session_ms: 3_600_000,
        }
    }
}
impl Block for BurnRateBlock {
    fn render(&self, ctx: &RenderCtx) -> Option<String> {
        let ms = ctx.input.cost.total_duration_ms;
        let cost = ctx.input.cost.total_cost_usd;
        if ms < self.min_session_ms || cost == 0.0 {
            return None;
        }
        let rate = cost / (ms as f64 / 3_600_000.0);
        let dot_color = if rate < 1.0 {
            GREEN
        } else if rate < 3.0 {
            YELLOW
        } else {
            RED
        };
        Some(format!("{} ${rate:.1}/h", col("●", dot_color)))
    }
}

/// Extra credits spent: "$0.00"
struct ExtraCreditsBlock;
impl Block for ExtraCreditsBlock {
    fn render(&self, ctx: &RenderCtx) -> Option<String> {
        let used_c = ctx.cache.extra_used_cents?;
        ctx.cache.extra_limit_cents?; // only show when limit is known
        let spent = used_c / 100.0;
        let color = if spent > 0.0 { YELLOW } else { GRAY };
        Some(col(format_args!("${spent:.2}"), color))
    }
}

/// Input / output token counts: "i:1k/o:340"
struct TokenCountBlock;
impl Block for TokenCountBlock {
    fn render(&self, ctx: &RenderCtx) -> Option<String> {
        let usage = ctx.input.context_window.current_usage.as_ref()?;
        let (Some(i), Some(o)) = (usage.input_tokens, usage.output_tokens) else {
            return None;
        };
        let mut s = String::with_capacity(32);
        col_to(&mut s, "i:", GRAY);
        col_to(&mut s, fmt_token(i), WHITE);
        col_to(&mut s, "/o:", GRAY);
        col_to(&mut s, fmt_token(o), WHITE);
        Some(s)
    }
}

/// Session wall-clock duration: "9m49s"
struct DurationBlock;
impl Block for DurationBlock {
    fn render(&self, ctx: &RenderCtx) -> Option<String> {
        let ms = ctx.input.cost.total_duration_ms;
        if ms == 0 {
            return None;
        }
        Some(col(fmt_duration_ms(ms), GRAY))
    }
}

/// Git diff lines added/removed: "+5/-2"
/// Data comes from DerivedCtx (computed once, not per-block).
struct GitDiffBlock;
impl Block for GitDiffBlock {
    fn render(&self, ctx: &RenderCtx) -> Option<String> {
        let (add, del) = ctx.derived.git_diff;
        let mut s = String::with_capacity(20);
        col_to(&mut s, format_args!("+{add}"), GREEN);
        s.push('/');
        col_to(&mut s, format_args!("-{del}"), RED);
        Some(s)
    }
}

/// Current directory basename + git branch: "myproject (main)"
/// Data comes from DerivedCtx (computed once, not per-block).
struct DirBlock;
impl Block for DirBlock {
    fn render(&self, ctx: &RenderCtx) -> Option<String> {
        let mut s = String::with_capacity(64);
        col_to(&mut s, &ctx.derived.dir_name, WHITE);
        if let Some(b) = ctx.derived.git_branch.as_deref() {
            s.push(' ');
            col_to(&mut s, format_args!("({b})"), GRAY);
        }
        Some(s)
    }
}

// ─── Registry ─────────────────────────────────────────────────────────────────

/// A factory function that creates a Block.
type BlockFactory = fn() -> Box<dyn Block>;

/// Cached registry of all available blocks by name.
pub(crate) static BLOCK_REGISTRY: OnceLock<HashMap<&'static str, BlockFactory>> = OnceLock::new();

pub(crate) fn block_registry() -> &'static HashMap<&'static str, BlockFactory> {
    BLOCK_REGISTRY.get_or_init(|| {
        let mut m: HashMap<&'static str, BlockFactory> = HashMap::new();

        m.insert("model", || Box::new(ModelBlock));
        m.insert("context_bar", || Box::new(ContextBarBlock::default()));
        m.insert("cost", || Box::new(CostBlock));
        m.insert("session_usage", || Box::new(SessionUsageBlock));
        m.insert("session_reset", || Box::new(SessionResetBlock));
        m.insert("daily_spend", || Box::new(DailySpendBlock));
        m.insert("burn_rate", || Box::new(BurnRateBlock::default()));
        m.insert("extra_credits", || Box::new(ExtraCreditsBlock));
        m.insert("tokens", || Box::new(TokenCountBlock));
        m.insert("duration", || Box::new(DurationBlock));
        m.insert("git_diff", || Box::new(GitDiffBlock));
        m.insert("dir", || Box::new(DirBlock));

        m
    })
}

/// Create a block by name. Returns `None` if name is unknown.
pub(crate) fn make_block(name: &str) -> Option<Box<dyn Block>> {
    block_registry().get(name).map(|f| f())
}

/// Wrap a guard block and its members into a `BlockGroup` for use in a `Row`.
/// Convenience for layout config parsing.
pub(crate) fn make_group(guard: Box<dyn Block>, members: Vec<Box<dyn Block>>) -> BlockGroup {
    BlockGroup::new(guard, members)
}

/// Wrap blocks into a `Row` for use in a `Layout`.
/// Convenience for layout config parsing.
pub(crate) fn make_row(blocks: Vec<Box<dyn Block>>) -> Row {
    Row::new(blocks)
}
