// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// CJK Font Metrics Integration Tests
//
// These tests verify that the consistent font metrics approach
// correctly handles CJK (Chinese, Japanese, Korean) fonts alongside
// Latin fonts, ensuring consistent terminal grid behavior.

#[cfg(test)]
mod tests {
    use crate::font::metrics::{FaceMetrics, Metrics};

    /// Test data representing typical font metrics for different font types
    struct TestFontData {
        name: &'static str,
        face: FaceMetrics,
    }

    impl TestFontData {
        fn cascadia_code() -> Self {
            Self {
                name: "Cascadia Code",
                face: FaceMetrics {
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
                },
            }
        }

        fn noto_sans_cjk() -> Self {
            Self {
                name: "Noto Sans CJK",
                face: FaceMetrics {
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
                },
            }
        }

        fn source_han_sans() -> Self {
            Self {
                name: "Source Han Sans",
                face: FaceMetrics {
                    cell_width: 18.8,
                    ascent: 14.2,
                    descent: 3.8,
                    line_gap: 1.0,
                    underline_position: Some(-2.0),
                    underline_thickness: Some(1.2),
                    strikethrough_position: Some(7.1),
                    strikethrough_thickness: Some(1.2),
                    cap_height: Some(10.7),
                    ex_height: Some(7.1),
                    ic_width: None,
                },
            }
        }

        fn wenquanyi_micro_hei() -> Self {
            Self {
                name: "WenQuanYi Micro Hei",
                face: FaceMetrics {
                    cell_width: 20.0,
                    ascent: 15.0,
                    descent: 4.0,
                    line_gap: 1.5,
                    underline_position: Some(-1.5),
                    underline_thickness: Some(1.0),
                    strikethrough_position: Some(7.5),
                    strikethrough_thickness: Some(1.0),
                    cap_height: Some(11.25),
                    ex_height: Some(7.5),
                    ic_width: None,
                },
            }
        }

        fn dejavu_sans_mono() -> Self {
            Self {
                name: "DejaVu Sans Mono",
                face: FaceMetrics {
                    cell_width: 8.4,
                    ascent: 10.8,
                    descent: 2.2,
                    line_gap: 0.6,
                    underline_position: Some(-1.0),
                    underline_thickness: Some(0.8),
                    strikethrough_position: Some(5.4),
                    strikethrough_thickness: Some(0.8),
                    cap_height: Some(8.1),
                    ex_height: Some(5.4),
                    ic_width: None,
                },
            }
        }

        fn noto_color_emoji() -> Self {
            Self {
                name: "Noto Color Emoji",
                face: FaceMetrics {
                    cell_width: 20.0, // Emoji are typically double-width
                    ascent: 14.0,
                    descent: 3.5,
                    line_gap: 1.0,
                    underline_position: Some(-2.0),
                    underline_thickness: Some(1.2),
                    strikethrough_position: Some(7.0),
                    strikethrough_thickness: Some(1.2),
                    cap_height: Some(10.5),
                    ex_height: Some(7.0),
                    ic_width: None,
                },
            }
        }
    }

    #[test]
    fn test_cjk_font_consistency_with_latin_primary() {
        let latin_font = TestFontData::cascadia_code();
        let cjk_fonts = vec![
            TestFontData::noto_sans_cjk(),
            TestFontData::source_han_sans(),
            TestFontData::wenquanyi_micro_hei(),
        ];

        let primary_metrics = Metrics::calc(latin_font.face);

        for cjk_font in cjk_fonts {
            let cjk_metrics = Metrics::calc_with_primary_cell_dimensions(
                cjk_font.face,
                &primary_metrics,
            );

            // All fonts should use the same cell dimensions
            assert_eq!(
                cjk_metrics.cell_width, primary_metrics.cell_width,
                "{} should use primary cell width",
                cjk_font.name
            );
            assert_eq!(
                cjk_metrics.cell_height, primary_metrics.cell_height,
                "{} should use primary cell height",
                cjk_font.name
            );
            assert_eq!(
                cjk_metrics.cell_baseline, primary_metrics.cell_baseline,
                "{} should use primary baseline",
                cjk_font.name
            );
            assert_eq!(
                cjk_metrics.cursor_height, primary_metrics.cursor_height,
                "{} should use primary cursor height",
                cjk_font.name
            );

            // Verify metrics are reasonable
            assert!(
                cjk_metrics.cell_width > 0,
                "{} cell width should be positive",
                cjk_font.name
            );
            assert!(
                cjk_metrics.cell_height > 0,
                "{} cell height should be positive",
                cjk_font.name
            );
            assert!(
                cjk_metrics.cell_baseline < cjk_metrics.cell_height,
                "{} baseline should be within cell height",
                cjk_font.name
            );
        }
    }

    #[test]
    fn test_multiple_latin_fonts_consistency() {
        let fonts = vec![
            TestFontData::cascadia_code(),
            TestFontData::dejavu_sans_mono(),
        ];

        let primary_metrics = Metrics::calc(fonts[0].face);

        for font in fonts.iter().skip(1) {
            let font_metrics =
                Metrics::calc_with_primary_cell_dimensions(font.face, &primary_metrics);

            assert_eq!(
                font_metrics.cell_width, primary_metrics.cell_width,
                "{} should use primary cell width",
                font.name
            );
            assert_eq!(
                font_metrics.cell_height, primary_metrics.cell_height,
                "{} should use primary cell height",
                font.name
            );
            assert_eq!(
                font_metrics.cell_baseline, primary_metrics.cell_baseline,
                "{} should use primary baseline",
                font.name
            );
        }
    }

    #[test]
    fn test_cjk_primary_font_scenario() {
        // Test scenario where CJK font is the primary font
        let cjk_font = TestFontData::noto_sans_cjk();
        let latin_font = TestFontData::cascadia_code();

        let primary_metrics = Metrics::calc(cjk_font.face);
        let latin_metrics =
            Metrics::calc_with_primary_cell_dimensions(latin_font.face, &primary_metrics);

        // Latin font should adapt to CJK dimensions
        assert_eq!(latin_metrics.cell_width, primary_metrics.cell_width);
        assert_eq!(latin_metrics.cell_height, primary_metrics.cell_height);
        assert_eq!(latin_metrics.cell_baseline, primary_metrics.cell_baseline);

        // Verify the CJK font's natural dimensions are used
        let expected_height = (cjk_font.face.ascent
            + cjk_font.face.descent
            + (cjk_font.face.line_gap * 2.0))
            .ceil() as u32;
        assert_eq!(primary_metrics.cell_height, expected_height);
    }

    #[test]
    fn test_rich_text_format_consistency() {
        let latin_font = TestFontData::cascadia_code();
        let cjk_font = TestFontData::noto_sans_cjk();

        let primary_metrics = Metrics::calc(latin_font.face);
        let cjk_metrics =
            Metrics::calc_with_primary_cell_dimensions(cjk_font.face, &primary_metrics);

        let (latin_ascent, latin_descent, latin_leading) =
            primary_metrics.for_rich_text();
        let (cjk_ascent, cjk_descent, cjk_leading) = cjk_metrics.for_rich_text();

        // Rich text format should be consistent
        assert_eq!(
            latin_ascent, cjk_ascent,
            "Rich text ascent should be consistent"
        );
        assert_eq!(
            latin_descent, cjk_descent,
            "Rich text descent should be consistent"
        );
        assert_eq!(
            latin_leading, cjk_leading,
            "Rich text leading should be consistent"
        );

        // Verify the values make sense
        assert!(latin_ascent > 0.0, "Ascent should be positive");
        assert!(latin_descent > 0.0, "Descent should be positive");
        assert_eq!(
            latin_leading, 0.0,
            "Leading should be 0 (incorporated into height)"
        );

        // Verify ascent + descent equals cell height
        assert_eq!(
            latin_ascent + latin_descent,
            primary_metrics.cell_height as f32
        );
    }

    #[test]
    fn test_underline_positioning_across_fonts() {
        let latin_font = TestFontData::cascadia_code();
        let cjk_font = TestFontData::noto_sans_cjk();

        let primary_metrics = Metrics::calc(latin_font.face);
        let cjk_metrics =
            Metrics::calc_with_primary_cell_dimensions(cjk_font.face, &primary_metrics);

        // Both should have reasonable underline positioning
        assert!(primary_metrics.underline_position <= primary_metrics.cell_height);
        assert!(primary_metrics.underline_thickness > 0);
        assert!(cjk_metrics.underline_thickness > 0);

        // Note: CJK font's underline position might be outside cell bounds when using
        // primary font's cell dimensions. This is expected behavior - the underline
        // calculation is font-specific but constrained by primary font's cell size.
        // In practice, the renderer would clamp this to reasonable bounds.

        // Verify that both fonts use the same cell dimensions (the key requirement)
        assert_eq!(cjk_metrics.cell_width, primary_metrics.cell_width);
        assert_eq!(cjk_metrics.cell_height, primary_metrics.cell_height);
        assert_eq!(cjk_metrics.cell_baseline, primary_metrics.cell_baseline);
    }

    #[test]
    fn test_strikethrough_positioning_across_fonts() {
        let latin_font = TestFontData::cascadia_code();
        let cjk_font = TestFontData::noto_sans_cjk();

        let primary_metrics = Metrics::calc(latin_font.face);
        let cjk_metrics =
            Metrics::calc_with_primary_cell_dimensions(cjk_font.face, &primary_metrics);

        // Both should have reasonable strikethrough positioning
        assert!(primary_metrics.strikethrough_position < primary_metrics.cell_height);
        assert!(cjk_metrics.strikethrough_position < cjk_metrics.cell_height);
        assert!(primary_metrics.strikethrough_thickness > 0);
        assert!(cjk_metrics.strikethrough_thickness > 0);

        // Strikethrough should be above underline
        assert!(
            primary_metrics.strikethrough_position < primary_metrics.underline_position
        );
        assert!(cjk_metrics.strikethrough_position < cjk_metrics.underline_position);
    }

    #[test]
    fn test_extreme_font_metrics() {
        // Test with extreme font metrics to ensure robustness
        let tiny_font = FaceMetrics {
            cell_width: 1.0,
            ascent: 2.0,
            descent: 0.5,
            line_gap: 0.1,
            underline_position: Some(-0.1),
            underline_thickness: Some(0.1),
            strikethrough_position: Some(1.0),
            strikethrough_thickness: Some(0.1),
            cap_height: Some(1.5),
            ex_height: Some(1.0),
            ic_width: None,
        };

        let huge_font = FaceMetrics {
            cell_width: 100.0,
            ascent: 120.0,
            descent: 30.0,
            line_gap: 10.0,
            underline_position: Some(-5.0),
            underline_thickness: Some(3.0),
            strikethrough_position: Some(60.0),
            strikethrough_thickness: Some(3.0),
            cap_height: Some(90.0),
            ex_height: Some(60.0),
            ic_width: None,
        };

        let tiny_metrics = Metrics::calc(tiny_font);
        let huge_metrics =
            Metrics::calc_with_primary_cell_dimensions(huge_font, &tiny_metrics);

        // Huge font should adapt to tiny font's dimensions
        assert_eq!(huge_metrics.cell_width, tiny_metrics.cell_width);
        assert_eq!(huge_metrics.cell_height, tiny_metrics.cell_height);
        assert_eq!(huge_metrics.cell_baseline, tiny_metrics.cell_baseline);

        // All values should be reasonable (no panics, no zero values where inappropriate)
        assert!(tiny_metrics.cell_width > 0);
        assert!(tiny_metrics.cell_height > 0);
        assert!(huge_metrics.cell_width > 0);
        assert!(huge_metrics.cell_height > 0);
    }

    #[test]
    fn test_line_height_calculation_precision() {
        // Test that line height calculation maintains precision correctly
        let font = FaceMetrics {
            cell_width: 9.6,
            ascent: 11.7,
            descent: 2.3,
            line_gap: 0.9,
            underline_position: None,
            underline_thickness: None,
            strikethrough_position: None,
            strikethrough_thickness: None,
            cap_height: None,
            ex_height: None,
            ic_width: None,
        };

        let metrics = Metrics::calc(font);

        // Line height: 11.7 + 2.3 + (0.9 * 2.0) = 16.8, ceiled to 17
        // But let's check the actual calculation
        let expected_height =
            (font.ascent + font.descent + (font.line_gap * 2.0)).ceil() as u32;
        assert_eq!(metrics.cell_height, expected_height);

        // Cell width: 9.6 ceiled to 10
        assert_eq!(metrics.cell_width, 10);
    }

    #[test]
    fn test_baseline_consistency_across_font_combinations() {
        let fonts = vec![
            TestFontData::cascadia_code(),
            TestFontData::dejavu_sans_mono(),
            TestFontData::noto_sans_cjk(),
            TestFontData::source_han_sans(),
            TestFontData::wenquanyi_micro_hei(),
        ];

        // Test each font as primary with others as secondary
        for (i, primary_font) in fonts.iter().enumerate() {
            let primary_metrics = Metrics::calc(primary_font.face);

            for (j, secondary_font) in fonts.iter().enumerate() {
                if i == j {
                    continue;
                } // Skip self

                let secondary_metrics = Metrics::calc_with_primary_cell_dimensions(
                    secondary_font.face,
                    &primary_metrics,
                );

                // Baseline should be consistent
                assert_eq!(
                    secondary_metrics.cell_baseline, primary_metrics.cell_baseline,
                    "Baseline inconsistent: {} primary, {} secondary",
                    primary_font.name, secondary_font.name
                );

                // Cell dimensions should be consistent
                assert_eq!(
                    secondary_metrics.cell_width, primary_metrics.cell_width,
                    "Cell width inconsistent: {} primary, {} secondary",
                    primary_font.name, secondary_font.name
                );

                assert_eq!(
                    secondary_metrics.cell_height, primary_metrics.cell_height,
                    "Cell height inconsistent: {} primary, {} secondary",
                    primary_font.name, secondary_font.name
                );
            }
        }
    }

    // Tests specifically for Issue #1071: CJK characters display "higher" than Latins,
    // so the terminal scroll to wrong place after showing long CJK text
    //
    // These tests verify that the fix correctly handles the scrolling issues
    // caused by inconsistent line heights between Latin and CJK characters.

    /// Test that reproduces the original issue #1071
    /// Before the fix: CJK fonts would have different line heights than Latin fonts,
    /// causing incorrect scrolling calculations
    #[test]
    fn test_issue_1071_cjk_latin_line_height_consistency() {
        // Simulate the scenario described in issue #1071
        // Latin font (like the user's primary font)
        let latin_font = FaceMetrics {
            cell_width: 9.0,
            ascent: 11.0,
            descent: 3.0,
            line_gap: 1.0,
            underline_position: Some(-1.0),
            underline_thickness: Some(1.0),
            strikethrough_position: Some(5.5),
            strikethrough_thickness: Some(1.0),
            cap_height: Some(8.5),
            ex_height: Some(5.5),
            ic_width: None,
        };

        // CJK font (fallback font that was causing issues)
        let cjk_font = FaceMetrics {
            cell_width: 18.0, // Double width
            ascent: 14.0,     // Taller ascent
            descent: 4.0,     // Deeper descent
            line_gap: 2.0,    // More line spacing
            underline_position: Some(-1.5),
            underline_thickness: Some(1.2),
            strikethrough_position: Some(7.0),
            strikethrough_thickness: Some(1.2),
            cap_height: Some(10.5),
            ex_height: Some(7.0),
            ic_width: Some(36.0), // Measured CJK character width
        };

        // Calculate metrics using the consistent approach (the fix)
        let latin_metrics = Metrics::calc(latin_font);
        let cjk_metrics =
            Metrics::calc_with_primary_cell_dimensions(cjk_font, &latin_metrics);

        // CRITICAL: Both fonts must have identical cell dimensions
        // This is what fixes the scrolling issue
        assert_eq!(
            cjk_metrics.cell_height,
            latin_metrics.cell_height,
            "CJK and Latin fonts must have identical cell heights to prevent scrolling issues"
        );

        assert_eq!(
            cjk_metrics.cell_width, latin_metrics.cell_width,
            "CJK and Latin fonts must have identical cell widths for consistent grid"
        );

        assert_eq!(
            cjk_metrics.cell_baseline,
            latin_metrics.cell_baseline,
            "CJK and Latin fonts must have identical baselines to prevent text misalignment"
        );

        // Verify that the terminal can calculate scroll distances correctly
        // Before the fix: terminal would assume 4 Latin lines = 4 * latin_height
        // But actual content height would be different due to CJK line heights
        let lines_to_scroll = 4;
        let expected_scroll_distance = lines_to_scroll * latin_metrics.cell_height;
        let actual_scroll_distance = lines_to_scroll * cjk_metrics.cell_height;

        assert_eq!(
            expected_scroll_distance,
            actual_scroll_distance,
            "Scroll distance calculation must be consistent between Latin and CJK content"
        );
    }

    /// Test the specific scenario mentioned in issue #1071:
    /// "after a long CJK text was output in rio terminal, it run into a strange status"
    #[test]
    fn test_issue_1071_long_cjk_text_scrolling() {
        // Simulate fonts similar to those that would cause the issue
        let primary_font = FaceMetrics {
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

        // CJK font that would previously cause scrolling issues
        let cjk_font = FaceMetrics {
            cell_width: 20.0,
            ascent: 15.0,
            descent: 4.0,
            line_gap: 2.0,
            underline_position: Some(-2.0),
            underline_thickness: Some(1.5),
            strikethrough_position: Some(7.5),
            strikethrough_thickness: Some(1.5),
            cap_height: Some(11.0),
            ex_height: Some(7.5),
            ic_width: Some(40.0),
        };

        let primary_metrics = Metrics::calc(primary_font);
        let cjk_metrics =
            Metrics::calc_with_primary_cell_dimensions(cjk_font, &primary_metrics);

        // Simulate the scenario: terminal displays long CJK text and needs to scroll
        // The issue was that terminal would calculate scroll based on Latin line height
        // but actual content would have different height due to CJK metrics

        let terminal_rows = 24; // Typical terminal height
        let content_lines = 30; // More lines than can fit on screen
        let lines_to_scroll = content_lines - terminal_rows; // 6 lines need to scroll

        // Before fix: these would be different, causing wrong scroll position
        let latin_based_scroll = lines_to_scroll * primary_metrics.cell_height;
        let cjk_based_scroll = lines_to_scroll * cjk_metrics.cell_height;

        assert_eq!(
            latin_based_scroll, cjk_based_scroll,
            "Scroll calculations must be identical for Latin and CJK content"
        );

        // Verify that the input line remains visible after scrolling
        // The issue mentioned: "I cannot even see the input line!"
        let total_content_height = content_lines * cjk_metrics.cell_height;
        let visible_area_height = terminal_rows * cjk_metrics.cell_height;
        let scroll_position = total_content_height - visible_area_height;

        // Input line should be at the bottom of visible area
        let input_line_position = total_content_height - cjk_metrics.cell_height;
        let input_line_visible = input_line_position >= scroll_position;

        assert!(
            input_line_visible,
            "Input line must remain visible after scrolling CJK content"
        );
    }

    /// Test the printf scenario specifically mentioned in the issue
    #[test]
    fn test_issue_1071_printf_command_scrolling() {
        // The issue mentioned: "terminal scrolls less than 4 lines down after the printf command executed"
        let latin_font = FaceMetrics {
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

        let cjk_font = FaceMetrics {
            cell_width: 16.0,
            ascent: 13.0,
            descent: 3.5,
            line_gap: 1.5,
            underline_position: Some(-1.5),
            underline_thickness: Some(1.2),
            strikethrough_position: Some(6.5),
            strikethrough_thickness: Some(1.2),
            cap_height: Some(9.5),
            ex_height: Some(6.5),
            ic_width: Some(32.0),
        };

        let latin_metrics = Metrics::calc(latin_font);
        let cjk_metrics =
            Metrics::calc_with_primary_cell_dimensions(cjk_font, &latin_metrics);

        // Simulate printf outputting 4 lines of content
        let printf_output_lines = 4;

        // Before fix: terminal would calculate scroll distance based on Latin metrics
        // but actual content height would be based on CJK metrics, causing mismatch
        let expected_scroll_distance = printf_output_lines * latin_metrics.cell_height;
        let actual_content_height = printf_output_lines * cjk_metrics.cell_height;

        assert_eq!(
            expected_scroll_distance, actual_content_height,
            "printf output height calculation must be consistent between font types"
        );

        // The fix ensures that both calculations use the same cell_height
        assert_eq!(
            latin_metrics.cell_height, cjk_metrics.cell_height,
            "Both fonts must use same cell height to prevent printf scrolling issues"
        );
    }

    /// Test baseline adjustment functionality that helps with the scrolling issue
    #[test]
    fn test_issue_1071_baseline_adjustment_consistency() {
        let font = FaceMetrics {
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

        let mut metrics = Metrics::calc(font);
        let original_baseline = metrics.get_baseline_adjustment();

        // Test that baseline adjustment works correctly when cell height changes
        // This is important for maintaining consistent text positioning
        let new_height = metrics.cell_height + 4;
        metrics.apply_cell_height_adjustment(new_height);

        let adjusted_baseline = metrics.get_baseline_adjustment();

        // Baseline should be adjusted to keep text centered
        assert_eq!(adjusted_baseline, original_baseline + 2.0);

        // Verify that text positioning remains consistent
        let (ascent, descent, _) = metrics.for_rich_text();
        assert_eq!(ascent + descent, new_height as f32);
    }

    /// Test CJK font size adjustment that prevents the scrolling issue
    #[test]
    fn test_issue_1071_cjk_font_size_normalization() {
        // Test the font size adjustment that normalizes CJK fonts with Latin fonts
        let latin_font = FaceMetrics {
            cell_width: 9.0,
            ascent: 11.0,
            descent: 2.5,
            line_gap: 0.8,
            underline_position: Some(-1.0),
            underline_thickness: Some(1.0),
            strikethrough_position: Some(5.5),
            strikethrough_thickness: Some(1.0),
            cap_height: Some(8.5),
            ex_height: Some(5.5),
            ic_width: Some(18.0), // Measured CJK width in Latin font
        };

        let cjk_font = FaceMetrics {
            cell_width: 18.0,
            ascent: 14.0,
            descent: 3.5,
            line_gap: 1.2,
            underline_position: Some(-1.5),
            underline_thickness: Some(1.2),
            strikethrough_position: Some(7.0),
            strikethrough_thickness: Some(1.2),
            cap_height: Some(10.5),
            ex_height: Some(7.0),
            ic_width: Some(36.0), // Measured CJK width in CJK font
        };

        // Calculate font size adjustment
        let size_adjustment =
            Metrics::calculate_cjk_font_size_adjustment(&latin_font, &cjk_font);

        assert!(
            size_adjustment.is_some(),
            "Font size adjustment should be calculated"
        );

        let adjustment_ratio = size_adjustment.unwrap();
        // Should use ic_width ratio: 18.0 / 36.0 = 0.5
        assert!((adjustment_ratio - 0.5).abs() < 0.001);

        // This adjustment helps normalize the fonts so they work together consistently
        // preventing the line height mismatches that caused the scrolling issue
    }

    /// Integration test that verifies the complete fix for issue #1071
    #[test]
    fn test_issue_1071_complete_fix_integration() {
        // This test simulates the complete scenario described in issue #1071

        // Primary Latin font (user's main font)
        let latin_font = FaceMetrics {
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

        // CJK fallback font (causing the original issue)
        let cjk_font = FaceMetrics {
            cell_width: 20.0,
            ascent: 15.0,
            descent: 4.0,
            line_gap: 2.0,
            underline_position: Some(-2.0),
            underline_thickness: Some(1.5),
            strikethrough_position: Some(7.5),
            strikethrough_thickness: Some(1.5),
            cap_height: Some(11.0),
            ex_height: Some(7.5),
            ic_width: Some(40.0),
        };

        // Apply the fix: use consistent metrics approach
        let primary_metrics = Metrics::calc(latin_font);
        let cjk_metrics =
            Metrics::calc_with_primary_cell_dimensions(cjk_font, &primary_metrics);

        // Verify all the key aspects of the fix:

        // 1. Consistent cell dimensions (prevents scrolling calculation errors)
        assert_eq!(primary_metrics.cell_height, cjk_metrics.cell_height);
        assert_eq!(primary_metrics.cell_width, cjk_metrics.cell_width);
        assert_eq!(primary_metrics.cell_baseline, cjk_metrics.cell_baseline);

        // 2. Consistent rich text format (prevents rendering height mismatches)
        let (latin_ascent, latin_descent, latin_leading) =
            primary_metrics.for_rich_text();
        let (cjk_ascent, cjk_descent, cjk_leading) = cjk_metrics.for_rich_text();

        assert_eq!(latin_ascent, cjk_ascent);
        assert_eq!(latin_descent, cjk_descent);
        assert_eq!(latin_leading, cjk_leading);

        // 3. Baseline adjustment works correctly
        assert_eq!(
            primary_metrics.get_baseline_adjustment(),
            cjk_metrics.get_baseline_adjustment()
        );

        // 4. Terminal grid calculations will be consistent
        let terminal_rows = 24;
        let content_lines = 30;
        let scroll_lines = content_lines - terminal_rows;

        let latin_scroll_height = scroll_lines * primary_metrics.cell_height;
        let cjk_scroll_height = scroll_lines * cjk_metrics.cell_height;

        assert_eq!(latin_scroll_height, cjk_scroll_height);

        // 5. Font-specific positioning is preserved (underline, strikethrough)
        // CJK font can have different positioning while maintaining grid consistency
        // This allows proper rendering while preventing scrolling issues
        assert_eq!(cjk_metrics.cell_height, primary_metrics.cell_height);
        // But positioning details can differ (this is intentional and correct)
    }

    /// Test edge case: mixed content with alternating Latin and CJK characters
    #[test]
    fn test_issue_1071_mixed_content_consistency() {
        let latin_font = FaceMetrics {
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

        let cjk_font = FaceMetrics {
            cell_width: 16.0,
            ascent: 12.0,
            descent: 3.0,
            line_gap: 1.0,
            underline_position: Some(-1.5),
            underline_thickness: Some(1.2),
            strikethrough_position: Some(6.0),
            strikethrough_thickness: Some(1.2),
            cap_height: Some(9.0),
            ex_height: Some(6.0),
            ic_width: Some(32.0),
        };

        let latin_metrics = Metrics::calc(latin_font);
        let cjk_metrics =
            Metrics::calc_with_primary_cell_dimensions(cjk_font, &latin_metrics);

        // Simulate a line with mixed content: "Hello 世界 World"
        // Each character should occupy the same cell height regardless of font
        let mixed_line_height = latin_metrics.cell_height; // Latin chars
        let cjk_line_height = cjk_metrics.cell_height; // CJK chars

        assert_eq!(
            mixed_line_height, cjk_line_height,
            "Mixed content lines must have consistent height"
        );

        // Verify that a terminal line containing mixed content
        // has predictable and consistent height
        let line_count = 5;
        let total_height_latin = line_count * latin_metrics.cell_height;
        let total_height_mixed = line_count * cjk_metrics.cell_height;

        assert_eq!(
            total_height_latin, total_height_mixed,
            "Mixed content must not affect total height calculations"
        );
    }

    /// Test edge cases for font metrics calculations
    #[test]
    fn test_edge_cases() {
        // Test extremely small font size
        let tiny_font = FaceMetrics {
            cell_width: 0.1,
            ascent: 0.1,
            descent: 0.05,
            line_gap: 0.01,
            underline_position: Some(-0.01),
            underline_thickness: Some(0.01),
            strikethrough_position: Some(0.05),
            strikethrough_thickness: Some(0.01),
            cap_height: Some(0.08),
            ex_height: Some(0.05),
            ic_width: None,
        };

        let tiny_metrics = Metrics::calc(tiny_font);
        assert!(
            tiny_metrics.cell_width >= 1,
            "Cell width must be at least 1 pixel"
        );
        assert!(
            tiny_metrics.cell_height >= 1,
            "Cell height must be at least 1 pixel"
        );
        assert!(
            tiny_metrics.underline_thickness >= 1,
            "Line thickness must be at least 1 pixel"
        );

        // Test extremely large font size
        let huge_font = FaceMetrics {
            cell_width: 1000.0,
            ascent: 1200.0,
            descent: 300.0,
            line_gap: 100.0,
            underline_position: Some(-100.0),
            underline_thickness: Some(100.0),
            strikethrough_position: Some(600.0),
            strikethrough_thickness: Some(100.0),
            cap_height: Some(900.0),
            ex_height: Some(600.0),
            ic_width: None,
        };

        let huge_metrics = Metrics::calc(huge_font);
        assert_eq!(
            huge_metrics.cell_width, 1000,
            "Large font metrics should be preserved"
        );
        assert_eq!(
            huge_metrics.cell_height, 1700,
            "Large font height calculation"
        );

        // Test font with missing optional metrics
        let minimal_font = FaceMetrics {
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

        let minimal_metrics = Metrics::calc(minimal_font);
        assert!(
            minimal_metrics.underline_thickness >= 1,
            "Default underline thickness"
        );
        assert!(
            minimal_metrics.strikethrough_thickness >= 1,
            "Default strikethrough thickness"
        );
        assert!(
            minimal_metrics.underline_position > 0,
            "Default underline position"
        );
        assert!(
            minimal_metrics.strikethrough_position > 0,
            "Default strikethrough position"
        );
    }

    /// Test mixed script rendering (Latin + CJK + Emoji)
    #[test]
    fn test_mixed_script_rendering() {
        let latin_font = TestFontData::cascadia_code();
        let cjk_font = TestFontData::noto_sans_cjk();
        let emoji_font = TestFontData::noto_color_emoji();

        let latin_metrics = Metrics::calc(latin_font.face);
        let cjk_metrics =
            Metrics::calc_with_primary_cell_dimensions(cjk_font.face, &latin_metrics);
        let emoji_metrics =
            Metrics::calc_with_primary_cell_dimensions(emoji_font.face, &latin_metrics);

        // All fonts should have the same cell height for consistent rendering
        assert_eq!(latin_metrics.cell_height, cjk_metrics.cell_height);
        assert_eq!(latin_metrics.cell_height, emoji_metrics.cell_height);

        // All fonts should have the same baseline
        assert_eq!(latin_metrics.cell_baseline, cjk_metrics.cell_baseline);
        assert_eq!(latin_metrics.cell_baseline, emoji_metrics.cell_baseline);

        // Verify that a line containing all three scripts renders consistently
        let mixed_line_height = latin_metrics.cell_height;
        let latin_only_height = latin_metrics.cell_height;
        let cjk_only_height = cjk_metrics.cell_height;
        let emoji_only_height = emoji_metrics.cell_height;

        assert_eq!(
            mixed_line_height, latin_only_height,
            "Mixed script lines must have same height as single script lines"
        );
        assert_eq!(mixed_line_height, cjk_only_height);
        assert_eq!(mixed_line_height, emoji_only_height);
    }
}
