// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use crate::Quad;
use serde::Deserialize;

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum SugarCursor {
    Block([f32; 4]),
    HollowBlock([f32; 4]),
    Caret([f32; 4]),
    Underline([f32; 4]),
}

#[derive(Default, Clone, Deserialize, Debug, PartialEq)]
pub struct ImageProperties {
    #[serde(default = "String::default")]
    pub path: String,
    #[serde(default = "Option::default")]
    pub width: Option<f32>,
    #[serde(default = "Option::default")]
    pub height: Option<f32>,
    #[serde(default = "f32::default")]
    pub x: f32,
    #[serde(default = "f32::default")]
    pub y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RichTextLinesRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RichText {
    pub id: usize,
    pub position: [f32; 2],
    pub lines: Option<RichTextLinesRange>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Object {
    Quad(Quad),
    RichText(RichText),
}

pub enum CornerType {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

pub fn drawable_character(character: char) -> Option<DrawableChar> {
    match character {
        '\u{2500}'..='\u{259f}'
        | '\u{1fb00}'..='\u{1fb3b}'
        // Powerlines
        | '\u{e0b0}'..='\u{e0bf}'
        // Brailles
        | '\u{2800}'..='\u{28FF}'
        // Sextants
        | '\u{1FB00}'..='\u{1FB3F}'
        // Octants
        | '\u{1CD00}'..='\u{1CDE5}' => {
            if let Ok(character) = DrawableChar::try_from(character) {
                return Some(character)
            }
        }
        _ => {},
    }

    None
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DrawableChar {
    // Original box-drawing characters
    Horizontal,       // ─
    DoubleHorizontal, // ═
    Vertical,         // │
    DoubleVertical,   // ║
    HeavyHorizontal,  // ━
    HeavyVertical,    // ┃
    TopRight,         // └
    TopLeft,          // ┘
    BottomRight,      // ┌
    BottomLeft,       // ┐
    Cross,            // ┼
    VerticalRight,    // ├
    VerticalLeft,     // ┤
    HorizontalDown,   // ┬
    HorizontalUp,     // ┴
    ArcTopLeft,       // ╯
    ArcBottomRight,   // ╭
    ArcBottomLeft,    // ╮
    ArcTopRight,      // ╰

    DownDoubleAndHorizontalSingle,     // ╥
    DownSingleAndHorizontalDouble,     // ╤
    DoubleUpAndRight,                  // ╚
    DoubleUpAndLeft,                   // ╝
    UpSingleAndRightDouble,            // ╘
    UpSingleAndLeftDouble,             // ╛
    VerticalSingleAndHorizontalDouble, // ╪

    // Misc
    LowerOneEighthBlock,     // ▁
    LowerOneQuarterBlock,    // ▂
    LowerThreeEighthsBlock,  // ▃
    LeftOneQuarterBlock,     // ▎
    LeftThreeEighthsBlock,   // ▍
    LeftThreeQuartersBlock,  // ▊
    RightOneQuarterBlock,    //▕
    RightThreeEighthsBlock,  // 🮈
    RightThreeQuartersBlock, // 🮊
    UpperOneEighthBlock,     // ▔
    UpperThreeEighthsBlock,  // 🮃
    UpperThreeQuartersBlock, // 🮅

    // Horizontal dashes
    HorizontalLightDash,       // ┄
    HorizontalHeavyDash,       // ┅
    HorizontalLightDoubleDash, // ┈
    HorizontalHeavyDoubleDash, // ┉
    HorizontalLightTripleDash, // ╌
    HorizontalHeavyTripleDash, // ╍
    // Vertical dashes
    VerticalLightDash,       // ┆
    VerticalHeavyDash,       // ┇
    VerticalLightDoubleDash, // ┊
    VerticalHeavyDoubleDash, // ┋
    VerticalLightTripleDash, // ╎
    VerticalHeavyTripleDash, // ╏
    // Shade blocks
    LightShade,  // ░
    MediumShade, // ▒
    DarkShade,   // ▓
    FullBlock,   // █

    // Additional double line box drawing
    DoubleCross,                       // ╬
    DoubleVerticalRight,               // ╠
    DoubleVerticalLeft,                // ╣
    DoubleHorizontalDown,              // ╦
    DoubleHorizontalUp,                // ╩
    VerticalDoubleAndHorizontalSingle, // ╫

    // Additional double/single hybrid box drawing
    DownDoubleAndRightSingle,     // ╓
    DownDoubleAndLeftSingle,      // ╖
    VerticalDoubleAndRightSingle, // ╟
    VerticalDoubleAndLeftSingle,  // ╢
    VerticalSingleAndRightDouble, // ╞
    VerticalSingleAndLeftDouble,  // ╡
    DownSingleAndRightDouble,     // ╒
    DownSingleAndLeftDouble,      // ╕

    // Heavy box drawing
    HeavyDownAndRight,      // ┏
    HeavyDownAndLeft,       // ┓
    HeavyUpAndRight,        // ┗
    HeavyUpAndLeft,         // ┛
    HeavyVerticalAndRight,  // ┣
    HeavyVerticalAndLeft,   // ┫
    HeavyHorizontalAndDown, // ┳
    HeavyHorizontalAndUp,   // ┻
    HeavyCross,             // ╋

    // Mixed weight box drawing
    LightDownAndHeavyRight, // ┍
    LightDownAndHeavyLeft,  // ┑
    HeavyDownAndLightRight, // ┎
    HeavyDownAndLightLeft,  // ┒
    LightUpAndHeavyRight,   // ┕
    LightUpAndHeavyLeft,    // ┙
    HeavyUpAndLightRight,   // ┖
    HeavyUpAndLightLeft,    // ┚

    LowerHalf,                       // ▄
    LeftHalf,                        // ▌
    RightHalf,                       // ▐
    UpperHalf,                       // ▀
    UpperOneQuarterBlock,            // ▀
    LowerFiveEighthsBlock,           // ▅
    LowerThreeQuartersBlock,         // ▆
    LowerSevenEighthsBlock,          // ▇
    QuadrantUpperLeftAndLowerLeft,   // ▚
    QuadrantUpperLeftAndLowerRight,  // ▞
    QuadrantUpperLeftAndUpperRight,  // ▀
    QuadrantUpperRightAndLowerLeft,  // ▟
    QuadrantUpperRightAndLowerRight, // ▙
    QuadrantUpperLeft,               // ▘
    QuadrantUpperRight,              // ▝
    QuadrantLowerLeft,               // ▖
    QuadrantLowerRight,              // ▗

    // Separated Quadrants
    SeparatedQuadrantUpperLeft,  // 🬓
    SeparatedQuadrantUpperRight, // 🬔
    SeparatedQuadrantLowerLeft,  // 🬕
    SeparatedQuadrantLowerRight, // 🬖

    // Additional diagonal and rounded elements
    DiagonalRisingBar,  // ╱
    DiagonalFallingBar, // ╲
    DiagonalCross,      // ╳

    Sextant(u8), // Represents any of the 64 possible sextant patterns
    Octant(u8),  // Represents any of the 256 possible octant patterns

    // LeftHalfBlackCircle, // ◖
    // RightHalfBlackCircle, // ◗

    // Powerline triangles
    PowerlineLeftSolid,
    PowerlineRightSolid,
    PowerlineLeftHollow,
    PowerlineRightHollow,
    PowerlineCurvedRightSolid,
    PowerlineCurvedRightHollow,
    PowerlineCurvedLeftSolid,
    PowerlineCurvedLeftHollow,
    PowerlineLowerLeftTriangle,
    PowerlineBackslashSeparator,
    PowerlineLowerRightTriangle,
    PowerlineForwardslashSeparator,
    PowerlineUpperLeftTriangle,
    PowerlineForwardslashSeparatorRedundant,
    PowerlineUpperRightTriangle,
    PowerlineBackslashSeparatorRedundant,

    // Complete set of Braille characters (U+2800 to U+28FF)
    // First row (no dot 7, no dot 8)
    BrailleBlank, // ⠀ U+2800 BRAILLE PATTERN BLANK
    Braille(Braille),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Braille {
    Dots1,      // ⠁ U+2801 BRAILLE PATTERN DOTS-1
    Dots2,      // ⠂ U+2802 BRAILLE PATTERN DOTS-2
    Dots12,     // ⠃ U+2803 BRAILLE PATTERN DOTS-12
    Dots3,      // ⠄ U+2804 BRAILLE PATTERN DOTS-3
    Dots13,     // ⠅ U+2805 BRAILLE PATTERN DOTS-13
    Dots23,     // ⠆ U+2806 BRAILLE PATTERN DOTS-23
    Dots123,    // ⠇ U+2807 BRAILLE PATTERN DOTS-123
    Dots4,      // ⠈ U+2808 BRAILLE PATTERN DOTS-4
    Dots14,     // ⠉ U+2809 BRAILLE PATTERN DOTS-14
    Dots24,     // ⠊ U+280A BRAILLE PATTERN DOTS-24
    Dots124,    // ⠋ U+280B BRAILLE PATTERN DOTS-124
    Dots34,     // ⠌ U+280C BRAILLE PATTERN DOTS-34
    Dots134,    // ⠍ U+280D BRAILLE PATTERN DOTS-134
    Dots234,    // ⠎ U+280E BRAILLE PATTERN DOTS-234
    Dots1234,   // ⠏ U+280F BRAILLE PATTERN DOTS-1234
    Dots5,      // ⠐ U+2810 BRAILLE PATTERN DOTS-5
    Dots15,     // ⠑ U+2811 BRAILLE PATTERN DOTS-15
    Dots25,     // ⠒ U+2812 BRAILLE PATTERN DOTS-25
    Dots125,    // ⠓ U+2813 BRAILLE PATTERN DOTS-125
    Dots35,     // ⠔ U+2814 BRAILLE PATTERN DOTS-35
    Dots135,    // ⠕ U+2815 BRAILLE PATTERN DOTS-135
    Dots235,    // ⠖ U+2816 BRAILLE PATTERN DOTS-235
    Dots1235,   // ⠗ U+2817 BRAILLE PATTERN DOTS-1235
    Dots45,     // ⠘ U+2818 BRAILLE PATTERN DOTS-45
    Dots145,    // ⠙ U+2819 BRAILLE PATTERN DOTS-145
    Dots245,    // ⠚ U+281A BRAILLE PATTERN DOTS-245
    Dots1245,   // ⠛ U+281B BRAILLE PATTERN DOTS-1245
    Dots345,    // ⠜ U+281C BRAILLE PATTERN DOTS-345
    Dots1345,   // ⠝ U+281D BRAILLE PATTERN DOTS-1345
    Dots2345,   // ⠞ U+281E BRAILLE PATTERN DOTS-2345
    Dots12345,  // ⠟ U+281F BRAILLE PATTERN DOTS-12345
    Dots6,      // ⠠ U+2820 BRAILLE PATTERN DOTS-6
    Dots16,     // ⠡ U+2821 BRAILLE PATTERN DOTS-16
    Dots26,     // ⠢ U+2822 BRAILLE PATTERN DOTS-26
    Dots126,    // ⠣ U+2823 BRAILLE PATTERN DOTS-126
    Dots36,     // ⠤ U+2824 BRAILLE PATTERN DOTS-36
    Dots136,    // ⠥ U+2825 BRAILLE PATTERN DOTS-136
    Dots236,    // ⠦ U+2826 BRAILLE PATTERN DOTS-236
    Dots1236,   // ⠧ U+2827 BRAILLE PATTERN DOTS-1236
    Dots46,     // ⠨ U+2828 BRAILLE PATTERN DOTS-46
    Dots146,    // ⠩ U+2829 BRAILLE PATTERN DOTS-146
    Dots246,    // ⠪ U+282A BRAILLE PATTERN DOTS-246
    Dots1246,   // ⠫ U+282B BRAILLE PATTERN DOTS-1246
    Dots346,    // ⠬ U+282C BRAILLE PATTERN DOTS-346
    Dots1346,   // ⠭ U+282D BRAILLE PATTERN DOTS-1346
    Dots2346,   // ⠮ U+282E BRAILLE PATTERN DOTS-2346
    Dots12346,  // ⠯ U+282F BRAILLE PATTERN DOTS-12346
    Dots56,     // ⠰ U+2830 BRAILLE PATTERN DOTS-56
    Dots156,    // ⠱ U+2831 BRAILLE PATTERN DOTS-156
    Dots256,    // ⠲ U+2832 BRAILLE PATTERN DOTS-256
    Dots1256,   // ⠳ U+2833 BRAILLE PATTERN DOTS-1256
    Dots356,    // ⠴ U+2834 BRAILLE PATTERN DOTS-356
    Dots1356,   // ⠵ U+2835 BRAILLE PATTERN DOTS-1356
    Dots2356,   // ⠶ U+2836 BRAILLE PATTERN DOTS-2356
    Dots12356,  // ⠷ U+2837 BRAILLE PATTERN DOTS-12356
    Dots456,    // ⠸ U+2838 BRAILLE PATTERN DOTS-456
    Dots1456,   // ⠹ U+2839 BRAILLE PATTERN DOTS-1456
    Dots2456,   // ⠺ U+283A BRAILLE PATTERN DOTS-2456
    Dots12456,  // ⠻ U+283B BRAILLE PATTERN DOTS-12456
    Dots3456,   // ⠼ U+283C BRAILLE PATTERN DOTS-3456
    Dots13456,  // ⠽ U+283D BRAILLE PATTERN DOTS-13456
    Dots23456,  // ⠾ U+283E BRAILLE PATTERN DOTS-23456
    Dots123456, // ⠿ U+283F BRAILLE PATTERN DOTS-123456

    // Second row (with dot 7, no dot 8)
    Dots7,       // ⡀ U+2840 BRAILLE PATTERN DOTS-7
    Dots17,      // ⡁ U+2841 BRAILLE PATTERN DOTS-17
    Dots27,      // ⡂ U+2842 BRAILLE PATTERN DOTS-27
    Dots127,     // ⡃ U+2843 BRAILLE PATTERN DOTS-127
    Dots37,      // ⡄ U+2844 BRAILLE PATTERN DOTS-37
    Dots137,     // ⡅ U+2845 BRAILLE PATTERN DOTS-137
    Dots237,     // ⡆ U+2846 BRAILLE PATTERN DOTS-237
    Dots1237,    // ⡇ U+2847 BRAILLE PATTERN DOTS-1237
    Dots47,      // ⡈ U+2848 BRAILLE PATTERN DOTS-47
    Dots147,     // ⡉ U+2849 BRAILLE PATTERN DOTS-147
    Dots247,     // ⡊ U+284A BRAILLE PATTERN DOTS-247
    Dots1247,    // ⡋ U+284B BRAILLE PATTERN DOTS-1247
    Dots347,     // ⡌ U+284C BRAILLE PATTERN DOTS-347
    Dots1347,    // ⡍ U+284D BRAILLE PATTERN DOTS-1347
    Dots2347,    // ⡎ U+284E BRAILLE PATTERN DOTS-2347
    Dots12347,   // ⡏ U+284F BRAILLE PATTERN DOTS-12347
    Dots57,      // ⡐ U+2850 BRAILLE PATTERN DOTS-57
    Dots157,     // ⡑ U+2851 BRAILLE PATTERN DOTS-157
    Dots257,     // ⡒ U+2852 BRAILLE PATTERN DOTS-257
    Dots1257,    // ⡓ U+2853 BRAILLE PATTERN DOTS-1257
    Dots357,     // ⡔ U+2854 BRAILLE PATTERN DOTS-357
    Dots1357,    // ⡕ U+2855 BRAILLE PATTERN DOTS-1357
    Dots2357,    // ⡖ U+2856 BRAILLE PATTERN DOTS-2357
    Dots12357,   // ⡗ U+2857 BRAILLE PATTERN DOTS-12357
    Dots457,     // ⡘ U+2858 BRAILLE PATTERN DOTS-457
    Dots1457,    // ⡙ U+2859 BRAILLE PATTERN DOTS-1457
    Dots2457,    // ⡚ U+285A BRAILLE PATTERN DOTS-2457
    Dots12457,   // ⡛ U+285B BRAILLE PATTERN DOTS-12457
    Dots3457,    // ⡜ U+285C BRAILLE PATTERN DOTS-3457
    Dots13457,   // ⡝ U+285D BRAILLE PATTERN DOTS-13457
    Dots23457,   // ⡞ U+285E BRAILLE PATTERN DOTS-23457
    Dots123457,  // ⡟ U+285F BRAILLE PATTERN DOTS-123457
    Dots67,      // ⡠ U+2860 BRAILLE PATTERN DOTS-67
    Dots167,     // ⡡ U+2861 BRAILLE PATTERN DOTS-167
    Dots267,     // ⡢ U+2862 BRAILLE PATTERN DOTS-267
    Dots1267,    // ⡣ U+2863 BRAILLE PATTERN DOTS-1267
    Dots367,     // ⡤ U+2864 BRAILLE PATTERN DOTS-367
    Dots1367,    // ⡥ U+2865 BRAILLE PATTERN DOTS-1367
    Dots2367,    // ⡦ U+2866 BRAILLE PATTERN DOTS-2367
    Dots12367,   // ⡧ U+2867 BRAILLE PATTERN DOTS-12367
    Dots467,     // ⡨ U+2868 BRAILLE PATTERN DOTS-467
    Dots1467,    // ⡩ U+2869 BRAILLE PATTERN DOTS-1467
    Dots2467,    // ⡪ U+286A BRAILLE PATTERN DOTS-2467
    Dots12467,   // ⡫ U+286B BRAILLE PATTERN DOTS-12467
    Dots3467,    // ⡬ U+286C BRAILLE PATTERN DOTS-3467
    Dots13467,   // ⡭ U+286D BRAILLE PATTERN DOTS-13467
    Dots23467,   // ⡮ U+286E BRAILLE PATTERN DOTS-23467
    Dots123467,  // ⡯ U+286F BRAILLE PATTERN DOTS-123467
    Dots567,     // ⡰ U+2870 BRAILLE PATTERN DOTS-567
    Dots1567,    // ⡱ U+2871 BRAILLE PATTERN DOTS-1567
    Dots2567,    // ⡲ U+2872 BRAILLE PATTERN DOTS-2567
    Dots12567,   // ⡳ U+2873 BRAILLE PATTERN DOTS-12567
    Dots3567,    // ⡴ U+2874 BRAILLE PATTERN DOTS-3567
    Dots13567,   // ⡵ U+2875 BRAILLE PATTERN DOTS-13567
    Dots23567,   // ⡶ U+2876 BRAILLE PATTERN DOTS-23567
    Dots123567,  // ⡷ U+2877 BRAILLE PATTERN DOTS-123567
    Dots4567,    // ⡸ U+2878 BRAILLE PATTERN DOTS-4567
    Dots14567,   // ⡹ U+2879 BRAILLE PATTERN DOTS-14567
    Dots24567,   // ⡺ U+287A BRAILLE PATTERN DOTS-24567
    Dots124567,  // ⡻ U+287B BRAILLE PATTERN DOTS-124567
    Dots34567,   // ⡼ U+287C BRAILLE PATTERN DOTS-34567
    Dots134567,  // ⡽ U+287D BRAILLE PATTERN DOTS-134567
    Dots234567,  // ⡾ U+287E BRAILLE PATTERN DOTS-234567
    Dots1234567, // ⡿ U+287F BRAILLE PATTERN DOTS-1234567
    Dots235678,  // ⣶ U+28F6 BRAILLE PATTERN DOTS-235678

    // Third row (no dot 7, with dot 8)
    Dots8,       // ⢀ U+2880 BRAILLE PATTERN DOTS-8
    Dots18,      // ⢁ U+2881 BRAILLE PATTERN DOTS-18
    Dots28,      // ⢂ U+2882 BRAILLE PATTERN DOTS-28
    Dots128,     // ⢃ U+2883 BRAILLE PATTERN DOTS-128
    Dots38,      // ⢄ U+2884 BRAILLE PATTERN DOTS-38
    Dots138,     // ⢅ U+2885 BRAILLE PATTERN DOTS-138
    Dots238,     // ⢆ U+2886 BRAILLE PATTERN DOTS-238
    Dots1238,    // ⢇ U+2887 BRAILLE PATTERN DOTS-1238
    Dots48,      // ⢈ U+2888 BRAILLE PATTERN DOTS-48
    Dots148,     // ⢉ U+2889 BRAILLE PATTERN DOTS-148
    Dots248,     // ⢊ U+288A BRAILLE PATTERN DOTS-248
    Dots1248,    // ⢋ U+288B BRAILLE PATTERN DOTS-1248
    Dots348,     // ⢌ U+288C BRAILLE PATTERN DOTS-348
    Dots1348,    // ⢍ U+288D BRAILLE PATTERN DOTS-1348
    Dots2348,    // ⢎ U+288E BRAILLE PATTERN DOTS-2348
    Dots12348,   // ⢏ U+288F BRAILLE PATTERN DOTS-12348
    Dots58,      // ⢐ U+2890 BRAILLE PATTERN DOTS-58
    Dots158,     // ⢑ U+2891 BRAILLE PATTERN DOTS-158
    Dots258,     // ⢒ U+2892 BRAILLE PATTERN DOTS-258
    Dots1258,    // ⢓ U+2893 BRAILLE PATTERN DOTS-1258
    Dots358,     // ⢔ U+2894 BRAILLE PATTERN DOTS-358
    Dots1358,    // ⢕ U+2895 BRAILLE PATTERN DOTS-1358
    Dots2358,    // ⢖ U+2896 BRAILLE PATTERN DOTS-2358
    Dots12358,   // ⢗ U+2897 BRAILLE PATTERN DOTS-12358
    Dots458,     // ⢘ U+2898 BRAILLE PATTERN DOTS-458
    Dots1458,    // ⢙ U+2899 BRAILLE PATTERN DOTS-1458
    Dots2458,    // ⢚ U+289A BRAILLE PATTERN DOTS-2458
    Dots12458,   // ⢛ U+289B BRAILLE PATTERN DOTS-12458
    Dots3458,    // ⢜ U+289C BRAILLE PATTERN DOTS-3458
    Dots13458,   // ⢝ U+289D BRAILLE PATTERN DOTS-13458
    Dots23458,   // ⢞ U+289E BRAILLE PATTERN DOTS-23458
    Dots123458,  // ⢟ U+289F BRAILLE PATTERN DOTS-123458
    Dots68,      // ⢠ U+28A0 BRAILLE PATTERN DOTS-68
    Dots168,     // ⢡ U+28A1 BRAILLE PATTERN DOTS-168
    Dots268,     // ⢢ U+28A2 BRAILLE PATTERN DOTS-268
    Dots1268,    // ⢣ U+28A3 BRAILLE PATTERN DOTS-1268
    Dots368,     // ⢤ U+28A4 BRAILLE PATTERN DOTS-368
    Dots1368,    // ⢥ U+28A5 BRAILLE PATTERN DOTS-1368
    Dots2368,    // ⢦ U+28A6 BRAILLE PATTERN DOTS-2368
    Dots12368,   // ⢧ U+28A7 BRAILLE PATTERN DOTS-12368
    Dots468,     // ⢨ U+28A8 BRAILLE PATTERN DOTS-468
    Dots1468,    // ⢩ U+28A9 BRAILLE PATTERN DOTS-1468
    Dots2468,    // ⢪ U+28AA BRAILLE PATTERN DOTS-2468
    Dots12468,   // ⢫ U+28AB BRAILLE PATTERN DOTS-12468
    Dots3468,    // ⢬ U+28AC BRAILLE PATTERN DOTS-3468
    Dots13468,   // ⢭ U+28AD BRAILLE PATTERN DOTS-13468
    Dots23468,   // ⢮ U+28AE BRAILLE PATTERN DOTS-23468
    Dots123468,  // ⢯ U+28AF BRAILLE PATTERN DOTS-123468
    Dots568,     // ⢰ U+28B0 BRAILLE PATTERN DOTS-568
    Dots1568,    // ⢱ U+28B1 BRAILLE PATTERN DOTS-1568
    Dots2568,    // ⢲ U+28B2 BRAILLE PATTERN DOTS-2568
    Dots12568,   // ⢳ U+28B3 BRAILLE PATTERN DOTS-12568
    Dots3568,    // ⢴ U+28B4 BRAILLE PATTERN DOTS-3568
    Dots13568,   // ⢵ U+28B5 BRAILLE PATTERN DOTS-13568
    Dots23568,   // ⢶ U+28B6 BRAILLE PATTERN DOTS-23568
    Dots123568,  // ⢷ U+28B7 BRAILLE PATTERN DOTS-123568
    Dots4568,    // ⢸ U+28B8 BRAILLE PATTERN DOTS-4568
    Dots14568,   // ⢹ U+28B9 BRAILLE PATTERN DOTS-14568
    Dots24568,   // ⢺ U+28BA BRAILLE PATTERN DOTS-24568
    Dots124568,  // ⢻ U+28BB BRAILLE PATTERN DOTS-124568
    Dots34568,   // ⢼ U+28BC BRAILLE PATTERN DOTS-34568
    Dots134568,  // ⢽ U+28BD BRAILLE PATTERN DOTS-134568
    Dots234568,  // ⢾ U+28BE BRAILLE PATTERN DOTS-234568
    Dots1234568, // ⢿ U+28BF BRAILLE PATTERN DOTS-1234568

    // Fourth row (with dot 7, with dot 8)
    Dots78,      // ⣀ U+28C0 BRAILLE PATTERN DOTS-78
    Dots178,     // ⣁ U+28C1 BRAILLE PATTERN DOTS-178
    Dots278,     // ⣂ U+28C2 BRAILLE PATTERN DOTS-278
    Dots1278,    // ⣃ U+28C3 BRAILLE PATTERN DOTS-1278
    Dots378,     // ⣄ U+28C4 BRAILLE PATTERN DOTS-378
    Dots1378,    // ⣅ U+28C5 BRAILLE PATTERN DOTS-1378
    Dots2378,    // ⣆ U+28C6 BRAILLE PATTERN DOTS-2378
    Dots12378,   // ⣇ U+28C7 BRAILLE PATTERN DOTS-12378
    Dots478,     // ⣈ U+28C8 BRAILLE PATTERN DOTS-478
    Dots1478,    // ⣉ U+28C9 BRAILLE PATTERN DOTS-1478
    Dots2478,    // ⣊ U+28CA BRAILLE PATTERN DOTS-2478
    Dots12478,   // ⣋ U+28CB BRAILLE PATTERN DOTS-12478
    Dots3478,    // ⣌ U+28CC BRAILLE PATTERN DOTS-3478
    Dots13478,   // ⣍ U+28CD BRAILLE PATTERN DOTS-13478
    Dots23478,   // ⣎ U+28CE BRAILLE PATTERN DOTS-23478
    Dots123478,  // ⣏ U+28CF BRAILLE PATTERN DOTS-123478
    Dots578,     // ⣐ U+28D0 BRAILLE PATTERN DOTS-578
    Dots1578,    // ⣑ U+28D1 BRAILLE PATTERN DOTS-1578
    Dots2578,    // ⣒ U+28D2 BRAILLE PATTERN DOTS-2578
    Dots12578,   // ⣓ U+28D3 BRAILLE PATTERN DOTS-12578
    Dots3578,    // ⣔ U+28D4 BRAILLE PATTERN DOTS-3578
    Dots13578,   // ⣕ U+28D5 BRAILLE PATTERN DOTS-13578
    Dots23578,   // ⣖ U+28D6 BRAILLE PATTERN DOTS-23578
    Dots123578,  // ⣗ U+28D7 BRAILLE PATTERN DOTS-123578
    Dots4578,    // ⣘ U+28D8 BRAILLE PATTERN DOTS-4578
    Dots14578,   // ⣙ U+28D9 BRAILLE PATTERN DOTS-14578
    Dots24578,   // ⣚ U+28DA BRAILLE PATTERN DOTS-24578
    Dots124578,  // ⣛ U+28DB BRAILLE PATTERN DOTS-124578
    Dots34578,   // ⣜ U+28DC BRAILLE PATTERN DOTS-34578
    Dots134578,  // ⣝ U+28DD BRAILLE PATTERN DOTS-134578
    Dots234578,  // ⣞ U+28DE BRAILLE PATTERN DOTS-234578
    Dots1234578, // ⣟ U+28DF BRAILLE PATTERN DOTS-1234578
    Dots678,     // ⣠ U+28E0 BRAILLE PATTERN DOTS-678
    Dots1678,    // ⣡ U+28E1 BRAILLE PATTERN DOTS-1678
    Dots2678,    // ⣢ U+28E2 BRAILLE PATTERN DOTS-2678
    Dots12678,   // ⣣ U+28E3 BRAILLE PATTERN DOTS-12678
    Dots3678,    // ⣤ U+28E4 BRAILLE PATTERN DOTS-3678
    Dots13678,   // ⣥ U+28E5 BRAILLE PATTERN DOTS-13678
    Dots23678,   // ⣦ U+28E6 BRAILLE PATTERN DOTS-23678
    Dots123678,  // ⣧ U+28E7 BRAILLE PATTERN DOTS-123678
    Dots4678,    // ⣨ U+28E8 BRAILLE PATTERN DOTS-4678
    Dots14678,   // ⣩ U+28E9 BRAILLE PATTERN DOTS-14678
    Dots24678,   // ⣪ U+28EA BRAILLE PATTERN DOTS-24678
    Dots124678,  // ⣫ U+28EB BRAILLE PATTERN DOTS-124678
    Dots34678,   // ⣬ U+28EC BRAILLE PATTERN DOTS-34678
    Dots134678,  // ⣭ U+28ED BRAILLE PATTERN DOTS-134678
    Dots234678,  // ⣮ U+28EE BRAILLE PATTERN DOTS-234678
    Dots1234678, // ⣯ U+28EF BRAILLE PATTERN DOTS-1234678
    Dots5678,    // ⣰ U+28F0 BRAILLE PATTERN DOTS-5678
    Dots15678,   // ⣱ U+28F1 BRAILLE PATTERN DOTS-15678
    Dots25678,   // ⣲ U+28F2 BRAILLE PATTERN DOTS-25678
    Dots125678,  // ⣳ U+28F3 BRAILLE PATTERN DOTS

    Dots12345678, // ⣿ U+28DF BRAILLE PATTERN DOTS-12345678
    Dots45678,    // ⣸ U+28F8 Braille Pattern Dots-45678
    Dots35678,    // ⣴ U+28F4
    Dots345678,   // ⣼ U+28FC
    Dots2345678,  // ⣾ U+28FF
    Dots1235678,  // ⣷ U+28F7

    Dots135678,  // ⣵
    Dots1345678, // ⣽
    Dots1245678, // ⣻
    Dots145678,  // ⣹
    Dots245678,  // ⣺
}

impl TryFrom<char> for DrawableChar {
    type Error = char;

    fn try_from(val: char) -> Result<Self, Self::Error> {
        let drawbable_char = match val {
            '─' => DrawableChar::Horizontal,
            '═' => DrawableChar::DoubleHorizontal,
            '│' => DrawableChar::Vertical,
            '║' => DrawableChar::DoubleVertical,
            '━' => DrawableChar::HeavyHorizontal,
            '┃' => DrawableChar::HeavyVertical,
            '└' => DrawableChar::TopRight,
            '┘' => DrawableChar::TopLeft,
            '┌' => DrawableChar::BottomRight,
            '┐' => DrawableChar::BottomLeft,
            '┼' => DrawableChar::Cross,
            '├' => DrawableChar::VerticalRight,
            '┤' => DrawableChar::VerticalLeft,
            '┬' => DrawableChar::HorizontalDown,
            '┴' => DrawableChar::HorizontalUp,

            '╯' => DrawableChar::ArcTopLeft,
            '╭' => DrawableChar::ArcBottomRight,
            '╮' => DrawableChar::ArcBottomLeft,
            '╰' => DrawableChar::ArcTopRight,

            '╥' => DrawableChar::DownDoubleAndHorizontalSingle,
            '╤' => DrawableChar::DownSingleAndHorizontalDouble,
            '╚' => DrawableChar::DoubleUpAndRight,
            '╝' => DrawableChar::DoubleUpAndLeft,
            '╘' => DrawableChar::UpSingleAndRightDouble,
            '╛' => DrawableChar::UpSingleAndLeftDouble,
            '╪' => DrawableChar::VerticalSingleAndHorizontalDouble,

            '▁' => DrawableChar::LowerOneEighthBlock,
            '▂' => DrawableChar::LowerOneQuarterBlock,
            '▃' => DrawableChar::LowerThreeEighthsBlock,
            '▎' => DrawableChar::LeftOneQuarterBlock,
            '▍' => DrawableChar::LeftThreeEighthsBlock,
            '▊' => DrawableChar::LeftThreeQuartersBlock,
            '▕' => DrawableChar::RightOneQuarterBlock,
            '🮈' => DrawableChar::RightThreeEighthsBlock,
            '🮊' => DrawableChar::RightThreeQuartersBlock,
            '▔' => DrawableChar::UpperOneEighthBlock,
            '🮃' => DrawableChar::UpperThreeEighthsBlock,
            '🮅' => DrawableChar::UpperThreeQuartersBlock,

            '┄' => DrawableChar::HorizontalLightDash,
            '┅' => DrawableChar::HorizontalHeavyDash,
            '┈' => DrawableChar::HorizontalLightDoubleDash,
            '┉' => DrawableChar::HorizontalHeavyDoubleDash,
            '╌' => DrawableChar::HorizontalLightTripleDash,
            '╍' => DrawableChar::HorizontalHeavyTripleDash,
            '┆' => DrawableChar::VerticalLightDash,
            '┇' => DrawableChar::VerticalHeavyDash,
            '┊' => DrawableChar::VerticalLightDoubleDash,
            '┋' => DrawableChar::VerticalHeavyDoubleDash,
            '╎' => DrawableChar::VerticalLightTripleDash,
            '╏' => DrawableChar::VerticalHeavyTripleDash,

            '▘' => DrawableChar::QuadrantUpperLeft,
            '▝' => DrawableChar::QuadrantUpperRight,
            '▖' => DrawableChar::QuadrantLowerLeft,
            '▗' => DrawableChar::QuadrantLowerRight,
            '▀' => DrawableChar::UpperHalf,
            '▄' => DrawableChar::LowerHalf,
            '▌' => DrawableChar::LeftHalf,
            '▐' => DrawableChar::RightHalf,
            '░' => DrawableChar::LightShade,
            '▒' => DrawableChar::MediumShade,
            '▓' => DrawableChar::DarkShade,
            '█' => DrawableChar::FullBlock,

            '🬓' => DrawableChar::SeparatedQuadrantUpperLeft,
            '🬔' => DrawableChar::SeparatedQuadrantUpperRight,
            '🬕' => DrawableChar::SeparatedQuadrantLowerLeft,
            '🬖' => DrawableChar::SeparatedQuadrantLowerRight,

            '╬' => DrawableChar::DoubleCross,
            '╠' => DrawableChar::DoubleVerticalRight,
            '╣' => DrawableChar::DoubleVerticalLeft,
            '╦' => DrawableChar::DoubleHorizontalDown,
            '╩' => DrawableChar::DoubleHorizontalUp,
            '╫' => DrawableChar::VerticalDoubleAndHorizontalSingle,

            '╓' => DrawableChar::DownDoubleAndRightSingle,
            '╖' => DrawableChar::DownDoubleAndLeftSingle,
            '╟' => DrawableChar::VerticalDoubleAndRightSingle,
            '╢' => DrawableChar::VerticalDoubleAndLeftSingle,
            '╞' => DrawableChar::VerticalSingleAndRightDouble,
            '╡' => DrawableChar::VerticalSingleAndLeftDouble,
            '╒' => DrawableChar::DownSingleAndRightDouble,
            '╕' => DrawableChar::DownSingleAndLeftDouble,

            '┏' => DrawableChar::HeavyDownAndRight,
            '┓' => DrawableChar::HeavyDownAndLeft,
            '┗' => DrawableChar::HeavyUpAndRight,
            '┛' => DrawableChar::HeavyUpAndLeft,
            '┣' => DrawableChar::HeavyVerticalAndRight,
            '┫' => DrawableChar::HeavyVerticalAndLeft,
            '┳' => DrawableChar::HeavyHorizontalAndDown,
            '┻' => DrawableChar::HeavyHorizontalAndUp,
            '╋' => DrawableChar::HeavyCross,

            '┍' => DrawableChar::LightDownAndHeavyRight,
            '┑' => DrawableChar::LightDownAndHeavyLeft,
            '┎' => DrawableChar::HeavyDownAndLightRight,
            '┒' => DrawableChar::HeavyDownAndLightLeft,
            '┕' => DrawableChar::LightUpAndHeavyRight,
            '┙' => DrawableChar::LightUpAndHeavyLeft,
            '┖' => DrawableChar::HeavyUpAndLightRight,
            '┚' => DrawableChar::HeavyUpAndLightLeft,

            '▅' => DrawableChar::LowerFiveEighthsBlock,
            '▆' => DrawableChar::LowerThreeQuartersBlock,
            '▇' => DrawableChar::LowerSevenEighthsBlock,
            '▚' => DrawableChar::QuadrantUpperLeftAndLowerLeft,
            '▞' => DrawableChar::QuadrantUpperLeftAndLowerRight,
            '▟' => DrawableChar::QuadrantUpperRightAndLowerLeft,
            '▙' => DrawableChar::QuadrantUpperRightAndLowerRight,

            '╱' => DrawableChar::DiagonalRisingBar,
            '╲' => DrawableChar::DiagonalFallingBar,
            '╳' => DrawableChar::DiagonalCross,

            // Quick test:
            // echo "\ue0b0 \ue0b1 \ue0b2 \ue0b3 \ue0b4 \ue0b5 \ue0b6 \ue0b7"
            '\u{e0b0}' => DrawableChar::PowerlineRightSolid,
            '\u{e0b1}' => DrawableChar::PowerlineRightHollow,
            '\u{e0b2}' => DrawableChar::PowerlineLeftSolid,
            '\u{e0b3}' => DrawableChar::PowerlineLeftHollow,
            '\u{e0b4}' => DrawableChar::PowerlineCurvedRightSolid,
            '\u{e0b5}' => DrawableChar::PowerlineCurvedRightHollow,
            '\u{e0b6}' => DrawableChar::PowerlineCurvedLeftSolid,
            '\u{e0b7}' => DrawableChar::PowerlineCurvedLeftHollow,
            // Quick test:
            // echo "\ue0b8 \ue0b9 \ue0ba \ue0bb \ue0bc \ue0bd \ue0be \ue0bf"
            '\u{e0b8}' => DrawableChar::PowerlineLowerLeftTriangle,
            '\u{e0b9}' => DrawableChar::PowerlineBackslashSeparator,
            '\u{e0ba}' => DrawableChar::PowerlineLowerRightTriangle,
            '\u{e0bb}' => DrawableChar::PowerlineForwardslashSeparator,
            '\u{e0bc}' => DrawableChar::PowerlineUpperLeftTriangle,
            '\u{e0bd}' => DrawableChar::PowerlineForwardslashSeparatorRedundant,
            '\u{e0be}' => DrawableChar::PowerlineUpperRightTriangle,
            '\u{e0bf}' => DrawableChar::PowerlineBackslashSeparatorRedundant,

            // Sextant characters (Unicode block U+1FB00 to U+1FB3F)
            c @ '\u{1FB00}'..='\u{1FB3F}' => {
                DrawableChar::Sextant(SEXTANT_PATTERNS[(c as u32 & 0x3f) as usize])
            }

            // Octant characters
            c @ '\u{1CD00}'..='\u{1CDE5}' => {
                DrawableChar::Octant(OCTANT_PATTERNS[(c as u32 & 0xff) as usize])
            }
            // [𜺠] RIGHT HALF LOWER ONE QUARTER BLOCK (corresponds to OCTANT-8)
            '\u{1CEA0}' => DrawableChar::Octant(0b10000000),
            // [𜺣; EFT HALF LOWER ONE QUARTER BLOCK (corresponds to OCTANT-7)
            '\u{1CEA3}' => DrawableChar::Octant(0b01000000),
            // [𜺨] LEFT HALF UPPER ONE QUARTER BLOCK (corresponds to OCTANT-1)
            '\u{1CEA8}' => DrawableChar::Octant(0b00000001),
            // [𜺫] RIGHT HALF UPPER ONE QUARTER BLOCK (corresponds to OCTANT-2)
            '\u{1CEAB}' => DrawableChar::Octant(0b00000010),
            // [🯦] MIDDLE LEFT ONE QUARTER BLOCK (corresponds to OCTANT-35)
            '\u{1FBE6}' => DrawableChar::Octant(0b00010100),
            // [🯧] MIDDLE RIGHT ONE QUARTER BLOCK (corresponds to OCTANT-46)
            '\u{1FBE7}' => DrawableChar::Octant(0b00101000),

            '⠀' => DrawableChar::BrailleBlank,
            '⠁' => DrawableChar::Braille(Braille::Dots1),
            '⠂' => DrawableChar::Braille(Braille::Dots2),
            '⠃' => DrawableChar::Braille(Braille::Dots12),
            '⠄' => DrawableChar::Braille(Braille::Dots3),
            '⠅' => DrawableChar::Braille(Braille::Dots13),
            '⠆' => DrawableChar::Braille(Braille::Dots23),
            '⠇' => DrawableChar::Braille(Braille::Dots123),
            '⠈' => DrawableChar::Braille(Braille::Dots4),
            '⠉' => DrawableChar::Braille(Braille::Dots14),
            '⠊' => DrawableChar::Braille(Braille::Dots24),
            '⠋' => DrawableChar::Braille(Braille::Dots124),
            '⠌' => DrawableChar::Braille(Braille::Dots34),
            '⠍' => DrawableChar::Braille(Braille::Dots134),
            '⠎' => DrawableChar::Braille(Braille::Dots234),
            '⠏' => DrawableChar::Braille(Braille::Dots1234),
            '⠐' => DrawableChar::Braille(Braille::Dots5),
            '⠑' => DrawableChar::Braille(Braille::Dots15),
            '⠒' => DrawableChar::Braille(Braille::Dots25),
            '⠓' => DrawableChar::Braille(Braille::Dots125),
            '⠔' => DrawableChar::Braille(Braille::Dots35),
            '⠕' => DrawableChar::Braille(Braille::Dots135),
            '⠖' => DrawableChar::Braille(Braille::Dots235),
            '⠗' => DrawableChar::Braille(Braille::Dots1235),
            '⠘' => DrawableChar::Braille(Braille::Dots45),
            '⠙' => DrawableChar::Braille(Braille::Dots145),
            '⠚' => DrawableChar::Braille(Braille::Dots245),
            '⠛' => DrawableChar::Braille(Braille::Dots1245),
            '⠜' => DrawableChar::Braille(Braille::Dots345),
            '⠝' => DrawableChar::Braille(Braille::Dots1345),
            '⠞' => DrawableChar::Braille(Braille::Dots2345),
            '⠟' => DrawableChar::Braille(Braille::Dots12345),
            '⠠' => DrawableChar::Braille(Braille::Dots6),
            '⠡' => DrawableChar::Braille(Braille::Dots16),
            '⠢' => DrawableChar::Braille(Braille::Dots26),
            '⠣' => DrawableChar::Braille(Braille::Dots126),
            '⠤' => DrawableChar::Braille(Braille::Dots36),
            '⠥' => DrawableChar::Braille(Braille::Dots136),
            '⠦' => DrawableChar::Braille(Braille::Dots236),
            '⠧' => DrawableChar::Braille(Braille::Dots1236),
            '⠨' => DrawableChar::Braille(Braille::Dots46),
            '⠩' => DrawableChar::Braille(Braille::Dots146),
            '⠪' => DrawableChar::Braille(Braille::Dots246),
            '⠫' => DrawableChar::Braille(Braille::Dots1246),
            '⠬' => DrawableChar::Braille(Braille::Dots346),
            '⠭' => DrawableChar::Braille(Braille::Dots1346),
            '⠮' => DrawableChar::Braille(Braille::Dots2346),
            '⠯' => DrawableChar::Braille(Braille::Dots12346),
            '⠰' => DrawableChar::Braille(Braille::Dots56),
            '⠱' => DrawableChar::Braille(Braille::Dots156),
            '⠲' => DrawableChar::Braille(Braille::Dots256),
            '⠳' => DrawableChar::Braille(Braille::Dots1256),
            '⠴' => DrawableChar::Braille(Braille::Dots356),
            '⠵' => DrawableChar::Braille(Braille::Dots1356),
            '⠶' => DrawableChar::Braille(Braille::Dots2356),
            '⠷' => DrawableChar::Braille(Braille::Dots12356),
            '⠸' => DrawableChar::Braille(Braille::Dots456),
            '⠹' => DrawableChar::Braille(Braille::Dots1456),
            '⠺' => DrawableChar::Braille(Braille::Dots2456),
            '⠻' => DrawableChar::Braille(Braille::Dots12456),
            '⠼' => DrawableChar::Braille(Braille::Dots3456),
            '⠽' => DrawableChar::Braille(Braille::Dots13456),
            '⠾' => DrawableChar::Braille(Braille::Dots23456),
            '⠿' => DrawableChar::Braille(Braille::Dots123456),

            '⡀' => DrawableChar::Braille(Braille::Dots7),
            '⡁' => DrawableChar::Braille(Braille::Dots17),
            '⡂' => DrawableChar::Braille(Braille::Dots27),
            '⡃' => DrawableChar::Braille(Braille::Dots127),
            '⡄' => DrawableChar::Braille(Braille::Dots37),
            '⡅' => DrawableChar::Braille(Braille::Dots137),
            '⡆' => DrawableChar::Braille(Braille::Dots237),
            '⡇' => DrawableChar::Braille(Braille::Dots1237),
            '⡈' => DrawableChar::Braille(Braille::Dots47),
            '⡉' => DrawableChar::Braille(Braille::Dots147),
            '⡊' => DrawableChar::Braille(Braille::Dots247),
            '⡋' => DrawableChar::Braille(Braille::Dots1247),
            '⡌' => DrawableChar::Braille(Braille::Dots347),
            '⡍' => DrawableChar::Braille(Braille::Dots1347),
            '⡎' => DrawableChar::Braille(Braille::Dots2347),
            '⡏' => DrawableChar::Braille(Braille::Dots12347),
            '⡐' => DrawableChar::Braille(Braille::Dots57),
            '⡑' => DrawableChar::Braille(Braille::Dots157),
            '⡒' => DrawableChar::Braille(Braille::Dots257),
            '⡓' => DrawableChar::Braille(Braille::Dots1257),
            '⡔' => DrawableChar::Braille(Braille::Dots357),
            '⡕' => DrawableChar::Braille(Braille::Dots1357),
            '⡖' => DrawableChar::Braille(Braille::Dots2357),
            '⡗' => DrawableChar::Braille(Braille::Dots12357),
            '⡘' => DrawableChar::Braille(Braille::Dots457),
            '⡙' => DrawableChar::Braille(Braille::Dots1457),
            '⡚' => DrawableChar::Braille(Braille::Dots2457),
            '⡛' => DrawableChar::Braille(Braille::Dots12457),
            '⡜' => DrawableChar::Braille(Braille::Dots3457),
            '⡝' => DrawableChar::Braille(Braille::Dots13457),
            '⡞' => DrawableChar::Braille(Braille::Dots23457),
            '⡟' => DrawableChar::Braille(Braille::Dots123457),
            '⡠' => DrawableChar::Braille(Braille::Dots67),
            '⡡' => DrawableChar::Braille(Braille::Dots167),
            '⡢' => DrawableChar::Braille(Braille::Dots267),
            '⡣' => DrawableChar::Braille(Braille::Dots1267),
            '⡤' => DrawableChar::Braille(Braille::Dots367),
            '⡥' => DrawableChar::Braille(Braille::Dots1367),
            '⡦' => DrawableChar::Braille(Braille::Dots2367),
            '⡧' => DrawableChar::Braille(Braille::Dots12367),
            '⡨' => DrawableChar::Braille(Braille::Dots467),
            '⡩' => DrawableChar::Braille(Braille::Dots1467),
            '⡪' => DrawableChar::Braille(Braille::Dots2467),
            '⡫' => DrawableChar::Braille(Braille::Dots12467),
            '⡬' => DrawableChar::Braille(Braille::Dots3467),
            '⡭' => DrawableChar::Braille(Braille::Dots13467),
            '⡮' => DrawableChar::Braille(Braille::Dots23467),
            '⡯' => DrawableChar::Braille(Braille::Dots123467),
            '⡰' => DrawableChar::Braille(Braille::Dots567),
            '⡱' => DrawableChar::Braille(Braille::Dots1567),
            '⡲' => DrawableChar::Braille(Braille::Dots2567),
            '⡳' => DrawableChar::Braille(Braille::Dots12567),
            '⡴' => DrawableChar::Braille(Braille::Dots3567),
            '⡵' => DrawableChar::Braille(Braille::Dots13567),
            '⡶' => DrawableChar::Braille(Braille::Dots23567),
            '⡷' => DrawableChar::Braille(Braille::Dots123567),
            '⡸' => DrawableChar::Braille(Braille::Dots4567),
            '⡹' => DrawableChar::Braille(Braille::Dots14567),
            '⡺' => DrawableChar::Braille(Braille::Dots24567),
            '⡻' => DrawableChar::Braille(Braille::Dots124567),
            '⡼' => DrawableChar::Braille(Braille::Dots34567),
            '⡽' => DrawableChar::Braille(Braille::Dots134567),
            '⡾' => DrawableChar::Braille(Braille::Dots234567),
            '⡿' => DrawableChar::Braille(Braille::Dots1234567),

            '⢀' => DrawableChar::Braille(Braille::Dots8),
            '⢁' => DrawableChar::Braille(Braille::Dots18),
            '⢂' => DrawableChar::Braille(Braille::Dots28),
            '⢃' => DrawableChar::Braille(Braille::Dots128),
            '⢄' => DrawableChar::Braille(Braille::Dots38),
            '⢅' => DrawableChar::Braille(Braille::Dots138),
            '⢆' => DrawableChar::Braille(Braille::Dots238),
            '⢇' => DrawableChar::Braille(Braille::Dots1238),
            '⢈' => DrawableChar::Braille(Braille::Dots48),
            '⢉' => DrawableChar::Braille(Braille::Dots148),
            '⢊' => DrawableChar::Braille(Braille::Dots248),
            '⢋' => DrawableChar::Braille(Braille::Dots1248),
            '⢌' => DrawableChar::Braille(Braille::Dots348),
            '⢍' => DrawableChar::Braille(Braille::Dots1348),
            '⢎' => DrawableChar::Braille(Braille::Dots2348),
            '⢏' => DrawableChar::Braille(Braille::Dots12348),
            '⢐' => DrawableChar::Braille(Braille::Dots58),
            '⢑' => DrawableChar::Braille(Braille::Dots158),
            '⢒' => DrawableChar::Braille(Braille::Dots258),
            '⢓' => DrawableChar::Braille(Braille::Dots1258),
            '⢔' => DrawableChar::Braille(Braille::Dots358),
            '⢕' => DrawableChar::Braille(Braille::Dots1358),
            '⢖' => DrawableChar::Braille(Braille::Dots2358),
            '⢗' => DrawableChar::Braille(Braille::Dots12358),
            '⢘' => DrawableChar::Braille(Braille::Dots458),
            '⢙' => DrawableChar::Braille(Braille::Dots1458),
            '⢚' => DrawableChar::Braille(Braille::Dots2458),
            '⢛' => DrawableChar::Braille(Braille::Dots12458),
            '⢜' => DrawableChar::Braille(Braille::Dots3458),
            '⢝' => DrawableChar::Braille(Braille::Dots13458),
            '⢞' => DrawableChar::Braille(Braille::Dots23458),
            '⢟' => DrawableChar::Braille(Braille::Dots123458),
            '⢠' => DrawableChar::Braille(Braille::Dots68),
            '⢡' => DrawableChar::Braille(Braille::Dots168),
            '⢢' => DrawableChar::Braille(Braille::Dots268),
            '⢣' => DrawableChar::Braille(Braille::Dots1268),
            '⢤' => DrawableChar::Braille(Braille::Dots368),
            '⢥' => DrawableChar::Braille(Braille::Dots1368),
            '⢦' => DrawableChar::Braille(Braille::Dots2368),
            '⢧' => DrawableChar::Braille(Braille::Dots12368),
            '⢨' => DrawableChar::Braille(Braille::Dots468),
            '⢩' => DrawableChar::Braille(Braille::Dots1468),
            '⢪' => DrawableChar::Braille(Braille::Dots2468),
            '⢫' => DrawableChar::Braille(Braille::Dots12468),
            '⢬' => DrawableChar::Braille(Braille::Dots3468),
            '⢭' => DrawableChar::Braille(Braille::Dots13468),
            '⢮' => DrawableChar::Braille(Braille::Dots23468),
            '⢯' => DrawableChar::Braille(Braille::Dots123468),
            '⢰' => DrawableChar::Braille(Braille::Dots568),
            '⢱' => DrawableChar::Braille(Braille::Dots1568),
            '⢲' => DrawableChar::Braille(Braille::Dots2568),
            '⢳' => DrawableChar::Braille(Braille::Dots12568),
            '⢴' => DrawableChar::Braille(Braille::Dots3568),
            '⢵' => DrawableChar::Braille(Braille::Dots13568),
            '⢶' => DrawableChar::Braille(Braille::Dots23568),
            '⢷' => DrawableChar::Braille(Braille::Dots123568),
            '⢸' => DrawableChar::Braille(Braille::Dots4568),
            '⢹' => DrawableChar::Braille(Braille::Dots14568),
            '⢺' => DrawableChar::Braille(Braille::Dots24568),
            '⢻' => DrawableChar::Braille(Braille::Dots124568),
            '⢼' => DrawableChar::Braille(Braille::Dots34568),
            '⢽' => DrawableChar::Braille(Braille::Dots134568),
            '⢾' => DrawableChar::Braille(Braille::Dots234568),
            '⢿' => DrawableChar::Braille(Braille::Dots1234568),

            '⣀' => DrawableChar::Braille(Braille::Dots78),
            '⣁' => DrawableChar::Braille(Braille::Dots178),
            '⣂' => DrawableChar::Braille(Braille::Dots278),
            '⣃' => DrawableChar::Braille(Braille::Dots1278),
            '⣄' => DrawableChar::Braille(Braille::Dots378),
            '⣅' => DrawableChar::Braille(Braille::Dots1378),
            '⣆' => DrawableChar::Braille(Braille::Dots2378),
            '⣇' => DrawableChar::Braille(Braille::Dots12378),
            '⣈' => DrawableChar::Braille(Braille::Dots478),
            '⣉' => DrawableChar::Braille(Braille::Dots1478),
            '⣊' => DrawableChar::Braille(Braille::Dots2478),
            '⣋' => DrawableChar::Braille(Braille::Dots12478),
            '⣌' => DrawableChar::Braille(Braille::Dots3478),
            '⣍' => DrawableChar::Braille(Braille::Dots13478),
            '⣎' => DrawableChar::Braille(Braille::Dots23478),
            '⣏' => DrawableChar::Braille(Braille::Dots123478),
            '⣐' => DrawableChar::Braille(Braille::Dots578),
            '⣑' => DrawableChar::Braille(Braille::Dots1578),
            '⣒' => DrawableChar::Braille(Braille::Dots2578),
            '⣓' => DrawableChar::Braille(Braille::Dots12578),
            '⣔' => DrawableChar::Braille(Braille::Dots3578),
            '⣕' => DrawableChar::Braille(Braille::Dots13578),
            '⣖' => DrawableChar::Braille(Braille::Dots23578),
            '⣗' => DrawableChar::Braille(Braille::Dots123578),
            '⣘' => DrawableChar::Braille(Braille::Dots4578),
            '⣙' => DrawableChar::Braille(Braille::Dots14578),
            '⣚' => DrawableChar::Braille(Braille::Dots24578),
            '⣛' => DrawableChar::Braille(Braille::Dots124578),
            '⣜' => DrawableChar::Braille(Braille::Dots34578),
            '⣝' => DrawableChar::Braille(Braille::Dots134578),
            '⣞' => DrawableChar::Braille(Braille::Dots234578),
            '⣟' => DrawableChar::Braille(Braille::Dots1234578),
            '⣠' => DrawableChar::Braille(Braille::Dots678),
            '⣡' => DrawableChar::Braille(Braille::Dots1678),
            '⣢' => DrawableChar::Braille(Braille::Dots2678),
            '⣣' => DrawableChar::Braille(Braille::Dots12678),
            '⣤' => DrawableChar::Braille(Braille::Dots3678),
            '⣥' => DrawableChar::Braille(Braille::Dots13678),
            '⣦' => DrawableChar::Braille(Braille::Dots23678),
            '⣧' => DrawableChar::Braille(Braille::Dots123678),
            '⣨' => DrawableChar::Braille(Braille::Dots4678),
            '⣩' => DrawableChar::Braille(Braille::Dots14678),
            '⣪' => DrawableChar::Braille(Braille::Dots24678),
            '⣫' => DrawableChar::Braille(Braille::Dots124678),
            '⣬' => DrawableChar::Braille(Braille::Dots34678),
            '⣭' => DrawableChar::Braille(Braille::Dots134678),
            '⣮' => DrawableChar::Braille(Braille::Dots234678),
            '⣯' => DrawableChar::Braille(Braille::Dots1234678),
            '⣰' => DrawableChar::Braille(Braille::Dots5678),
            '⣱' => DrawableChar::Braille(Braille::Dots15678),
            '⣲' => DrawableChar::Braille(Braille::Dots25678),
            '⣳' => DrawableChar::Braille(Braille::Dots125678),
            '⣿' => DrawableChar::Braille(Braille::Dots12345678),
            '⣶' => DrawableChar::Braille(Braille::Dots235678),
            '⣸' => DrawableChar::Braille(Braille::Dots45678),
            '⣴' => DrawableChar::Braille(Braille::Dots35678),
            '⣼' => DrawableChar::Braille(Braille::Dots345678),
            '⣾' => DrawableChar::Braille(Braille::Dots2345678),
            '⣷' => DrawableChar::Braille(Braille::Dots1235678),

            '⣵' => DrawableChar::Braille(Braille::Dots135678),
            '⣽' => DrawableChar::Braille(Braille::Dots1345678),
            '⣻' => DrawableChar::Braille(Braille::Dots1245678),
            '⣹' => DrawableChar::Braille(Braille::Dots145678),
            '⣺' => DrawableChar::Braille(Braille::Dots245678),
            _ => return Err(val),
        };
        Ok(drawbable_char)
    }
}

pub fn contains_braille_dot(braille_dots: &Braille, dot_number: u8) -> bool {
    match dot_number {
        1 => matches!(
            braille_dots,
            Braille::Dots1
                | Braille::Dots12
                | Braille::Dots13
                | Braille::Dots123
                | Braille::Dots14
                | Braille::Dots124
                | Braille::Dots134
                | Braille::Dots1234
                | Braille::Dots15
                | Braille::Dots125
                | Braille::Dots135
                | Braille::Dots1235
                | Braille::Dots145
                | Braille::Dots1245
                | Braille::Dots1345
                | Braille::Dots12345
                | Braille::Dots16
                | Braille::Dots126
                | Braille::Dots136
                | Braille::Dots1236
                | Braille::Dots146
                | Braille::Dots1246
                | Braille::Dots1346
                | Braille::Dots12346
                | Braille::Dots156
                | Braille::Dots1256
                | Braille::Dots1356
                | Braille::Dots12356
                | Braille::Dots1456
                | Braille::Dots12456
                | Braille::Dots13456
                | Braille::Dots123456
                // Dot 7 combinations with dot 1
                | Braille::Dots17
                | Braille::Dots127
                | Braille::Dots137
                | Braille::Dots1237
                | Braille::Dots147
                | Braille::Dots1247
                | Braille::Dots1347
                | Braille::Dots12347
                | Braille::Dots157
                | Braille::Dots1257
                | Braille::Dots1357
                | Braille::Dots12357
                | Braille::Dots1457
                | Braille::Dots12457
                | Braille::Dots13457
                | Braille::Dots123457
                | Braille::Dots167
                | Braille::Dots1267
                | Braille::Dots1367
                | Braille::Dots12367
                | Braille::Dots1467
                | Braille::Dots12467
                | Braille::Dots13467
                | Braille::Dots123467
                | Braille::Dots1567
                | Braille::Dots12567
                | Braille::Dots13567
                | Braille::Dots123567
                | Braille::Dots14567
                | Braille::Dots124567
                | Braille::Dots134567
                | Braille::Dots1234567
                // Dot 8 combinations with dot 1
                | Braille::Dots18
                | Braille::Dots128
                | Braille::Dots138
                | Braille::Dots1238
                | Braille::Dots148
                | Braille::Dots1248
                | Braille::Dots1348
                | Braille::Dots12348
                | Braille::Dots158
                | Braille::Dots1258
                | Braille::Dots1358
                | Braille::Dots12358
                | Braille::Dots1458
                | Braille::Dots12458
                | Braille::Dots13458
                | Braille::Dots123458
                | Braille::Dots168
                | Braille::Dots1268
                | Braille::Dots1368
                | Braille::Dots12368
                | Braille::Dots1468
                | Braille::Dots12468
                | Braille::Dots13468
                | Braille::Dots123468
                | Braille::Dots1568
                | Braille::Dots12568
                | Braille::Dots13568
                | Braille::Dots123568
                | Braille::Dots14568
                | Braille::Dots124568
                | Braille::Dots134568
                | Braille::Dots1234568
                // Combined dot 7 and 8 with dot 1
                | Braille::Dots178
                | Braille::Dots1278
                | Braille::Dots1378
                | Braille::Dots12378
                | Braille::Dots1478
                | Braille::Dots12478
                | Braille::Dots13478
                | Braille::Dots123478
                | Braille::Dots1578
                | Braille::Dots12578
                | Braille::Dots13578
                | Braille::Dots123578
                | Braille::Dots14578
                | Braille::Dots124578
                | Braille::Dots134578
                | Braille::Dots1234578
                | Braille::Dots1678
                | Braille::Dots12678
                | Braille::Dots13678
                | Braille::Dots123678
                | Braille::Dots14678
                | Braille::Dots124678
                | Braille::Dots134678
                | Braille::Dots1234678
                | Braille::Dots15678
                | Braille::Dots125678
                | Braille::Dots12345678
                | Braille::Dots1235678
                | Braille::Dots135678
                | Braille::Dots1345678
                | Braille::Dots1245678
                | Braille::Dots145678
        ),
        2 => matches!(
            braille_dots,
            Braille::Dots2
                | Braille::Dots12
                | Braille::Dots23
                | Braille::Dots123
                | Braille::Dots24
                | Braille::Dots124
                | Braille::Dots234
                | Braille::Dots1234
                | Braille::Dots25
                | Braille::Dots125
                | Braille::Dots235
                | Braille::Dots1235
                | Braille::Dots245
                | Braille::Dots1245
                | Braille::Dots2345
                | Braille::Dots12345
                | Braille::Dots26
                | Braille::Dots126
                | Braille::Dots236
                | Braille::Dots1236
                | Braille::Dots246
                | Braille::Dots1246
                | Braille::Dots2346
                | Braille::Dots12346
                | Braille::Dots256
                | Braille::Dots1256
                | Braille::Dots2356
                | Braille::Dots12356
                | Braille::Dots2456
                | Braille::Dots12456
                | Braille::Dots23456
                | Braille::Dots123456
                // Dot 7 combinations with dot 2
                | Braille::Dots27
                | Braille::Dots127
                | Braille::Dots237
                | Braille::Dots1237
                | Braille::Dots247
                | Braille::Dots1247
                | Braille::Dots2347
                | Braille::Dots12347
                | Braille::Dots257
                | Braille::Dots1257
                | Braille::Dots2357
                | Braille::Dots12357
                | Braille::Dots2457
                | Braille::Dots12457
                | Braille::Dots23457
                | Braille::Dots123457
                | Braille::Dots267
                | Braille::Dots1267
                | Braille::Dots2367
                | Braille::Dots12367
                | Braille::Dots2467
                | Braille::Dots12467
                | Braille::Dots23467
                | Braille::Dots123467
                | Braille::Dots2567
                | Braille::Dots12567
                | Braille::Dots23567
                | Braille::Dots123567
                | Braille::Dots24567
                | Braille::Dots124567
                | Braille::Dots234567
                | Braille::Dots1234567
                // Dot 8 combinations with dot 2
                | Braille::Dots28
                | Braille::Dots128
                | Braille::Dots238
                | Braille::Dots1238
                | Braille::Dots248
                | Braille::Dots1248
                | Braille::Dots2348
                | Braille::Dots12348
                | Braille::Dots258
                | Braille::Dots1258
                | Braille::Dots2358
                | Braille::Dots12358
                | Braille::Dots2458
                | Braille::Dots12458
                | Braille::Dots23458
                | Braille::Dots123458
                | Braille::Dots268
                | Braille::Dots1268
                | Braille::Dots2368
                | Braille::Dots12368
                | Braille::Dots2468
                | Braille::Dots12468
                | Braille::Dots23468
                | Braille::Dots123468
                | Braille::Dots2568
                | Braille::Dots12568
                | Braille::Dots23568
                | Braille::Dots123568
                | Braille::Dots24568
                | Braille::Dots124568
                | Braille::Dots234568
                | Braille::Dots1234568
                // Combined dot 7 and 8 with dot 2
                | Braille::Dots278
                | Braille::Dots1278
                | Braille::Dots2378
                | Braille::Dots12378
                | Braille::Dots2478
                | Braille::Dots12478
                | Braille::Dots23478
                | Braille::Dots123478
                | Braille::Dots2578
                | Braille::Dots12578
                | Braille::Dots23578
                | Braille::Dots123578
                | Braille::Dots24578
                | Braille::Dots124578
                | Braille::Dots234578
                | Braille::Dots1234578
                | Braille::Dots2678
                | Braille::Dots12678
                | Braille::Dots23678
                | Braille::Dots123678
                | Braille::Dots24678
                | Braille::Dots124678
                | Braille::Dots234678
                | Braille::Dots1234678
                | Braille::Dots25678
                | Braille::Dots125678
                | Braille::Dots12345678
                | Braille::Dots235678
                | Braille::Dots2345678
                | Braille::Dots1235678
                | Braille::Dots1245678
                | Braille::Dots245678
        ),
        3 => matches!(
            braille_dots,
            Braille::Dots3
                | Braille::Dots13
                | Braille::Dots23
                | Braille::Dots123
                | Braille::Dots34
                | Braille::Dots134
                | Braille::Dots234
                | Braille::Dots1234
                | Braille::Dots35
                | Braille::Dots135
                | Braille::Dots235
                | Braille::Dots1235
                | Braille::Dots345
                | Braille::Dots1345
                | Braille::Dots2345
                | Braille::Dots12345
                | Braille::Dots36
                | Braille::Dots136
                | Braille::Dots236
                | Braille::Dots1236
                | Braille::Dots346
                | Braille::Dots1346
                | Braille::Dots2346
                | Braille::Dots12346
                | Braille::Dots356
                | Braille::Dots1356
                | Braille::Dots2356
                | Braille::Dots12356
                | Braille::Dots3456
                | Braille::Dots13456
                | Braille::Dots23456
                | Braille::Dots123456
                // Dot 7 combinations with dot 3
                | Braille::Dots37
                | Braille::Dots137
                | Braille::Dots237
                | Braille::Dots1237
                | Braille::Dots347
                | Braille::Dots1347
                | Braille::Dots2347
                | Braille::Dots12347
                | Braille::Dots357
                | Braille::Dots1357
                | Braille::Dots2357
                | Braille::Dots12357
                | Braille::Dots3457
                | Braille::Dots13457
                | Braille::Dots23457
                | Braille::Dots123457
                | Braille::Dots367
                | Braille::Dots1367
                | Braille::Dots2367
                | Braille::Dots12367
                | Braille::Dots3467
                | Braille::Dots13467
                | Braille::Dots23467
                | Braille::Dots123467
                | Braille::Dots3567
                | Braille::Dots13567
                | Braille::Dots23567
                | Braille::Dots123567
                | Braille::Dots34567
                | Braille::Dots134567
                | Braille::Dots234567
                | Braille::Dots1234567
                // Dot 8 combinations with dot 3
                | Braille::Dots38
                | Braille::Dots138
                | Braille::Dots238
                | Braille::Dots1238
                | Braille::Dots348
                | Braille::Dots1348
                | Braille::Dots2348
                | Braille::Dots12348
                | Braille::Dots358
                | Braille::Dots1358
                | Braille::Dots2358
                | Braille::Dots12358
                | Braille::Dots3458
                | Braille::Dots13458
                | Braille::Dots23458
                | Braille::Dots123458
                | Braille::Dots368
                | Braille::Dots1368
                | Braille::Dots2368
                | Braille::Dots12368
                | Braille::Dots3468
                | Braille::Dots13468
                | Braille::Dots23468
                | Braille::Dots123468
                | Braille::Dots3568
                | Braille::Dots13568
                | Braille::Dots23568
                | Braille::Dots123568
                | Braille::Dots34568
                | Braille::Dots134568
                | Braille::Dots234568
                | Braille::Dots1234568
                // Combined dot 7 and 8 with dot 3
                | Braille::Dots378
                | Braille::Dots1378
                | Braille::Dots2378
                | Braille::Dots12378
                | Braille::Dots3478
                | Braille::Dots13478
                | Braille::Dots23478
                | Braille::Dots123478
                | Braille::Dots3578
                | Braille::Dots13578
                | Braille::Dots23578
                | Braille::Dots123578
                | Braille::Dots34578
                | Braille::Dots134578
                | Braille::Dots234578
                | Braille::Dots1234578
                | Braille::Dots3678
                | Braille::Dots13678
                | Braille::Dots23678
                | Braille::Dots123678
                | Braille::Dots34678
                | Braille::Dots134678
                | Braille::Dots234678
                | Braille::Dots1234678
                | Braille::Dots12345678
                | Braille::Dots235678
                | Braille::Dots35678
                | Braille::Dots345678
                | Braille::Dots2345678
                | Braille::Dots1235678
                | Braille::Dots135678
                | Braille::Dots1345678
        ),
        4 => matches!(
            braille_dots,
            Braille::Dots4
                | Braille::Dots14
                | Braille::Dots24
                | Braille::Dots124
                | Braille::Dots34
                | Braille::Dots134
                | Braille::Dots234
                | Braille::Dots1234
                | Braille::Dots45
                | Braille::Dots145
                | Braille::Dots245
                | Braille::Dots1245
                | Braille::Dots345
                | Braille::Dots1345
                | Braille::Dots2345
                | Braille::Dots12345
                | Braille::Dots46
                | Braille::Dots146
                | Braille::Dots246
                | Braille::Dots1246
                | Braille::Dots346
                | Braille::Dots1346
                | Braille::Dots2346
                | Braille::Dots12346
                | Braille::Dots456
                | Braille::Dots1456
                | Braille::Dots2456
                | Braille::Dots12456
                | Braille::Dots3456
                | Braille::Dots13456
                | Braille::Dots23456
                | Braille::Dots123456
                // Dot 7 combinations with dot 4
                | Braille::Dots47
                | Braille::Dots147
                | Braille::Dots247
                | Braille::Dots1247
                | Braille::Dots347
                | Braille::Dots1347
                | Braille::Dots2347
                | Braille::Dots12347
                | Braille::Dots457
                | Braille::Dots1457
                | Braille::Dots2457
                | Braille::Dots12457
                | Braille::Dots3457
                | Braille::Dots13457
                | Braille::Dots23457
                | Braille::Dots123457
                | Braille::Dots467
                | Braille::Dots1467
                | Braille::Dots2467
                | Braille::Dots12467
                | Braille::Dots3467
                | Braille::Dots13467
                | Braille::Dots23467
                | Braille::Dots123467
                | Braille::Dots4567
                | Braille::Dots14567
                | Braille::Dots24567
                | Braille::Dots124567
                | Braille::Dots34567
                | Braille::Dots134567
                | Braille::Dots234567
                | Braille::Dots1234567
                // Dot 8 combinations with dot 4
                | Braille::Dots48
                | Braille::Dots148
                | Braille::Dots248
                | Braille::Dots1248
                | Braille::Dots348
                | Braille::Dots1348
                | Braille::Dots2348
                | Braille::Dots12348
                | Braille::Dots458
                | Braille::Dots1458
                | Braille::Dots2458
                | Braille::Dots12458
                | Braille::Dots3458
                | Braille::Dots13458
                | Braille::Dots23458
                | Braille::Dots123458
                | Braille::Dots468
                | Braille::Dots1468
                | Braille::Dots2468
                | Braille::Dots12468
                | Braille::Dots3468
                | Braille::Dots13468
                | Braille::Dots23468
                | Braille::Dots123468
                | Braille::Dots4568
                | Braille::Dots14568
                | Braille::Dots24568
                | Braille::Dots124568
                | Braille::Dots34568
                | Braille::Dots134568
                | Braille::Dots234568
                | Braille::Dots1234568
                // Combined dot 7 and 8 with dot 4
                | Braille::Dots478
                | Braille::Dots1478
                | Braille::Dots2478
                | Braille::Dots12478
                | Braille::Dots3478
                | Braille::Dots13478
                | Braille::Dots23478
                | Braille::Dots123478
                | Braille::Dots4578
                | Braille::Dots14578
                | Braille::Dots24578
                | Braille::Dots124578
                | Braille::Dots34578
                | Braille::Dots134578
                | Braille::Dots234578
                | Braille::Dots1234578
                | Braille::Dots4678
                | Braille::Dots14678
                | Braille::Dots24678
                | Braille::Dots124678
                | Braille::Dots34678
                | Braille::Dots134678
                | Braille::Dots234678
                | Braille::Dots1234678
                | Braille::Dots12345678
                | Braille::Dots45678
                | Braille::Dots345678
                | Braille::Dots2345678
                | Braille::Dots1345678
                | Braille::Dots1245678
                | Braille::Dots145678
                | Braille::Dots245678
        ),
        5 => matches!(
            braille_dots,
            Braille::Dots5
                    | Braille::Dots15
                    | Braille::Dots25
                    | Braille::Dots125
                    | Braille::Dots35
                    | Braille::Dots135
                    | Braille::Dots235
                    | Braille::Dots1235
                    | Braille::Dots45
                    | Braille::Dots145
                    | Braille::Dots245
                    | Braille::Dots1245
                    | Braille::Dots345
                    | Braille::Dots1345
                    | Braille::Dots2345
                    | Braille::Dots12345
                    | Braille::Dots56
                    | Braille::Dots156
                    | Braille::Dots256
                    | Braille::Dots1256
                    | Braille::Dots356
                    | Braille::Dots1356
                    | Braille::Dots2356
                    | Braille::Dots12356
                    | Braille::Dots456
                    | Braille::Dots1456
                    | Braille::Dots2456
                    | Braille::Dots12456
                    | Braille::Dots3456
                    | Braille::Dots13456
                    | Braille::Dots23456
                    | Braille::Dots123456
                    // Dot 7 combinations with dot 5
                    | Braille::Dots57
                    | Braille::Dots157
                    | Braille::Dots257
                    | Braille::Dots1257
                    | Braille::Dots357
                    | Braille::Dots1357
                    | Braille::Dots2357
                    | Braille::Dots12357
                    | Braille::Dots457
                    | Braille::Dots1457
                    | Braille::Dots2457
                    | Braille::Dots12457
                    | Braille::Dots3457
                    | Braille::Dots13457
                    | Braille::Dots23457
                    | Braille::Dots123457
                    | Braille::Dots567
                    | Braille::Dots1567
                    | Braille::Dots2567
                    | Braille::Dots12567
                    | Braille::Dots3567
                    | Braille::Dots13567
                    | Braille::Dots23567
                    | Braille::Dots123567
                    | Braille::Dots4567
                    | Braille::Dots14567
                    | Braille::Dots24567
                    | Braille::Dots124567
                    | Braille::Dots34567
                    | Braille::Dots134567
                    | Braille::Dots234567
                    | Braille::Dots1234567
                    // Dot 8 combinations with dot 5
                    | Braille::Dots58
                    | Braille::Dots158
                    | Braille::Dots258
                    | Braille::Dots1258
                    | Braille::Dots358
                    | Braille::Dots1358
                    | Braille::Dots2358
                    | Braille::Dots12358
                    | Braille::Dots458
                    | Braille::Dots1458
                    | Braille::Dots2458
                    | Braille::Dots12458
                    | Braille::Dots3458
                    | Braille::Dots13458
                    | Braille::Dots23458
                    | Braille::Dots123458
                    | Braille::Dots568
                    | Braille::Dots1568
                    | Braille::Dots2568
                    | Braille::Dots12568
                    | Braille::Dots3568
                    | Braille::Dots13568
                    | Braille::Dots23568
                    | Braille::Dots123568
                    | Braille::Dots4568
                    | Braille::Dots14568
                    | Braille::Dots24568
                    | Braille::Dots124568
                    | Braille::Dots34568
                    | Braille::Dots134568
                    | Braille::Dots234568
                    | Braille::Dots1234568
                    // Dots 5, 7, and 8 combinations
                    | Braille::Dots578
                    | Braille::Dots1578
                    | Braille::Dots2578
                    | Braille::Dots12578
                    | Braille::Dots3578
                    | Braille::Dots13578
                    | Braille::Dots23578
                    | Braille::Dots123578
                    | Braille::Dots4578
                    | Braille::Dots14578
                    | Braille::Dots24578
                    | Braille::Dots124578
                    | Braille::Dots34578
                    | Braille::Dots134578
                    | Braille::Dots234578
                    | Braille::Dots1234578
                    | Braille::Dots5678
                    | Braille::Dots15678
                    | Braille::Dots25678
                    | Braille::Dots125678
                    | Braille::Dots35678
                    | Braille::Dots135678
                    | Braille::Dots1235678
                    | Braille::Dots45678
                    | Braille::Dots145678
                    | Braille::Dots245678
                    | Braille::Dots1245678
                    | Braille::Dots345678
                    | Braille::Dots1345678
                    | Braille::Dots12345678
                    | Braille::Dots235678
                    | Braille::Dots2345678
        ),
        6 => matches!(
            braille_dots,
            Braille::Dots6
                    | Braille::Dots16
                    | Braille::Dots26
                    | Braille::Dots126
                    | Braille::Dots36
                    | Braille::Dots136
                    | Braille::Dots236
                    | Braille::Dots1236
                    | Braille::Dots46
                    | Braille::Dots146
                    | Braille::Dots246
                    | Braille::Dots1246
                    | Braille::Dots346
                    | Braille::Dots1346
                    | Braille::Dots2346
                    | Braille::Dots12346
                    | Braille::Dots56
                    | Braille::Dots156
                    | Braille::Dots256
                    | Braille::Dots1256
                    | Braille::Dots356
                    | Braille::Dots1356
                    | Braille::Dots2356
                    | Braille::Dots12356
                    | Braille::Dots456
                    | Braille::Dots1456
                    | Braille::Dots2456
                    | Braille::Dots12456
                    | Braille::Dots3456
                    | Braille::Dots13456
                    | Braille::Dots23456
                    | Braille::Dots123456
                    // Dot 7 combinations with dot 6
                    | Braille::Dots67
                    | Braille::Dots167
                    | Braille::Dots267
                    | Braille::Dots1267
                    | Braille::Dots367
                    | Braille::Dots1367
                    | Braille::Dots2367
                    | Braille::Dots12367
                    | Braille::Dots467
                    | Braille::Dots1467
                    | Braille::Dots2467
                    | Braille::Dots12467
                    | Braille::Dots3467
                    | Braille::Dots13467
                    | Braille::Dots23467
                    | Braille::Dots123467
                    | Braille::Dots567
                    | Braille::Dots1567
                    | Braille::Dots2567
                    | Braille::Dots12567
                    | Braille::Dots3567
                    | Braille::Dots13567
                    | Braille::Dots23567
                    | Braille::Dots123567
                    | Braille::Dots4567
                    | Braille::Dots14567
                    | Braille::Dots24567
                    | Braille::Dots124567
                    | Braille::Dots34567
                    | Braille::Dots134567
                    | Braille::Dots234567
                    | Braille::Dots1234567
                    // Dot 8 combinations with dot 6
                    | Braille::Dots68
                    | Braille::Dots168
                    | Braille::Dots268
                    | Braille::Dots1268
                    | Braille::Dots368
                    | Braille::Dots1368
                    | Braille::Dots2368
                    | Braille::Dots12368
                    | Braille::Dots468
                    | Braille::Dots1468
                    | Braille::Dots2468
                    | Braille::Dots12468
                    | Braille::Dots3468
                    | Braille::Dots13468
                    | Braille::Dots23468
                    | Braille::Dots123468
                    | Braille::Dots568
                    | Braille::Dots1568
                    | Braille::Dots2568
                    | Braille::Dots12568
                    | Braille::Dots3568
                    | Braille::Dots13568
                    | Braille::Dots23568
                    | Braille::Dots123568
                    | Braille::Dots4568
                    | Braille::Dots14568
                    | Braille::Dots24568
                    | Braille::Dots124568
                    | Braille::Dots34568
                    | Braille::Dots134568
                    | Braille::Dots234568
                    | Braille::Dots1234568
                    // Dots 6, 7, and 8 combinations
                    | Braille::Dots678
                    | Braille::Dots1678
                    | Braille::Dots2678
                    | Braille::Dots12678
                    | Braille::Dots3678
                    | Braille::Dots13678
                    | Braille::Dots23678
                    | Braille::Dots123678
                    | Braille::Dots4678
                    | Braille::Dots14678
                    | Braille::Dots24678
                    | Braille::Dots124678
                    | Braille::Dots34678
                    | Braille::Dots134678
                    | Braille::Dots234678
                    | Braille::Dots1234678
                    | Braille::Dots5678
                    | Braille::Dots15678
                    | Braille::Dots25678
                    | Braille::Dots125678
                    | Braille::Dots35678
                    | Braille::Dots135678
                    | Braille::Dots1235678
                    | Braille::Dots45678
                    | Braille::Dots145678
                    | Braille::Dots245678
                    | Braille::Dots1245678
                    | Braille::Dots345678
                    | Braille::Dots1345678
                    | Braille::Dots2345678
                    | Braille::Dots235678
                    | Braille::Dots12345678
        ),
        7 => matches!(
            braille_dots,
            Braille::Dots7
                    | Braille::Dots17
                    | Braille::Dots27
                    | Braille::Dots127
                    | Braille::Dots37
                    | Braille::Dots137
                    | Braille::Dots237
                    | Braille::Dots1237
                    | Braille::Dots47
                    | Braille::Dots147
                    | Braille::Dots247
                    | Braille::Dots1247
                    | Braille::Dots347
                    | Braille::Dots1347
                    | Braille::Dots2347
                    | Braille::Dots12347
                    | Braille::Dots57
                    | Braille::Dots157
                    | Braille::Dots257
                    | Braille::Dots1257
                    | Braille::Dots357
                    | Braille::Dots1357
                    | Braille::Dots2357
                    | Braille::Dots12357
                    | Braille::Dots457
                    | Braille::Dots1457
                    | Braille::Dots2457
                    | Braille::Dots12457
                    | Braille::Dots3457
                    | Braille::Dots13457
                    | Braille::Dots23457
                    | Braille::Dots123457
                    | Braille::Dots67
                    | Braille::Dots167
                    | Braille::Dots267
                    | Braille::Dots1267
                    | Braille::Dots367
                    | Braille::Dots1367
                    | Braille::Dots2367
                    | Braille::Dots12367
                    | Braille::Dots467
                    | Braille::Dots1467
                    | Braille::Dots2467
                    | Braille::Dots12467
                    | Braille::Dots3467
                    | Braille::Dots13467
                    | Braille::Dots23467
                    | Braille::Dots123467
                    | Braille::Dots567
                    | Braille::Dots1567
                    | Braille::Dots2567
                    | Braille::Dots12567
                    | Braille::Dots3567
                    | Braille::Dots13567
                    | Braille::Dots23567
                    | Braille::Dots123567
                    | Braille::Dots4567
                    | Braille::Dots14567
                    | Braille::Dots24567
                    | Braille::Dots124567
                    | Braille::Dots34567
                    | Braille::Dots134567
                    | Braille::Dots234567
                    | Braille::Dots1234567
                    // Dots 7 and 8 combinations
                    | Braille::Dots78
                    | Braille::Dots178
                    | Braille::Dots278
                    | Braille::Dots1278
                    | Braille::Dots378
                    | Braille::Dots1378
                    | Braille::Dots2378
                    | Braille::Dots12378
                    | Braille::Dots478
                    | Braille::Dots1478
                    | Braille::Dots2478
                    | Braille::Dots12478
                    | Braille::Dots3478
                    | Braille::Dots13478
                    | Braille::Dots23478
                    | Braille::Dots123478
                    | Braille::Dots578
                    | Braille::Dots1578
                    | Braille::Dots2578
                    | Braille::Dots12578
                    | Braille::Dots3578
                    | Braille::Dots13578
                    | Braille::Dots23578
                    | Braille::Dots123578
                    | Braille::Dots4578
                    | Braille::Dots14578
                    | Braille::Dots24578
                    | Braille::Dots124578
                    | Braille::Dots34578
                    | Braille::Dots134578
                    | Braille::Dots234578
                    | Braille::Dots1234578
                    | Braille::Dots678
                    | Braille::Dots1678
                    | Braille::Dots2678
                    | Braille::Dots12678
                    | Braille::Dots3678
                    | Braille::Dots13678
                    | Braille::Dots23678
                    | Braille::Dots123678
                    | Braille::Dots4678
                    | Braille::Dots14678
                    | Braille::Dots24678
                    | Braille::Dots124678
                    | Braille::Dots34678
                    | Braille::Dots134678
                    | Braille::Dots234678
                    | Braille::Dots1234678
                    | Braille::Dots5678
                    | Braille::Dots15678
                    | Braille::Dots25678
                    | Braille::Dots125678
                    | Braille::Dots35678
                    | Braille::Dots135678
                    | Braille::Dots235678
                    | Braille::Dots1235678
                    | Braille::Dots45678
                    | Braille::Dots145678
                    | Braille::Dots245678
                    | Braille::Dots1245678
                    | Braille::Dots345678
                    | Braille::Dots1345678
                    | Braille::Dots2345678
                    | Braille::Dots12345678
        ),
        8 => matches!(
            braille_dots,
            Braille::Dots8
                    | Braille::Dots18
                    | Braille::Dots28
                    | Braille::Dots128
                    | Braille::Dots38
                    | Braille::Dots138
                    | Braille::Dots238
                    | Braille::Dots1238
                    | Braille::Dots48
                    | Braille::Dots148
                    | Braille::Dots248
                    | Braille::Dots1248
                    | Braille::Dots348
                    | Braille::Dots1348
                    | Braille::Dots2348
                    | Braille::Dots12348
                    | Braille::Dots58
                    | Braille::Dots158
                    | Braille::Dots258
                    | Braille::Dots1258
                    | Braille::Dots358
                    | Braille::Dots1358
                    | Braille::Dots2358
                    | Braille::Dots12358
                    | Braille::Dots458
                    | Braille::Dots1458
                    | Braille::Dots2458
                    | Braille::Dots12458
                    | Braille::Dots3458
                    | Braille::Dots13458
                    | Braille::Dots23458
                    | Braille::Dots123458
                    | Braille::Dots68
                    | Braille::Dots168
                    | Braille::Dots268
                    | Braille::Dots1268
                    | Braille::Dots368
                    | Braille::Dots1368
                    | Braille::Dots2368
                    | Braille::Dots12368
                    | Braille::Dots468
                    | Braille::Dots1468
                    | Braille::Dots2468
                    | Braille::Dots12468
                    | Braille::Dots3468
                    | Braille::Dots13468
                    | Braille::Dots23468
                    | Braille::Dots123468
                    | Braille::Dots568
                    | Braille::Dots1568
                    | Braille::Dots2568
                    | Braille::Dots12568
                    | Braille::Dots3568
                    | Braille::Dots13568
                    | Braille::Dots23568
                    | Braille::Dots123568
                    | Braille::Dots4568
                    | Braille::Dots14568
                    | Braille::Dots24568
                    | Braille::Dots124568
                    | Braille::Dots34568
                    | Braille::Dots134568
                    | Braille::Dots234568
                    | Braille::Dots1234568
                    // Dots 7 and 8 combinations
                    | Braille::Dots78
                    | Braille::Dots178
                    | Braille::Dots278
                    | Braille::Dots1278
                    | Braille::Dots378
                    | Braille::Dots1378
                    | Braille::Dots2378
                    | Braille::Dots12378
                    | Braille::Dots478
                    | Braille::Dots1478
                    | Braille::Dots2478
                    | Braille::Dots12478
                    | Braille::Dots3478
                    | Braille::Dots13478
                    | Braille::Dots23478
                    | Braille::Dots123478
                    | Braille::Dots578
                    | Braille::Dots1578
                    | Braille::Dots2578
                    | Braille::Dots12578
                    | Braille::Dots3578
                    | Braille::Dots13578
                    | Braille::Dots23578
                    | Braille::Dots123578
                    | Braille::Dots4578
                    | Braille::Dots14578
                    | Braille::Dots24578
                    | Braille::Dots124578
                    | Braille::Dots34578
                    | Braille::Dots134578
                    | Braille::Dots234578
                    | Braille::Dots1234578
                    | Braille::Dots678
                    | Braille::Dots1678
                    | Braille::Dots2678
                    | Braille::Dots12678
                    | Braille::Dots3678
                    | Braille::Dots13678
                    | Braille::Dots23678
                    | Braille::Dots123678
                    | Braille::Dots4678
                    | Braille::Dots14678
                    | Braille::Dots24678
                    | Braille::Dots124678
                    | Braille::Dots34678
                    | Braille::Dots134678
                    | Braille::Dots234678
                    | Braille::Dots1234678
                    | Braille::Dots5678
                    | Braille::Dots15678
                    | Braille::Dots25678
                    | Braille::Dots125678
                    | Braille::Dots35678
                    | Braille::Dots135678
                    | Braille::Dots235678
                    | Braille::Dots1235678
                    | Braille::Dots45678
                    | Braille::Dots145678
                    | Braille::Dots245678
                    | Braille::Dots1245678
                    | Braille::Dots345678
                    | Braille::Dots1345678
                    | Braille::Dots2345678
                    | Braille::Dots12345678
        ),
        _ => false,
    }
}

// Retired from https://github.com/wezterm/wezterm/blob/d4b50f6cc34aa0d8729f0914e1926ee6c6e19369/wezterm-gui/src/customglyph.rs#L256
// Lookup table from sextant Unicode range 0x1fb00..=0x1fb3b to sextant pattern:
// `pattern` is a byte whose bits corresponds to elements on a 2 by 3 grid.
// The position of a sextant for a bit position (0-indexed) is as follows:
// ╭───┬───╮
// │ 0 │ 1 │
// ├───┼───┤
// │ 2 │ 3 │
// ├───┼───┤
// │ 4 │ 5 │
// ╰───┴───╯
const SEXTANT_PATTERNS: [u8; 64] = [
    0b000000, // [🬀] BLOCK SEXTANT (empty)
    0b000001, // [🬁] BLOCK SEXTANT-1
    0b000010, // [🬁] BLOCK SEXTANT-2
    0b000011, // [🬂] BLOCK SEXTANT-12
    0b000100, // [🬃] BLOCK SEXTANT-3
    0b000101, // [🬄] BLOCK SEXTANT-13
    0b000110, // [🬅] BLOCK SEXTANT-23
    0b000111, // [🬆] BLOCK SEXTANT-123
    0b001000, // [🬇] BLOCK SEXTANT-4
    0b001001, // [🬈] BLOCK SEXTANT-14
    0b001010, // [🬉] BLOCK SEXTANT-24
    0b001011, // [🬊] BLOCK SEXTANT-124
    0b001100, // [🬋] BLOCK SEXTANT-34
    0b001101, // [🬌] BLOCK SEXTANT-134
    0b001110, // [🬍] BLOCK SEXTANT-234
    0b001111, // [🬎] BLOCK SEXTANT-1234
    0b010000, // [🬏] BLOCK SEXTANT-5
    0b010001, // [🬐] BLOCK SEXTANT-15
    0b010010, // [🬑] BLOCK SEXTANT-25
    0b010011, // [🬒] BLOCK SEXTANT-125
    0b010100, // [🬓] BLOCK SEXTANT-35
    0b010101, // [🬔] BLOCK SEXTANT-135
    0b010110, // [🬕] BLOCK SEXTANT-235
    0b010111, // [🬕] BLOCK SEXTANT-1235
    0b011000, // [🬖] BLOCK SEXTANT-45
    0b011001, // [🬗] BLOCK SEXTANT-145
    0b011010, // [🬘] BLOCK SEXTANT-245
    0b011011, // [🬙] BLOCK SEXTANT-1245
    0b011100, // [🬚] BLOCK SEXTANT-345
    0b011101, // [🬛] BLOCK SEXTANT-1345
    0b011110, // [🬜] BLOCK SEXTANT-2345
    0b011111, // [🬝] BLOCK SEXTANT-12345
    0b100000, // [🬞] BLOCK SEXTANT-6
    0b100001, // [🬟] BLOCK SEXTANT-16
    0b100010, // [🬠] BLOCK SEXTANT-26
    0b100011, // [🬡] BLOCK SEXTANT-126
    0b100100, // [🬢] BLOCK SEXTANT-36
    0b100101, // [🬣] BLOCK SEXTANT-136
    0b100110, // [🬤] BLOCK SEXTANT-236
    0b100111, // [🬥] BLOCK SEXTANT-1236
    0b101000, // [🬦] BLOCK SEXTANT-46
    0b101001, // [🬧] BLOCK SEXTANT-146
    0b101010, // [🬨] BLOCK SEXTANT-246
    0b101011, // [🬩] BLOCK SEXTANT-1246
    0b101100, // [🬩] BLOCK SEXTANT-346
    0b101101, // [🬪] BLOCK SEXTANT-1346
    0b101110, // [🬫] BLOCK SEXTANT-2346
    0b101111, // [🬬] BLOCK SEXTANT-12346
    0b110000, // [🬭] BLOCK SEXTANT-56
    0b110001, // [🬮] BLOCK SEXTANT-156
    0b110010, // [🬯] BLOCK SEXTANT-256
    0b110011, // [🬰] BLOCK SEXTANT-1256
    0b110100, // [🬱] BLOCK SEXTANT-356
    0b110101, // [🬲] BLOCK SEXTANT-1356
    0b110110, // [🬳] BLOCK SEXTANT-2356
    0b110111, // [🬴] BLOCK SEXTANT-12356
    0b111000, // [🬵] BLOCK SEXTANT-456
    0b111001, // [🬶] BLOCK SEXTANT-1456
    0b111010, // [🬷] BLOCK SEXTANT-2456
    0b111011, // [🬸] BLOCK SEXTANT-12456
    0b111100, // [🬹] BLOCK SEXTANT-3456
    0b111101, // [🬺] BLOCK SEXTANT-13456
    0b111110, // [🬻] BLOCK SEXTANT-23456
    0b111111, // [🬼] BLOCK SEXTANT-123456 (full)
];

// Retired from https://github.com/wezterm/wezterm/blob/d4b50f6cc34aa0d8729f0914e1926ee6c6e19369/wezterm-gui/src/customglyph.rs#L329
// Lookup table from octant Unicode range 0x1cd00..=0x1cde5 to octant pattern:
// `pattern` is a byte whose bits corresponds to elements on a 2 by 4 grid.
// The position of a octant for a bit position (0-indexed) is as follows:
// ╭───┬───╮
// │ 0 │ 1 │
// ├───┼───┤
// │ 2 │ 3 │
// ├───┼───┤
// │ 4 │ 5 │
// ├───┼───┤
// │ 6 │ 7 │
// ╰───┴───╯
const OCTANT_PATTERNS: [u8; 230] = [
    0b00000100, // 1CD00;BLOCK OCTANT-3
    0b00000110, // 1CD01;BLOCK OCTANT-23
    0b00000111, // 1CD02;BLOCK OCTANT-123
    0b00001000, // 1CD03;BLOCK OCTANT-4
    0b00001001, // 1CD04;BLOCK OCTANT-14
    0b00001011, // 1CD05;BLOCK OCTANT-124
    0b00001100, // 1CD06;BLOCK OCTANT-34
    0b00001101, // 1CD07;BLOCK OCTANT-134
    0b00001110, // 1CD08;BLOCK OCTANT-234
    0b00010000, // 1CD09;BLOCK OCTANT-5
    0b00010001, // 1CD0A;BLOCK OCTANT-15
    0b00010010, // 1CD0B;BLOCK OCTANT-25
    0b00010011, // 1CD0C;BLOCK OCTANT-125
    0b00010101, // 1CD0D;BLOCK OCTANT-135
    0b00010110, // 1CD0E;BLOCK OCTANT-235
    0b00010111, // 1CD0F;BLOCK OCTANT-1235
    0b00011000, // 1CD10;BLOCK OCTANT-45
    0b00011001, // 1CD11;BLOCK OCTANT-145
    0b00011010, // 1CD12;BLOCK OCTANT-245
    0b00011011, // 1CD13;BLOCK OCTANT-1245
    0b00011100, // 1CD14;BLOCK OCTANT-345
    0b00011101, // 1CD15;BLOCK OCTANT-1345
    0b00011110, // 1CD16;BLOCK OCTANT-2345
    0b00011111, // 1CD17;BLOCK OCTANT-12345
    0b00100000, // 1CD18;BLOCK OCTANT-6
    0b00100001, // 1CD19;BLOCK OCTANT-16
    0b00100010, // 1CD1A;BLOCK OCTANT-26
    0b00100011, // 1CD1B;BLOCK OCTANT-126
    0b00100100, // 1CD1C;BLOCK OCTANT-36
    0b00100101, // 1CD1D;BLOCK OCTANT-136
    0b00100110, // 1CD1E;BLOCK OCTANT-236
    0b00100111, // 1CD1F;BLOCK OCTANT-1236
    0b00101001, // 1CD20;BLOCK OCTANT-146
    0b00101010, // 1CD21;BLOCK OCTANT-246
    0b00101011, // 1CD22;BLOCK OCTANT-1246
    0b00101100, // 1CD23;BLOCK OCTANT-346
    0b00101101, // 1CD24;BLOCK OCTANT-1346
    0b00101110, // 1CD25;BLOCK OCTANT-2346
    0b00101111, // 1CD26;BLOCK OCTANT-12346
    0b00110000, // 1CD27;BLOCK OCTANT-56
    0b00110001, // 1CD28;BLOCK OCTANT-156
    0b00110010, // 1CD29;BLOCK OCTANT-256
    0b00110011, // 1CD2A;BLOCK OCTANT-1256
    0b00110100, // 1CD2B;BLOCK OCTANT-356
    0b00110101, // 1CD2C;BLOCK OCTANT-1356
    0b00110110, // 1CD2D;BLOCK OCTANT-2356
    0b00110111, // 1CD2E;BLOCK OCTANT-12356
    0b00111000, // 1CD2F;BLOCK OCTANT-456
    0b00111001, // 1CD30;BLOCK OCTANT-1456
    0b00111010, // 1CD31;BLOCK OCTANT-2456
    0b00111011, // 1CD32;BLOCK OCTANT-12456
    0b00111100, // 1CD33;BLOCK OCTANT-3456
    0b00111101, // 1CD34;BLOCK OCTANT-13456
    0b00111110, // 1CD35;BLOCK OCTANT-23456
    0b01000001, // 1CD36;BLOCK OCTANT-17
    0b01000010, // 1CD37;BLOCK OCTANT-27
    0b01000011, // 1CD38;BLOCK OCTANT-127
    0b01000100, // 1CD39;BLOCK OCTANT-37
    0b01000101, // 1CD3A;BLOCK OCTANT-137
    0b01000110, // 1CD3B;BLOCK OCTANT-237
    0b01000111, // 1CD3C;BLOCK OCTANT-1237
    0b01001000, // 1CD3D;BLOCK OCTANT-47
    0b01001001, // 1CD3E;BLOCK OCTANT-147
    0b01001010, // 1CD3F;BLOCK OCTANT-247
    0b01001011, // 1CD40;BLOCK OCTANT-1247
    0b01001100, // 1CD41;BLOCK OCTANT-347
    0b01001101, // 1CD42;BLOCK OCTANT-1347
    0b01001110, // 1CD43;BLOCK OCTANT-2347
    0b01001111, // 1CD44;BLOCK OCTANT-12347
    0b01010001, // 1CD45;BLOCK OCTANT-157
    0b01010010, // 1CD46;BLOCK OCTANT-257
    0b01010011, // 1CD47;BLOCK OCTANT-1257
    0b01010100, // 1CD48;BLOCK OCTANT-357
    0b01010110, // 1CD49;BLOCK OCTANT-2357
    0b01010111, // 1CD4A;BLOCK OCTANT-12357
    0b01011000, // 1CD4B;BLOCK OCTANT-457
    0b01011001, // 1CD4C;BLOCK OCTANT-1457
    0b01011011, // 1CD4D;BLOCK OCTANT-12457
    0b01011100, // 1CD4E;BLOCK OCTANT-3457
    0b01011101, // 1CD4F;BLOCK OCTANT-13457
    0b01011110, // 1CD50;BLOCK OCTANT-23457
    0b01100000, // 1CD51;BLOCK OCTANT-67
    0b01100001, // 1CD52;BLOCK OCTANT-167
    0b01100010, // 1CD53;BLOCK OCTANT-267
    0b01100011, // 1CD54;BLOCK OCTANT-1267
    0b01100100, // 1CD55;BLOCK OCTANT-367
    0b01100101, // 1CD56;BLOCK OCTANT-1367
    0b01100110, // 1CD57;BLOCK OCTANT-2367
    0b01100111, // 1CD58;BLOCK OCTANT-12367
    0b01101000, // 1CD59;BLOCK OCTANT-467
    0b01101001, // 1CD5A;BLOCK OCTANT-1467
    0b01101010, // 1CD5B;BLOCK OCTANT-2467
    0b01101011, // 1CD5C;BLOCK OCTANT-12467
    0b01101100, // 1CD5D;BLOCK OCTANT-3467
    0b01101101, // 1CD5E;BLOCK OCTANT-13467
    0b01101110, // 1CD5F;BLOCK OCTANT-23467
    0b01101111, // 1CD60;BLOCK OCTANT-123467
    0b01110000, // 1CD61;BLOCK OCTANT-567
    0b01110001, // 1CD62;BLOCK OCTANT-1567
    0b01110010, // 1CD63;BLOCK OCTANT-2567
    0b01110011, // 1CD64;BLOCK OCTANT-12567
    0b01110100, // 1CD65;BLOCK OCTANT-3567
    0b01110101, // 1CD66;BLOCK OCTANT-13567
    0b01110110, // 1CD67;BLOCK OCTANT-23567
    0b01110111, // 1CD68;BLOCK OCTANT-123567
    0b01111000, // 1CD69;BLOCK OCTANT-4567
    0b01111001, // 1CD6A;BLOCK OCTANT-14567
    0b01111010, // 1CD6B;BLOCK OCTANT-24567
    0b01111011, // 1CD6C;BLOCK OCTANT-124567
    0b01111100, // 1CD6D;BLOCK OCTANT-34567
    0b01111101, // 1CD6E;BLOCK OCTANT-134567
    0b01111110, // 1CD6F;BLOCK OCTANT-234567
    0b01111111, // 1CD70;BLOCK OCTANT-1234567
    0b10000001, // 1CD71;BLOCK OCTANT-18
    0b10000010, // 1CD72;BLOCK OCTANT-28
    0b10000011, // 1CD73;BLOCK OCTANT-128
    0b10000100, // 1CD74;BLOCK OCTANT-38
    0b10000101, // 1CD75;BLOCK OCTANT-138
    0b10000110, // 1CD76;BLOCK OCTANT-238
    0b10000111, // 1CD77;BLOCK OCTANT-1238
    0b10001000, // 1CD78;BLOCK OCTANT-48
    0b10001001, // 1CD79;BLOCK OCTANT-148
    0b10001010, // 1CD7A;BLOCK OCTANT-248
    0b10001011, // 1CD7B;BLOCK OCTANT-1248
    0b10001100, // 1CD7C;BLOCK OCTANT-348
    0b10001101, // 1CD7D;BLOCK OCTANT-1348
    0b10001110, // 1CD7E;BLOCK OCTANT-2348
    0b10001111, // 1CD7F;BLOCK OCTANT-12348
    0b10010000, // 1CD80;BLOCK OCTANT-58
    0b10010001, // 1CD81;BLOCK OCTANT-158
    0b10010010, // 1CD82;BLOCK OCTANT-258
    0b10010011, // 1CD83;BLOCK OCTANT-1258
    0b10010100, // 1CD84;BLOCK OCTANT-358
    0b10010101, // 1CD85;BLOCK OCTANT-1358
    0b10010110, // 1CD86;BLOCK OCTANT-2358
    0b10010111, // 1CD87;BLOCK OCTANT-12358
    0b10011000, // 1CD88;BLOCK OCTANT-458
    0b10011001, // 1CD89;BLOCK OCTANT-1458
    0b10011010, // 1CD8A;BLOCK OCTANT-2458
    0b10011011, // 1CD8B;BLOCK OCTANT-12458
    0b10011100, // 1CD8C;BLOCK OCTANT-3458
    0b10011101, // 1CD8D;BLOCK OCTANT-13458
    0b10011110, // 1CD8E;BLOCK OCTANT-23458
    0b10011111, // 1CD8F;BLOCK OCTANT-123458
    0b10100001, // 1CD90;BLOCK OCTANT-168
    0b10100010, // 1CD91;BLOCK OCTANT-268
    0b10100011, // 1CD92;BLOCK OCTANT-1268
    0b10100100, // 1CD93;BLOCK OCTANT-368
    0b10100110, // 1CD94;BLOCK OCTANT-2368
    0b10100111, // 1CD95;BLOCK OCTANT-12368
    0b10101000, // 1CD96;BLOCK OCTANT-468
    0b10101001, // 1CD97;BLOCK OCTANT-1468
    0b10101011, // 1CD98;BLOCK OCTANT-12468
    0b10101100, // 1CD99;BLOCK OCTANT-3468
    0b10101101, // 1CD9A;BLOCK OCTANT-13468
    0b10101110, // 1CD9B;BLOCK OCTANT-23468
    0b10110000, // 1CD9C;BLOCK OCTANT-568
    0b10110001, // 1CD9D;BLOCK OCTANT-1568
    0b10110010, // 1CD9E;BLOCK OCTANT-2568
    0b10110011, // 1CD9F;BLOCK OCTANT-12568
    0b10110100, // 1CDA0;BLOCK OCTANT-3568
    0b10110101, // 1CDA1;BLOCK OCTANT-13568
    0b10110110, // 1CDA2;BLOCK OCTANT-23568
    0b10110111, // 1CDA3;BLOCK OCTANT-123568
    0b10111000, // 1CDA4;BLOCK OCTANT-4568
    0b10111001, // 1CDA5;BLOCK OCTANT-14568
    0b10111010, // 1CDA6;BLOCK OCTANT-24568
    0b10111011, // 1CDA7;BLOCK OCTANT-124568
    0b10111100, // 1CDA8;BLOCK OCTANT-34568
    0b10111101, // 1CDA9;BLOCK OCTANT-134568
    0b10111110, // 1CDAA;BLOCK OCTANT-234568
    0b10111111, // 1CDAB;BLOCK OCTANT-1234568
    0b11000001, // 1CDAC;BLOCK OCTANT-178
    0b11000010, // 1CDAD;BLOCK OCTANT-278
    0b11000011, // 1CDAE;BLOCK OCTANT-1278
    0b11000100, // 1CDAF;BLOCK OCTANT-378
    0b11000101, // 1CDB0;BLOCK OCTANT-1378
    0b11000110, // 1CDB1;BLOCK OCTANT-2378
    0b11000111, // 1CDB2;BLOCK OCTANT-12378
    0b11001000, // 1CDB3;BLOCK OCTANT-478
    0b11001001, // 1CDB4;BLOCK OCTANT-1478
    0b11001010, // 1CDB5;BLOCK OCTANT-2478
    0b11001011, // 1CDB6;BLOCK OCTANT-12478
    0b11001100, // 1CDB7;BLOCK OCTANT-3478
    0b11001101, // 1CDB8;BLOCK OCTANT-13478
    0b11001110, // 1CDB9;BLOCK OCTANT-23478
    0b11001111, // 1CDBA;BLOCK OCTANT-123478
    0b11010000, // 1CDBB;BLOCK OCTANT-578
    0b11010001, // 1CDBC;BLOCK OCTANT-1578
    0b11010010, // 1CDBD;BLOCK OCTANT-2578
    0b11010011, // 1CDBE;BLOCK OCTANT-12578
    0b11010100, // 1CDBF;BLOCK OCTANT-3578
    0b11010101, // 1CDC0;BLOCK OCTANT-13578
    0b11010110, // 1CDC1;BLOCK OCTANT-23578
    0b11010111, // 1CDC2;BLOCK OCTANT-123578
    0b11011000, // 1CDC3;BLOCK OCTANT-4578
    0b11011001, // 1CDC4;BLOCK OCTANT-14578
    0b11011010, // 1CDC5;BLOCK OCTANT-24578
    0b11011011, // 1CDC6;BLOCK OCTANT-124578
    0b11011100, // 1CDC7;BLOCK OCTANT-34578
    0b11011101, // 1CDC8;BLOCK OCTANT-134578
    0b11011110, // 1CDC9;BLOCK OCTANT-234578
    0b11011111, // 1CDCA;BLOCK OCTANT-1234578
    0b11100000, // 1CDCB;BLOCK OCTANT-678
    0b11100001, // 1CDCC;BLOCK OCTANT-1678
    0b11100010, // 1CDCD;BLOCK OCTANT-2678
    0b11100011, // 1CDCE;BLOCK OCTANT-12678
    0b11100100, // 1CDCF;BLOCK OCTANT-3678
    0b11100101, // 1CDD0;BLOCK OCTANT-13678
    0b11100110, // 1CDD1;BLOCK OCTANT-23678
    0b11100111, // 1CDD2;BLOCK OCTANT-123678
    0b11101000, // 1CDD3;BLOCK OCTANT-4678
    0b11101001, // 1CDD4;BLOCK OCTANT-14678
    0b11101010, // 1CDD5;BLOCK OCTANT-24678
    0b11101011, // 1CDD6;BLOCK OCTANT-124678
    0b11101100, // 1CDD7;BLOCK OCTANT-34678
    0b11101101, // 1CDD8;BLOCK OCTANT-134678
    0b11101110, // 1CDD9;BLOCK OCTANT-234678
    0b11101111, // 1CDDA;BLOCK OCTANT-1234678
    0b11110001, // 1CDDB;BLOCK OCTANT-15678
    0b11110010, // 1CDDC;BLOCK OCTANT-25678
    0b11110011, // 1CDDD;BLOCK OCTANT-125678
    0b11110100, // 1CDDE;BLOCK OCTANT-35678
    0b11110110, // 1CDDF;BLOCK OCTANT-235678
    0b11110111, // 1CDE0;BLOCK OCTANT-1235678
    0b11111000, // 1CDE1;BLOCK OCTANT-45678
    0b11111001, // 1CDE2;BLOCK OCTANT-145678
    0b11111011, // 1CDE3;BLOCK OCTANT-1245678
    0b11111101, // 1CDE4;BLOCK OCTANT-1345678
    0b11111110, // 1CDE5;BLOCK OCTANT-2345678
];
