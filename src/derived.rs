//! Data derived once per render and shared across all blocks.

use crate::git::{git_branch, git_diff_stat};
use crate::input::ClaudeInput;
use std::path::Path;

pub(crate) struct DerivedCtx {
    /// basename of `workspace.current_dir`
    pub(crate) dir_name: String,
    /// current git branch, if any
    pub(crate) git_branch: Option<String>,
    /// (lines_added, lines_removed) from cost struct or live git diff
    pub(crate) git_diff: (i64, i64),
}

impl DerivedCtx {
    pub(crate) fn build(input: &ClaudeInput) -> Self {
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
