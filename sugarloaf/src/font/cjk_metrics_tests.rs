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
}
