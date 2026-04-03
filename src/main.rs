use chrono::{NaiveDateTime, TimeZone, Utc};
use owo_colors::OwoColorize;
use serde::{Deserialize, Serialize};
use std::fmt::Write;
use std::io::Read;
use std::path::Path;
use std::process::Command;

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

/// Write colored string to buffer, avoiding intermediate allocations.
fn col_to(buf: &mut String, s: &str, (r, g, b): (u8, u8, u8)) {
    write!(buf, "{}", s.truecolor(r, g, b)).unwrap();
}

/// Write bold colored string to buffer.
fn col_bold_to(buf: &mut String, s: &str, (r, g, b): (u8, u8, u8)) {
    write!(buf, "{}", s.truecolor(r, g, b).bold()).unwrap();
}

/// Convenience: returns colored string (for simple cases where buffer isn't worth it).
fn col(s: &str, rgb: (u8, u8, u8)) -> String {
    format!("{}", s.truecolor(rgb.0, rgb.1, rgb.2))
}

// ─── Progress bar ─────────────────────────────────────────────────────────────

fn make_bar(used_pct: u32, width: usize) -> String {
    let filled = ((used_pct as usize) * width) / 100;

    let fg = if used_pct < 50 {
        palette::BAR_GREEN
    } else if used_pct < 75 {
        palette::BAR_YELLOW
    } else {
        palette::BAR_RED
    };

    let (br, bg, bb) = palette::BAR_BG;
    let (dr, dg, db) = palette::DIM;

    // Pre-allocate: each char needs ~9 ANSI codes + 1 char
    let mut s = String::with_capacity(width * 12);
    for _ in 0..filled {
        write!(
            s,
            "{}",
            "█".truecolor(fg.0, fg.1, fg.2).on_truecolor(br, bg, bb)
        )
        .unwrap();
    }
    for _ in 0..(width - filled) {
        write!(
            s,
            "{}",
            "░".truecolor(dr, dg, db).on_truecolor(br, bg, bb)
        )
        .unwrap();
    }
    s
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn now_epoch() -> u64 {
    Utc::now().timestamp() as u64
}

fn iso_to_epoch(s: &str) -> Option<u64> {
    // Handle ISO format with optional fractional seconds
    let s = s.trim_end_matches('Z');
    let s = if s.contains('.') {
        // Truncate fractional seconds to 3 digits for chrono
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() == 2 {
            let frac = parts[1].chars().take(3).collect::<String>();
            format!("{}.{}Z", parts[0], frac)
        } else {
            format!("{}Z", s)
        }
    } else {
        format!("{}Z", s)
    };
    NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S%.fZ")
        .ok()
        .map(|dt| dt.and_utc().timestamp() as u64)
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
        // Integer math: divide by 100000 to get tenths of millions
        let tenths = n / 100_000;
        let whole = tenths / 10;
        let frac = tenths % 10;
        format!("{whole}.{frac}M")
    } else if n >= 1_000 {
        // Integer math: divide by 100 to get tenths of thousands
        let tenths = n / 100;
        let whole = tenths / 10;
        let frac = tenths % 10;
        if frac == 0 {
            format!("{whole}k")
        } else {
            format!("{whole}.{frac}k")
        }
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

fn debug_log(msg: &str) {
    if std::env::var("STATUSLINE_DEBUG").is_ok() {
        if let Some(path) = dirs::home_dir().map(|h| h.join(".claude/statusline_debug.log")) {
            let line = format!("[{}] {}\n", now_epoch(), msg);
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
            {
                let _ = f.write_all(line.as_bytes());
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Input structs
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, Default)]
struct ClaudeInput {
    model: ModelInfo,
    workspace: Workspace,
    cost: Cost,
    context_window: ContextWindow,
}

#[derive(Debug, Deserialize, Default)]
struct ModelInfo {
    display_name: String,
}

#[derive(Debug, Deserialize, Default)]
struct Workspace {
    current_dir: String,
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

fn parse_input(buf: &str) -> ClaudeInput {
    match serde_json::from_str(buf) {
        Ok(v) => v,
        Err(e) => {
            debug_log(&format!("parse_input error: {e}\n---\n{buf}"));
            ClaudeInput::default()
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Derived context  — computed once, shared across all blocks
// ═══════════════════════════════════════════════════════════════════════════════

struct DerivedCtx {
    /// basename of workspace.current_dir
    dir_name: String,
    /// current git branch, if any
    git_branch: Option<String>,
    /// (lines_added, lines_removed) from cost struct or live git diff
    git_diff: (i64, i64),
}

impl DerivedCtx {
    fn build(input: &ClaudeInput) -> Self {
        let cwd = &input.workspace.current_dir;

        let dir_name = Path::new(cwd)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("~")
            .to_owned();

        let git_branch = git_branch(cwd);

        // Prefer values already in the cost struct; only shell out if both are 0.
        let git_diff = {
            let la = input.cost.total_lines_added;
            let lr = input.cost.total_lines_removed;
            if la != 0 || lr != 0 {
                (la, lr)
            } else {
                git_diff_stat(cwd)
            }
        };

        DerivedCtx {
            dir_name,
            git_branch,
            git_diff,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Block system
// ═══════════════════════════════════════════════════════════════════════════════

/// Shared data passed to every block at render time.
struct RenderCtx<'a> {
    input: &'a ClaudeInput,
    cache: &'a Cache,
    derived: &'a DerivedCtx,
}

/// A Block renders itself into an optional string.
/// Returning `None` means "nothing to show" — the block is skipped entirely.
trait Block {
    fn render(&self, ctx: &RenderCtx) -> Option<String>;
}

// ── Row ───────────────────────────────────────────────────────────────────────

struct Row {
    blocks: Vec<Box<dyn Block>>,
    separator: String,
}

impl Row {
    /// Standard row: blocks separated by a dim │
    fn new(blocks: Vec<Box<dyn Block>>) -> Self {
        Row {
            blocks,
            separator: col(" │ ", palette::DIM),
        }
    }

    fn render(&self, ctx: &RenderCtx) -> Option<String> {
        let parts: Vec<String> = self.blocks.iter().filter_map(|b| b.render(ctx)).collect();
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(&self.separator))
        }
    }
}

// ── BlockGroup ────────────────────────────────────────────────────────────────

/// A group where the first block acts as a guard: if it returns `None`,
/// the entire group is suppressed (including the remaining members).
/// Useful for "only show reset countdown when usage data exists".
struct BlockGroup {
    guard: Box<dyn Block>,
    members: Vec<Box<dyn Block>>,
    separator: String,
}

impl BlockGroup {
    fn new(guard: Box<dyn Block>, members: Vec<Box<dyn Block>>) -> Self {
        BlockGroup {
            guard,
            members,
            separator: col(" │ ", palette::DIM),
        }
    }
}

impl Block for BlockGroup {
    fn render(&self, ctx: &RenderCtx) -> Option<String> {
        // Guard must produce output; if not, the whole group is suppressed.
        let guard_out = self.guard.render(ctx)?;
        let mut parts = vec![guard_out];
        parts.extend(self.members.iter().filter_map(|b| b.render(ctx)));
        Some(parts.join(&self.separator))
    }
}

// ── Layout ────────────────────────────────────────────────────────────────────

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
        let mut s = String::with_capacity(name.len() + 20);
        col_bold_to(&mut s, name, palette::CYAN);
        Some(s)
    }
}

/// Context window bar + percentage/size label.
/// `width` controls the bar character count.
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
        col_to(&mut s, &format!("{used_pct}%/{size_k}k"), palette::WHITE);
        Some(s)
    }
}

/// Session cost (yellow bold)
struct CostBlock;
impl Block for CostBlock {
    fn render(&self, ctx: &RenderCtx) -> Option<String> {
        let cost = ctx.input.cost.total_cost_usd;
        let mut s = String::with_capacity(16);
        col_bold_to(&mut s, &format!("${cost:.2}"), palette::YELLOW);
        Some(s)
    }
}

/// 5-hour session usage percentage.
/// Acts as the guard for `SessionGroup` — if this returns None, reset/spend
/// blocks are suppressed too.
struct SessionUsageBlock;
impl Block for SessionUsageBlock {
    fn render(&self, ctx: &RenderCtx) -> Option<String> {
        let pct = ctx.cache.session_pct?;
        Some(col(&format!("~{pct}%"), usage_color(pct)))
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
        let dt = Utc.timestamp_opt(reset_epoch as i64, 0).single().unwrap_or_default();
        let hhmm = dt.format("%H:%M").to_string();
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

/// Cost burn rate: "● $2.6/h".
/// `min_session_ms` controls how long a session must run before the rate appears.
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
        let mut s = String::with_capacity(32);
        col_to(&mut s, "i:", palette::GRAY);
        col_to(&mut s, &fmt_token(i), palette::WHITE);
        col_to(&mut s, "/o:", palette::GRAY);
        col_to(&mut s, &fmt_token(o), palette::WHITE);
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
        Some(col(&fmt_duration_ms(ms), palette::GRAY))
    }
}

/// Git diff lines added/removed: "+5/-2"
/// Data comes from DerivedCtx (computed once, not per-block).
struct GitDiffBlock;
impl Block for GitDiffBlock {
    fn render(&self, ctx: &RenderCtx) -> Option<String> {
        let (add, del) = ctx.derived.git_diff;
        let mut s = String::with_capacity(20);
        col_to(&mut s, &format!("+{add}"), palette::GREEN);
        s.push('/');
        col_to(&mut s, &format!("-{del}"), palette::RED);
        Some(s)
    }
}

/// Current directory basename + git branch: "myproject (main)"
/// Data comes from DerivedCtx (computed once, not per-block).
struct DirBlock;
impl Block for DirBlock {
    fn render(&self, ctx: &RenderCtx) -> Option<String> {
        let mut s = String::with_capacity(64);
        col_to(&mut s, &ctx.derived.dir_name, palette::WHITE);
        if let Some(b) = ctx.derived.git_branch.as_deref() {
            s.push(' ');
            col_to(&mut s, &format!("({b})"), palette::GRAY);
        }
        Some(s)
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Usage API / Cache
// ═══════════════════════════════════════════════════════════════════════════════

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

fn cache_needs_refresh(cache: &Cache) -> bool {
    let now = now_epoch();
    cache.timestamp == 0 || now.saturating_sub(cache.timestamp) >= CACHE_TTL
}

// ─── OAuth token resolution ───────────────────────────────────────────────────

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

// ─── Async cache refresh ──────────────────────────────────────────────────────

/// Fetch from the API and write to the cache file.
/// Only ever called from a background thread.
fn fetch_and_save_cache() {
    let now = now_epoch();

    // Re-check inside the thread: another invocation may have refreshed already.
    let mut cache = load_cache();
    if !cache_needs_refresh(&cache) {
        return;
    }

    let token = match get_oauth_token() {
        Some(t) => t,
        None => {
            debug_log("fetch_and_save_cache: no oauth token found");
            cache.timestamp = now;
            save_cache(&cache);
            return;
        }
    };

    let resp = ureq::get("https://api.anthropic.com/api/oauth/usage")
        .header("Authorization", &format!("Bearer {token}"))
        .header("anthropic-beta", "oauth-2025-04-20")
        .call();

    let usage: UsageResponse = match resp {
        Ok(r) => match r.into_body().read_json() {
            Ok(u) => u,
            Err(e) => {
                debug_log(&format!("fetch_and_save_cache: json parse error: {e}"));
                cache.timestamp = now;
                save_cache(&cache);
                return;
            }
        },
        Err(e) => {
            debug_log(&format!("fetch_and_save_cache: http error: {e}"));
            cache.timestamp = now;
            save_cache(&cache);
            return;
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
    debug_log("fetch_and_save_cache: done");
}

/// Kick off a background refresh. The JoinHandle is dropped (detached);
/// the main thread never waits. The *next* invocation reads the fresh cache.
fn refresh_cache_background() {
    let _ = std::thread::spawn(fetch_and_save_cache);
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

// ═══════════════════════════════════════════════════════════════════════════════
// Main
// ═══════════════════════════════════════════════════════════════════════════════

fn main() {
    let mut buf = String::new();
    let _ = std::io::stdin().read_to_string(&mut buf);
    let input = parse_input(&buf);

    // Load cache instantly (file read only). Stale → background refresh;
    // this invocation renders with the old data, next one gets the fresh data.
    let cache = load_cache();
    if cache_needs_refresh(&cache) {
        refresh_cache_background();
    }

    // Compute derived data once — shared across all blocks, no repeated git calls.
    let derived = DerivedCtx::build(&input);

    let ctx = RenderCtx {
        input: &input,
        cache: &cache,
        derived: &derived,
    };

    // ── Layout ───────────────────────────────────────────────────────────────
    //
    // Adding a block:  implement Block, drop it into the row you want.
    // Reordering:      move it within the vec.
    // Removing:        delete the line.
    // Custom separator: Row::with_sep(blocks, "  ") instead of Row::new(blocks).
    // Conditional group: BlockGroup::new(guard, members) — members only render
    //                    when guard returns Some.
    //
    let layout = Layout(vec![
        // Row 1: model · context bar · cost
        Row::new(vec![
            Box::new(ModelBlock),
            Box::new(ContextBarBlock::default()),
            Box::new(CostBlock),
        ]),
        // Row 2: session usage (guard) + reset/spend/rate as a group.
        // If session data is absent the whole row is suppressed.
        Row::new(vec![Box::new(BlockGroup::new(
            Box::new(SessionUsageBlock),
            vec![
                Box::new(SessionResetBlock),
                Box::new(DailySpendBlock),
                Box::new(BurnRateBlock::default()),
            ],
        ))]),
        // Row 3: extra credits · tokens · duration · git diff · dir
        Row::new(vec![
            Box::new(ExtraCreditsBlock),
            Box::new(TokenCountBlock),
            Box::new(DurationBlock),
            Box::new(GitDiffBlock),
            Box::new(DirBlock),
        ]),
    ]);

    layout.print(&ctx);
}
