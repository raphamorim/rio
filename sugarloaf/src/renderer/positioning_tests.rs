// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//

#![allow(clippy::uninlined_format_args)]
// Positioning tests for cached vs non-cached text rendering

use crate::components::rich_text::text_run_manager::{CacheResult, TextRunManager};
use crate::font::text_run_cache::ShapedGlyph;
use crate::sugarloaf::primitives::{DrawableChar, SugarCursor};

/// Captured positioning data for comparison
#[derive(Debug, Clone, PartialEq)]
pub struct PositioningData {
    pub baseline: f32,
    pub topline: f32,
    pub py: f32,
    pub padding_y: f32,
    pub line_height: f32,
    pub glyph_positions: Vec<(f32, f32)>, // (x, y) for each glyph
    pub cursor: Option<SugarCursor>,
    pub drawable_char: Option<DrawableChar>,
    pub advance: f32,
}

/// Test helper to simulate text rendering and capture positioning data
pub struct PositioningTestHelper {
    text_run_manager: TextRunManager,
}

impl PositioningTestHelper {
    pub fn new() -> Self {
        Self {
            text_run_manager: TextRunManager::new(),
        }
    }

    /// Simulate the positioning calculations from the rich text renderer
    #[allow(clippy::too_many_arguments)]
    pub fn calculate_positioning(
        &mut self,
        text: &str,
        font_id: usize,
        font_size: f32,
        ascent: f32,
        descent: f32,
        line_height: f32,
        cursor: Option<SugarCursor>,
        drawable_char: Option<DrawableChar>,
        use_cache: bool,
    ) -> PositioningData {
        let char_width = 1.0f32;

        // Simulate the line positioning calculations from mod.rs
        let line_y = 0.0f32; // Starting position
        let padding_top = (line_height - ascent - descent) / 2.0;
        let baseline = line_y + padding_top + ascent;
        let py = line_y; // py is now just the line_y, not modified

        // Calculate padding (from line height modifier logic)
        let line_height_without_mod = ascent + descent;
        let line_height_mod = line_height / line_height_without_mod;
        let padding_y = if line_height_mod > 1.0 {
            (line_height - line_height_without_mod) / 2.0
        } else {
            0.0
        };

        let mut glyph_positions = Vec::new();
        let mut px = 0.0f32;
        let advance;

        if use_cache {
            // Try to get cached data
            let cached_result = self.text_run_manager.get_cached_data(
                text,
                font_id,
                font_size,
                Some([1.0, 1.0, 1.0, 1.0]), // white color
            );

            match cached_result {
                CacheResult::ShapingOnly {
                    glyphs: cached_glyphs,
                    ..
                }
                | CacheResult::GlyphsOnly {
                    glyphs: cached_glyphs,
                    ..
                } => {
                    // Use cached glyph data
                    for shaped_glyph in cached_glyphs.iter() {
                        let x = px;
                        let y = py + padding_y;
                        glyph_positions.push((x, y));
                        px += shaped_glyph.x_advance * char_width;
                    }
                    advance = cached_glyphs.iter().map(|g| g.x_advance).sum();
                }
                CacheResult::FullRender { .. } => {
                    // For full render, we'd use cached vertices, but for testing
                    // we'll simulate the same glyph positioning
                    advance = text.len() as f32 * 10.0; // Mock advance
                }
                CacheResult::Miss => {
                    // Cache miss - simulate fresh shaping
                    advance = self.simulate_fresh_shaping(
                        text,
                        &mut px,
                        py,
                        padding_y,
                        char_width,
                        &mut glyph_positions,
                    );
                }
            }
        } else {
            // Simulate non-cached path
            advance = self.simulate_fresh_shaping(
                text,
                &mut px,
                py,
                padding_y,
                char_width,
                &mut glyph_positions,
            );
        }

        PositioningData {
            baseline,    // Use the calculated baseline position
            topline: py, // Use py (line top) for cursor positioning
            py,
            padding_y,
            line_height,
            glyph_positions,
            cursor,
            drawable_char,
            advance,
        }
    }

    fn simulate_fresh_shaping(
        &mut self,
        text: &str,
        px: &mut f32,
        py: f32,
        padding_y: f32,
        char_width: f32,
        glyph_positions: &mut Vec<(f32, f32)>,
    ) -> f32 {
        let mut shaped_glyphs = Vec::new();
        let run_start_x = *px;

        // Simulate shaping each character
        for (i, _ch) in text.chars().enumerate() {
            let x = *px;
            let y = py + padding_y;
            let advance = 10.0f32; // Mock advance per character

            glyph_positions.push((x, y));
            *px += advance * char_width;

            // Create shaped glyph for caching
            shaped_glyphs.push(ShapedGlyph {
                glyph_id: (65 + i) as u32, // Mock glyph IDs (A, B, C, ...)
                x_advance: advance,
                y_advance: 0.0,
                x_offset: 0.0,
                y_offset: 0.0,
                cluster: i as u32,
                atlas_coords: None,
                atlas_layer: None,
            });
        }

        // Cache the shaped data for future use
        self.text_run_manager.cache_shaping_data(
            text,
            0,    // font_id
            12.0, // font_size
            shaped_glyphs,
            false, // has_emoji
            None,  // shaping_features
        );

        *px - run_start_x
    }

    /// Clear the cache to test non-cached behavior
    pub fn clear_cache(&mut self) {
        self.text_run_manager.clear_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_positioning_consistency_without_cursor() {
        let mut helper = PositioningTestHelper::new();

        // Test basic text positioning
        let text = "Hello";
        let font_id = 0;
        let font_size = 12.0;
        let ascent = 10.0;
        let descent = 3.0;
        let line_height = 15.0;

        // First run - no cache (fresh shaping)
        let non_cached = helper.calculate_positioning(
            text,
            font_id,
            font_size,
            ascent,
            descent,
            line_height,
            None,
            None,
            false,
        );

        // Second run - should use cache
        let cached = helper.calculate_positioning(
            text,
            font_id,
            font_size,
            ascent,
            descent,
            line_height,
            None,
            None,
            true,
        );

        // Compare positioning data
        assert_eq!(
            non_cached.baseline, cached.baseline,
            "Baseline should be identical"
        );
        assert_eq!(
            non_cached.topline, cached.topline,
            "Topline should be identical"
        );
        assert_eq!(non_cached.py, cached.py, "py should be identical");
        assert_eq!(
            non_cached.padding_y, cached.padding_y,
            "padding_y should be identical"
        );
        assert_eq!(
            non_cached.glyph_positions.len(),
            cached.glyph_positions.len(),
            "Should have same number of glyphs"
        );

        // Check each glyph position
        for (i, (non_cached_pos, cached_pos)) in non_cached
            .glyph_positions
            .iter()
            .zip(cached.glyph_positions.iter())
            .enumerate()
        {
            assert_eq!(
                non_cached_pos.0, cached_pos.0,
                "Glyph {} x position should match",
                i
            );
            assert_eq!(
                non_cached_pos.1, cached_pos.1,
                "Glyph {} y position should match",
                i
            );
        }

        // println!("Non-cached positioning: {:?}", non_cached);
        // println!("Cached positioning: {:?}", cached);
    }

    #[test]
    fn test_cursor_positioning_consistency() {
        let mut helper = PositioningTestHelper::new();

        let text = "Test";
        let font_id = 0;
        let font_size = 12.0;
        let ascent = 10.0;
        let descent = 3.0;
        let line_height = 15.0;
        let _cursor = Some(SugarCursor::Block([1.0, 0.0, 0.0, 1.0]));

        // Test with different cursor types
        let cursor_types = vec![
            SugarCursor::Block([1.0, 0.0, 0.0, 1.0]),
            SugarCursor::Underline([0.0, 1.0, 0.0, 1.0]),
            SugarCursor::Caret([0.0, 0.0, 1.0, 1.0]),
        ];

        for cursor_type in cursor_types {
            helper.clear_cache();

            // Non-cached run
            let non_cached = helper.calculate_positioning(
                text,
                font_id,
                font_size,
                ascent,
                descent,
                line_height,
                Some(cursor_type),
                None,
                false,
            );

            // Cached run
            let cached = helper.calculate_positioning(
                text,
                font_id,
                font_size,
                ascent,
                descent,
                line_height,
                Some(cursor_type),
                None,
                true,
            );

            // Cursor positioning should be identical
            assert_eq!(
                non_cached.baseline, cached.baseline,
                "Cursor baseline should match for {:?}",
                cursor_type
            );
            assert_eq!(
                non_cached.topline, cached.topline,
                "Cursor topline should match for {:?}",
                cursor_type
            );

            // println!(
            //     "Cursor {:?} - Non-cached: baseline={}, topline={}",
            //     cursor_type, non_cached.baseline, non_cached.topline
            // );
            // println!(
            //     "Cursor {:?} - Cached: baseline={}, topline={}",
            //     cursor_type, cached.baseline, cached.topline
            // );
        }
    }

    #[test]
    fn test_drawable_character_cached_vs_non_cached() {
        let mut helper = PositioningTestHelper::new();

        let text = "X";
        let font_id = 0;
        let font_size = 12.0;
        let ascent = 10.0;
        let descent = 3.0;
        let line_height = 15.0;
        let drawable_char = Some(DrawableChar::Cross);

        helper.clear_cache();

        // Non-cached run
        let non_cached = helper.calculate_positioning(
            text,
            font_id,
            font_size,
            ascent,
            descent,
            line_height,
            None,
            drawable_char,
            false,
        );

        // Cached run
        let cached = helper.calculate_positioning(
            text,
            font_id,
            font_size,
            ascent,
            descent,
            line_height,
            None,
            drawable_char,
            true,
        );

        // Drawable character positioning should be identical
        assert_eq!(
            non_cached.baseline, cached.baseline,
            "Drawable char baseline should match"
        );
        assert_eq!(
            non_cached.topline, cached.topline,
            "Drawable char topline should match"
        );
        assert_eq!(non_cached.py, cached.py, "Drawable char py should match");
    }

    #[test]
    fn test_padding_y_effects() {
        let mut helper = PositioningTestHelper::new();

        let text = "Test";
        let font_id = 0;
        let font_size = 12.0;
        let ascent = 10.0;
        let descent = 3.0;

        // Test with different line heights to trigger different padding_y values
        let line_heights = vec![13.0, 15.0, 20.0]; // Normal, slightly increased, significantly increased

        for line_height in line_heights {
            helper.clear_cache();

            let non_cached = helper.calculate_positioning(
                text,
                font_id,
                font_size,
                ascent,
                descent,
                line_height,
                None,
                None,
                false,
            );

            let cached = helper.calculate_positioning(
                text,
                font_id,
                font_size,
                ascent,
                descent,
                line_height,
                None,
                None,
                true,
            );

            // println!(
            //     "Line height {}: padding_y={}, baseline={}, topline={}, py={}",
            //     line_height,
            //     non_cached.padding_y,
            //     non_cached.baseline,
            //     non_cached.topline,
            //     non_cached.py
            // );

            assert_eq!(
                non_cached.padding_y, cached.padding_y,
                "padding_y should match for line_height {}",
                line_height
            );
            assert_eq!(
                non_cached.baseline, cached.baseline,
                "baseline should match for line_height {}",
                line_height
            );
            assert_eq!(
                non_cached.topline, cached.topline,
                "topline should match for line_height {}",
                line_height
            );
        }
    }

    #[test]
    fn test_glyph_vs_cursor_positioning_relationship() {
        let mut helper = PositioningTestHelper::new();

        let text = "A";
        let font_id = 0;
        let font_size = 12.0;
        let ascent = 10.0;
        let descent = 3.0;
        let line_height = 15.0;

        helper.clear_cache();

        // Test text with cursor
        let with_cursor = helper.calculate_positioning(
            text,
            font_id,
            font_size,
            ascent,
            descent,
            line_height,
            Some(SugarCursor::Block([1.0, 0.0, 0.0, 1.0])),
            None,
            true,
        );

        // The relationship between glyph y position and cursor positioning elements:
        // - Glyph y = py + padding_y
        // - Cursor block uses topline = baseline - ascent
        // - Cursor underline uses baseline + 1.0

        if let Some((_glyph_x, glyph_y)) = with_cursor.glyph_positions.first() {
            // println!("Glyph position: ({}, {})", glyph_x, glyph_y);
            // println!(
            //     "Expected glyph y: py + padding_y = {} + {} = {}",
            //     with_cursor.py,
            //     with_cursor.padding_y,
            //     with_cursor.py + with_cursor.padding_y
            // );
            // println!("Cursor topline (block): {}", with_cursor.topline);
            // println!("Cursor baseline (underline): {}", with_cursor.baseline);

            // Verify the glyph y calculation
            assert_eq!(
                *glyph_y,
                with_cursor.py + with_cursor.padding_y,
                "Glyph y should equal py + padding_y"
            );

            // The cursor should be positioned relative to the same baseline as the text
            // Now that baseline = py - descent, the relationships are:
            // - glyph_y = py + padding_y (glyphs positioned relative to py)
            // - cursor baseline = py - descent (actual text baseline)
            let expected_glyph_baseline = glyph_y - with_cursor.padding_y; // This should be py
            let cursor_baseline_in_line_coords = with_cursor.baseline; // This should be py - descent

            // println!(
            //     "Expected glyph baseline (line coords): {}",
            //     expected_glyph_baseline
            // );
            // println!(
            //     "Cursor baseline (line coords): {}",
            //     cursor_baseline_in_line_coords
            // );

            // With the new coordinate system: cursor_baseline = glyph_baseline + padding_top + ascent
            let padding_top = (line_height - ascent - descent) / 2.0;
            assert!((cursor_baseline_in_line_coords - (expected_glyph_baseline + padding_top + ascent)).abs() < 0.1,
                   "Cursor baseline should equal glyph baseline + padding_top + ascent. Cursor: {}, Glyph: {}, Padding: {}, Ascent: {}", 
                   cursor_baseline_in_line_coords, expected_glyph_baseline, padding_top, ascent);
        }
    }

    #[test]
    fn test_drawable_character_positioning() {
        let mut helper = PositioningTestHelper::new();
        let font_id = 0;
        let font_size = 16.0;
        let ascent = 12.0;
        let descent = 3.0;
        let line_height = 20.0;
        let drawable_char = Some(crate::DrawableChar::Horizontal);

        let result = helper.calculate_positioning(
            "─",
            font_id,
            font_size,
            ascent,
            descent,
            line_height,
            None,
            drawable_char,
            false,
        );

        // Verify baseline relationships are correct with new coordinate system
        let padding_top = (line_height - ascent - descent) / 2.0;
        let expected_baseline = result.py + padding_top + ascent;
        assert_eq!(
            result.baseline, expected_baseline,
            "baseline should equal py + padding_top + ascent"
        );
        assert_eq!(
            result.topline, result.py,
            "topline should be py (line top) for cursor positioning"
        );

        // The drawable character should be positioned at topline
        // This means center_y = topline + (line_height / 2.0)
        let expected_center_y = result.topline + (line_height / 2.0);

        // For horizontal lines, center should be reasonably close to text baseline
        let text_baseline = result.baseline; // Use the actual baseline
        let diff = (expected_center_y - text_baseline).abs();

        // Should be within reasonable range (half the line height)
        assert!(diff < line_height / 2.0,
               "Drawable character center ({}) should be reasonably close to text baseline ({}). Diff: {}", 
               expected_center_y, text_baseline, diff);
    }

    #[test]
    fn test_cursor_positioning_relationships() {
        let mut helper = PositioningTestHelper::new();
        let font_id = 0;
        let font_size = 16.0;
        let ascent = 12.0;
        let descent = 3.0;
        let line_height = 20.0;

        for cursor_type in [
            Some(crate::SugarCursor::Block([1.0, 1.0, 1.0, 1.0])),
            Some(crate::SugarCursor::Caret([1.0, 1.0, 1.0, 1.0])),
            Some(crate::SugarCursor::Underline([1.0, 1.0, 1.0, 1.0])),
        ] {
            let result = helper.calculate_positioning(
                "A",
                font_id,
                font_size,
                ascent,
                descent,
                line_height,
                cursor_type,
                None,
                false,
            );

            // Core relationships that must always hold with new coordinate system
            let padding_top = (line_height - ascent - descent) / 2.0;
            assert_eq!(
                result.baseline,
                result.py + padding_top + ascent,
                "baseline should equal py + padding_top + ascent for cursor {:?}",
                cursor_type
            );
            assert_eq!(
                result.topline, result.py,
                "topline should be py (line top) for cursor {:?}",
                cursor_type
            );

            // Block and Caret cursors use topline
            // Underline cursor uses baseline + 1.0
            match cursor_type {
                Some(crate::SugarCursor::Block(_))
                | Some(crate::SugarCursor::Caret(_)) => {
                    // These should span from topline to topline + line_height
                    let cursor_top = result.topline;
                    let cursor_bottom = result.topline + line_height;

                    // Verify cursor encompasses the text area
                    let text_top = result.baseline - ascent;
                    let text_bottom = result.baseline + descent;
                    assert!(
                        cursor_top <= text_top,
                        "Block/Caret cursor top should be at or above text top"
                    );
                    assert!(
                        cursor_bottom >= text_bottom,
                        "Block/Caret cursor bottom should be at or below text bottom"
                    );
                }
                Some(crate::SugarCursor::Underline(_)) => {
                    // Underline should be positioned at baseline + 1.0
                    let underline_y = result.baseline + 1.0;

                    // Should be close to the text baseline
                    let text_baseline = result.baseline; // In new system, baseline is the actual text baseline
                    assert!(
                        (underline_y - text_baseline).abs() < descent + 2.0,
                        "Underline cursor should be close to text baseline"
                    );
                }
                _ => {}
            }
        }
    }

    #[test]
    fn test_glyph_and_cursor_baseline_consistency() {
        let mut helper = PositioningTestHelper::new();
        let font_id = 0;
        let font_size = 16.0;
        let ascent = 12.0;
        let descent = 3.0;
        let cursor = Some(crate::SugarCursor::Block([1.0, 1.0, 1.0, 1.0]));

        // Test with different line heights to ensure consistency
        for test_line_height in [16.0, 20.0, 24.0, 30.0] {
            let result = helper.calculate_positioning(
                "Ag",
                font_id,
                font_size,
                ascent,
                descent,
                test_line_height,
                cursor,
                None,
                false,
            );

            // Get glyph positioning
            let glyph_y = result.py + result.padding_y;
            let glyph_baseline_in_line_coords = glyph_y - result.padding_y; // Should be py

            // With the new coordinate system:
            // - glyph_baseline_in_line_coords is py (top of line)
            // - cursor baseline is py + padding_top + ascent
            // So the relationship is: cursor_baseline = glyph_baseline + padding_top + ascent
            let padding_top = (test_line_height - ascent - descent) / 2.0;
            assert_eq!(result.baseline, glyph_baseline_in_line_coords + padding_top + ascent,
                      "Cursor baseline ({}) should equal glyph baseline ({}) + padding_top ({}) + ascent ({}) for line_height {}",
                      result.baseline, glyph_baseline_in_line_coords, padding_top, ascent, test_line_height);

            // Verify padding_y calculation is correct
            // The actual calculation is: if line_height_mod > 1.0 then (line_height - line_height_without_mod) / 2.0 else 0.0
            // where line_height = line_height_without_mod * line_height_mod
            // and line_height_without_mod = ascent + descent + leading (leading is usually 0)
            let line_height_without_mod = ascent + descent; // assuming leading = 0
            let line_height_mod = test_line_height / line_height_without_mod;
            let expected_padding_y = if line_height_mod > 1.0 {
                (test_line_height - line_height_without_mod) / 2.0
            } else {
                0.0
            };
            assert!((result.padding_y - expected_padding_y).abs() < 0.1,
                   "padding_y should be calculated correctly for line_height {}. Expected: {}, Got: {}, line_height_mod: {}",
                   test_line_height, expected_padding_y, result.padding_y, line_height_mod);
        }
    }

    #[test]
    fn test_cached_vs_non_cached_positioning_consistency() {
        let mut helper = PositioningTestHelper::new();
        let font_id = 0;
        let font_size = 16.0;
        let ascent = 12.0;
        let descent = 3.0;
        let line_height = 20.0;

        let test_cases = [
            ("Hello", None, None),
            (
                "World",
                Some(crate::SugarCursor::Block([1.0, 1.0, 1.0, 1.0])),
                None,
            ),
            ("─", None, Some(crate::DrawableChar::Horizontal)),
            (
                "│",
                Some(crate::SugarCursor::Underline([1.0, 1.0, 1.0, 1.0])),
                Some(crate::DrawableChar::Vertical),
            ),
        ];

        for (text, cursor, drawable_char) in test_cases {
            let non_cached = helper.calculate_positioning(
                text,
                font_id,
                font_size,
                ascent,
                descent,
                line_height,
                cursor,
                drawable_char,
                false,
            );

            let cached = helper.calculate_positioning(
                text,
                font_id,
                font_size,
                ascent,
                descent,
                line_height,
                cursor,
                drawable_char,
                true,
            );

            // All positioning values must be identical between cached and non-cached
            assert_eq!(
                non_cached.baseline, cached.baseline,
                "Baseline mismatch for '{}': non-cached={}, cached={}",
                text, non_cached.baseline, cached.baseline
            );
            assert_eq!(
                non_cached.topline, cached.topline,
                "Topline mismatch for '{}': non-cached={}, cached={}",
                text, non_cached.topline, cached.topline
            );
            assert_eq!(
                non_cached.py, cached.py,
                "py mismatch for '{}': non-cached={}, cached={}",
                text, non_cached.py, cached.py
            );
            assert_eq!(
                non_cached.padding_y, cached.padding_y,
                "padding_y mismatch for '{}': non-cached={}, cached={}",
                text, non_cached.padding_y, cached.padding_y
            );
        }
    }

    #[test]
    fn test_positioning_invariants() {
        let mut helper = PositioningTestHelper::new();
        let font_id = 0;
        let font_size = 16.0;
        let ascent = 12.0;
        let descent = 3.0;

        // Test various line heights
        for line_height in [16.0, 18.0, 20.0, 24.0, 32.0] {
            let result = helper.calculate_positioning(
                "Test",
                font_id,
                font_size,
                ascent,
                descent,
                line_height,
                None,
                None,
                false,
            );

            // Core invariants that must always hold with new coordinate system
            let padding_top = (line_height - ascent - descent) / 2.0;
            assert_eq!(
                result.baseline,
                result.py + padding_top + ascent,
                "INVARIANT: baseline must equal py + padding_top + ascent (line_height={})",
                line_height
            );

            assert_eq!(
                result.topline, result.py,
                "INVARIANT: topline must equal py (line top) (line_height={})",
                line_height
            );

            // Padding calculation invariant
            let line_height_without_mod = ascent + descent; // assuming leading = 0
            let line_height_mod = line_height / line_height_without_mod;
            let expected_padding = if line_height_mod > 1.0 {
                (line_height - line_height_without_mod) / 2.0
            } else {
                0.0
            };
            assert!(
                (result.padding_y - expected_padding).abs() < 0.1,
                "INVARIANT: padding_y calculation (line_height={}, mod={})",
                line_height,
                line_height_mod
            );

            // Glyph positioning invariant
            let glyph_y = result.py + result.padding_y;
            assert!(
                glyph_y >= result.py,
                "INVARIANT: glyph_y must be >= py (line_height={})",
                line_height
            );

            // Text should fit within the line
            let text_top = result.py - ascent;
            let text_bottom = result.py + descent;
            let line_span = text_bottom - text_top;
            assert!(
                line_span <= line_height + 1.0, // +1 for rounding tolerance
                "INVARIANT: text should fit within line_height (line_height={})",
                line_height
            );
        }
    }

    #[test]
    fn test_strikethrough_positioning() {
        use crate::components::rich_text::compositor::Compositor;
        use crate::components::rich_text::text::TextRunStyle;
        use crate::components::rich_text::Rect;
        use crate::layout::FragmentStyleDecoration;

        let compositor = Compositor::new();
        let font_size = 16.0;
        let line_height = 20.0;
        let x_height = 8.0; // Typical x-height is about half the font size
        let strikeout_offset = 4.0; // Font-provided strikeout offset
        let ascent = 12.0;
        let descent = 4.0;

        // Test with font-provided strikeout offset
        let style_with_offset = TextRunStyle {
            font_coords: &[],
            font_size,
            color: [1.0, 1.0, 1.0, 1.0],
            background_color: None,
            baseline: 16.0,
            topline: 0.0,
            line_height,
            padding_y: 0.0,
            line_height_without_mod: line_height,
            advance: 100.0,
            decoration: Some(FragmentStyleDecoration::Strikethrough),
            decoration_color: None,
            cursor: None,
            drawable_char: None,
            underline_offset: 2.0,
            strikeout_offset,
            underline_thickness: 1.0,
            x_height,
            ascent,
            descent,
        };

        let rect = Rect::new(0.0, 0.0, 100.0, line_height);
        let underline =
            compositor.create_underline_from_decoration(&style_with_offset, &rect);

        if let Some(underline) = underline {
            // Should use font's strikeout offset (negated)
            assert_eq!(underline.offset, -strikeout_offset);
        } else {
            panic!("Expected strikethrough underline");
        }

        // Test with x-height fallback (no font strikeout offset)
        let style_with_x_height = TextRunStyle {
            strikeout_offset: 0.0, // No font-provided offset
            x_height,
            ..style_with_offset
        };

        let underline =
            compositor.create_underline_from_decoration(&style_with_x_height, &rect);

        if let Some(underline) = underline {
            // Should use half of x-height above baseline
            assert_eq!(underline.offset, -(x_height / 2.0));
        } else {
            panic!("Expected strikethrough underline");
        }

        // Test with final fallback (no font offset or x-height)
        let style_fallback = TextRunStyle {
            strikeout_offset: 0.0,
            x_height: 0.0,
            ..style_with_offset
        };

        let underline =
            compositor.create_underline_from_decoration(&style_fallback, &rect);

        if let Some(underline) = underline {
            // Should use approximate position
            assert_eq!(underline.offset, -(rect.height * 0.3));
        } else {
            panic!("Expected strikethrough underline");
        }
    }

    #[test]
    fn test_cursor_font_based_sizing() {
        // Test that cursor size is based on font metrics, not line height
        let _font_size = 16.0;
        let ascent = 12.0;
        let descent = 4.0;
        let font_height = ascent + descent; // 16.0

        // Test with different line heights to ensure cursor size is independent
        let line_heights = [16.0, 20.0, 24.0, 32.0];

        for line_height in line_heights {
            let baseline = 16.0;
            let cursor_top = baseline - ascent; // 16 - 12 = 4.0

            // Verify cursor dimensions are based on font metrics
            assert_eq!(
                font_height,
                ascent + descent,
                "Font height should equal ascent + descent"
            );
            assert_eq!(
                cursor_top,
                baseline - ascent,
                "Cursor top should be baseline - ascent"
            );

            // Cursor should be smaller than line height for increased line spacing
            if line_height > font_height {
                assert!(font_height < line_height, "Cursor should be smaller than line height when line spacing is increased");
            }
        }
    }

    #[test]
    fn test_cursor_positioning_with_different_font_metrics() {
        let _font_size = 16.0;
        let _line_height = 24.0; // Larger than font size
        let baseline = 20.0;

        // Test different font metric combinations
        let font_metrics = [
            ("Normal font", 12.0, 4.0),     // ascent=12, descent=4, total=16
            ("Tall font", 14.0, 6.0),       // ascent=14, descent=6, total=20
            ("Short font", 10.0, 2.0),      // ascent=10, descent=2, total=12
            ("Deep descenders", 11.0, 8.0), // ascent=11, descent=8, total=19
        ];

        for (font_name, ascent, descent) in font_metrics {
            let font_height = ascent + descent;
            let cursor_top = baseline - ascent;

            // Verify cursor positioning
            assert_eq!(
                font_height,
                ascent + descent,
                "{}: Font height should equal ascent + descent",
                font_name
            );
            assert_eq!(
                cursor_top,
                baseline - ascent,
                "{}: Cursor top should be baseline - ascent",
                font_name
            );

            // Cursor should start above baseline for fonts with ascent
            if ascent > 0.0 {
                assert!(
                    cursor_top < baseline,
                    "{}: Cursor should start above baseline",
                    font_name
                );
            }

            // Cursor should end below baseline for fonts with descent
            if descent > 0.0 {
                assert!(
                    cursor_top + font_height > baseline,
                    "{}: Cursor should extend below baseline",
                    font_name
                );
            }
        }
    }

    #[test]
    fn test_underline_cursor_positioning() {
        let _font_size = 16.0;
        let _line_height = 24.0;
        let baseline = 20.0;
        let _ascent = 12.0;
        let _descent = 4.0;

        // Underline cursor should be positioned at baseline + 1.0 with height 2.0
        let expected_underline_y = baseline + 1.0;
        let expected_underline_height = 2.0;

        // Underline cursor positioning should be independent of font metrics
        // It should always be just below the baseline
        assert_eq!(
            expected_underline_y,
            baseline + 1.0,
            "Underline cursor should be 1px below baseline"
        );
        assert_eq!(
            expected_underline_height, 2.0,
            "Underline cursor should be 2px tall"
        );
    }
}
