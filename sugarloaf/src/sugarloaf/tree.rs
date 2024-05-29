// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use crate::sugarloaf::SugarloafLayout;
use crate::{Sugar, SugarBlock, SugarLine};

#[derive(Debug, Default, Copy, Clone, PartialEq)]
pub struct DiffChar {
    pub line: usize,
    pub column: usize,
    pub before: Sugar,
    pub after: Sugar,
}

#[derive(Debug, Default, Copy, Clone, PartialEq)]
pub struct DiffLine {
    pub line: usize,
    pub before: usize,
    pub after: usize,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Diff {
    Char(DiffChar),
    // (previous size, next size)
    Line(DiffLine),
    Hash(bool),
}

#[derive(Debug, PartialEq)]
pub enum SugarTreeDiff {
    Equal,
    Different,
    BlocksAreDifferent,
    LineQuantity(i32),
    LayoutIsDifferent,
    Changes(Vec<Diff>),
}

#[derive(Clone)]
pub struct SugarTree {
    pub lines: Vec<SugarLine>,
    pub blocks: Vec<SugarBlock>,
    pub layout: SugarloafLayout,
}

impl Default for SugarTree {
    fn default() -> Self {
        Self {
            lines: Vec::new(),
            blocks: Vec::with_capacity(20),
            layout: SugarloafLayout::default(),
        }
    }
}

impl SugarTree {
    #[inline]
    pub fn calculate_diff(
        &self,
        next: &SugarTree,
        exact: bool,
        is_dirty: bool,
    ) -> SugarTreeDiff {
        if self.layout != next.layout {
            // In layout case, doesn't matter if blocks are different
            // or texts are different, it will repaint everything
            return SugarTreeDiff::LayoutIsDifferent;
        }

        if self.blocks != next.blocks {
            return SugarTreeDiff::BlocksAreDifferent;
        }

        if is_dirty {
            return SugarTreeDiff::Different;
        }

        let current_len = self.lines.len();
        let next_len = next.len();
        let mut changes: Vec<Diff> = vec![];

        if current_len == next_len {
            for line_number in 0..current_len {
                let line: &SugarLine = &self.lines[line_number];
                let next_line: &SugarLine = &next.lines[line_number];

                // .width stands for column size and .len() sugar elements
                // this needs to be differenciated
                // if line.width != next_line.width {
                //     return SugarTreeDiff::ColumnsLengthIsDifferent(
                //         line.width as i32 - next_line.width as i32,
                //     );
                // }

                if line.len() != next_line.len() {
                    changes.push(Diff::Line(DiffLine {
                        line: line_number,
                        before: line.len(),
                        after: next_line.len(),
                    }));
                } else if line.hash_key() != next_line.hash_key() {
                    if !exact {
                        changes.push(Diff::Hash(true));
                        break;
                    } else {
                        for column in 0..line.len() {
                            if line[column] != next_line[column] {
                                changes.push(Diff::Char(DiffChar {
                                    line: line_number,
                                    column,
                                    before: line[column],
                                    after: next_line[column],
                                }));
                            }
                        }
                    }
                }
            }

            if !changes.is_empty() {
                return SugarTreeDiff::Changes(changes);
            }
        } else {
            return SugarTreeDiff::LineQuantity(current_len as i32 - next_len as i32);
        }

        SugarTreeDiff::Equal
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    #[inline]
    pub fn insert(&mut self, id: usize, content: SugarLine) {
        self.lines.insert(id, content);
    }

    #[inline]
    pub fn insert_last(&mut self, content: SugarLine) {
        self.lines.insert(self.lines.len(), content);
    }

    #[inline]
    pub fn line_mut(&mut self, line_number: usize) -> &mut SugarLine {
        &mut self.lines[line_number]
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::SugarDecoration::Disabled;
    use crate::{Sugar, SugarCursor, SugarStyle};

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
            sugartree_a.calculate_diff(&sugartree_b, true, false),
            SugarTreeDiff::Equal
        );

        sugartree_a.insert(0, SugarLine::default());
        sugartree_a.line_mut(0).insert(&Sugar {
            content: 'b',
            ..Sugar::default()
        });

        sugartree_b.insert(0, SugarLine::default());
        sugartree_b.line_mut(0).insert(&Sugar {
            content: 'b',
            ..Sugar::default()
        });

        assert_eq!(
            sugartree_a.calculate_diff(&sugartree_b, true, false),
            SugarTreeDiff::Equal
        );

        sugartree_a.layout.width = 300.0;

        assert_eq!(
            sugartree_a.calculate_diff(&sugartree_b, true, false),
            SugarTreeDiff::LayoutIsDifferent
        );
    }

    #[test]
    fn test_sugartree_insert_last() {
        let mut sugartree_a = SugarTree::default();

        assert_eq!(sugartree_a.len(), 0);

        sugartree_a.insert_last(SugarLine::default());

        assert_eq!(sugartree_a.len(), 1);

        sugartree_a.insert_last(SugarLine::default());

        assert_eq!(sugartree_a.len(), 2);
    }

    #[test]
    fn test_sugartree_calculate_diff_lines_length_is_different() {
        let mut sugartree_a = SugarTree::default();
        let mut sugartree_b = SugarTree::default();

        sugartree_a.insert(0, SugarLine::default());

        assert_eq!(
            sugartree_a.calculate_diff(&sugartree_b, true, false),
            SugarTreeDiff::LineQuantity(1)
        );

        sugartree_a.insert(1, SugarLine::default());

        assert_eq!(
            sugartree_a.calculate_diff(&sugartree_b, true, false),
            SugarTreeDiff::LineQuantity(2)
        );

        sugartree_b.insert(0, SugarLine::default());
        sugartree_b.insert(1, SugarLine::default());
        sugartree_b.insert(2, SugarLine::default());

        assert_eq!(
            sugartree_a.calculate_diff(&sugartree_b, true, false),
            SugarTreeDiff::LineQuantity(-1)
        );
    }

    #[test]
    fn test_sugartree_calculate_diff_columns_length_is_different() {
        let mut sugartree_a = SugarTree::default();
        let mut sugartree_b = SugarTree::default();

        sugartree_a.insert(0, SugarLine::default());
        sugartree_a.line_mut(0).insert(&Sugar {
            content: 'b',
            ..Sugar::default()
        });

        sugartree_b.insert(0, SugarLine::default());
        sugartree_b.line_mut(0).insert(&Sugar {
            content: 'b',
            ..Sugar::default()
        });

        sugartree_b.line_mut(0).insert(&Sugar {
            content: 'a',
            ..Sugar::default()
        });

        assert_eq!(
            sugartree_a.calculate_diff(&sugartree_b, true, false),
            SugarTreeDiff::Changes(vec![Diff::Line(DiffLine {
                line: 0,
                before: 1,
                after: 2,
            })])
        );

        sugartree_b.line_mut(0).insert(&Sugar {
            content: 'c',
            ..Sugar::default()
        });

        assert_eq!(
            sugartree_a.calculate_diff(&sugartree_b, true, false),
            SugarTreeDiff::Changes(vec![Diff::Line(DiffLine {
                line: 0,
                before: 1,
                after: 3,
            })])
        );

        sugartree_b.line_mut(0).insert(&Sugar {
            content: 'z',
            ..Sugar::default()
        });
        sugartree_b.line_mut(0).insert(&Sugar {
            content: 't',
            ..Sugar::default()
        });
        sugartree_b.line_mut(0).insert(&Sugar {
            content: 'o',
            ..Sugar::default()
        });

        assert_eq!(
            sugartree_a.calculate_diff(&sugartree_b, true, false),
            SugarTreeDiff::Changes(vec![Diff::Line(DiffLine {
                line: 0,
                before: 1,
                after: 6,
            })])
        );

        sugartree_a.line_mut(0).insert(&Sugar {
            content: 'z',
            ..Sugar::default()
        });

        assert_eq!(
            sugartree_a.calculate_diff(&sugartree_b, true, false),
            SugarTreeDiff::Changes(vec![Diff::Line(DiffLine {
                line: 0,
                before: 2,
                after: 6,
            })])
        );

        sugartree_a.insert(1, SugarLine::default());
        assert_eq!(
            sugartree_a.calculate_diff(&sugartree_b, true, false),
            SugarTreeDiff::LineQuantity(1)
        );
    }

    #[test]
    fn test_sugartree_calculate_diff_chages() {
        let mut sugartree_a = SugarTree::default();
        let mut sugartree_b = SugarTree::default();

        sugartree_a.insert(0, SugarLine::default());
        sugartree_a.line_mut(0).insert(&Sugar {
            content: 'a',
            ..Sugar::default()
        });

        sugartree_b.insert(0, SugarLine::default());
        sugartree_b.line_mut(0).insert(&Sugar {
            content: 'b',
            ..Sugar::default()
        });

        let mut changes = vec![Diff::Char(DiffChar {
            line: 0,
            column: 0,
            before: Sugar {
                content: 'a',
                foreground_color: [0.0, 0.0, 0.0, 0.0],
                background_color: [0.0, 0.0, 0.0, 0.0],
                style: SugarStyle::Disabled,
                repeated: 0,
                decoration: Disabled,
                cursor: SugarCursor::Disabled,
                media: None,
            },
            after: Sugar {
                content: 'b',
                foreground_color: [0.0, 0.0, 0.0, 0.0],
                background_color: [0.0, 0.0, 0.0, 0.0],
                style: SugarStyle::Disabled,
                repeated: 0,
                decoration: Disabled,
                cursor: SugarCursor::Disabled,
                media: None,
            },
        })];

        assert_eq!(
            sugartree_a.calculate_diff(&sugartree_b, true, false),
            SugarTreeDiff::Changes(changes.clone())
        );

        sugartree_a.line_mut(0).insert(&Sugar {
            content: 'k',
            ..Sugar::default()
        });

        sugartree_b.line_mut(0).insert(&Sugar {
            content: 'z',
            ..Sugar::default()
        });

        changes.push(Diff::Char(DiffChar {
            line: 0,
            column: 1,
            before: Sugar {
                content: 'k',
                foreground_color: [0.0, 0.0, 0.0, 0.0],
                background_color: [0.0, 0.0, 0.0, 0.0],
                style: SugarStyle::Disabled,
                repeated: 0,
                decoration: Disabled,
                cursor: SugarCursor::Disabled,
                media: None,
            },
            after: Sugar {
                content: 'z',
                foreground_color: [0.0, 0.0, 0.0, 0.0],
                background_color: [0.0, 0.0, 0.0, 0.0],
                style: SugarStyle::Disabled,
                repeated: 0,
                decoration: Disabled,
                cursor: SugarCursor::Disabled,
                media: None,
            },
        }));

        assert_eq!(
            sugartree_a.calculate_diff(&sugartree_b, true, false),
            SugarTreeDiff::Changes(changes)
        );
    }
}
