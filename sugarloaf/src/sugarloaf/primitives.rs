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
pub struct RichText {
    pub id: usize,
    pub position: [f32; 2],
}

#[derive(Clone, Debug, PartialEq)]
pub enum Object {
    Quad(Quad, Option<usize>),
    RichText(RichText, Option<usize>),
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
    Horizontal,      // ─
    DoubleHorizontal,// ═
    Vertical,        // │
    DoubleVertical,  // ║
    HeavyHorizontal, // ━
    HeavyVertical,   // ┃
    TopRight,        // └
    TopLeft,         // ┘
    BottomRight,     // ┌
    BottomLeft,      // ┐
    Cross,           // ┼
    VerticalRight,   // ├
    VerticalLeft,    // ┤
    HorizontalDown,  // ┬
    HorizontalUp,    // ┴
    ArcTopLeft,      // ╯
    ArcBottomRight,  // ╭
    ArcBottomLeft,   // ╮
    ArcTopRight,     // ╰

    DownDoubleAndHorizontalSingle, // ╥
    DownSingleAndHorizontalDouble, // ╤
    DoubleUpAndRight, // ╚
    DoubleUpAndLeft, // ╝
    UpSingleAndRightDouble, // ╘
    UpSingleAndLeftDouble, // ╛
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
    // Complete set of Braille characters (U+2800 to U+28FF)
    // First row (no dot 7, no dot 8)
    // BrailleBlank,      // ⠀ U+2800 BRAILLE PATTERN BLANK
    // BrailleDots1,      // ⠁ U+2801 BRAILLE PATTERN DOTS-1
    // BrailleDots2,      // ⠂ U+2802 BRAILLE PATTERN DOTS-2
    // BrailleDots12,     // ⠃ U+2803 BRAILLE PATTERN DOTS-12
    // BrailleDots3,      // ⠄ U+2804 BRAILLE PATTERN DOTS-3
    // BrailleDots13,     // ⠅ U+2805 BRAILLE PATTERN DOTS-13
    // BrailleDots23,     // ⠆ U+2806 BRAILLE PATTERN DOTS-23
    // BrailleDots123,    // ⠇ U+2807 BRAILLE PATTERN DOTS-123
    // BrailleDots4,      // ⠈ U+2808 BRAILLE PATTERN DOTS-4
    // BrailleDots14,     // ⠉ U+2809 BRAILLE PATTERN DOTS-14
    // BrailleDots24,     // ⠊ U+280A BRAILLE PATTERN DOTS-24
    // BrailleDots124,    // ⠋ U+280B BRAILLE PATTERN DOTS-124
    // BrailleDots34,     // ⠌ U+280C BRAILLE PATTERN DOTS-34
    // BrailleDots134,    // ⠍ U+280D BRAILLE PATTERN DOTS-134
    // BrailleDots234,    // ⠎ U+280E BRAILLE PATTERN DOTS-234
    // BrailleDots1234,   // ⠏ U+280F BRAILLE PATTERN DOTS-1234
    // BrailleDots5,      // ⠐ U+2810 BRAILLE PATTERN DOTS-5
    // BrailleDots15,     // ⠑ U+2811 BRAILLE PATTERN DOTS-15
    // BrailleDots25,     // ⠒ U+2812 BRAILLE PATTERN DOTS-25
    // BrailleDots125,    // ⠓ U+2813 BRAILLE PATTERN DOTS-125
    // BrailleDots35,     // ⠔ U+2814 BRAILLE PATTERN DOTS-35
    // BrailleDots135,    // ⠕ U+2815 BRAILLE PATTERN DOTS-135
    // BrailleDots235,    // ⠖ U+2816 BRAILLE PATTERN DOTS-235
    // BrailleDots1235,   // ⠗ U+2817 BRAILLE PATTERN DOTS-1235
    // BrailleDots45,     // ⠘ U+2818 BRAILLE PATTERN DOTS-45
    // BrailleDots145,    // ⠙ U+2819 BRAILLE PATTERN DOTS-145
    // BrailleDots245,    // ⠚ U+281A BRAILLE PATTERN DOTS-245
    // BrailleDots1245,   // ⠛ U+281B BRAILLE PATTERN DOTS-1245
    // BrailleDots345,    // ⠜ U+281C BRAILLE PATTERN DOTS-345
    // BrailleDots1345,   // ⠝ U+281D BRAILLE PATTERN DOTS-1345
    // BrailleDots2345,   // ⠞ U+281E BRAILLE PATTERN DOTS-2345
    // BrailleDots12345,  // ⠟ U+281F BRAILLE PATTERN DOTS-12345
    // BrailleDots6,      // ⠠ U+2820 BRAILLE PATTERN DOTS-6
    // BrailleDots16,     // ⠡ U+2821 BRAILLE PATTERN DOTS-16
    // BrailleDots26,     // ⠢ U+2822 BRAILLE PATTERN DOTS-26
    // BrailleDots126,    // ⠣ U+2823 BRAILLE PATTERN DOTS-126
    // BrailleDots36,     // ⠤ U+2824 BRAILLE PATTERN DOTS-36
    // BrailleDots136,    // ⠥ U+2825 BRAILLE PATTERN DOTS-136
    // BrailleDots236,    // ⠦ U+2826 BRAILLE PATTERN DOTS-236
    // BrailleDots1236,   // ⠧ U+2827 BRAILLE PATTERN DOTS-1236
    // BrailleDots46,     // ⠨ U+2828 BRAILLE PATTERN DOTS-46
    // BrailleDots146,    // ⠩ U+2829 BRAILLE PATTERN DOTS-146
    // BrailleDots246,    // ⠪ U+282A BRAILLE PATTERN DOTS-246
    // BrailleDots1246,   // ⠫ U+282B BRAILLE PATTERN DOTS-1246
    // BrailleDots346,    // ⠬ U+282C BRAILLE PATTERN DOTS-346
    // BrailleDots1346,   // ⠭ U+282D BRAILLE PATTERN DOTS-1346
    // BrailleDots2346,   // ⠮ U+282E BRAILLE PATTERN DOTS-2346
    // BrailleDots12346,  // ⠯ U+282F BRAILLE PATTERN DOTS-12346
    // BrailleDots56,     // ⠰ U+2830 BRAILLE PATTERN DOTS-56
    // BrailleDots156,    // ⠱ U+2831 BRAILLE PATTERN DOTS-156
    // BrailleDots256,    // ⠲ U+2832 BRAILLE PATTERN DOTS-256
    // BrailleDots1256,   // ⠳ U+2833 BRAILLE PATTERN DOTS-1256
    // BrailleDots356,    // ⠴ U+2834 BRAILLE PATTERN DOTS-356
    // BrailleDots1356,   // ⠵ U+2835 BRAILLE PATTERN DOTS-1356
    // BrailleDots2356,   // ⠶ U+2836 BRAILLE PATTERN DOTS-2356
    // BrailleDots12356,  // ⠷ U+2837 BRAILLE PATTERN DOTS-12356
    // BrailleDots456,    // ⠸ U+2838 BRAILLE PATTERN DOTS-456
    // BrailleDots1456,   // ⠹ U+2839 BRAILLE PATTERN DOTS-1456
    // BrailleDots2456,   // ⠺ U+283A BRAILLE PATTERN DOTS-2456
    // BrailleDots12456,  // ⠻ U+283B BRAILLE PATTERN DOTS-12456
    // BrailleDots3456,   // ⠼ U+283C BRAILLE PATTERN DOTS-3456
    // BrailleDots13456,  // ⠽ U+283D BRAILLE PATTERN DOTS-13456
    // BrailleDots23456,  // ⠾ U+283E BRAILLE PATTERN DOTS-23456
    // BrailleDots123456, // ⠿ U+283F BRAILLE PATTERN DOTS-123456

    // // Second row (with dot 7, no dot 8)
    // BrailleDots7,       // ⡀ U+2840 BRAILLE PATTERN DOTS-7
    // BrailleDots17,      // ⡁ U+2841 BRAILLE PATTERN DOTS-17
    // BrailleDots27,      // ⡂ U+2842 BRAILLE PATTERN DOTS-27
    // BrailleDots127,     // ⡃ U+2843 BRAILLE PATTERN DOTS-127
    // BrailleDots37,      // ⡄ U+2844 BRAILLE PATTERN DOTS-37
    // BrailleDots137,     // ⡅ U+2845 BRAILLE PATTERN DOTS-137
    // BrailleDots237,     // ⡆ U+2846 BRAILLE PATTERN DOTS-237
    // BrailleDots1237,    // ⡇ U+2847 BRAILLE PATTERN DOTS-1237
    // BrailleDots47,      // ⡈ U+2848 BRAILLE PATTERN DOTS-47
    // BrailleDots147,     // ⡉ U+2849 BRAILLE PATTERN DOTS-147
    // BrailleDots247,     // ⡊ U+284A BRAILLE PATTERN DOTS-247
    // BrailleDots1247,    // ⡋ U+284B BRAILLE PATTERN DOTS-1247
    // BrailleDots347,     // ⡌ U+284C BRAILLE PATTERN DOTS-347
    // BrailleDots1347,    // ⡍ U+284D BRAILLE PATTERN DOTS-1347
    // BrailleDots2347,    // ⡎ U+284E BRAILLE PATTERN DOTS-2347
    // BrailleDots12347,   // ⡏ U+284F BRAILLE PATTERN DOTS-12347
    // BrailleDots57,      // ⡐ U+2850 BRAILLE PATTERN DOTS-57
    // BrailleDots157,     // ⡑ U+2851 BRAILLE PATTERN DOTS-157
    // BrailleDots257,     // ⡒ U+2852 BRAILLE PATTERN DOTS-257
    // BrailleDots1257,    // ⡓ U+2853 BRAILLE PATTERN DOTS-1257
    // BrailleDots357,     // ⡔ U+2854 BRAILLE PATTERN DOTS-357
    // BrailleDots1357,    // ⡕ U+2855 BRAILLE PATTERN DOTS-1357
    // BrailleDots2357,    // ⡖ U+2856 BRAILLE PATTERN DOTS-2357
    // BrailleDots12357,   // ⡗ U+2857 BRAILLE PATTERN DOTS-12357
    // BrailleDots457,     // ⡘ U+2858 BRAILLE PATTERN DOTS-457
    // BrailleDots1457,    // ⡙ U+2859 BRAILLE PATTERN DOTS-1457
    // BrailleDots2457,    // ⡚ U+285A BRAILLE PATTERN DOTS-2457
    // BrailleDots12457,   // ⡛ U+285B BRAILLE PATTERN DOTS-12457
    // BrailleDots3457,    // ⡜ U+285C BRAILLE PATTERN DOTS-3457
    // BrailleDots13457,   // ⡝ U+285D BRAILLE PATTERN DOTS-13457
    // BrailleDots23457,   // ⡞ U+285E BRAILLE PATTERN DOTS-23457
    // BrailleDots123457,  // ⡟ U+285F BRAILLE PATTERN DOTS-123457
    // BrailleDots67,      // ⡠ U+2860 BRAILLE PATTERN DOTS-67
    // BrailleDots167,     // ⡡ U+2861 BRAILLE PATTERN DOTS-167
    // BrailleDots267,     // ⡢ U+2862 BRAILLE PATTERN DOTS-267
    // BrailleDots1267,    // ⡣ U+2863 BRAILLE PATTERN DOTS-1267
    // BrailleDots367,     // ⡤ U+2864 BRAILLE PATTERN DOTS-367
    // BrailleDots1367,    // ⡥ U+2865 BRAILLE PATTERN DOTS-1367
    // BrailleDots2367,    // ⡦ U+2866 BRAILLE PATTERN DOTS-2367
    // BrailleDots12367,   // ⡧ U+2867 BRAILLE PATTERN DOTS-12367
    // BrailleDots467,     // ⡨ U+2868 BRAILLE PATTERN DOTS-467
    // BrailleDots1467,    // ⡩ U+2869 BRAILLE PATTERN DOTS-1467
    // BrailleDots2467,    // ⡪ U+286A BRAILLE PATTERN DOTS-2467
    // BrailleDots12467,   // ⡫ U+286B BRAILLE PATTERN DOTS-12467
    // BrailleDots3467,    // ⡬ U+286C BRAILLE PATTERN DOTS-3467
    // BrailleDots13467,   // ⡭ U+286D BRAILLE PATTERN DOTS-13467
    // BrailleDots23467,   // ⡮ U+286E BRAILLE PATTERN DOTS-23467
    // BrailleDots123467,  // ⡯ U+286F BRAILLE PATTERN DOTS-123467
    // BrailleDots567,     // ⡰ U+2870 BRAILLE PATTERN DOTS-567
    // BrailleDots1567,    // ⡱ U+2871 BRAILLE PATTERN DOTS-1567
    // BrailleDots2567,    // ⡲ U+2872 BRAILLE PATTERN DOTS-2567
    // BrailleDots12567,   // ⡳ U+2873 BRAILLE PATTERN DOTS-12567
    // BrailleDots3567,    // ⡴ U+2874 BRAILLE PATTERN DOTS-3567
    // BrailleDots13567,   // ⡵ U+2875 BRAILLE PATTERN DOTS-13567
    // BrailleDots23567,   // ⡶ U+2876 BRAILLE PATTERN DOTS-23567
    // BrailleDots123567,  // ⡷ U+2877 BRAILLE PATTERN DOTS-123567
    // BrailleDots4567,    // ⡸ U+2878 BRAILLE PATTERN DOTS-4567
    // BrailleDots14567,   // ⡹ U+2879 BRAILLE PATTERN DOTS-14567
    // BrailleDots24567,   // ⡺ U+287A BRAILLE PATTERN DOTS-24567
    // BrailleDots124567,  // ⡻ U+287B BRAILLE PATTERN DOTS-124567
    // BrailleDots34567,   // ⡼ U+287C BRAILLE PATTERN DOTS-34567
    // BrailleDots134567,  // ⡽ U+287D BRAILLE PATTERN DOTS-134567
    // BrailleDots234567,  // ⡾ U+287E BRAILLE PATTERN DOTS-234567
    // BrailleDots1234567, // ⡿ U+287F BRAILLE PATTERN DOTS-1234567

    // // Third row (no dot 7, with dot 8)
    // BrailleDots8,       // ⢀ U+2880 BRAILLE PATTERN DOTS-8
    // BrailleDots18,      // ⢁ U+2881 BRAILLE PATTERN DOTS-18
    // BrailleDots28,      // ⢂ U+2882 BRAILLE PATTERN DOTS-28
    // BrailleDots128,     // ⢃ U+2883 BRAILLE PATTERN DOTS-128
    // BrailleDots38,      // ⢄ U+2884 BRAILLE PATTERN DOTS-38
    // BrailleDots138,     // ⢅ U+2885 BRAILLE PATTERN DOTS-138
    // BrailleDots238,     // ⢆ U+2886 BRAILLE PATTERN DOTS-238
    // BrailleDots1238,    // ⢇ U+2887 BRAILLE PATTERN DOTS-1238
    // BrailleDots48,      // ⢈ U+2888 BRAILLE PATTERN DOTS-48
    // BrailleDots148,     // ⢉ U+2889 BRAILLE PATTERN DOTS-148
    // BrailleDots248,     // ⢊ U+288A BRAILLE PATTERN DOTS-248
    // BrailleDots1248,    // ⢋ U+288B BRAILLE PATTERN DOTS-1248
    // BrailleDots348,     // ⢌ U+288C BRAILLE PATTERN DOTS-348
    // BrailleDots1348,    // ⢍ U+288D BRAILLE PATTERN DOTS-1348
    // BrailleDots2348,    // ⢎ U+288E BRAILLE PATTERN DOTS-2348
    // BrailleDots12348,   // ⢏ U+288F BRAILLE PATTERN DOTS-12348
    // BrailleDots58,      // ⢐ U+2890 BRAILLE PATTERN DOTS-58
    // BrailleDots158,     // ⢑ U+2891 BRAILLE PATTERN DOTS-158
    // BrailleDots258,     // ⢒ U+2892 BRAILLE PATTERN DOTS-258
    // BrailleDots1258,    // ⢓ U+2893 BRAILLE PATTERN DOTS-1258
    // BrailleDots358,     // ⢔ U+2894 BRAILLE PATTERN DOTS-358
    // BrailleDots1358,    // ⢕ U+2895 BRAILLE PATTERN DOTS-1358
    // BrailleDots2358,    // ⢖ U+2896 BRAILLE PATTERN DOTS-2358
    // BrailleDots12358,   // ⢗ U+2897 BRAILLE PATTERN DOTS-12358
    // BrailleDots458,     // ⢘ U+2898 BRAILLE PATTERN DOTS-458
    // BrailleDots1458,    // ⢙ U+2899 BRAILLE PATTERN DOTS-1458
    // BrailleDots2458,    // ⢚ U+289A BRAILLE PATTERN DOTS-2458
    // BrailleDots12458,   // ⢛ U+289B BRAILLE PATTERN DOTS-12458
    // BrailleDots3458,    // ⢜ U+289C BRAILLE PATTERN DOTS-3458
    // BrailleDots13458,   // ⢝ U+289D BRAILLE PATTERN DOTS-13458
    // BrailleDots23458,   // ⢞ U+289E BRAILLE PATTERN DOTS-23458
    // BrailleDots123458,  // ⢟ U+289F BRAILLE PATTERN DOTS-123458
    // BrailleDots68,      // ⢠ U+28A0 BRAILLE PATTERN DOTS-68
    // BrailleDots168,     // ⢡ U+28A1 BRAILLE PATTERN DOTS-168
    // BrailleDots268,     // ⢢ U+28A2 BRAILLE PATTERN DOTS-268
    // BrailleDots1268,    // ⢣ U+28A3 BRAILLE PATTERN DOTS-1268
    // BrailleDots368,     // ⢤ U+28A4 BRAILLE PATTERN DOTS-368
    // BrailleDots1368,    // ⢥ U+28A5 BRAILLE PATTERN DOTS-1368
    // BrailleDots2368,    // ⢦ U+28A6 BRAILLE PATTERN DOTS-2368
    // BrailleDots12368,   // ⢧ U+28A7 BRAILLE PATTERN DOTS-12368
    // BrailleDots468,     // ⢨ U+28A8 BRAILLE PATTERN DOTS-468
    // BrailleDots1468,    // ⢩ U+28A9 BRAILLE PATTERN DOTS-1468
    // BrailleDots2468,    // ⢪ U+28AA BRAILLE PATTERN DOTS-2468
    // BrailleDots12468,   // ⢫ U+28AB BRAILLE PATTERN DOTS-12468
    // BrailleDots3468,    // ⢬ U+28AC BRAILLE PATTERN DOTS-3468
    // BrailleDots13468,   // ⢭ U+28AD BRAILLE PATTERN DOTS-13468
    // BrailleDots23468,   // ⢮ U+28AE BRAILLE PATTERN DOTS-23468
    // BrailleDots123468,  // ⢯ U+28AF BRAILLE PATTERN DOTS-123468
    // BrailleDots568,     // ⢰ U+28B0 BRAILLE PATTERN DOTS-568
    // BrailleDots1568,    // ⢱ U+28B1 BRAILLE PATTERN DOTS-1568
    // BrailleDots2568,    // ⢲ U+28B2 BRAILLE PATTERN DOTS-2568
    // BrailleDots12568,   // ⢳ U+28B3 BRAILLE PATTERN DOTS-12568
    // BrailleDots3568,    // ⢴ U+28B4 BRAILLE PATTERN DOTS-3568
    // BrailleDots13568,   // ⢵ U+28B5 BRAILLE PATTERN DOTS-13568
    // BrailleDots23568,   // ⢶ U+28B6 BRAILLE PATTERN DOTS-23568
    // BrailleDots123568,  // ⢷ U+28B7 BRAILLE PATTERN DOTS-123568
    // BrailleDots4568,    // ⢸ U+28B8 BRAILLE PATTERN DOTS-4568
    // BrailleDots14568,   // ⢹ U+28B9 BRAILLE PATTERN DOTS-14568
    // BrailleDots24568,   // ⢺ U+28BA BRAILLE PATTERN DOTS-24568
    // BrailleDots124568,  // ⢻ U+28BB BRAILLE PATTERN DOTS-124568
    // BrailleDots34568,   // ⢼ U+28BC BRAILLE PATTERN DOTS-34568
    // BrailleDots134568,  // ⢽ U+28BD BRAILLE PATTERN DOTS-134568
    // BrailleDots234568,  // ⢾ U+28BE BRAILLE PATTERN DOTS-234568
    // BrailleDots1234568, // ⢿ U+28BF BRAILLE PATTERN DOTS-1234568

    // // Fourth row (with dot 7, with dot 8)
    // BrailleDots78,      // ⣀ U+28C0 BRAILLE PATTERN DOTS-78
    // BrailleDots178,     // ⣁ U+28C1 BRAILLE PATTERN DOTS-178
    // BrailleDots278,     // ⣂ U+28C2 BRAILLE PATTERN DOTS-278
    // BrailleDots1278,    // ⣃ U+28C3 BRAILLE PATTERN DOTS-1278
    // BrailleDots378,     // ⣄ U+28C4 BRAILLE PATTERN DOTS-378
    // BrailleDots1378,    // ⣅ U+28C5 BRAILLE PATTERN DOTS-1378
    // BrailleDots2378,    // ⣆ U+28C6 BRAILLE PATTERN DOTS-2378
    // BrailleDots12378,   // ⣇ U+28C7 BRAILLE PATTERN DOTS-12378
    // BrailleDots478,     // ⣈ U+28C8 BRAILLE PATTERN DOTS-478
    // BrailleDots1478,    // ⣉ U+28C9 BRAILLE PATTERN DOTS-1478
    // BrailleDots2478,    // ⣊ U+28CA BRAILLE PATTERN DOTS-2478
    // BrailleDots12478,   // ⣋ U+28CB BRAILLE PATTERN DOTS-12478
    // BrailleDots3478,    // ⣌ U+28CC BRAILLE PATTERN DOTS-3478
    // BrailleDots13478,   // ⣍ U+28CD BRAILLE PATTERN DOTS-13478
    // BrailleDots23478,   // ⣎ U+28CE BRAILLE PATTERN DOTS-23478
    // BrailleDots123478,  // ⣏ U+28CF BRAILLE PATTERN DOTS-123478
    // BrailleDots578,     // ⣐ U+28D0 BRAILLE PATTERN DOTS-578
    // BrailleDots1578,    // ⣑ U+28D1 BRAILLE PATTERN DOTS-1578
    // BrailleDots2578,    // ⣒ U+28D2 BRAILLE PATTERN DOTS-2578
    // BrailleDots12578,   // ⣓ U+28D3 BRAILLE PATTERN DOTS-12578
    // BrailleDots3578,    // ⣔ U+28D4 BRAILLE PATTERN DOTS-3578
    // BrailleDots13578,   // ⣕ U+28D5 BRAILLE PATTERN DOTS-13578
    // BrailleDots23578,   // ⣖ U+28D6 BRAILLE PATTERN DOTS-23578
    // BrailleDots123578,  // ⣗ U+28D7 BRAILLE PATTERN DOTS-123578
    // BrailleDots4578,    // ⣘ U+28D8 BRAILLE PATTERN DOTS-4578
    // BrailleDots14578,   // ⣙ U+28D9 BRAILLE PATTERN DOTS-14578
    // BrailleDots24578,   // ⣚ U+28DA BRAILLE PATTERN DOTS-24578
    // BrailleDots124578,  // ⣛ U+28DB BRAILLE PATTERN DOTS-124578
    // BrailleDots34578,   // ⣜ U+28DC BRAILLE PATTERN DOTS-34578
    // BrailleDots134578,  // ⣝ U+28DD BRAILLE PATTERN DOTS-134578
    // BrailleDots234578,  // ⣞ U+28DE BRAILLE PATTERN DOTS-234578
    // BrailleDots1234578, // ⣟ U+28DF BRAILLE PATTERN DOTS-1234578
    // BrailleDots678,     // ⣠ U+28E0 BRAILLE PATTERN DOTS-678
    // BrailleDots1678,    // ⣡ U+28E1 BRAILLE PATTERN DOTS-1678
    // BrailleDots2678,    // ⣢ U+28E2 BRAILLE PATTERN DOTS-2678
    // BrailleDots12678,   // ⣣ U+28E3 BRAILLE PATTERN DOTS-12678
    // BrailleDots3678,    // ⣤ U+28E4 BRAILLE PATTERN DOTS-3678
    // BrailleDots13678,   // ⣥ U+28E5 BRAILLE PATTERN DOTS-13678
    // BrailleDots23678,   // ⣦ U+28E6 BRAILLE PATTERN DOTS-23678
    // BrailleDots123678,  // ⣧ U+28E7 BRAILLE PATTERN DOTS-123678
    // BrailleDots4678,    // ⣨ U+28E8 BRAILLE PATTERN DOTS-4678
    // BrailleDots14678,   // ⣩ U+28E9 BRAILLE PATTERN DOTS-14678
    // BrailleDots24678,   // ⣪ U+28EA BRAILLE PATTERN DOTS-24678
    // BrailleDots124678,  // ⣫ U+28EB BRAILLE PATTERN DOTS-124678
    // BrailleDots34678,   // ⣬ U+28EC BRAILLE PATTERN DOTS-34678
    // BrailleDots134678,  // ⣭ U+28ED BRAILLE PATTERN DOTS-134678
    // BrailleDots234678,  // ⣮ U+28EE BRAILLE PATTERN DOTS-234678
    // BrailleDots1234678, // ⣯ U+28EF BRAILLE PATTERN DOTS-1234678
    // BrailleDots5678,    // ⣰ U+28F0 BRAILLE PATTERN DOTS-5678
    // BrailleDots15678,   // ⣱ U+28F1 BRAILLE PATTERN DOTS-15678
    // BrailleDots25678,   // ⣲ U+28F2 BRAILLE PATTERN DOTS-25678
    // BrailleDots125678,  // ⣳ U+28F3 BRAILLE PATTERN DOTS
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
            _ => return Err(val),
        };
        Ok(drawbable_char)
    }
}
