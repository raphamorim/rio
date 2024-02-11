// use std::ops::Range;
use crate::SugarBlock;
use crate::sugarloaf::SugarloafLayout;
use crate::Sugar;

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct SugarTreeChange {
    pub line: usize,
    pub column: usize,
    pub before: Sugar,
    pub after: Sugar,
    // range: Range<usize>,
    // content: Vec<char>,
}

#[derive(Debug, PartialEq)]
pub enum SugarTreeDiff {
    Equal,
    CurrentTreeWasEmpty,
    LineLengthIsDifferent(i32),
    ColumnsLengthIsDifferent(i32),
    WidthIsDifferent,
    HeightIsDifferent,
    ScaleIsDifferent,
    MarginIsDifferent,
    LayoutIsDifferent,
    Changes(Vec<SugarTreeChange>),
}

// const LINE_MAX_LINES: usize = 140;

#[derive(Clone)]
pub struct SugarTree {
    pub inner: Vec<SugarBlock>,
    pub layout: SugarloafLayout,
}

impl Default for SugarTree {
    fn default() -> Self {
        Self {
            inner: Vec::with_capacity(400),
            layout: SugarloafLayout::default(),
        }
    }
}

// impl Default for SugarTree {
//     fn default() -> Self {
//         // let inner = {
//         //     // Create an array of uninitialized values.
//         //     let mut array: [MaybeUninit<SugarBlock>; LINE_MAX_LINES] =
//         //         unsafe { MaybeUninit::uninit().assume_init() };

//         //     for element in array.iter_mut() {
//         //         *element = MaybeUninit::new(SugarBlock::default());
//         //     }

//         //     unsafe { std::mem::transmute::<_, [SugarBlock; LINE_MAX_LINES]>(array) }
//         // };
//         let mut inner_vec: Vec<SugarBlock> = Vec::with_capacity(LINE_MAX_LINES);
//         for _i in 0..LINE_MAX_LINES {
//             inner_vec.push(SugarBlock::default());
//         }

//         Self {
//             // hash: 00000000000000,
//             inner: inner_vec.try_into().unwrap(),
//             len: 0,
//             cursor_position: (0, 0),
//         }
//     }
// }

impl SugarTree {
    #[inline]
    pub fn calculate_diff(&self, next: &SugarTree) -> SugarTreeDiff {
        if self.layout != next.layout {
            return SugarTreeDiff::LayoutIsDifferent;
        }

        let current_len = self.inner.len();
        let next_len = next.len();
        let mut changes: Vec<SugarTreeChange> = vec![];

        if current_len == next_len {
            for line_number in 0..current_len {
                let line: SugarBlock = self.inner[line_number];
                let next_line: SugarBlock = next.inner[line_number];
                if line.len != next_line.len {
                    return SugarTreeDiff::ColumnsLengthIsDifferent(
                        line.len as i32 - next_line.len as i32,
                    );
                }

                for column in 0..line.acc {
                    if line[column] != next_line[column] {
                        changes.push(SugarTreeChange {
                            line: line_number,
                            column: column,
                            before: line[column],
                            after: next_line[column],
                        });
                    }
                }
            }

            if !changes.is_empty() {
                return SugarTreeDiff::Changes(changes);
            }
        } else {
            return SugarTreeDiff::LineLengthIsDifferent(
                current_len as i32 - next_len as i32,
            );
        }

        SugarTreeDiff::Equal
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[inline]
    pub fn insert(&mut self, id: usize, content: SugarBlock) {
        self.inner.insert(id, content);
    }

    #[inline]
    pub fn insert_last(&mut self, content: SugarBlock) {
        self.inner.insert(self.inner.len(), content);
    }

    #[inline]
    pub fn line_mut(&mut self, line_number: usize) -> &mut SugarBlock {
        &mut self.inner[line_number]
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::Sugar;
    use crate::SugarDecoration::Disabled;
    use crate::SugarStyle;

    #[test]
    fn test_sugartree_calculate_is_empty() {
        let sugartree_a = SugarTree::default();
        let sugartree_b = SugarTree::default();

        assert!(sugartree_a.is_empty());
        assert!(sugartree_b.is_empty());
    }

    #[test]
    fn test_sugartree_calculate_diff_no_changes() {
        let mut sugartree_a = SugarTree::default();
        let mut sugartree_b = SugarTree::default();

        assert_eq!(
            sugartree_a.calculate_diff(&sugartree_b),
            SugarTreeDiff::Equal
        );

        sugartree_a.insert(0, SugarBlock::default());
        sugartree_a.line_mut(0).insert(Sugar {
            content: 'b',
            ..Sugar::default()
        });

        sugartree_b.insert(0, SugarBlock::default());
        sugartree_b.line_mut(0).insert(Sugar {
            content: 'b',
            ..Sugar::default()
        });

        assert_eq!(
            sugartree_a.calculate_diff(&sugartree_b),
            SugarTreeDiff::Equal
        );

        sugartree_a.layout.width = 300.0;

        assert_eq!(
            sugartree_a.calculate_diff(&sugartree_b),
            SugarTreeDiff::LayoutIsDifferent
        );

        // sugartree_a.width = 0.0;
        // sugartree_a.height = 100.0;

        // assert_eq!(
        //     sugartree_a.calculate_diff(&sugartree_b),
        //     SugarTreeDiff::HeightIsDifferent
        // );
    }

    #[test]
    fn test_sugartree_insert_last() {
        let mut sugartree_a = SugarTree::default();

        assert_eq!(sugartree_a.len(), 0);

        sugartree_a.insert_last(SugarBlock::default());

        assert_eq!(sugartree_a.len(), 1);

        sugartree_a.insert_last(SugarBlock::default());

        assert_eq!(sugartree_a.len(), 2);
    }

    #[test]
    fn test_sugartree_calculate_diff_lines_length_is_different() {
        let mut sugartree_a = SugarTree::default();
        let mut sugartree_b = SugarTree::default();

        sugartree_a.insert(0, SugarBlock::default());

        assert_eq!(
            sugartree_a.calculate_diff(&sugartree_b),
            SugarTreeDiff::LineLengthIsDifferent(1)
        );

        sugartree_a.insert(1, SugarBlock::default());

        assert_eq!(
            sugartree_a.calculate_diff(&sugartree_b),
            SugarTreeDiff::LineLengthIsDifferent(2)
        );

        sugartree_b.insert(0, SugarBlock::default());
        sugartree_b.insert(1, SugarBlock::default());
        sugartree_b.insert(2, SugarBlock::default());

        assert_eq!(
            sugartree_a.calculate_diff(&sugartree_b),
            SugarTreeDiff::LineLengthIsDifferent(-1)
        );
    }

    #[test]
    fn test_sugartree_calculate_diff_columns_length_is_different() {
        let mut sugartree_a = SugarTree::default();
        let mut sugartree_b = SugarTree::default();

        sugartree_a.insert(0, SugarBlock::default());
        sugartree_a.line_mut(0).insert(Sugar {
            content: 'b',
            ..Sugar::default()
        });

        sugartree_b.insert(0, SugarBlock::default());
        sugartree_b.line_mut(0).insert(Sugar {
            content: 'b',
            ..Sugar::default()
        });

        sugartree_b.line_mut(0).insert(Sugar {
            content: 'a',
            ..Sugar::default()
        });

        assert_eq!(
            sugartree_a.calculate_diff(&sugartree_b),
            SugarTreeDiff::ColumnsLengthIsDifferent(-1)
        );

        sugartree_b.line_mut(0).insert(Sugar {
            content: 'c',
            ..Sugar::default()
        });

        assert_eq!(
            sugartree_a.calculate_diff(&sugartree_b),
            SugarTreeDiff::ColumnsLengthIsDifferent(-2)
        );

        sugartree_a.line_mut(0).insert(Sugar {
            content: 'z',
            ..Sugar::default()
        });
        sugartree_a.line_mut(0).insert(Sugar {
            content: 't',
            ..Sugar::default()
        });
        sugartree_a.line_mut(0).insert(Sugar {
            content: 'o',
            ..Sugar::default()
        });

        assert_eq!(
            sugartree_a.calculate_diff(&sugartree_b),
            SugarTreeDiff::ColumnsLengthIsDifferent(1)
        );
    }

    #[test]
    fn test_sugartree_calculate_diff_chages() {
        let mut sugartree_a = SugarTree::default();
        let mut sugartree_b = SugarTree::default();

        sugartree_a.insert(0, SugarBlock::default());
        sugartree_a.line_mut(0).insert(Sugar {
            content: 'a',
            ..Sugar::default()
        });

        sugartree_b.insert(0, SugarBlock::default());
        sugartree_b.line_mut(0).insert(Sugar {
            content: 'b',
            ..Sugar::default()
        });

        let mut changes = vec![SugarTreeChange {
            line: 0,
            column: 0,
            before: Sugar {
                content: 'a',
                foreground_color: [0.0, 0.0, 0.0, 0.0],
                background_color: [0.0, 0.0, 0.0, 0.0],
                style: SugarStyle {
                    is_italic: false,
                    is_bold: false,
                    is_bold_italic: false,
                },
                repeated: 0,
                decoration: Disabled,
                cursor: crate::SugarCursor::Disabled,
                media: None,
            },
            after: Sugar {
                content: 'b',
                foreground_color: [0.0, 0.0, 0.0, 0.0],
                background_color: [0.0, 0.0, 0.0, 0.0],
                style: SugarStyle {
                    is_italic: false,
                    is_bold: false,
                    is_bold_italic: false,
                },
                repeated: 0,
                decoration: Disabled,
                cursor: crate::SugarCursor::Disabled,
                media: None,
            },
        }];

        assert_eq!(
            sugartree_a.calculate_diff(&sugartree_b),
            SugarTreeDiff::Changes(changes.clone())
        );

        sugartree_a.line_mut(0).insert(Sugar {
            content: 'k',
            ..Sugar::default()
        });

        sugartree_b.line_mut(0).insert(Sugar {
            content: 'z',
            ..Sugar::default()
        });

        changes.push(SugarTreeChange {
            line: 0,
            column: 1,
            before: Sugar {
                content: 'k',
                foreground_color: [0.0, 0.0, 0.0, 0.0],
                background_color: [0.0, 0.0, 0.0, 0.0],
                style: SugarStyle {
                    is_italic: false,
                    is_bold: false,
                    is_bold_italic: false,
                },
                repeated: 0,
                decoration: Disabled,
                cursor: crate::SugarCursor::Disabled,
                media: None,
            },
            after: Sugar {
                content: 'z',
                foreground_color: [0.0, 0.0, 0.0, 0.0],
                background_color: [0.0, 0.0, 0.0, 0.0],
                style: SugarStyle {
                    is_italic: false,
                    is_bold: false,
                    is_bold_italic: false,
                },
                repeated: 0,
                decoration: Disabled,
                cursor: crate::SugarCursor::Disabled,
                media: None,
            },
        });

        assert_eq!(
            sugartree_a.calculate_diff(&sugartree_b),
            SugarTreeDiff::Changes(changes)
        );
    }
}
