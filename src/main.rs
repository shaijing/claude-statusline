use chrono::{DateTime, Utc};
use owo_colors::OwoColorize;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

// ─── Catppuccin Mocha palette ─────────────────────────────────────────────────
mod palette {
    // General UI
    pub const CYAN: (u8, u8, u8) = (137, 220, 235);
    pub const YELLOW: (u8, u8, u8) = (249, 226, 175);
    pub const GREEN: (u8, u8, u8) = (166, 227, 161);
    pub const RED: (u8, u8, u8) = (243, 139, 168);
    pub const GRAY: (u8, u8, u8) = (108, 112, 134);
    pub const WHITE: (u8, u8, u8) = (205, 214, 244);
    // pub const DIM: (u8, u8, u8) = (69, 71, 90);
    pub const DIM: (u8, u8, u8) = (110, 110, 110);
    // Progress bar — darker variants for transparent-bg readability
    pub const BAR_GREEN: (u8, u8, u8) = (74, 153, 90); // dark green
    pub const BAR_YELLOW: (u8, u8, u8) = (180, 140, 40); // dark amber
    pub const BAR_RED: (u8, u8, u8) = (180, 60, 60); // dark red
    pub const BAR_BG: (u8, u8, u8) = (66, 84, 42); // very dark green tint
}

// ─── Shorthand color helpers ──────────────────────────────────────────────────

fn col(s: &str, (r, g, b): (u8, u8, u8)) -> String {
    format!("{}", s.truecolor(r, g, b))
}

fn col_bold(s: &str, (r, g, b): (u8, u8, u8)) -> String {
    format!("{}", s.truecolor(r, g, b).bold())
}

fn sep() -> String {
    col(" │ ", palette::DIM)
}

// ─── Progress bar ─────────────────────────────────────────────────────────────

/// Render a `width`-char block bar. `used_pct` is 0–100.
/// Each char gets a dark background so it reads clearly on transparent terminals.
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

    let filled_str: String = (0..filled)
        .map(|_| {
            format!(
                "{}",
                "█".truecolor(fg.0, fg.1, fg.2).on_truecolor(br, bg, bb)
            )
        })
        .collect();

    let empty_str: String = (0..empty)
        .map(|_| format!("{}", "░".truecolor(dr, dg, db).on_truecolor(br, bg, bb)))
        .collect();

    format!("{filled_str}{empty_str}")
}

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
}

#[derive(Debug, Deserialize)]
struct Workspace {
    current_dir: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ContextWindow {
    used_percentage: Option<f64>,
    context_window_size: Option<u64>,
    current_usage: Option<CurrentUsage>,
}

#[derive(Debug, Deserialize)]
struct CurrentUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
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

// ─── General helpers ──────────────────────────────────────────────────────────

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

// ─── OAuth token resolution ───────────────────────────────────────────────────

fn get_oauth_token() -> Option<String> {
    // 1. Env var override
    if let Ok(t) = std::env::var("CLAUDE_CODE_OAUTH_TOKEN") {
        if !t.is_empty() {
            return Some(t);
        }
    }

    // 2. macOS Keychain
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

    // 3. Linux credentials file
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

    // 4. GNOME Keyring via secret-tool
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

// ─── Usage API (5-min cache) ──────────────────────────────────────────────────

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
    let s = sep();

    // ── Extract ─────────────────────────────────────────────────────────────
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
        .map(|n| n / 1000)
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

    let mut rows: Vec<String> = Vec::new();

    // ── Row 1: Model  [bar] ctx%/size  │  #turn/total  $cost ────────────────
    {
        let mut parts: Vec<String> = Vec::new();

        parts.push(col_bold(model_name, palette::CYAN));

        let bar = make_bar(ctx_used_pct, 10);
        parts.push(format!(
            "{bar} {}",
            col(&format!("{ctx_used_pct}%/{ctx_size_k}k"), palette::WHITE)
        ));

        let cost_part = col_bold(&format!("${total_cost:.2}"), palette::YELLOW);
        let turn_cost = match (turns, total_turns) {
            (Some(t), Some(tt)) => format!(
                "{} {cost_part}",
                col_bold(&format!("#{t}/{tt}"), palette::YELLOW)
            ),
            (Some(t), None) => format!(
                "{} {cost_part}",
                col_bold(&format!("#{t}"), palette::YELLOW)
            ),
            _ => cost_part,
        };
        parts.push(turn_cost);

        rows.push(parts.join(&s));
    }

    // ── Row 2: ~session%  │  Xm→HH:MM  │  $spent/$dailyD  │  ● $/h ─────────
    {
        let mut parts: Vec<String> = Vec::new();

        if let Some(s_pct) = cache.session_pct {
            let usage_color = if s_pct < 50 {
                palette::GREEN
            } else if s_pct < 75 {
                palette::YELLOW
            } else {
                palette::RED
            };
            parts.push(col(&format!("~{s_pct}%"), usage_color));

            if let Some(reset_secs) = cache.session_reset_secs {
                if reset_secs > 0 {
                    let m = reset_secs / 60;
                    let reset_epoch = now_epoch() + reset_secs;
                    let dt =
                        DateTime::<Utc>::from_timestamp(reset_epoch as i64, 0).unwrap_or_default();
                    let hhmm = format!("{:02}:{:02}", dt.format("%H"), dt.format("%M"));
                    parts.push(col(&format!("{m}m→{hhmm}"), palette::GRAY));
                } else {
                    parts.push(col("resetting…", palette::GREEN));
                }
            }
        }

        if let (Some(used_c), Some(limit_c)) = (cache.extra_used_cents, cache.extra_limit_cents) {
            if limit_c > 0.0 {
                let spent = used_c / 100.0;
                let daily_avg = spent / 30.0;
                parts.push(col(&format!("${spent:.1}/${daily_avg:.1}D"), palette::GRAY));
            }
        }

        if duration_ms > 3_600_000 && total_cost > 0.0 {
            let rate = total_cost / (duration_ms as f64 / 3_600_000.0);
            let dot_color = if rate < 1.0 {
                palette::GREEN
            } else if rate < 3.0 {
                palette::YELLOW
            } else {
                palette::RED
            };
            parts.push(format!("{} ${rate:.1}/h", col("●", dot_color)));
        }

        if !parts.is_empty() {
            rows.push(parts.join(&s));
        }
    }

    // ── Row 3: $extra  i:N/o:N  duration  +add/-del  dir(branch) ─────────────
    {
        let mut parts: Vec<String> = Vec::new();

        if let (Some(used_c), Some(_)) = (cache.extra_used_cents, cache.extra_limit_cents) {
            let spent = used_c / 100.0;
            let color = if spent > 0.0 {
                palette::YELLOW
            } else {
                palette::GRAY
            };
            parts.push(col(&format!("${spent:.2}"), color));
        }

        if let (Some(i), Some(o)) = (in_tokens, out_tokens) {
            parts.push(format!(
                "{}{}{}{}",
                col("i:", palette::GRAY),
                col(&fmt_token(i), palette::WHITE),
                col("/o:", palette::GRAY),
                col(&fmt_token(o), palette::WHITE)
            ));
        }

        if duration_ms > 0 {
            parts.push(col(&fmt_duration_ms(duration_ms), palette::GRAY));
        }

        let (add, del) = if lines_added != 0 || lines_removed != 0 {
            (lines_added, lines_removed)
        } else {
            git_diff_stat(cwd)
        };
        parts.push(format!(
            "{}/{}",
            col(&format!("+{add}"), palette::GREEN),
            col(&format!("-{del}"), palette::RED)
        ));

        let dir = Path::new(cwd)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("~");
        let branch = git_branch(cwd)
            .map(|b| format!(" {}", col(&format!("({b})"), palette::GRAY)))
            .unwrap_or_default();
        parts.push(format!("{}{}", col(dir, palette::WHITE), branch));

        if !parts.is_empty() {
            rows.push(parts.join(&s));
        }
    }

    // ── Row 4: thinking effort hint ───────────────────────────────────────────
    // if !thinking_effort.is_empty() && thinking_effort != "auto" {
    //     let msg = match thinking_effort {
    //         "high" | "ultrathink" => col("Effort set to high for this turn", palette::YELLOW),
    //         "low" | "think" => col("Effort set to low for this turn", palette::GRAY),
    //         other => col(&format!("Effort: {other}"), palette::GRAY),
    //     };
    //     rows.push(msg);
    // }

    println!("{}", rows.join("\n"));
}
