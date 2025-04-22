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
    // Block elements
    QuadrantUpperLeft,  // ▘
    QuadrantUpperRight, // ▝
    QuadrantLowerLeft,  // ▖
    QuadrantLowerRight, // ▗
    UpperHalf,          // ▀
    LowerHalf,          // ▄
    LeftHalf,           // ▌
    RightHalf,          // ▐
    // Shade blocks
    LightShade,  // ░
    MediumShade, // ▒
    DarkShade,   // ▓
    FullBlock,   // █

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
