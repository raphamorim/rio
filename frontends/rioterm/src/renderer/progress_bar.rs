// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Per-pane OSC 9;4 progress bar rendering.
//!
//! Each terminal pane (split) owns its own [`ProgressTracker`] on its
//! `RenderableContent`, so multiple splits track progress independently
//! instead of clobbering a single window-level slot. This module is the
//! one place that knows how to actually paint a tracker as a thin bar.
//!
//! The visual behavior (height, indeterminate cycle period, color
//! selection by state) was lifted verbatim from the previous
//! single-instance implementation in `island.rs` so existing TUIs see no
//! visible change beyond "it's now per-split".

use rio_backend::event::{ProgressState, ProgressTracker};
use rio_backend::sugarloaf::Sugarloaf;

/// Height of the progress bar in pixels (logical, pre-scale).
pub const PROGRESS_BAR_HEIGHT: f32 = 3.0;

/// Indeterminate animation cycle, in milliseconds. Bar travels left→right
/// over this period, then repeats.
const INDETERMINATE_CYCLE_MS: f32 = 2000.0;

/// Width of the moving bar in indeterminate mode, as a fraction of the
/// pane width.
const INDETERMINATE_BAR_FRACTION: f32 = 0.2;

/// Draw the progress bar for a single pane.
///
/// `(x, y, width)` are in **logical** pixels (scale-divided), matching
/// the rest of the chrome layer. The caller decides where to anchor
/// (top of the pane in our case).
///
/// `color` is used for `Set` / `Indeterminate` / `Pause`; `error_color`
/// is used for `Error`. This split mirrors what the OSC 9;4 spec
/// (ConEmu / Windows Terminal) communicates.
pub fn draw(
    sugarloaf: &mut Sugarloaf,
    x: f32,
    y: f32,
    width: f32,
    tracker: &ProgressTracker,
    color: [f32; 4],
    error_color: [f32; 4],
) {
    let state = match tracker.state {
        Some(s) => s,
        None => return,
    };

    let bar_color = match state {
        ProgressState::Error => error_color,
        _ => color,
    };

    // Order 5 keeps the bar on top of the rich-text panel content,
    // mirroring how `scrollbar` paints its thumb above the terminal cells
    // (`TERMINAL_ORDER = 5`). The old window-wide implementation could
    // afford order 0 because it lived in the island chrome above the
    // terminal area; per-pane bars sit *inside* the panel, so the chrome
    // layer needs to win.
    const ORDER: u8 = 5;

    match state {
        ProgressState::Remove => {
            // Unreachable: Remove clears `state` to None inside `apply`.
        }
        ProgressState::Set | ProgressState::Error | ProgressState::Pause => {
            let progress = tracker.value.unwrap_or(0) as f32 / 100.0;
            let bar_width = width * progress;
            if bar_width > 0.0 {
                sugarloaf.rect(
                    None,
                    x,
                    y,
                    bar_width,
                    PROGRESS_BAR_HEIGHT,
                    bar_color,
                    0.0,
                    ORDER,
                );
            }
        }
        ProgressState::Indeterminate => {
            // Phase is anchored to `started_at` (set only on state
            // transition) — using `last_seen` would freeze the bar at
            // position 0 for any TUI that heartbeats its OSC 9;4;3
            // faster than the cycle period (issue #1509).
            let elapsed = tracker
                .started_at
                .map(|t| t.elapsed().as_millis() as f32)
                .unwrap_or(0.0);
            let position = (elapsed % INDETERMINATE_CYCLE_MS) / INDETERMINATE_CYCLE_MS;
            let bar_width = width * INDETERMINATE_BAR_FRACTION;
            let x_pos = x + position * (width - bar_width);
            sugarloaf.rect(
                None,
                x_pos,
                y,
                bar_width,
                PROGRESS_BAR_HEIGHT,
                bar_color,
                0.0,
                ORDER,
            );
        }
    }
}
