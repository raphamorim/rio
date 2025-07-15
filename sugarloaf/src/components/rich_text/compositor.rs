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

    /// Calculate proper underline offset using golden ratio positioning within descent area
    fn calculate_underline_offset(
        &self,
        style: &TextRunStyle,
        _underline_thickness: f32,
    ) -> f32 {
        // Use golden ratio (0.618) positioning within the descent area for optimal visual balance
        // This positions underlines in the descent area where descenders naturally extend
        
        let actual_descent = style.descent;
        let font_size = style.font_size;
        
        let golden_ratio_offset = if actual_descent > 0.0 && actual_descent < font_size {
            // Use actual descent metrics with golden ratio positioning
            actual_descent * 0.618
        } else {
            // Fallback: estimate descent as ~20% of font size
            let estimated_descent = font_size * 0.2;
            estimated_descent * 0.618
        };
        
        // Ensure adequate clearance below text to avoid clipping descenders
        let min_clearance = (font_size * 0.2) + (font_size * 0.05); // descent + 5% clearance
        golden_ratio_offset.max(min_clearance)
    }

    /// Calculate proper strikethrough offset using ascent-based positioning
    fn calculate_strikethrough_offset(&self, style: &TextRunStyle) -> f32 {
        // Position strikethrough based on ascent height for consistent text-middle placement
        // Formula: (ascent * 0.5) * 0.5 = ascent * 0.25
        // Made negative since it's above baseline in our coordinate system
        
        let font_size = style.font_size;
        
        if style.strikeout_offset.abs() > 0.1 {
            // Use font metrics, but ensure it's positioned above baseline (negative offset)
            -style.strikeout_offset.abs()
        } else {
            // Use ascent-based calculation: ascent * 0.25, made negative for above baseline
            // Typical ascent is ~80% of font size, so: font_size * 0.8 * 0.25 = font_size * 0.2
            -(font_size * 0.2)
        }
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
            baseline: line_height * 0.8,
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
            descent: font_size * 0.2,
        }
    }

    #[test]
    fn test_actual_rendering_positions() {
        let compositor = Compositor::new();
        
        println!("\n=== Testing ACTUAL Rendering Positions ===");
        
        let font_size = 16.0;
        let line_height = 20.0;
        let baseline = line_height * 0.8; // 16.0 - this is where text sits
        
        // Create style with typical font metrics
        let mut style = create_test_style(font_size, line_height, -2.4);
        style.strikeout_offset = 4.0; // Typical strikethrough position
        
        let underline_offset = compositor.calculate_underline_offset(&style, 1.0);
        let strikethrough_offset = compositor.calculate_strikethrough_offset(&style);
        
        println!("=== Font Metrics ===");
        println!("Font size: {}", font_size);
        println!("Line height: {}", line_height);
        println!("Baseline (where text sits): {}", baseline);
        println!("Font underline_position: {}", style.underline_offset);
        println!("Font strikeout_offset: {}", style.strikeout_offset);
        
        println!("\n=== Calculated Offsets ===");
        println!("Underline offset: {}", underline_offset);
        println!("Strikethrough offset: {}", strikethrough_offset);
        
        // Now let's see where these actually render
        // The key question: How does the rendering system use these offsets?
        
        // HYPOTHESIS 1: offset is added to baseline (baseline + offset)
        let underline_y_hypothesis1 = baseline + underline_offset;
        let strikethrough_y_hypothesis1 = baseline + strikethrough_offset;
        
        // HYPOTHESIS 2: offset is subtracted from baseline (baseline - offset)  
        let underline_y_hypothesis2 = baseline - underline_offset;
        let strikethrough_y_hypothesis2 = baseline - strikethrough_offset;
        
        println!("\n=== Rendering Position Hypotheses ===");
        println!("Baseline: {}", baseline);
        println!("Text top: ~{}", baseline - font_size * 0.8); // Approximate text top
        println!("Text bottom: ~{}", baseline + font_size * 0.2); // Approximate text bottom
        
        println!("\nHYPOTHESIS 1: y = baseline + offset");
        println!("  Underline at: {} (baseline + {})", underline_y_hypothesis1, underline_offset);
        println!("  Strikethrough at: {} (baseline + {})", strikethrough_y_hypothesis1, strikethrough_offset);
        
        println!("\nHYPOTHESIS 2: y = baseline - offset");
        println!("  Underline at: {} (baseline - {})", underline_y_hypothesis2, underline_offset);
        println!("  Strikethrough at: {} (baseline - {})", strikethrough_y_hypothesis2, strikethrough_offset);
        
        // Let's analyze which makes sense
        let text_top = baseline - font_size * 0.8;
        let text_bottom = baseline + font_size * 0.2;
        
        println!("\n=== Analysis ===");
        println!("For CORRECT positioning:");
        println!("- Underline should be BELOW text (y > {})", text_bottom);
        println!("- Strikethrough should be THROUGH middle of text ({} < y < {})", text_top, text_bottom);
        
        println!("\nHypothesis 1 results:");
        if underline_y_hypothesis1 > text_bottom {
            println!("  ✓ Underline below text");
        } else {
            println!("  ✗ Underline NOT below text");
        }
        
        if strikethrough_y_hypothesis1 > text_top && strikethrough_y_hypothesis1 < text_bottom {
            println!("  ✓ Strikethrough through text");
        } else {
            println!("  ✗ Strikethrough NOT through text");
        }
        
        println!("\nHypothesis 2 results:");
        if underline_y_hypothesis2 > text_bottom {
            println!("  ✓ Underline below text");
        } else {
            println!("  ✗ Underline NOT below text");
        }
        
        if strikethrough_y_hypothesis2 > text_top && strikethrough_y_hypothesis2 < text_bottom {
            println!("  ✓ Strikethrough through text");
        } else {
            println!("  ✗ Strikethrough NOT through text");
        }
        
        // The correct hypothesis should have:
        // - Underline below text bottom
        // - Strikethrough through middle of text
        
        // Based on testing, we determined that hypothesis 1 is correct
    }

    #[test]
    fn test_strikethrough_detailed_analysis() {
        let compositor = Compositor::new();
        
        println!("\n=== Detailed Strikethrough Analysis ===");
        
        let font_size = 16.0;
        let line_height = 20.0;
        let baseline = line_height * 0.8; // 16.0
        
        // Calculate text boundaries more precisely
        let ascent = font_size * 0.8; // Typical ascent is ~80% of font size
        let descent = font_size * 0.2; // Typical descent is ~20% of font size
        
        let text_top = baseline - ascent; // 16 - 12.8 = 3.2
        let text_bottom = baseline + descent; // 16 + 3.2 = 19.2
        let text_middle = baseline - (ascent * 0.5); // 16 - 6.4 = 9.6
        
        println!("Font size: {}", font_size);
        println!("Line height: {}", line_height);
        println!("Baseline: {}", baseline);
        println!("Ascent: {} ({}% of font size)", ascent, (ascent / font_size) * 100.0);
        println!("Descent: {} ({}% of font size)", descent, (descent / font_size) * 100.0);
        println!("Text top: {}", text_top);
        println!("Text middle: {}", text_middle);
        println!("Text bottom: {}", text_bottom);
        
        // Test different strikethrough offset percentages
        let test_percentages = vec![0.1, 0.15, 0.2, 0.25, 0.3, 0.35, 0.4, 0.45, 0.5];
        
        println!("\n=== Testing Different Strikethrough Percentages ===");
        for percentage in test_percentages {
            let offset = -(font_size * percentage);
            let render_y = baseline + offset;
            let distance_from_middle = f32::abs(render_y - text_middle);
            
            println!("{}% of font size: offset = {}, renders at y = {}, distance from middle = {}", 
                     percentage * 100.0, offset, render_y, distance_from_middle);
            
            if render_y >= text_top && render_y <= text_bottom {
                if distance_from_middle < 1.0 {
                    println!("  ✓ EXCELLENT - Very close to middle");
                } else if distance_from_middle < 2.0 {
                    println!("  ✓ GOOD - Close to middle");
                } else {
                    println!("  ⚠ OK - Through text but not centered");
                }
            } else {
                println!("  ✗ BAD - Outside text bounds");
            }
        }
        
        // What should the ideal offset be?
        let ideal_offset: f32 = text_middle - baseline;
        let ideal_percentage = ideal_offset.abs() / font_size;
        
        println!("\n=== Ideal Calculation ===");
        println!("Text middle: {}", text_middle);
        println!("Baseline: {}", baseline);
        println!("Ideal offset: {}", ideal_offset);
        println!("Ideal percentage: {}%", ideal_percentage * 100.0);
        
        // Test current implementation
        let mut style = create_test_style(font_size, line_height, -2.4);
        style.strikeout_offset = 0.05; // Force fallback
        let current_offset = compositor.calculate_strikethrough_offset(&style);
        let current_render_y = baseline + current_offset;
        
        println!("\n=== Current Implementation ===");
        println!("Current offset: {}", current_offset);
        println!("Current render y: {}", current_render_y);
        println!("Distance from ideal middle: {}", f32::abs(current_render_y - text_middle));
    }
}
