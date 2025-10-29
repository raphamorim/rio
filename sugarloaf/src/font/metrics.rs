// Font metrics implementation similar to consistent font metrics approach
// Key insight: Primary font determines cell dimensions for ALL fonts

use crate::font_introspector::Metrics as FontIntrospectorMetrics;

/// Font metrics similar to Rio's Metrics struct
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    /// Recommended cell width for monospace grid
    pub cell_width: u32,
    /// Recommended cell height for monospace grid  
    pub cell_height: u32,
    /// Distance from bottom of cell to text baseline
    pub cell_baseline: u32,
    /// Position of underline stroke (from top of cell)
    pub underline_position: u32,
    /// Thickness of underline stroke
    pub underline_thickness: u32,
    /// Position of strikethrough stroke (from top of cell)
    pub strikethrough_position: u32,
    /// Thickness of strikethrough stroke
    pub strikethrough_thickness: u32,
    /// Position of overline stroke (from top of cell)
    pub overline_position: i32,
    /// Thickness of overline stroke
    pub overline_thickness: u32,
    /// Thickness for box drawing characters
    pub box_thickness: u32,
    /// Height for cursor rendering
    pub cursor_height: u32,
}

/// Font face metrics extracted from font, for consistent font metrics
#[derive(Debug, Clone, Copy)]
pub struct FaceMetrics {
    /// Minimum cell width that can contain any glyph in ASCII range
    pub cell_width: f64,
    /// Typographic ascent metric from font
    pub ascent: f64,
    /// Typographic descent metric from font
    pub descent: f64,
    /// Typographic line gap from font
    pub line_gap: f64,
    /// Underline position (relative to baseline)
    pub underline_position: Option<f64>,
    /// Underline thickness
    pub underline_thickness: Option<f64>,
    /// Strikethrough position (relative to baseline)
    pub strikethrough_position: Option<f64>,
    /// Strikethrough thickness
    pub strikethrough_thickness: Option<f64>,
    /// Cap height
    pub cap_height: Option<f64>,
    /// X-height
    pub ex_height: Option<f64>,
    /// The width of the character "水" (CJK water ideograph, U+6C34),
    /// if present. This is used for font size adjustment, to normalize
    /// the width of CJK fonts mixed with latin fonts.
    ///
    /// Why "水" (water)?
    /// - It's a common CJK ideograph present in most CJK fonts
    /// - Has typical width characteristics of CJK characters
    /// - Simple structure makes it reliable for measurement
    /// - Part of the CJK Unified Ideographs block (U+4E00-U+9FFF)
    /// - Used as a standard reference in many font metrics systems
    ///
    /// NOTE: IC = Ideograph Character
    pub ic_width: Option<f64>,
}

impl FaceMetrics {
    /// Calculate line height (adjusted for Rio's font introspector format)
    /// Rio's descent is positive, and original code used leading * 2.0
    pub fn line_height(&self) -> f64 {
        // Rio's font introspector has positive descent, and original code doubled leading
        self.ascent + self.descent + (self.line_gap * 2.0)
    }
}

impl FaceMetrics {
    /// Create FaceMetrics from font reference and metrics with CJK character measurement
    ///
    /// This is the standard method for creating FaceMetrics that includes measuring
    /// the CJK water ideograph "水" (U+6C34) for proper font size normalization
    /// when mixing CJK and Latin fonts.
    pub fn from_font(
        font_ref: &crate::font_introspector::FontRef,
        metrics: &FontIntrospectorMetrics,
    ) -> Self {
        Self {
            cell_width: metrics.max_width as f64,
            ascent: metrics.ascent as f64,
            descent: metrics.descent as f64,
            line_gap: metrics.leading as f64,
            underline_position: Some(metrics.underline_offset as f64),
            underline_thickness: Some(metrics.stroke_size as f64),
            strikethrough_position: Some(metrics.strikeout_offset as f64),
            strikethrough_thickness: Some(metrics.stroke_size as f64),
            cap_height: Some(metrics.cap_height as f64),
            ex_height: Some(metrics.x_height as f64),
            // Measure actual CJK character width from the font
            ic_width: Self::measure_cjk_character_width(font_ref),
        }
    }

    /// Measure the width of the CJK water ideograph character "水" (U+6C34)
    ///
    /// This measurement is used for font size adjustment to normalize
    /// CJK fonts mixed with Latin fonts in the `calculate_cjk_font_size_adjustment` function.
    ///
    /// The water ideograph is chosen because:
    /// - It has consistent width across different CJK fonts
    /// - It's present in virtually all CJK fonts (basic kanji/hanzi)
    /// - Its width is representative of typical CJK character width
    /// - It avoids edge cases like punctuation or rare characters
    fn measure_cjk_character_width(
        font_ref: &crate::font_introspector::FontRef,
    ) -> Option<f64> {
        const CJK_WATER_IDEOGRAPH: u32 = 0x6C34; // "水"

        // Get character map
        let charmap = font_ref.charmap();

        // Get glyph ID for the CJK water ideograph
        let glyph_id = charmap.map(CJK_WATER_IDEOGRAPH);
        if glyph_id == 0 {
            // Character not found in font
            return None;
        }

        // Get glyph metrics
        let glyph_metrics =
            crate::font_introspector::GlyphMetrics::from_font(font_ref, &[]);

        // Get advance width for the glyph
        let advance_width = glyph_metrics.advance_width(glyph_id);

        if advance_width > 0.0 {
            Some(advance_width as f64)
        } else {
            None
        }
    }
}

impl Metrics {
    /// Calculate metrics from font face (consistent metrics calculation)
    pub fn calc(face: FaceMetrics) -> Self {
        // Use ceiling to ensure cell is large enough for any glyph
        let cell_width = face.cell_width.ceil();
        let cell_height = face.line_height().ceil();

        // Split line gap evenly between top and bottom of cell (but doubled as per Rio's original)
        let half_line_gap = face.line_gap; // Using full line_gap since we doubled it in line_height

        // Calculate baseline position from bottom of cell (adjusted for Rio's positive descent)
        // Rio uses positive descent format, baseline = half_line_gap + descent
        //
        // Baseline Adjustment Strategy:
        // - The baseline is positioned consistently for all fonts (primary and secondary)
        // - This ensures CJK characters align properly with Latin text
        // - The half_line_gap provides breathing room above and below text
        // - Using descent ensures proper positioning for characters with descenders
        let cell_baseline = (half_line_gap + face.descent).round();

        // Calculate top-to-baseline for other calculations
        let top_to_baseline = cell_height - cell_baseline;

        // Estimate cap height if not provided
        let cap_height = face.cap_height.unwrap_or(face.ascent * 0.75);

        // Estimate ex height if not provided
        let ex_height = face.ex_height.unwrap_or(cap_height * 0.75);

        // Calculate underline position and thickness
        let underline_thickness = face
            .underline_thickness
            .unwrap_or(0.15 * ex_height)
            .max(1.0);

        let underline_position = if let Some(pos) = face.underline_position {
            // Convert from baseline-relative to top-relative
            (top_to_baseline - pos).max(underline_thickness).round()
        } else {
            // Default: place 1 thickness below baseline
            (top_to_baseline + underline_thickness).round()
        };

        // Calculate strikethrough position and thickness
        let strikethrough_thickness = face
            .strikethrough_thickness
            .unwrap_or(underline_thickness)
            .max(1.0);

        let strikethrough_position = if let Some(pos) = face.strikethrough_position {
            // Convert from baseline-relative to top-relative
            (top_to_baseline - pos).round()
        } else {
            // Default: center at half ex height
            (top_to_baseline - ex_height * 0.5 + strikethrough_thickness * 0.5).round()
        };

        // Overline goes at the top
        let overline_position = 0;
        let overline_thickness = underline_thickness;

        // Box drawing thickness
        let box_thickness = underline_thickness;

        // Cursor height matches cell height
        let cursor_height = cell_height;

        Self {
            cell_width: cell_width as u32,
            cell_height: cell_height as u32,
            cell_baseline: cell_baseline as u32,
            underline_position: underline_position as u32,
            underline_thickness: underline_thickness as u32,
            strikethrough_position: strikethrough_position as u32,
            strikethrough_thickness: strikethrough_thickness as u32,
            overline_position,
            overline_thickness: overline_thickness as u32,
            box_thickness: box_thickness as u32,
            cursor_height: cursor_height as u32,
        }
    }

    /// Create metrics for non-primary font using primary font's cell dimensions
    /// This is the key insight: consistent cell dimensions, font-specific positioning
    /// Includes font size adjustment for CJK fonts
    pub fn calc_with_primary_cell_dimensions(
        face: FaceMetrics,
        primary_metrics: &Metrics,
    ) -> Self {
        // Calculate this font's natural metrics first
        let mut metrics = Self::calc(face);

        // Override with primary font's cell dimensions for consistency
        metrics.cell_width = primary_metrics.cell_width;
        metrics.cell_height = primary_metrics.cell_height;
        metrics.cell_baseline = primary_metrics.cell_baseline;
        metrics.cursor_height = primary_metrics.cursor_height;

        // Keep this font's specific positioning (underline, strikethrough, etc.)
        // This allows each font to have proper positioning within consistent cells

        metrics
    }

    /// Calculate font size adjustment for CJK fonts
    /// This normalizes CJK font sizes when mixed with Latin fonts
    pub fn calculate_cjk_font_size_adjustment(
        primary_face: &FaceMetrics,
        fallback_face: &FaceMetrics,
    ) -> Option<f64> {
        // Get primary font metrics for comparison
        let primary_ex = primary_face.ex_height.unwrap_or(
            primary_face
                .cap_height
                .unwrap_or(primary_face.ascent * 0.75)
                * 0.75,
        );

        let primary_ic = primary_face
            .ic_width
            .unwrap_or(primary_face.cell_width * 2.0);

        // Get fallback font metrics
        let fallback_ex = fallback_face.ex_height.unwrap_or(
            fallback_face
                .cap_height
                .unwrap_or(fallback_face.ascent * 0.75)
                * 0.75,
        );

        let fallback_ic = fallback_face
            .ic_width
            .unwrap_or(fallback_face.cell_width * 2.0);

        // If the fallback font has an ic_width, prefer that for normalization
        // of CJK font sizes when mixed with Latin fonts
        if fallback_face.ic_width.is_some() && fallback_ic > 0.0 && primary_ic > 0.0 {
            // Use ic_width ratio for CJK fonts
            Some(primary_ic / fallback_ic)
        } else if primary_ex > 0.0 && fallback_ex > 0.0 {
            // Fall back to ex_height ratio for general font size matching
            Some(primary_ex / fallback_ex)
        } else {
            None
        }
    }

    /// Apply baseline adjustment when cell height changes
    /// This ensures text remains vertically centered in the cell
    pub fn apply_cell_height_adjustment(&mut self, new_height: u32) {
        if new_height == self.cell_height {
            return;
        }

        let original_height = self.cell_height;

        // Split the difference in half to center the baseline in the cell
        if new_height > original_height {
            let diff = (new_height - original_height) / 2;
            self.cell_baseline = self.cell_baseline.saturating_add(diff);
            self.underline_position = self.underline_position.saturating_add(diff);
            self.strikethrough_position =
                self.strikethrough_position.saturating_add(diff);
        } else {
            let diff = (original_height - new_height) / 2;
            self.cell_baseline = self.cell_baseline.saturating_sub(diff);
            self.underline_position = self.underline_position.saturating_sub(diff);
            self.strikethrough_position =
                self.strikethrough_position.saturating_sub(diff);
        }

        self.cell_height = new_height;
    }

    /// Get baseline adjustment value (distance from bottom of cell to baseline)
    pub fn get_baseline_adjustment(&self) -> f32 {
        self.cell_baseline as f32
    }

    /// Get ascent/descent/leading for rich text rendering
    pub fn for_rich_text(&self) -> (f32, f32, f32) {
        let ascent = (self.cell_height - self.cell_baseline) as f32;
        let descent = self.cell_baseline as f32;
        let leading = 0.0; // Already incorporated into cell_height
        (ascent, descent, leading)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_primary_font_metrics() {
        let primary_face = FaceMetrics {
            cell_width: 10.0,
            ascent: 12.0,
            descent: 3.0, // Positive in Rio's format
            line_gap: 1.0,
            underline_position: Some(-1.0),
            underline_thickness: Some(1.0),
            strikethrough_position: Some(6.0),
            strikethrough_thickness: Some(1.0),
            cap_height: Some(9.0),
            ex_height: Some(6.0),
            ic_width: None,
        };

        let primary_metrics = Metrics::calc(primary_face);
        // Line height: 12 + 3 + (1 * 2) = 17
        assert_eq!(primary_metrics.cell_height, 17);
    }

    #[test]
    fn test_secondary_font_uses_primary_dimensions() {
        let primary_face = FaceMetrics {
            cell_width: 10.0,
            ascent: 12.0,
            descent: 3.0, // Positive in Rio's format
            line_gap: 1.0,
            underline_position: Some(-1.0),
            underline_thickness: Some(1.0),
            strikethrough_position: Some(6.0),
            strikethrough_thickness: Some(1.0),
            cap_height: Some(9.0),
            ex_height: Some(6.0),
            ic_width: None,
        };

        let cjk_face = FaceMetrics {
            cell_width: 12.0,
            ascent: 15.0,
            descent: 4.0, // Positive in Rio's format
            line_gap: 2.0,
            underline_position: Some(-2.0),
            underline_thickness: Some(1.5),
            strikethrough_position: Some(7.0),
            strikethrough_thickness: Some(1.5),
            cap_height: Some(11.0),
            ex_height: Some(8.0),
            ic_width: None,
        };

        let primary_metrics = Metrics::calc(primary_face);
        let cjk_metrics =
            Metrics::calc_with_primary_cell_dimensions(cjk_face, &primary_metrics);

        // CJK font should use primary font's cell dimensions
        assert_eq!(cjk_metrics.cell_width, primary_metrics.cell_width);
        assert_eq!(cjk_metrics.cell_height, primary_metrics.cell_height);
        assert_eq!(cjk_metrics.cell_baseline, primary_metrics.cell_baseline);

        // But can have its own positioning details
        // (underline_position, strikethrough_position might differ)
    }

    #[test]
    fn test_cjk_font_metrics_consistency() {
        // Test typical CJK font metrics (wider characters, different proportions)
        let latin_face = FaceMetrics {
            cell_width: 8.0,
            ascent: 10.0,
            descent: 2.0,
            line_gap: 0.5,
            underline_position: Some(-1.0),
            underline_thickness: Some(1.0),
            strikethrough_position: Some(5.0),
            strikethrough_thickness: Some(1.0),
            cap_height: Some(7.5),
            ex_height: Some(5.0),
            ic_width: None,
        };

        let cjk_face = FaceMetrics {
            cell_width: 16.0, // CJK characters are typically double-width
            ascent: 14.0,     // Often taller
            descent: 3.0,     // Deeper descent
            line_gap: 1.0,    // More line spacing
            underline_position: Some(-1.5),
            underline_thickness: Some(1.2),
            strikethrough_position: Some(7.0),
            strikethrough_thickness: Some(1.2),
            cap_height: Some(10.0),
            ex_height: Some(7.0),
            ic_width: None,
        };

        let latin_metrics = Metrics::calc(latin_face);
        let cjk_metrics =
            Metrics::calc_with_primary_cell_dimensions(cjk_face, &latin_metrics);

        // Both should have same cell dimensions for consistent terminal grid
        assert_eq!(cjk_metrics.cell_width, latin_metrics.cell_width);
        assert_eq!(cjk_metrics.cell_height, latin_metrics.cell_height);
        assert_eq!(cjk_metrics.cell_baseline, latin_metrics.cell_baseline);
        assert_eq!(cjk_metrics.cursor_height, latin_metrics.cursor_height);

        // Verify line height calculation: ascent + descent + (line_gap * 2.0)
        let expected_height =
            (latin_face.ascent + latin_face.descent + (latin_face.line_gap * 2.0)).ceil()
                as u32;
        assert_eq!(latin_metrics.cell_height, expected_height);
    }

    #[test]
    fn test_baseline_calculation_rio_format() {
        // Test that baseline calculation works correctly with Rio's positive descent
        let face = FaceMetrics {
            cell_width: 10.0,
            ascent: 12.0,
            descent: 4.0, // Positive in Rio's format (not negative like typical typography)
            line_gap: 2.0,
            underline_position: None,
            underline_thickness: None,
            strikethrough_position: None,
            strikethrough_thickness: None,
            cap_height: None,
            ex_height: None,
            ic_width: None,
        };

        let metrics = Metrics::calc(face);

        // Line height: 12 + 4 + (2 * 2) = 20
        assert_eq!(metrics.cell_height, 20);

        // Baseline: half_line_gap + descent = 2 + 4 = 6
        assert_eq!(metrics.cell_baseline, 6);

        // Verify rich text format conversion
        let (ascent, descent, leading) = metrics.for_rich_text();
        assert_eq!(ascent, 14.0); // cell_height - cell_baseline = 20 - 6
        assert_eq!(descent, 6.0); // cell_baseline
        assert_eq!(leading, 0.0); // Already incorporated
    }

    #[test]
    fn test_underline_positioning() {
        let face = FaceMetrics {
            cell_width: 10.0,
            ascent: 12.0,
            descent: 3.0,
            line_gap: 1.0,
            underline_position: Some(-2.0), // Below baseline
            underline_thickness: Some(1.5),
            strikethrough_position: None,
            strikethrough_thickness: None,
            cap_height: Some(9.0),
            ex_height: Some(6.0),
            ic_width: None,
        };

        let metrics = Metrics::calc(face);

        // Manual calculation:
        // Line height: 12 + 3 + (1 * 2) = 17
        // Baseline: 1 + 3 = 4 (half_line_gap + descent)
        // top_to_baseline: 17 - 4 = 13
        // underline_position: (13 - (-2.0)).max(1.5).round() = 15
        // underline_thickness: 1.5.max(1.0) as u32 = 1 (truncated, not rounded)

        assert_eq!(metrics.cell_height, 17);
        assert_eq!(metrics.cell_baseline, 4);
        assert_eq!(metrics.underline_position, 15);
        assert_eq!(metrics.underline_thickness, 1); // Truncated from 1.5
    }

    #[test]
    fn test_strikethrough_positioning() {
        let face = FaceMetrics {
            cell_width: 10.0,
            ascent: 12.0,
            descent: 3.0,
            line_gap: 1.0,
            underline_position: None,
            underline_thickness: None,
            strikethrough_position: Some(6.0), // Above baseline
            strikethrough_thickness: Some(1.2),
            cap_height: Some(9.0),
            ex_height: Some(6.0),
            ic_width: None,
        };

        let metrics = Metrics::calc(face);

        // Verify strikethrough is positioned correctly
        let top_to_baseline = metrics.cell_height - metrics.cell_baseline;
        let expected_strikethrough_pos = (top_to_baseline as f32 - 6.0).round() as u32;
        assert_eq!(metrics.strikethrough_position, expected_strikethrough_pos);
        assert_eq!(metrics.strikethrough_thickness, 1); // Rounded down from 1.2
    }

    #[test]
    fn test_fallback_values() {
        // Test metrics calculation with missing optional values
        let face = FaceMetrics {
            cell_width: 10.0,
            ascent: 12.0,
            descent: 3.0,
            line_gap: 1.0,
            underline_position: None,
            underline_thickness: None,
            strikethrough_position: None,
            strikethrough_thickness: None,
            cap_height: None,
            ex_height: None,
            ic_width: None,
        };

        let metrics = Metrics::calc(face);

        // Should calculate fallback values without panicking
        assert!(metrics.underline_thickness >= 1);
        assert!(metrics.strikethrough_thickness >= 1);
        assert!(metrics.overline_thickness >= 1);
        assert!(metrics.box_thickness >= 1);

        // Overline should be at top
        assert_eq!(metrics.overline_position, 0);
    }

    #[test]
    fn test_mixed_font_scenario() {
        // Simulate a real-world scenario with Latin primary and CJK fallback
        let cascadia_face = FaceMetrics {
            cell_width: 9.6,
            ascent: 11.5,
            descent: 2.5,
            line_gap: 0.8,
            underline_position: Some(-1.2),
            underline_thickness: Some(1.0),
            strikethrough_position: Some(5.8),
            strikethrough_thickness: Some(1.0),
            cap_height: Some(8.7),
            ex_height: Some(5.8),
            ic_width: None,
        };

        let noto_cjk_face = FaceMetrics {
            cell_width: 19.2, // Double width for CJK
            ascent: 13.8,
            descent: 3.2,
            line_gap: 1.2,
            underline_position: Some(-1.8),
            underline_thickness: Some(1.1),
            strikethrough_position: Some(6.9),
            strikethrough_thickness: Some(1.1),
            cap_height: Some(10.4),
            ex_height: Some(6.9),
            ic_width: None,
        };

        let primary_metrics = Metrics::calc(cascadia_face);
        let cjk_metrics =
            Metrics::calc_with_primary_cell_dimensions(noto_cjk_face, &primary_metrics);

        // Ensure consistent grid dimensions
        assert_eq!(cjk_metrics.cell_width, primary_metrics.cell_width);
        assert_eq!(cjk_metrics.cell_height, primary_metrics.cell_height);
        assert_eq!(cjk_metrics.cell_baseline, primary_metrics.cell_baseline);

        // Both should have reasonable values
        assert!(primary_metrics.cell_width > 0);
        assert!(primary_metrics.cell_height > 0);
        assert!(cjk_metrics.cell_width > 0);
        assert!(cjk_metrics.cell_height > 0);
    }

    #[test]
    fn test_line_height_calculation_edge_cases() {
        // Test with zero line gap
        let face_no_gap = FaceMetrics {
            cell_width: 10.0,
            ascent: 12.0,
            descent: 3.0,
            line_gap: 0.0,
            underline_position: None,
            underline_thickness: None,
            strikethrough_position: None,
            strikethrough_thickness: None,
            cap_height: None,
            ex_height: None,
            ic_width: None,
        };

        let metrics = Metrics::calc(face_no_gap);
        // Line height: 12 + 3 + (0 * 2) = 15
        assert_eq!(metrics.cell_height, 15);

        // Test with large line gap
        let face_large_gap = FaceMetrics {
            cell_width: 10.0,
            ascent: 12.0,
            descent: 3.0,
            line_gap: 5.0,
            underline_position: None,
            underline_thickness: None,
            strikethrough_position: None,
            strikethrough_thickness: None,
            cap_height: None,
            ex_height: None,
            ic_width: None,
        };

        let metrics_large = Metrics::calc(face_large_gap);
        // Line height: 12 + 3 + (5 * 2) = 25
        assert_eq!(metrics_large.cell_height, 25);
    }

    #[test]
    fn test_cjk_font_size_adjustment() {
        // Test CJK font size adjustment calculation
        let latin_face = FaceMetrics {
            cell_width: 8.0,
            ascent: 10.0,
            descent: 2.0,
            line_gap: 0.5,
            underline_position: Some(-1.0),
            underline_thickness: Some(1.0),
            strikethrough_position: Some(5.0),
            strikethrough_thickness: Some(1.0),
            cap_height: Some(7.5),
            ex_height: Some(5.0),
            ic_width: Some(16.0), // Latin font with measured CJK width
        };

        let cjk_face = FaceMetrics {
            cell_width: 16.0,
            ascent: 14.0,
            descent: 3.0,
            line_gap: 1.0,
            underline_position: Some(-1.5),
            underline_thickness: Some(1.2),
            strikethrough_position: Some(7.0),
            strikethrough_thickness: Some(1.2),
            cap_height: Some(10.0),
            ex_height: Some(7.0),
            ic_width: Some(32.0), // CJK font with measured CJK width
        };

        let adjustment =
            Metrics::calculate_cjk_font_size_adjustment(&latin_face, &cjk_face);

        // Should use ic_width ratio: 16.0 / 32.0 = 0.5
        assert!(adjustment.is_some());
        let ratio = adjustment.unwrap();
        assert!((ratio - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_baseline_adjustment() {
        let face = FaceMetrics {
            cell_width: 10.0,
            ascent: 12.0,
            descent: 3.0,
            line_gap: 1.0,
            underline_position: Some(-1.0),
            underline_thickness: Some(1.0),
            strikethrough_position: Some(6.0),
            strikethrough_thickness: Some(1.0),
            cap_height: Some(9.0),
            ex_height: Some(6.0),
            ic_width: None,
        };

        let mut metrics = Metrics::calc(face);
        let original_baseline = metrics.cell_baseline;
        let original_underline = metrics.underline_position;
        let original_strikethrough = metrics.strikethrough_position;

        // Test increasing cell height
        let new_height = metrics.cell_height + 10;
        metrics.apply_cell_height_adjustment(new_height);

        assert_eq!(metrics.cell_height, new_height);
        assert_eq!(metrics.cell_baseline, original_baseline + 5); // Half the difference
        assert_eq!(metrics.underline_position, original_underline + 5);
        assert_eq!(metrics.strikethrough_position, original_strikethrough + 5);

        // Test get_baseline_adjustment
        let baseline_adj = metrics.get_baseline_adjustment();
        assert_eq!(baseline_adj, metrics.cell_baseline as f32);
    }

    #[test]
    fn test_cjk_character_width_estimation() {
        // Test that ic_width is properly estimated when not provided
        let face_without_ic = FaceMetrics {
            cell_width: 10.0,
            ascent: 12.0,
            descent: 3.0,
            line_gap: 1.0,
            underline_position: None,
            underline_thickness: None,
            strikethrough_position: None,
            strikethrough_thickness: None,
            cap_height: None,
            ex_height: None,
            ic_width: None, // Not provided
        };

        let face_with_ic = FaceMetrics {
            cell_width: 10.0,
            ascent: 12.0,
            descent: 3.0,
            line_gap: 1.0,
            underline_position: None,
            underline_thickness: None,
            strikethrough_position: None,
            strikethrough_thickness: None,
            cap_height: None,
            ex_height: None,
            ic_width: Some(20.0), // Provided
        };

        // Test font size adjustment with estimated vs provided ic_width
        let adjustment_estimated =
            Metrics::calculate_cjk_font_size_adjustment(&face_without_ic, &face_with_ic);
        assert!(adjustment_estimated.is_some());

        // Should use estimated ic_width (cell_width * 2) vs provided ic_width
        // (10.0 * 2.0) / 20.0 = 1.0
        let ratio = adjustment_estimated.unwrap();
        assert!((ratio - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_rio_style_baseline_calculation() {
        // Test that baseline calculation matches Rio's approach
        // Rio: cell_baseline = half_line_gap + descent (with positive descent)

        let face = FaceMetrics {
            cell_width: 10.0,
            ascent: 12.0,
            descent: 4.0, // Positive in Rio (would be -4.0 in typical typography)
            line_gap: 2.0,
            underline_position: None,
            underline_thickness: None,
            strikethrough_position: None,
            strikethrough_thickness: None,
            cap_height: None,
            ex_height: None,
            ic_width: None,
        };

        let metrics = Metrics::calc(face);

        // Line height: 12 + 4 + (2 * 2) = 20
        assert_eq!(metrics.cell_height, 20);

        // Baseline: half_line_gap + descent = 2 + 4 = 6
        assert_eq!(metrics.cell_baseline, 6);

        // Verify baseline adjustment function
        assert_eq!(metrics.get_baseline_adjustment(), 6.0);
    }
}
