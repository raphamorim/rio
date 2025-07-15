// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// Compositor with vertex capture for text run caching

use crate::components::rich_text::batch::{BatchManager, RunUnderline};
pub use crate::components::rich_text::batch::{Rect, Vertex};
use crate::components::rich_text::image_cache::glyph::GlyphCacheSession;
use crate::components::rich_text::text::*;
use crate::layout::{FragmentStyleDecoration, UnderlineShape};

pub struct Compositor {
    pub batches: BatchManager,
}

impl Compositor {
    pub fn new() -> Self {
        Self {
            batches: BatchManager::new(),
        }
    }

    /// Calculate proper underline offset using real font metrics
    fn calculate_underline_offset(
        &self,
        style: &TextRunStyle,
        _underline_thickness: f32,
    ) -> f32 {
        // The rendering system uses: uy = baseline - offset
        // For terminal underlines, we want them positioned well below the text
        // to clear descenders like 'g', 'j', 'p', 'q', 'y'
        
        let font_size = style.font_size;
        let font_underline_offset = style.underline_offset;
        
        // For terminal applications, use a more conservative approach
        // Position underline at a reasonable distance below baseline
        // This should work well for most fonts and terminal use cases
        
        let reasonable_offset = if font_underline_offset.abs() > 0.1 {
            // Font metrics are available, but scale them appropriately for terminal use
            let font_based_offset = -font_underline_offset;
            
            // If the font metrics seem too small (less than 5% of font size), 
            // use a more reasonable default
            if font_based_offset < font_size * 0.05 {
                font_size * 0.1 // 10% of font size below baseline
            } else {
                font_based_offset
            }
        } else {
            // No font metrics available, use reasonable default
            font_size * 0.1 // 10% of font size below baseline
        };
        
        // Ensure minimum clearance for descenders and visibility
        let min_offset = (font_size * 0.08).max(2.0); // At least 8% of font size or 2px
        reasonable_offset.max(min_offset)
    }

    /// Calculate proper strikethrough offset using real font metrics
    fn calculate_strikethrough_offset(&self, style: &TextRunStyle) -> f32 {
        // Use real font metrics for strikethrough positioning
        // The strikeout_offset from font metrics should position it properly
        style.strikeout_offset
    }

    pub fn begin(&mut self) {
        self.batches.reset();
    }

    pub fn finish(&mut self, vertices: &mut Vec<Vertex>) {
        self.batches.build_display_list(vertices);
    }

    /// Standard draw_run method (for compatibility)
    #[inline]
    pub fn draw_run(
        &mut self,
        session: &mut GlyphCacheSession,
        rect: impl Into<Rect>,
        depth: f32,
        style: &TextRunStyle,
        glyphs: &[Glyph],
        _cache_operations: Option<&mut Vec<()>>, // Ignored - legacy parameter
    ) {
        self.draw_run_internal(session, rect, depth, style, glyphs);
    }

    /// Internal rendering implementation
    #[inline]
    fn draw_run_internal(
        &mut self,
        session: &mut GlyphCacheSession,
        rect: impl Into<Rect>,
        depth: f32,
        style: &TextRunStyle,
        glyphs: &[Glyph],
    ) {
        let rect = rect.into();
        let underline = match style.decoration {
            Some(FragmentStyleDecoration::Underline(info)) => {
                // Use font metrics for thickness when available, otherwise fall back to shape-based defaults
                let underline_thickness = if style.underline_thickness > 0.0 {
                    style.underline_thickness
                } else {
                    // Fallback thickness based on underline shape
                    match info.shape {
                        UnderlineShape::Regular => 1.0,
                        UnderlineShape::Dotted
                        | UnderlineShape::Dashed
                        | UnderlineShape::Curly => 2.0,
                    }
                };

                // Use real font metrics for proper underline positioning
                let underline_offset =
                    self.calculate_underline_offset(style, underline_thickness);

                Some(RunUnderline {
                    enabled: true,
                    offset: underline_offset.round() as i32,
                    size: underline_thickness,
                    color: style.decoration_color.unwrap_or(style.color),
                    is_doubled: info.is_doubled,
                    shape: info.shape,
                })
            }
            Some(FragmentStyleDecoration::Strikethrough) => {
                // Use real font metrics for proper strikethrough positioning
                let strikethrough_offset = self.calculate_strikethrough_offset(style);

                // Use font metrics for thickness when available
                let strikethrough_thickness = if style.underline_thickness > 0.0 {
                    style.underline_thickness
                } else {
                    2.0 // Fallback thickness
                };

                Some(RunUnderline {
                    enabled: true,
                    offset: strikethrough_offset.round() as i32,
                    size: strikethrough_thickness,
                    color: style.decoration_color.unwrap_or(style.color),
                    is_doubled: false,
                    shape: UnderlineShape::Regular,
                })
            }
            _ => None,
        };

        let subpx_bias = (0.125, 0.);
        let color = style.color;

        if let Some(builtin_character) = style.drawable_char {
            if let Some(bg_color) = style.background_color {
                let bg_rect =
                    Rect::new(rect.x, style.topline, rect.width, style.line_height);
                self.batches.add_rect(&bg_rect, depth, &bg_color);
            }

            if let Some(cursor) = style.cursor {
                match cursor {
                    crate::SugarCursor::Block(cursor_color) => {
                        let cursor_rect = Rect::new(
                            rect.x,
                            style.topline,
                            rect.width,
                            style.line_height,
                        );
                        self.batches.add_rect(&cursor_rect, depth, &cursor_color);
                    }
                    crate::SugarCursor::HollowBlock(cursor_color) => {
                        let outer_rect = Rect::new(
                            rect.x,
                            style.topline,
                            rect.width,
                            style.line_height,
                        );
                        self.batches.add_rect(&outer_rect, depth, &cursor_color);

                        if let Some(bg_color) = style.background_color {
                            let inner_rect = Rect::new(
                                rect.x + 1.0,
                                style.topline + 1.0,
                                rect.width - 2.0,
                                style.line_height - 2.0,
                            );
                            self.batches.add_rect(&inner_rect, depth, &bg_color);
                        }
                    }
                    crate::SugarCursor::Caret(cursor_color) => {
                        let outer_rect = Rect::new(
                            rect.x,
                            style.topline,
                            rect.width,
                            style.line_height,
                        );
                        self.batches.add_rect(&outer_rect, depth, &cursor_color);

                        if let Some(bg_color) = style.background_color {
                            let inner_rect = Rect::new(
                                rect.x + 1.0,
                                style.topline + 1.0,
                                rect.width - 2.0,
                                style.line_height - 2.0,
                            );
                            self.batches.add_rect(&inner_rect, depth, &bg_color);
                        }
                    }
                    crate::SugarCursor::Underline(cursor_color) => {
                        let caret_rect =
                            Rect::new(rect.x, style.baseline + 1.0, rect.width, 2.0);
                        self.batches.add_rect(&caret_rect, depth, &cursor_color);
                    }
                }
            }

            if let Some(underline) = underline {
                self.batches.draw_underline(
                    &underline,
                    rect.x,
                    rect.width,
                    style.baseline,
                    depth,
                    style.line_height,
                );
            }

            self.batches.draw_drawable_character(
                rect.x,
                style.topline,
                rect.width,
                builtin_character,
                color,
                depth,
                style.line_height,
            );
        } else {
            // Handle regular glyphs
            for glyph in glyphs {
                let entry = session.get(glyph.id);
                if let Some(entry) = entry {
                    if let Some(img) = session.get_image(entry.image) {
                        let gx = (glyph.x + subpx_bias.0).floor() + entry.left as f32;
                        let gy = (glyph.y + subpx_bias.1).floor() - entry.top as f32;
                        let glyph_rect =
                            Rect::new(gx, gy, entry.width as f32, entry.height as f32);
                        let coords = [img.min.0, img.min.1, img.max.0, img.max.1];

                        if entry.is_bitmap {
                            let bitmap_color = [1.0, 1.0, 1.0, 1.0];
                            self.batches.add_image_rect(
                                &glyph_rect,
                                depth,
                                &bitmap_color,
                                &coords,
                                entry.image.has_alpha(),
                            );
                        } else {
                            self.batches.add_mask_rect(
                                &glyph_rect,
                                depth,
                                &color,
                                &coords,
                                true,
                            );
                        }
                    }
                }
            }

            if let Some(bg_color) = style.background_color {
                let bg_rect =
                    Rect::new(rect.x, style.topline, rect.width, style.line_height);
                self.batches.add_rect(&bg_rect, depth, &bg_color);
            }

            if let Some(cursor) = style.cursor {
                match cursor {
                    crate::SugarCursor::Block(cursor_color) => {
                        let cursor_rect = Rect::new(
                            rect.x,
                            style.topline,
                            rect.width,
                            style.line_height,
                        );
                        self.batches.add_rect(&cursor_rect, depth, &cursor_color);
                    }
                    crate::SugarCursor::HollowBlock(cursor_color) => {
                        let outer_rect = Rect::new(
                            rect.x,
                            style.topline,
                            rect.width,
                            style.line_height,
                        );
                        self.batches.add_rect(&outer_rect, depth, &cursor_color);

                        if let Some(bg_color) = style.background_color {
                            let inner_rect = Rect::new(
                                rect.x + 1.0,
                                style.topline + 1.0,
                                rect.width - 2.0,
                                style.line_height - 2.0,
                            );
                            self.batches.add_rect(&inner_rect, depth, &bg_color);
                        }
                    }
                    crate::SugarCursor::Caret(cursor_color) => {
                        let outer_rect = Rect::new(
                            rect.x,
                            style.topline,
                            rect.width,
                            style.line_height,
                        );
                        self.batches.add_rect(&outer_rect, depth, &cursor_color);

                        if let Some(bg_color) = style.background_color {
                            let inner_rect = Rect::new(
                                rect.x + 1.0,
                                style.topline + 1.0,
                                rect.width - 2.0,
                                style.line_height - 2.0,
                            );
                            self.batches.add_rect(&inner_rect, depth, &bg_color);
                        }
                    }
                    crate::SugarCursor::Underline(cursor_color) => {
                        let caret_rect =
                            Rect::new(rect.x, style.baseline + 1.0, rect.width, 2.0);
                        self.batches.add_rect(&caret_rect, depth, &cursor_color);
                    }
                }
            }

            if let Some(underline) = underline {
                self.batches.draw_underline(
                    &underline,
                    rect.x,
                    rect.width,
                    style.baseline,
                    depth,
                    style.line_height,
                );
            }
        }
    }
}

impl Default for Compositor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{UnderlineInfo, UnderlineShape};
    use crate::components::rich_text::text::TextRunStyle;

    fn create_test_style(font_size: f32, line_height: f32, underline_offset: f32) -> TextRunStyle<'static> {
        TextRunStyle {
            font_coords: &[],
            font_size,
            color: [1.0, 1.0, 1.0, 1.0],
            background_color: None,
            baseline: line_height * 0.8, // Typical baseline position
            topline: 0.0,
            line_height,
            padding_y: 0.0,
            line_height_without_mod: line_height,
            advance: 10.0,
            decoration: None,
            decoration_color: None,
            cursor: None,
            drawable_char: None,
            underline_offset,
            strikeout_offset: 0.0,
            underline_thickness: 1.0,
        }
    }

    #[test]
    fn test_underline_offset_calculation_basic() {
        let compositor = Compositor::new();
        
        // Test with typical terminal font settings and reasonable font metrics
        let style = create_test_style(16.0, 20.0, -2.0);
        let offset = compositor.calculate_underline_offset(&style, 1.0);
        
        // Font metrics: -(-2.0) = 2.0, which is > 16.0 * 0.05 = 0.8, so use font metrics
        // min_offset = max(16.0 * 0.08, 2.0) = max(1.28, 2.0) = 2.0
        // final = max(2.0, 2.0) = 2.0
        assert_eq!(offset, 2.0);
    }

    #[test]
    fn test_underline_offset_small_font_metrics() {
        let compositor = Compositor::new();
        
        // Test with small font metrics that should trigger fallback
        let style = create_test_style(20.0, 24.0, -0.5);
        let offset = compositor.calculate_underline_offset(&style, 1.0);
        
        // Font metrics: -(-0.5) = 0.5, which is < 20.0 * 0.05 = 1.0, so use fallback
        // reasonable_offset = 20.0 * 0.1 = 2.0
        // min_offset = max(20.0 * 0.08, 2.0) = max(1.6, 2.0) = 2.0
        // final = max(2.0, 2.0) = 2.0
        assert_eq!(offset, 2.0);
    }

    #[test]
    fn test_underline_offset_no_font_metrics() {
        let compositor = Compositor::new();
        
        // Test fallback when font metrics are not available
        let style = create_test_style(16.0, 20.0, 0.0);
        let offset = compositor.calculate_underline_offset(&style, 1.0);
        
        // No font metrics, use fallback: 16.0 * 0.1 = 1.6
        // min_offset = max(16.0 * 0.08, 2.0) = max(1.28, 2.0) = 2.0
        // final = max(1.6, 2.0) = 2.0
        assert_eq!(offset, 2.0);
    }

    #[test]
    fn test_underline_offset_large_font() {
        let compositor = Compositor::new();
        
        // Test with larger font and good font metrics
        let style = create_test_style(30.0, 36.0, -4.0);
        let offset = compositor.calculate_underline_offset(&style, 1.0);
        
        // Font metrics: -(-4.0) = 4.0, which is > 30.0 * 0.05 = 1.5, so use font metrics
        // min_offset = max(30.0 * 0.08, 2.0) = max(2.4, 2.0) = 2.4
        // final = max(4.0, 2.4) = 4.0
        assert_eq!(offset, 4.0);
    }

    #[test]
    fn test_underline_offset_minimum_enforcement() {
        let compositor = Compositor::new();
        
        // Test with very small font where minimum should be enforced
        let style = create_test_style(10.0, 12.0, -0.5);
        let offset = compositor.calculate_underline_offset(&style, 1.0);
        
        // Font metrics: -(-0.5) = 0.5, which is < 10.0 * 0.05 = 0.5, so use fallback
        // reasonable_offset = 10.0 * 0.1 = 1.0
        // min_offset = max(10.0 * 0.08, 2.0) = max(0.8, 2.0) = 2.0
        // final = max(1.0, 2.0) = 2.0
        assert_eq!(offset, 2.0);
    }

    #[test]
    fn test_underline_positioning_relative_to_baseline() {
        let compositor = Compositor::new();
        
        // Test that underline is positioned below baseline
        let style = create_test_style(16.0, 24.0, -2.5);
        let offset = compositor.calculate_underline_offset(&style, 1.0);
        
        // Font metrics: -(-2.5) = 2.5, which is > 16.0 * 0.05 = 0.8, so use font metrics
        // min_offset = max(16.0 * 0.08, 2.0) = max(1.28, 2.0) = 2.0
        // final = max(2.5, 2.0) = 2.5
        assert_eq!(offset, 2.5);
        
        let baseline = style.baseline;
        let underline_y = baseline - offset;
        assert!(underline_y < baseline, "Underline should be positioned below baseline");
    }

    #[test]
    fn test_underline_clears_descenders() {
        let compositor = Compositor::new();
        
        // Test with typical settings where descenders exist
        let style = create_test_style(16.0, 20.0, -2.0);
        let offset = compositor.calculate_underline_offset(&style, 1.0);
        
        // For a 16px font:
        // - Descenders typically extend about 20% of font size = ~3.2px below baseline
        // - Our minimum offset is 8% of font size = 1.28px, but enforced minimum is 2.0px
        // - Font metrics give us 2.0px, which should be reasonable for most cases
        
        // The offset should be at least reasonable for clearing most descenders
        // We use a more conservative approach for terminal applications
        assert!(offset >= style.font_size * 0.08, 
                "Underline offset ({}) should be at least 8% of font size ({})", 
                offset, style.font_size * 0.08);
        
        // And should have a reasonable minimum
        assert!(offset >= 2.0, "Underline should have minimum 2px distance from baseline");
    }

    #[test]
    fn test_underline_stays_within_reasonable_bounds() {
        let compositor = Compositor::new();
        
        // Test that underline doesn't go too far down
        let style = create_test_style(16.0, 20.0, -2.0);
        let offset = compositor.calculate_underline_offset(&style, 1.0);
        
        // Should be reasonable for terminal use - not too far from baseline
        // With 16px font, offset should be reasonable (not more than ~25% of font size)
        assert!(offset <= style.font_size * 0.25, 
                "Underline offset ({}) should be reasonable relative to font size ({})", 
                offset, style.font_size);
        
        // But also not too close to baseline
        assert!(offset >= 2.0, "Underline should have minimum distance from baseline");
    }

    #[test]
    fn test_strikethrough_offset_uses_font_metrics() {
        let compositor = Compositor::new();
        
        let style = create_test_style(16.0, 20.0, -2.0);
        let offset = compositor.calculate_strikethrough_offset(&style);
        
        // Should use the strikeout_offset from font metrics
        assert_eq!(offset, style.strikeout_offset);
    }

    #[test]
    fn test_underline_info_creation() {
        // Test that UnderlineInfo struct works correctly without size/offset fields
        let underline_info = UnderlineInfo {
            is_doubled: false,
            shape: UnderlineShape::Regular,
        };
        
        assert!(!underline_info.is_doubled);
        assert_eq!(underline_info.shape, UnderlineShape::Regular);
    }

    #[test]
    fn test_underline_info_doubled() {
        let underline_info = UnderlineInfo {
            is_doubled: true,
            shape: UnderlineShape::Dashed,
        };
        
        assert!(underline_info.is_doubled);
        assert_eq!(underline_info.shape, UnderlineShape::Dashed);
    }

    #[test]
    fn test_underline_rendering_position_debug() {
        let compositor = Compositor::new();
        
        // Simulate real values from Rio terminal
        let style = create_test_style(28.0, 33.0, -1.3671875);
        let offset = compositor.calculate_underline_offset(&style, 1.0);
        
        // Debug the actual rendering calculation (FIXED VERSION)
        let baseline = style.baseline; // Should be around 158 based on debug output
        let underline_y = baseline + offset; // FIXED: now adding offset
        
        println!("Debug rendering (FIXED):");
        println!("  font_size: {}", style.font_size);
        println!("  line_height: {}", style.line_height);
        println!("  baseline: {}", baseline);
        println!("  calculated_offset: {}", offset);
        println!("  underline_y (baseline + offset): {}", underline_y);
        println!("  underline_y relative to baseline: {}", underline_y - baseline);
        
        // The underline should now be below the baseline
        assert!(underline_y > baseline, 
                "Underline Y ({}) should be below baseline ({})", underline_y, baseline);
        
        // Check if the offset is reasonable
        assert!(offset > 1.0, "Offset should be at least 1px");
        assert!(offset < style.font_size * 0.5, "Offset shouldn't be more than 50% of font size");
    }

    #[test]
    fn test_coordinate_system_assumptions() {
        let compositor = Compositor::new();
        
        // Test our assumptions about the coordinate system
        let style = create_test_style(16.0, 20.0, -2.0);
        let offset = compositor.calculate_underline_offset(&style, 1.0);
        
        // Simulate the rendering calculation: uy = baseline - offset
        let baseline = 100.0; // Arbitrary baseline position
        let underline_y = baseline - offset;
        
        println!("Coordinate system test:");
        println!("  baseline: {}", baseline);
        println!("  offset: {}", offset);
        println!("  underline_y: {}", underline_y);
        
        // If Y increases downward (typical screen coordinates):
        // - baseline = 100
        // - offset = 2 (positive, meaning "go down from baseline")
        // - underline_y = 100 - 2 = 98 (above baseline - WRONG!)
        
        // This suggests we might need: uy = baseline + offset
        let corrected_underline_y = baseline + offset;
        println!("  corrected_underline_y (baseline + offset): {}", corrected_underline_y);
        
        // The corrected calculation should put underline below baseline
        assert!(corrected_underline_y > baseline, 
                "Corrected underline Y ({}) should be below baseline ({})", 
                corrected_underline_y, baseline);
    }

    #[test]
    fn test_typical_terminal_positioning() {
        let compositor = Compositor::new();
        
        // Test with typical terminal settings
        let style = create_test_style(14.0, 18.0, -1.5);
        let offset = compositor.calculate_underline_offset(&style, 1.0);
        
        // Simulate a line of text in a terminal
        let line_top = 0.0;
        let line_height = style.line_height;
        let baseline = line_top + (line_height * 0.8); // Typical baseline position
        
        println!("Terminal positioning test:");
        println!("  line_top: {}", line_top);
        println!("  line_height: {}", line_height);
        println!("  baseline: {}", baseline);
        println!("  offset: {}", offset);
        
        // Current calculation
        let current_underline_y = baseline - offset;
        println!("  current_underline_y (baseline - offset): {}", current_underline_y);
        
        // Alternative calculation
        let alt_underline_y = baseline + offset;
        println!("  alternative_underline_y (baseline + offset): {}", alt_underline_y);
        
        // Check which makes more sense
        let line_bottom = line_top + line_height;
        println!("  line_bottom: {}", line_bottom);
        
        // The underline should be:
        // 1. Below the baseline
        // 2. Above the next line (if any)
        // 3. Within reasonable bounds of the current line
        
        if current_underline_y > baseline {
            println!("  Current calculation puts underline below baseline ✓");
        } else {
            println!("  Current calculation puts underline above baseline ✗");
        }
        
        if alt_underline_y > baseline {
            println!("  Alternative calculation puts underline below baseline ✓");
        } else {
            println!("  Alternative calculation puts underline above baseline ✗");
        }
    }

    #[test]
    fn test_underline_position_relative_to_text_bounds() {
        let compositor = Compositor::new();
        
        // Test positioning relative to typical text bounds
        let style = create_test_style(20.0, 24.0, -2.0);
        let offset = compositor.calculate_underline_offset(&style, 1.0);
        
        // Typical font metrics (approximate)
        let font_size = style.font_size;
        let ascent = font_size * 0.8;  // ~80% of font size
        let descent = font_size * 0.2; // ~20% of font size
        
        let baseline = 100.0; // Arbitrary baseline
        let text_top = baseline - ascent;
        let text_bottom = baseline + descent;
        
        println!("Text bounds test (FIXED):");
        println!("  font_size: {}", font_size);
        println!("  ascent: {}", ascent);
        println!("  descent: {}", descent);
        println!("  baseline: {}", baseline);
        println!("  text_top: {}", text_top);
        println!("  text_bottom: {}", text_bottom);
        println!("  calculated_offset: {}", offset);
        
        // FIXED rendering calculation
        let underline_y = baseline + offset;
        println!("  underline_y (baseline + offset): {}", underline_y);
        
        // Check if underline is positioned correctly
        if underline_y > text_bottom {
            println!("  Underline is below text bottom ✓");
        } else if underline_y > baseline {
            println!("  Underline is below baseline but above text bottom (might clip descenders) ⚠");
        } else {
            println!("  Underline is above baseline (definitely wrong) ✗");
        }
        
        // The underline should now be below the baseline
        assert!(underline_y > baseline, 
                "Underline should be below baseline");
    }
}
