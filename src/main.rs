//! Entry point: read stdin, render the statusline, print.

mod block;
mod blocks;
mod cache;
mod color;
mod debug;
mod derived;
mod format;
mod git;
mod input;
mod layout;

use std::io::Read;

use crate::block::RenderCtx;
use crate::cache::{cache_needs_refresh, load_cache, refresh_cache_background};
use crate::derived::DerivedCtx;
use crate::input::parse_input;
use crate::layout::{build_layout, load_config};

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

    // Load layout config (or use default). Config is cached per session.
    let config = load_config();
    let layout = build_layout(&config);

    layout.print(&ctx);
}
