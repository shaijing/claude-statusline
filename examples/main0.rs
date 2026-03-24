use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

// ─── Catppuccin Mocha palette ─────────────────────────────────────────────────
const CYAN: &str = "\x1b[38;2;137;220;235m";
const YELLOW: &str = "\x1b[38;2;249;226;175m";
const GREEN: &str = "\x1b[38;2;166;227;161m";
const RED: &str = "\x1b[38;2;243;139;168m";
const GRAY: &str = "\x1b[38;2;108;112;134m";
const WHITE: &str = "\x1b[38;2;205;214;244m";
const DIM: &str = "\x1b[38;2;69;71;90m";
const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";

// ─── Claude Code stdin JSON ───────────────────────────────────────────────────
#[derive(Debug, Deserialize, Default)]
struct ClaudeInput {
    model: Option<ModelInfo>,
    workspace: Option<Workspace>,
    context_window: Option<ContextWindow>,
    cost: Option<Cost>,
    session: Option<Session>,
}

#[derive(Debug, Deserialize)]
struct ModelInfo {
    display_name: Option<String>,
    #[serde(rename = "id")]
    id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Workspace {
    current_dir: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ContextWindow {
    used_percentage: Option<f64>,
    remaining_percentage: Option<f64>,
    context_window_size: Option<u64>,
    current_usage: Option<CurrentUsage>,
}

#[derive(Debug, Deserialize)]
struct CurrentUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct Cost {
    total_cost_usd: Option<f64>,
    total_lines_added: Option<i64>,
    total_lines_removed: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct Session {
    turns: Option<u32>,
    total_turns: Option<u32>,
    duration_ms: Option<u64>,
    thinking_effort: Option<String>,
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
    let with_z = format!("{trimmed}Z");
    with_z
        .parse::<DateTime<Utc>>()
        .ok()
        .map(|dt| dt.timestamp() as u64)
}

/// Render a colored block bar. `used_pct` is 0–100.
fn make_bar(used_pct: u32, width: usize) -> String {
    let filled = ((used_pct as usize) * width) / 100;
    let empty = width - filled;

    let color = if used_pct < 50 {
        GREEN // 暗绿
    } else if used_pct < 75 {
        YELLOW // 暗黄
    } else {
        RED // 暗红
    };

    // 背景色：深色半透明感
    let bg = "\x1b[48;2;30;35;30m"; // 深暗绿底
    let bg_reset = "\x1b[49m"; // 只重置背景

    let bar_on = format!("{bg}{color}█{bg_reset}{RESET}").repeat(filled);
    let bar_off = format!("{bg}{DIM}░{bg_reset}{RESET}").repeat(empty);
    format!("{bar_on}{bar_off}")
}
// fn make_bar(used_pct: u32, width: usize) -> String {
//     let filled = ((used_pct as usize) * width) / 100;
//     let empty = width - filled;

//     // Color based on how much is *remaining*
//     let remaining = 100u32.saturating_sub(used_pct);
//     let color = if remaining > 50 {
//         GREEN
//     } else if remaining > 25 {
//         YELLOW
//     } else {
//         RED
//     };

//     let bar_on = "█".repeat(filled);
//     let bar_off = "░".repeat(empty);
//     format!("{color}{bar_on}{DIM}{bar_off}{RESET}")
// }

/// Format seconds into "Xh Ym" countdown.
fn fmt_countdown_hm(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    format!("{h}h {:02}m", m)
}

/// Format milliseconds into "Xm Ys".
fn fmt_duration_ms(ms: u64) -> String {
    let total_secs = ms / 1000;
    let m = total_secs / 60;
    let s = total_secs % 60;
    if m > 0 {
        format!("{m}m{s:02}s")
    } else {
        format!("{s}s")
    }
}

// ─── OAuth + Usage API ────────────────────────────────────────────────────────

fn get_oauth_token() -> Option<String> {
    if let Ok(t) = std::env::var("CLAUDE_CODE_OAUTH_TOKEN") {
        if !t.is_empty() {
            return Some(t);
        }
    }

    // macOS Keychain
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

    // Linux credentials file
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

    // GNOME Keyring
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
    let out = Command::new("git")
        .args(["-C", cwd, "diff", "--numstat", "HEAD"])
        .output();
    let mut added = 0i64;
    let mut removed = 0i64;
    if let Ok(o) = out {
        if o.status.success() {
            for line in String::from_utf8_lossy(&o.stdout).lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    added += parts[0].parse::<i64>().unwrap_or(0);
                    removed += parts[1].parse::<i64>().unwrap_or(0);
                }
            }
        }
    }
    (added, removed)
}

// ─── Line builder ─────────────────────────────────────────────────────────────

struct Lines(Vec<String>);

impl Lines {
    fn new() -> Self {
        Lines(Vec::new())
    }
    fn push(&mut self, s: String) {
        self.0.push(s);
    }
    /// Print all lines joined by newlines (Claude Code shows multi-line statuslines).
    fn print(&self) {
        println!("{}", self.0.join("\n"));
    }
}

fn sep() -> &'static str {
    // dim vertical bar separator
    "\x1b[38;2;69;71;90m │ \x1b[0m"
}

// ─── Main ─────────────────────────────────────────────────────────────────────

fn main() {
    let mut buf = String::new();
    let _ = std::io::stdin().read_to_string(&mut buf);
    let input: ClaudeInput = serde_json::from_str(&buf).unwrap_or_default();

    let cache = fetch_usage();
    let mut lines = Lines::new();

    // ── Convenience extractors ──────────────────────────────────────────────
    let cwd = input
        .workspace
        .as_ref()
        .and_then(|w| w.current_dir.as_deref())
        .unwrap_or("");

    let model_name = input
        .model
        .as_ref()
        .and_then(|m| m.display_name.as_deref())
        .unwrap_or("Claude");

    let ctx_used_pct = input
        .context_window
        .as_ref()
        .and_then(|c| c.used_percentage)
        .unwrap_or(0.0) as u32;

    let ctx_size_k = input
        .context_window
        .as_ref()
        .and_then(|c| c.context_window_size)
        .map(|s| s / 1000)
        .unwrap_or(200);

    let total_cost = input
        .cost
        .as_ref()
        .and_then(|c| c.total_cost_usd)
        .unwrap_or(0.0);

    let (lines_added, lines_removed) = input
        .cost
        .as_ref()
        .map(|c| {
            (
                c.total_lines_added.unwrap_or(0),
                c.total_lines_removed.unwrap_or(0),
            )
        })
        .unwrap_or((0, 0));

    let duration_ms = input
        .session
        .as_ref()
        .and_then(|s| s.duration_ms)
        .unwrap_or(0);

    let turns = input.session.as_ref().and_then(|s| s.turns);
    let total_turns = input.session.as_ref().and_then(|s| s.total_turns);

    let thinking_effort = input
        .session
        .as_ref()
        .and_then(|s| s.thinking_effort.as_deref())
        .unwrap_or("");

    let in_tokens = input
        .context_window
        .as_ref()
        .and_then(|c| c.current_usage.as_ref())
        .and_then(|u| u.input_tokens);

    let out_tokens = input
        .context_window
        .as_ref()
        .and_then(|c| c.current_usage.as_ref())
        .and_then(|u| u.output_tokens);

    // ── Line 1: model · plan  [ctx bar]  ctx%/size  |  #turn/$cost ──────────
    {
        let mut parts: Vec<String> = Vec::new();

        // Model name (cyan + bold)
        parts.push(format!("{CYAN}{BOLD}{model_name}{RESET}"));

        // Context bar + percentage
        let ctx_bar = make_bar(ctx_used_pct, 10);
        let ctx_label = format!("{ctx_bar} {WHITE}{ctx_used_pct}%/{ctx_size_k}k{RESET}");
        parts.push(ctx_label);

        // Turn counter (#current/total) and cost — yellow, bold
        let turn_str = match (turns, total_turns) {
            (Some(t), Some(tt)) => format!("{YELLOW}{BOLD}#{t}/{tt}{RESET}"),
            (Some(t), None) => format!("{YELLOW}{BOLD}#{t}{RESET}"),
            _ => String::new(),
        };
        let cost_str = format!("{YELLOW}{BOLD}${total_cost:.2}{RESET}");

        if !turn_str.is_empty() {
            parts.push(format!("{turn_str} {cost_str}"));
        } else {
            parts.push(cost_str);
        }

        lines.push(parts.join(sep()));
    }

    // ── Line 2: session usage%  window  |  reset countdown  |  $/day  |  🟡  $/h ──
    {
        let mut parts: Vec<String> = Vec::new();

        if let Some(s_pct) = cache.session_pct {
            // ~27% 5h
            let color = if s_pct < 50 {
                GREEN
            } else if s_pct < 75 {
                YELLOW
            } else {
                RED
            };
            parts.push(format!("{color}~{s_pct}%{RESET}"));

            // reset countdown: "28m→02:00" style
            if let Some(reset_secs) = cache.session_reset_secs {
                if reset_secs > 0 {
                    let m = reset_secs / 60;
                    let reset_time = {
                        // compute wall-clock reset time HH:MM
                        let now = now_epoch();
                        let reset_epoch = now + reset_secs;
                        let dt = chrono::DateTime::<Utc>::from_timestamp(reset_epoch as i64, 0)
                            .unwrap_or_default();
                        // convert to local-ish by just using UTC (terminal shows local)
                        format!("{:02}:{:02}", dt.format("%H"), dt.format("%M"))
                    };
                    parts.push(format!("{GRAY}{m}m→{reset_time}{RESET}"));
                } else {
                    parts.push(format!("{GREEN}resetting…{RESET}"));
                }
            }
        }

        // Extra credits: $spent/$dailyAvg
        if let (Some(used_c), Some(limit_c)) = (cache.extra_used_cents, cache.extra_limit_cents) {
            if limit_c > 0.0 {
                let spent = used_c / 100.0;
                let daily_avg = (used_c / 100.0) / 30.0; // rough monthly→daily
                parts.push(format!("{GRAY}${spent:.1}/{daily_avg:.1}D{RESET}"));
            }
        }

        // $/h rate (rough estimate from session cost + duration)
        if duration_ms > 3600_000 && total_cost > 0.0 {
            let hours = duration_ms as f64 / 3_600_000.0;
            let rate = total_cost / hours;
            // indicator dot color
            let dot_color = if rate < 1.0 {
                GREEN
            } else if rate < 3.0 {
                YELLOW
            } else {
                RED
            };
            parts.push(format!("{dot_color}●{RESET} ${rate:.1}/h"));
        }

        if !parts.is_empty() {
            lines.push(parts.join(sep()));
        }
    }

    // ── Line 3: extra credits  D:tokens  i:in/o:out  duration  +lines/-lines  dir ──
    {
        let mut parts: Vec<String> = Vec::new();

        // Extra credits spent (dim if zero)
        if let (Some(used_c), Some(_limit_c)) = (cache.extra_used_cents, cache.extra_limit_cents) {
            let spent = used_c / 100.0;
            let color = if spent > 0.0 { YELLOW } else { GRAY };
            parts.push(format!("{color}${spent:.2}{RESET}"));
        }

        // Token counts from current_usage
        if let (Some(i), Some(o)) = (in_tokens, out_tokens) {
            let i_fmt = fmt_token(i);
            let o_fmt = fmt_token(o);
            parts.push(format!(
                "{GRAY}i:{WHITE}{i_fmt}{GRAY}/o:{WHITE}{o_fmt}{RESET}"
            ));
        }

        // Session duration
        if duration_ms > 0 {
            parts.push(format!("{GRAY}{}{RESET}", fmt_duration_ms(duration_ms)));
        }

        // Git diff lines added/removed
        let git_add = if lines_added > 0 {
            lines_added
        } else {
            // fall back to live git diff if stdin has no data
            git_diff_stat(cwd).0
        };
        let git_del = if lines_removed > 0 {
            lines_removed
        } else {
            git_diff_stat(cwd).1
        };
        parts.push(format!("{GREEN}+{git_add}{RESET}/{RED}-{git_del}{RESET}"));

        // Current dir basename
        let dir = Path::new(cwd)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("~");
        // Git branch
        let branch_str = if let Some(b) = git_branch(cwd) {
            format!(" {GRAY}({b}){RESET}")
        } else {
            String::new()
        };
        parts.push(format!("{WHITE}{dir}{RESET}{branch_str}"));

        if !parts.is_empty() {
            lines.push(parts.join(sep()));
        }
    }

    // ── Line 4: thinking effort hint (only when non-empty / non-default) ─────
    // if !thinking_effort.is_empty() && thinking_effort != "auto" {
    //     let label = match thinking_effort {
    //         "high" | "ultrathink" => format!("{YELLOW}Effort set to high for this turn{RESET}"),
    //         "low" | "think" => format!("{GRAY}Effort set to low for this turn{RESET}"),
    //         other => format!("{GRAY}Effort: {other}{RESET}"),
    //     };
    //     lines.push(label);
    // }

    lines.print();
}

/// Format a token count as "1.2k", "34k", "1.2M" etc.
fn fmt_token(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.0}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}
