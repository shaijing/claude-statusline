//! Usage API cache: disk persistence, OAuth token resolution, background refresh.

use crate::debug::debug_log;
use crate::format::{iso_to_epoch, now_epoch};
use serde::{Deserialize, Serialize};
use std::process::Command;

// ─── Cache struct & disk I/O ───────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Default)]
pub(crate) struct Cache {
    pub(crate) timestamp: u64,
    pub(crate) session_pct: Option<u32>,
    pub(crate) session_reset_secs: Option<u64>,
    pub(crate) extra_used_cents: Option<f64>,
    pub(crate) extra_limit_cents: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct UsageResponse {
    five_hour: Option<UsagePeriod>,
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

pub(crate) fn load_cache() -> Cache {
    cache_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_cache(c: &Cache) {
    if let Some(path) = cache_path()
        && let Ok(json) = serde_json::to_string(c)
    {
        let _ = std::fs::write(path, json);
    }
}

pub(crate) fn cache_needs_refresh(cache: &Cache) -> bool {
    let now = now_epoch();
    cache.timestamp == 0 || now.saturating_sub(cache.timestamp) >= CACHE_TTL
}

// ─── OAuth token resolution ───────────────────────────────────────────────────

/// Try in order: env var, macOS Keychain, credentials file, GNOME libsecret.
pub(crate) fn get_oauth_token() -> Option<String> {
    if let Ok(t) = std::env::var("CLAUDE_CODE_OAUTH_TOKEN")
        && !t.is_empty()
    {
        return Some(t);
    }
    if cfg!(target_os = "macos")
        && let Ok(out) = Command::new("security")
            .args([
                "find-generic-password",
                "-s",
                "Claude Code-credentials",
                "-w",
            ])
            .output()
        && out.status.success()
    {
        let blob = String::from_utf8_lossy(&out.stdout);
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(blob.trim())
            && let Some(t) = v["claudeAiOauth"]["accessToken"].as_str()
            && !t.is_empty()
            && t != "null"
        {
            return Some(t.to_owned());
        }
    }
    if let Some(path) = dirs::home_dir().map(|h| h.join(".claude/.credentials.json"))
        && let Ok(content) = std::fs::read_to_string(path)
        && let Ok(v) = serde_json::from_str::<serde_json::Value>(&content)
        && let Some(t) = v["claudeAiOauth"]["accessToken"].as_str()
        && !t.is_empty()
        && t != "null"
    {
        return Some(t.to_owned());
    }
    if let Ok(out) = Command::new("secret-tool")
        .args(["lookup", "service", "Claude Code-credentials"])
        .output()
        && out.status.success()
    {
        let blob = String::from_utf8_lossy(&out.stdout);
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(blob.trim())
            && let Some(t) = v["claudeAiOauth"]["accessToken"].as_str()
            && !t.is_empty()
            && t != "null"
        {
            return Some(t.to_owned());
        }
    }
    None
}

// ─── Background cache refresh ─────────────────────────────────────────────────

/// Fetch from the API and write to the cache file. Background thread only —
/// the main thread never blocks on this.
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
    if let Some(ex) = &usage.extra_usage {
        cache.extra_used_cents = ex.used_credits;
        cache.extra_limit_cents = ex.monthly_limit;
    }

    save_cache(&cache);
    debug_log("fetch_and_save_cache: done");
}

/// Kick off a background refresh. The JoinHandle is dropped (detached);
/// the main thread never waits. The *next* invocation reads the fresh cache.
pub(crate) fn refresh_cache_background() {
    let _ = std::thread::spawn(fetch_and_save_cache);
}
