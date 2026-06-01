//! Block system: trait, rendering context, row/group/layout containers.

use crate::cache::Cache;
use crate::color::{col, DIM};
use crate::derived::DerivedCtx;
use crate::input::ClaudeInput;
use std::sync::OnceLock;

/// Shared data passed to every block at render time.
pub(crate) struct RenderCtx<'a> {
    pub(crate) input: &'a ClaudeInput,
    pub(crate) cache: &'a Cache,
    pub(crate) derived: &'a DerivedCtx,
}

/// A Block renders itself into an optional string.
/// Returning `None` means "nothing to show" — the block is skipped entirely.
pub(crate) trait Block {
    fn render(&self, ctx: &RenderCtx) -> Option<String>;
}

// ── Row ───────────────────────────────────────────────────────────────────────

/// Cached separator string (dim │).
static SEPARATOR: OnceLock<String> = OnceLock::new();

pub(crate) fn get_separator() -> &'static str {
    SEPARATOR.get_or_init(|| col(" │ ", DIM))
}

pub(crate) struct Row {
    blocks: Vec<Box<dyn Block>>,
}

impl Row {
    /// Standard row: blocks separated by a dim │
    pub(crate) fn new(blocks: Vec<Box<dyn Block>>) -> Self {
        Row { blocks }
    }

    /// Render into a single buffer, skipping blocks that return `None`.
    /// No intermediate `Vec<String>` or `join` allocation.
    pub(crate) fn render(&self, ctx: &RenderCtx) -> Option<String> {
        let mut buf = String::new();
        let mut first = true;
        for b in &self.blocks {
            if let Some(s) = b.render(ctx) {
                if !first {
                    buf.push_str(get_separator());
                }
                buf.push_str(&s);
                first = false;
            }
        }
        if buf.is_empty() { None } else { Some(buf) }
    }
}

// ── BlockGroup ────────────────────────────────────────────────────────────────

/// A group where the first block acts as a guard: if it returns `None`,
/// the entire group is suppressed (including the remaining members).
/// Useful for "only show reset countdown when usage data exists".
pub(crate) struct BlockGroup {
    guard: Box<dyn Block>,
    members: Vec<Box<dyn Block>>,
}

impl BlockGroup {
    pub(crate) fn new(guard: Box<dyn Block>, members: Vec<Box<dyn Block>>) -> Self {
        BlockGroup { guard, members }
    }
}

impl Block for BlockGroup {
    fn render(&self, ctx: &RenderCtx) -> Option<String> {
        // Guard must produce output; if not, the whole group is suppressed.
        let guard_out = self.guard.render(ctx)?;
        let mut buf = String::with_capacity(guard_out.len() + 32);
        buf.push_str(&guard_out);
        for b in &self.members {
            if let Some(s) = b.render(ctx) {
                buf.push_str(get_separator());
                buf.push_str(&s);
            }
        }
        Some(buf)
    }
}

// ── Layout ────────────────────────────────────────────────────────────────────

pub(crate) struct Layout(Vec<Row>);

impl Layout {
    pub(crate) fn new(rows: Vec<Row>) -> Self {
        Layout(rows)
    }

    /// Print all non-empty rows joined by newlines, in a single buffer.
    pub(crate) fn print(&self, ctx: &RenderCtx) {
        let mut buf = String::new();
        let mut first = true;
        for row in &self.0 {
            if let Some(line) = row.render(ctx) {
                if !first {
                    buf.push('\n');
                }
                buf.push_str(&line);
                first = false;
            }
        }
        println!("{buf}");
    }
}
