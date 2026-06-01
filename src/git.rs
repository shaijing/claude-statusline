//! Git shell-out helpers — best-effort, never fail loudly.

use std::process::Command;

/// Current branch name, or `None` if not in a git repo / detached HEAD.
pub(crate) fn git_branch(cwd: &str) -> Option<String> {
    Command::new("git")
        .args(["-C", cwd, "branch", "--show-current"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .filter(|b| !b.is_empty())
}

/// `(lines_added, lines_removed)` from `git diff --numstat HEAD`.
/// Returns `(0, 0)` on any failure.
pub(crate) fn git_diff_stat(cwd: &str) -> (i64, i64) {
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
