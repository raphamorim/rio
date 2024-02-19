// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

// use std::ops::Range;
use crate::text::Text;
use crate::text_area::TextArea;
use crate::rectangle::Rectangle;
use crate::sugarloaf::SugarloafLayout;

// use smallvec::SmallVec;

#[derive(Debug, PartialEq)]
pub enum SugarTreeDiff {
    Equal,
    Different,
    LineLengthIsDifferent(i32),
    ColumnsLengthIsDifferent(i32),
    WidthIsDifferent,
    HeightIsDifferent,
    ScaleIsDifferent,
    MarginIsDifferent,
    LayoutIsDifferent,
    Changes(Vec<usize>),
}

pub struct SugarTree {
    pub text_areas: Vec<TextArea>,
    pub rectangles: Vec<Rectangle>,
    pub texts: Vec<Text>,
    pub layout: SugarloafLayout,
}

impl Default for SugarTree {
    fn default() -> Self {
        Self {
            text_areas: Vec::with_capacity(9),
            rectangles: Vec::with_capacity(100),
            texts: Vec::with_capacity(100),
            layout: SugarloafLayout::default(),
        }
    }
}

impl SugarTree {
    // #[inline]
    // pub fn calculate_diff(&self, next: &SugarTree) -> SugarTreeDiff {
    //     if self.layout != next.layout {
    //         // In layout case, doesn't matter if blocks are different
    //         // or texts are different, it will repaint everything
    //         return SugarTreeDiff::LayoutIsDifferent;
    //     }

    //     let current_len = self.lines.len();
    //     let next_len = next.len();
    //     let mut changes: Vec<Diff> = vec![];

    //     // TODO: Improve blocks comparisson
    //     if self.blocks != next.blocks {
    //         changes.push(Diff::block());
    //     }

    //     if current_len == next_len {
    //         for line_number in 0..current_len {
    //             let line: &SugarLine = &self.lines[line_number];
    //             let next_line: &SugarLine = &next.lines[line_number];
    //             if line.len() != next_line.len() {
    //                 return SugarTreeDiff::ColumnsLengthIsDifferent(
    //                     line.len() as i32 - next_line.len() as i32,
    //                 );
    //             }

    //             for column in 0..line.acc {
    //                 if line[column] != next_line[column] {
    //                     changes.push(Diff {
    //                         kind: DiffKind::Sugar,
    //                         line: line_number,
    //                         column,
    //                         before: line[column],
    //                         after: next_line[column],
    //                     });
    //                 }
    //             }
    //         }

    //         if !changes.is_empty() {
    //             return SugarTreeDiff::Changes(changes);
    //         }
    //     } else {
    //         return SugarTreeDiff::LineLengthIsDifferent(
    //             current_len as i32 - next_len as i32,
    //         );
    //     }

    //     SugarTreeDiff::Equal
    // }

    #[inline]
    pub fn calculate_diff(&self, next: &SugarTree) -> SugarTreeDiff {
        if self.layout != next.layout {
            // In layout case, doesn't matter if blocks are different
            // or texts are different, it will repaint everything
            return SugarTreeDiff::LayoutIsDifferent;
        }

        let mut changes: Vec<usize> = vec![];

        if self.texts.len() != next.texts.len() {
            return SugarTreeDiff::Different;
        }

        for (idx, text) in self.texts.iter().enumerate() {
            if text != &next.texts[idx] {
               changes.push(idx); 
            }
        }

        if self.rectangles.len() != next.rectangles.len() {
            return SugarTreeDiff::Different;
        }

        for (idx, rectangle) in self.rectangles.iter().enumerate() {
            if rectangle != &next.rectangles[idx] {
               changes.push(idx); 
            }
        }

        if self.text_areas.len() != next.text_areas.len() {
            return SugarTreeDiff::Different;
        }

        for (idx, text_area) in self.text_areas.iter().enumerate() {
            if text_area != &next.text_areas[idx] {
               changes.push(idx); 
            }
        }

        if !changes.is_empty() {
            return SugarTreeDiff::Changes(changes);
        }

        SugarTreeDiff::Equal
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.text_areas.len() + self.rectangles.len() + self.texts.len()
    }

    #[inline]
    pub fn clear(&mut self) {
        self.text_areas.clear();
        self.rectangles.clear();
        self.texts.clear();
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
pub mod test {
    use super::*;
    // use crate::SugarDecoration::Disabled;
    // use crate::SugarStyle;

    // #[test]
    // fn test_sugartree_calculate_is_empty() {
    //     let sugartree_a = SugarTree::default();
    //     let sugartree_b = SugarTree::default();

    //     assert!(sugartree_a.is_empty());
    //     assert!(sugartree_b.is_empty());
    // }

    // #[test]
    // fn test_sugartree_calculate_diff_no_changes() {
    //     let mut sugartree_a = SugarTree::default();
    //     let mut sugartree_b = SugarTree::default();

    //     assert_eq!(
    //         sugartree_a.calculate_diff(&sugartree_b),
    //         SugarTreeDiff::Equal
    //     );

    //     sugartree_a.insert(0, SugarLine::default());
    //     sugartree_a.line_mut(0).insert(&Sugar {
    //         content: 'b',
    //         ..Sugar::default()
    //     });

    //     sugartree_b.insert(0, SugarLine::default());
    //     sugartree_b.line_mut(0).insert(&Sugar {
    //         content: 'b',
    //         ..Sugar::default()
    //     });

    //     assert_eq!(
    //         sugartree_a.calculate_diff(&sugartree_b),
    //         SugarTreeDiff::Equal
    //     );

    //     sugartree_a.layout.width = 300.0;

    //     assert_eq!(
    //         sugartree_a.calculate_diff(&sugartree_b),
    //         SugarTreeDiff::LayoutIsDifferent
    //     );

    //     // sugartree_a.width = 0.0;
    //     // sugartree_a.height = 100.0;

    //     // assert_eq!(
    //     //     sugartree_a.calculate_diff(&sugartree_b),
    //     //     SugarTreeDiff::HeightIsDifferent
    //     // );
    // }

    // #[test]
    // fn test_sugartree_insert_last() {
    //     let mut sugartree_a = SugarTree::default();

    //     assert_eq!(sugartree_a.len(), 0);

    //     sugartree_a.insert_last(SugarLine::default());

    //     assert_eq!(sugartree_a.len(), 1);

    //     sugartree_a.insert_last(SugarLine::default());

    //     assert_eq!(sugartree_a.len(), 2);
    // }

    // #[test]
    // fn test_sugartree_calculate_diff_lines_length_is_different() {
    //     let mut sugartree_a = SugarTree::default();
    //     let mut sugartree_b = SugarTree::default();

    //     sugartree_a.insert(0, SugarLine::default());

    //     assert_eq!(
    //         sugartree_a.calculate_diff(&sugartree_b),
    //         SugarTreeDiff::LineLengthIsDifferent(1)
    //     );

    //     sugartree_a.insert(1, SugarLine::default());

    //     assert_eq!(
    //         sugartree_a.calculate_diff(&sugartree_b),
    //         SugarTreeDiff::LineLengthIsDifferent(2)
    //     );

    //     sugartree_b.insert(0, SugarLine::default());
    //     sugartree_b.insert(1, SugarLine::default());
    //     sugartree_b.insert(2, SugarLine::default());

    //     assert_eq!(
    //         sugartree_a.calculate_diff(&sugartree_b),
    //         SugarTreeDiff::LineLengthIsDifferent(-1)
    //     );
    // }

    // #[test]
    // fn test_sugartree_calculate_diff_columns_length_is_different() {
    //     let mut sugartree_a = SugarTree::default();
    //     let mut sugartree_b = SugarTree::default();

    //     sugartree_a.insert(0, SugarLine::default());
    //     sugartree_a.line_mut(0).insert(&Sugar {
    //         content: 'b',
    //         ..Sugar::default()
    //     });

    //     sugartree_b.insert(0, SugarLine::default());
    //     sugartree_b.line_mut(0).insert(&Sugar {
    //         content: 'b',
    //         ..Sugar::default()
    //     });

    //     sugartree_b.line_mut(0).insert(&Sugar {
    //         content: 'a',
    //         ..Sugar::default()
    //     });

    //     assert_eq!(
    //         sugartree_a.calculate_diff(&sugartree_b),
    //         SugarTreeDiff::ColumnsLengthIsDifferent(-1)
    //     );

    //     sugartree_b.line_mut(0).insert(&Sugar {
    //         content: 'c',
    //         ..Sugar::default()
    //     });

    //     assert_eq!(
    //         sugartree_a.calculate_diff(&sugartree_b),
    //         SugarTreeDiff::ColumnsLengthIsDifferent(-2)
    //     );

    //     sugartree_a.line_mut(0).insert(&Sugar {
    //         content: 'z',
    //         ..Sugar::default()
    //     });
    //     sugartree_a.line_mut(0).insert(&Sugar {
    //         content: 't',
    //         ..Sugar::default()
    //     });
    //     sugartree_a.line_mut(0).insert(&Sugar {
    //         content: 'o',
    //         ..Sugar::default()
    //     });

    //     assert_eq!(
    //         sugartree_a.calculate_diff(&sugartree_b),
    //         SugarTreeDiff::ColumnsLengthIsDifferent(1)
    //     );
    // }

    // #[test]
    // fn test_sugartree_calculate_diff_chages() {
    //     let mut sugartree_a = SugarTree::default();
    //     let mut sugartree_b = SugarTree::default();

    //     sugartree_a.insert(0, SugarLine::default());
    //     sugartree_a.line_mut(0).insert(&Sugar {
    //         content: 'a',
    //         ..Sugar::default()
    //     });

    //     sugartree_b.insert(0, SugarLine::default());
    //     sugartree_b.line_mut(0).insert(&Sugar {
    //         content: 'b',
    //         ..Sugar::default()
    //     });

    //     let mut changes = vec![Diff {
    //         kind: DiffKind::Sugar,
    //         line: 0,
    //         column: 0,
    //         before: Sugar {
    //             content: 'a',
    //             foreground_color: [0.0, 0.0, 0.0, 0.0],
    //             background_color: [0.0, 0.0, 0.0, 0.0],
    //             style: SugarStyle {
    //                 is_italic: false,
    //                 is_bold: false,
    //                 is_bold_italic: false,
    //             },
    //             repeated: 0,
    //             decoration: Disabled,
    //             cursor: crate::SugarCursor::Disabled,
    //             media: None,
    //         },
    //         after: Sugar {
    //             content: 'b',
    //             foreground_color: [0.0, 0.0, 0.0, 0.0],
    //             background_color: [0.0, 0.0, 0.0, 0.0],
    //             style: SugarStyle {
    //                 is_italic: false,
    //                 is_bold: false,
    //                 is_bold_italic: false,
    //             },
    //             repeated: 0,
    //             decoration: Disabled,
    //             cursor: crate::SugarCursor::Disabled,
    //             media: None,
    //         },
    //     }];

    //     assert_eq!(
    //         sugartree_a.calculate_diff(&sugartree_b),
    //         SugarTreeDiff::Changes(changes.clone())
    //     );

    //     sugartree_a.line_mut(0).insert(&Sugar {
    //         content: 'k',
    //         ..Sugar::default()
    //     });

    //     sugartree_b.line_mut(0).insert(&Sugar {
    //         content: 'z',
    //         ..Sugar::default()
    //     });

    //     changes.push(Diff {
    //         kind: DiffKind::Sugar,
    //         line: 0,
    //         column: 1,
    //         before: Sugar {
    //             content: 'k',
    //             foreground_color: [0.0, 0.0, 0.0, 0.0],
    //             background_color: [0.0, 0.0, 0.0, 0.0],
    //             style: SugarStyle {
    //                 is_italic: false,
    //                 is_bold: false,
    //                 is_bold_italic: false,
    //             },
    //             repeated: 0,
    //             decoration: Disabled,
    //             cursor: crate::SugarCursor::Disabled,
    //             media: None,
    //         },
    //         after: Sugar {
    //             content: 'z',
    //             foreground_color: [0.0, 0.0, 0.0, 0.0],
    //             background_color: [0.0, 0.0, 0.0, 0.0],
    //             style: SugarStyle {
    //                 is_italic: false,
    //                 is_bold: false,
    //                 is_bold_italic: false,
    //             },
    //             repeated: 0,
    //             decoration: Disabled,
    //             cursor: crate::SugarCursor::Disabled,
    //             media: None,
    //         },
    //     });

    //     assert_eq!(
    //         sugartree_a.calculate_diff(&sugartree_b),
    //         SugarTreeDiff::Changes(changes)
    //     );
    // }
}
