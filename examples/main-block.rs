use chrono::{DateTime, Utc};
use owo_colors::OwoColorize;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

// ─── Catppuccin Mocha palette ─────────────────────────────────────────────────
mod palette {
    pub const CYAN: (u8, u8, u8) = (137, 220, 235);
    pub const YELLOW: (u8, u8, u8) = (249, 226, 175);
    pub const GREEN: (u8, u8, u8) = (166, 227, 161);
    pub const RED: (u8, u8, u8) = (243, 139, 168);
    pub const GRAY: (u8, u8, u8) = (108, 112, 134);
    pub const WHITE: (u8, u8, u8) = (205, 214, 244);
    pub const DIM: (u8, u8, u8) = (110, 110, 110);
    // Progress bar — darker variants for transparent-bg readability
    pub const BAR_GREEN: (u8, u8, u8) = (74, 153, 90);
    pub const BAR_YELLOW: (u8, u8, u8) = (180, 140, 40);
    pub const BAR_RED: (u8, u8, u8) = (180, 60, 60);
    pub const BAR_BG: (u8, u8, u8) = (66, 84, 42);
}

// ─── Color helpers ────────────────────────────────────────────────────────────

fn col(s: &str, (r, g, b): (u8, u8, u8)) -> String {
    format!("{}", s.truecolor(r, g, b))
}

fn col_bold(s: &str, (r, g, b): (u8, u8, u8)) -> String {
    format!("{}", s.truecolor(r, g, b).bold())
}

fn dim_sep() -> String {
    col(" │ ", palette::DIM)
}

// ─── Progress bar ─────────────────────────────────────────────────────────────

fn make_bar(used_pct: u32, width: usize) -> String {
    let filled = ((used_pct as usize) * width) / 100;
    let empty = width - filled;

    let fg = if used_pct < 50 {
        palette::BAR_GREEN
    } else if used_pct < 75 {
        palette::BAR_YELLOW
    } else {
        palette::BAR_RED
    };

    let (br, bg, bb) = palette::BAR_BG;
    let (dr, dg, db) = palette::DIM;

    let filled_str = "█"
        .repeat(filled)
        .truecolor(fg.0, fg.1, fg.2)
        .on_truecolor(br, bg, bb)
        .to_string();

    let empty_str = "░"
        .repeat(empty)
        .truecolor(dr, dg, db)
        .on_truecolor(br, bg, bb)
        .to_string();

    format!("{filled_str}{empty_str}")
}

// ═══════════════════════════════════════════════════════════════════════════════
// Block system
// ═══════════════════════════════════════════════════════════════════════════════

/// A Block renders itself into an optional string.
/// Returning `None` means "nothing to show" — the block is skipped entirely.
trait Block {
    fn render(&self, ctx: &RenderCtx) -> Option<String>;
}

/// Shared data passed to every block at render time.
struct RenderCtx<'a> {
    input: &'a ClaudeInput,
    cache: &'a Cache,
}

/// A Row is an ordered list of blocks joined by `dim_sep()`.
/// Empty blocks (those that return `None`) are automatically skipped.
struct Row(Vec<Box<dyn Block>>);

impl Row {
    fn render(&self, ctx: &RenderCtx) -> Option<String> {
        let parts: Vec<String> = self.0.iter().filter_map(|b| b.render(ctx)).collect();
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(&dim_sep()))
        }
    }
}

/// Top-level layout: an ordered list of rows, each printed on its own line.
struct Layout(Vec<Row>);

impl Layout {
    fn print(&self, ctx: &RenderCtx) {
        let lines: Vec<String> = self.0.iter().filter_map(|row| row.render(ctx)).collect();
        println!("{}", lines.join("\n"));
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Block implementations
// ═══════════════════════════════════════════════════════════════════════════════

/// Model name (bold cyan)
struct ModelBlock;
impl Block for ModelBlock {
    fn render(&self, ctx: &RenderCtx) -> Option<String> {
        let name = &ctx.input.model.display_name;
        if name.is_empty() {
            return None;
        }
        Some(col_bold(name, palette::CYAN))
    }
}

/// Context window bar + percentage/size label
struct ContextBarBlock;
impl Block for ContextBarBlock {
    fn render(&self, ctx: &RenderCtx) -> Option<String> {
        let cw = &ctx.input.context_window;
        let used_pct = cw.used_percentage.unwrap_or(0);
        let size_k = cw.context_window_size / 1000;
        let bar = make_bar(used_pct, 10);
        let label = col(&format!("{used_pct}%/{size_k}k"), palette::WHITE);
        Some(format!("{bar} {label}"))
    }
}

/// Session cost (yellow bold)
struct CostBlock;
impl Block for CostBlock {
    fn render(&self, ctx: &RenderCtx) -> Option<String> {
        let cost = ctx.input.cost.total_cost_usd;
        Some(col_bold(&format!("${cost:.2}"), palette::YELLOW))
    }
}

/// 5-hour session usage percentage
struct SessionUsageBlock;
impl Block for SessionUsageBlock {
    fn render(&self, ctx: &RenderCtx) -> Option<String> {
        let pct = ctx.cache.session_pct?;
        let color = usage_color(pct);
        Some(col(&format!("~{pct}%"), color))
    }
}

/// Countdown to 5-hour window reset: "28m→02:00"
struct SessionResetBlock;
impl Block for SessionResetBlock {
    fn render(&self, ctx: &RenderCtx) -> Option<String> {
        let reset_secs = ctx.cache.session_reset_secs?;
        if reset_secs == 0 {
            return Some(col("resetting…", palette::GREEN));
        }
        let m = reset_secs / 60;
        let reset_epoch = now_epoch() + reset_secs;
        let dt = DateTime::<Utc>::from_timestamp(reset_epoch as i64, 0).unwrap_or_default();
        let hhmm = format!("{:02}:{:02}", dt.format("%H"), dt.format("%M"));
        Some(col(&format!("{m}m→{hhmm}"), palette::GRAY))
    }
}

/// Daily spend + daily average from extra credits: "$9.6/$1.4D"
struct DailySpendBlock;
impl Block for DailySpendBlock {
    fn render(&self, ctx: &RenderCtx) -> Option<String> {
        let used_c = ctx.cache.extra_used_cents?;
        let limit_c = ctx.cache.extra_limit_cents?;
        if limit_c <= 0.0 {
            return None;
        }
        let spent = used_c / 100.0;
        let daily_avg = spent / 30.0;
        Some(col(&format!("${spent:.1}/${daily_avg:.1}D"), palette::GRAY))
    }
}

/// Cost burn rate: "● $2.6/h" (only shown after 1h of session time)
struct BurnRateBlock;
impl Block for BurnRateBlock {
    fn render(&self, ctx: &RenderCtx) -> Option<String> {
        let ms = ctx.input.cost.total_duration_ms;
        let cost = ctx.input.cost.total_cost_usd;
        if ms < 3_600_000 || cost == 0.0 {
            return None;
        }
        let rate = cost / (ms as f64 / 3_600_000.0);
        let dot_color = if rate < 1.0 {
            palette::GREEN
        } else if rate < 3.0 {
            palette::YELLOW
        } else {
            palette::RED
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
        let color = if spent > 0.0 {
            palette::YELLOW
        } else {
            palette::GRAY
        };
        Some(col(&format!("${spent:.2}"), color))
    }
}

/// Input / output token counts: "i:1k/o:340"
struct TokenCountBlock;
impl Block for TokenCountBlock {
    fn render(&self, ctx: &RenderCtx) -> Option<String> {
        let usage = ctx.input.context_window.current_usage.as_ref()?;
        let i = usage.input_tokens?;
        let o = usage.output_tokens?;
        Some(format!(
            "{}{}{}{}",
            col("i:", palette::GRAY),
            col(&fmt_token(i), palette::WHITE),
            col("/o:", palette::GRAY),
            col(&fmt_token(o), palette::WHITE),
        ))
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
        Some(col(&fmt_duration_ms(ms), palette::GRAY))
    }
}

/// Git diff lines added/removed: "+5/-2"
/// Prefers values from `cost` struct; falls back to live `git diff --numstat HEAD`.
struct GitDiffBlock;
impl Block for GitDiffBlock {
    fn render(&self, ctx: &RenderCtx) -> Option<String> {
        let (add, del) = {
            let la = ctx.input.cost.total_lines_added;
            let lr = ctx.input.cost.total_lines_removed;
            if la != 0 || lr != 0 {
                (la, lr)
            } else {
                git_diff_stat(&ctx.input.workspace.current_dir)
            }
        };
        Some(format!(
            "{}/{}",
            col(&format!("+{add}"), palette::GREEN),
            col(&format!("-{del}"), palette::RED),
        ))
    }
}

/// Current directory basename + git branch: "myproject (main)"
struct DirBlock;
impl Block for DirBlock {
    fn render(&self, ctx: &RenderCtx) -> Option<String> {
        let cwd = &ctx.input.workspace.current_dir;
        let dir = Path::new(cwd)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("~");
        let branch = git_branch(cwd)
            .map(|b| format!(" {}", col(&format!("({b})"), palette::GRAY)))
            .unwrap_or_default();
        Some(format!("{}{}", col(dir, palette::WHITE), branch))
    }
}

struct VimModeBlock;
impl Block for VimModeBlock {
    fn render(&self, ctx: &RenderCtx) -> Option<String> {
        let mode = ctx.input.vim.as_ref()?.mode.as_str();
        Some(col(&format!("[{mode}]"), palette::CYAN))
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Input structs
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, Default)]
struct ClaudeInput {
    cwd: String,
    session_id: String,
    transcript_path: String,
    model: ModelInfo,
    workspace: Workspace,
    version: String,
    output_style: OutputStyle,
    cost: Cost,
    context_window: ContextWindow,
    #[serde(skip_serializing_if = "Option::is_none")]
    vim: Option<Vim>,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent: Option<Agent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    worktree: Option<Worktree>,
}

#[derive(Debug, Deserialize, Default)]
struct ModelInfo {
    id: String,
    display_name: String,
}

#[derive(Debug, Deserialize, Default)]
struct Workspace {
    current_dir: String,
    project_dir: String,
}
#[derive(Debug, Deserialize, Default)]
struct OutputStyle {
    name: String,
}

#[derive(Debug, Deserialize, Default)]
struct ContextWindow {
    context_window_size: u64,
    used_percentage: Option<u32>,
    current_usage: Option<CurrentUsage>,
}

#[derive(Debug, Deserialize, Default)]
struct CurrentUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
}

#[derive(Debug, Deserialize, Default)]
struct Cost {
    total_cost_usd: f64,
    total_duration_ms: u64,
    total_lines_added: i64,
    total_lines_removed: i64,
}
#[derive(Debug, Deserialize)]
struct Vim {
    mode: String,
}
#[derive(Debug, Deserialize)]
struct Agent {
    name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Worktree {
    pub name: String,
    pub path: String,
    pub branch: String,
    pub original_cwd: String,
    pub original_branch: String,
}
// ─── Usage API / Cache ────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Default)]
struct Cache {
    timestamp: u64,
    session_pct: Option<u32>,
    session_reset_secs: Option<u64>,
    weekly_pct: Option<u32>,
    weekly_reset_secs: Option<u64>,
    extra_used_cents: Option<f64>,
    extra_limit_cents: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct UsageResponse {
    five_hour: Option<UsagePeriod>,
    seven_day: Option<UsagePeriod>,
    extra_usage: Option<ExtraUsage>,
}

#[derive(Debug, Deserialize)]
struct UsagePeriod {
    utilization: Option<f64>,
    resets_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ExtraUsage {
    used_credits: Option<f64>,
    monthly_limit: Option<f64>,
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn iso_to_epoch(s: &str) -> Option<u64> {
    let trimmed = s.split('.').next().unwrap_or(s).trim_end_matches('Z');
    format!("{trimmed}Z")
        .parse::<DateTime<Utc>>()
        .ok()
        .map(|dt| dt.timestamp() as u64)
}

fn fmt_duration_ms(ms: u64) -> String {
    let total = ms / 1000;
    let m = total / 60;
    let s = total % 60;
    if m > 0 {
        format!("{m}m{s:02}s")
    } else {
        format!("{s}s")
    }
}

fn fmt_token(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.0}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

fn usage_color(pct: u32) -> (u8, u8, u8) {
    if pct < 50 {
        palette::GREEN
    } else if pct < 75 {
        palette::YELLOW
    } else {
        palette::RED
    }
}

// ─── OAuth + Usage API ────────────────────────────────────────────────────────

fn get_oauth_token() -> Option<String> {
    if let Ok(t) = std::env::var("CLAUDE_CODE_OAUTH_TOKEN") {
        if !t.is_empty() {
            return Some(t);
        }
    }
    if cfg!(target_os = "macos") {
        if let Ok(out) = Command::new("security")
            .args([
                "find-generic-password",
                "-s",
                "Claude Code-credentials",
                "-w",
            ])
            .output()
        {
            if out.status.success() {
                let blob = String::from_utf8_lossy(&out.stdout);
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(blob.trim()) {
                    if let Some(t) = v["claudeAiOauth"]["accessToken"].as_str() {
                        if !t.is_empty() && t != "null" {
                            return Some(t.to_owned());
                        }
                    }
                }
            }
        }
    }
    if let Some(path) = dirs::home_dir().map(|h| h.join(".claude/.credentials.json")) {
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(t) = v["claudeAiOauth"]["accessToken"].as_str() {
                    if !t.is_empty() && t != "null" {
                        return Some(t.to_owned());
                    }
                }
            }
        }
    }
    if let Ok(out) = Command::new("secret-tool")
        .args(["lookup", "service", "Claude Code-credentials"])
        .output()
    {
        if out.status.success() {
            let blob = String::from_utf8_lossy(&out.stdout);
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(blob.trim()) {
                if let Some(t) = v["claudeAiOauth"]["accessToken"].as_str() {
                    if !t.is_empty() && t != "null" {
                        return Some(t.to_owned());
                    }
                }
            }
        }
    }
    None
}

const CACHE_TTL: u64 = 300;

fn cache_path() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude/statusline_cache.json"))
}

fn load_cache() -> Cache {
    cache_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_cache(c: &Cache) {
    if let Some(path) = cache_path() {
        if let Ok(json) = serde_json::to_string(c) {
            let _ = std::fs::write(path, json);
        }
    }
}

fn fetch_usage() -> Cache {
    let now = now_epoch();
    let mut cache = load_cache();
    if cache.timestamp > 0 && now.saturating_sub(cache.timestamp) < CACHE_TTL {
        return cache;
    }
    let token = match get_oauth_token() {
        Some(t) => t,
        None => {
            cache.timestamp = now;
            save_cache(&cache);
            return cache;
        }
    };
    let resp = ureq::get("https://api.anthropic.com/api/oauth/usage")
        .header("Authorization", &format!("Bearer {token}"))
        .header("anthropic-beta", "oauth-2025-04-20")
        .call();
    let usage: UsageResponse = match resp {
        Ok(r) => match r.into_body().read_json() {
            Ok(u) => u,
            Err(_) => {
                cache.timestamp = now;
                save_cache(&cache);
                return cache;
            }
        },
        Err(_) => {
            cache.timestamp = now;
            save_cache(&cache);
            return cache;
        }
    };
    cache.timestamp = now;
    if let Some(fh) = &usage.five_hour {
        cache.session_pct = fh.utilization.map(|u| u as u32);
        cache.session_reset_secs = fh
            .resets_at
            .as_deref()
            .and_then(iso_to_epoch)
            .map(|e| e.saturating_sub(now));
    }
    if let Some(sd) = &usage.seven_day {
        cache.weekly_pct = sd.utilization.map(|u| u as u32);
        cache.weekly_reset_secs = sd
            .resets_at
            .as_deref()
            .and_then(iso_to_epoch)
            .map(|e| e.saturating_sub(now));
    }
    if let Some(ex) = &usage.extra_usage {
        cache.extra_used_cents = ex.used_credits;
        cache.extra_limit_cents = ex.monthly_limit;
    }
    save_cache(&cache);
    cache
}

// ─── Git helpers ──────────────────────────────────────────────────────────────

fn git_branch(cwd: &str) -> Option<String> {
    Command::new("git")
        .args(["-C", cwd, "branch", "--show-current"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .filter(|b| !b.is_empty())
}

fn git_diff_stat(cwd: &str) -> (i64, i64) {
    let Ok(out) = Command::new("git")
        .args(["-C", cwd, "diff", "--numstat", "HEAD"])
        .output()
    else {
        return (0, 0);
    };
    if !out.status.success() {
        return (0, 0);
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .fold((0, 0), |(a, r), line| {
            let p: Vec<&str> = line.split_whitespace().collect();
            if p.len() >= 2 {
                (
                    a + p[0].parse::<i64>().unwrap_or(0),
                    r + p[1].parse::<i64>().unwrap_or(0),
                )
            } else {
                (a, r)
            }
        })
}

// ─── Main ─────────────────────────────────────────────────────────────────────

fn main() {
    let mut buf = String::new();
    let _ = std::io::stdin().read_to_string(&mut buf);
    let input: ClaudeInput = serde_json::from_str(&buf).unwrap_or_default();
    let cache = fetch_usage();
    let ctx = RenderCtx {
        input: &input,
        cache: &cache,
    };

    // ── Layout definition ────────────────────────────────────────────────────
    // To add a new block: implement Block, then drop it into the row you want.
    // To reorder: move it within the vec. To remove: delete the line.
    let layout = Layout(vec![
        Row(vec![
            Box::new(ModelBlock),
            Box::new(ContextBarBlock),
            Box::new(CostBlock),
        ]),
        Row(vec![
            Box::new(SessionUsageBlock),
            Box::new(SessionResetBlock),
            Box::new(DailySpendBlock),
            Box::new(BurnRateBlock),
        ]),
        Row(vec![
            Box::new(ExtraCreditsBlock),
            Box::new(TokenCountBlock),
            Box::new(DurationBlock),
            Box::new(GitDiffBlock),
            Box::new(VimModeBlock),
            Box::new(DirBlock),
        ]),
    ]);

    layout.print(&ctx);
}
