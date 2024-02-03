use crate::sugarloaf::SugarLine;
use fnv::FnvHashMap;

#[derive(Debug, PartialEq)]
pub enum SugarTreeDiff {
    Equal,
    LineQuantityIsDifferent(i32),
    LineChange(usize),
    ColumunsChanged(usize),
    InsertedCharacter,
    DeletedCharacter,
}

// const LINE_MAX_LINES: usize = 140;

#[derive(Default)]
pub struct SugarTree {
    inner: FnvHashMap<usize, SugarLine>,
    // cursor_position: (u16, u16), // (col, line)
}

// impl Default for SugarTree {
//     fn default() -> Self {
//         // let inner = {
//         //     // Create an array of uninitialized values.
//         //     let mut array: [MaybeUninit<SugarLine>; LINE_MAX_LINES] =
//         //         unsafe { MaybeUninit::uninit().assume_init() };

//         //     for element in array.iter_mut() {
//         //         *element = MaybeUninit::new(SugarLine::default());
//         //     }

//         //     unsafe { std::mem::transmute::<_, [SugarLine; LINE_MAX_LINES]>(array) }
//         // };
//         let mut inner_vec: Vec<SugarLine> = Vec::with_capacity(LINE_MAX_LINES);
//         for _i in 0..LINE_MAX_LINES {
//             inner_vec.push(SugarLine::default());
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
        let current_len = self.inner.len();
        let next_len = next.len();

        if current_len == next_len {
            for line_number in 0..self.len() {
                let current_line: SugarLine = self.inner[&line_number];
                let next_line: SugarLine = next.inner[&line_number];
                if current_line != next_line {
                    return SugarTreeDiff::LineChange(line_number);
                }
            }
        } else {
            return SugarTreeDiff::LineQuantityIsDifferent(
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
    pub fn insert(&mut self, id: usize, content: SugarLine) {
        self.inner.insert(id, content);
    }

    #[inline]
    pub fn line_mut(&mut self, line_number: usize) -> Option<&mut SugarLine> {
        self.inner.get_mut(&line_number)
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

        sugartree_a.insert(0, SugarLine::default());
        sugartree_a.line_mut(0).unwrap().insert(Sugar {
            content: 'b',
            ..Sugar::default()
        });

        sugartree_b.insert(0, SugarLine::default());
        sugartree_b.line_mut(0).unwrap().insert(Sugar {
            content: 'b',
            ..Sugar::default()
        });

        assert_eq!(
            sugartree_a.calculate_diff(&sugartree_b),
            SugarTreeDiff::Equal
        );
    }

    #[test]
    fn test_sugartree_calculate_diff_line_len_change() {
        let mut sugartree_a = SugarTree::default();
        let mut sugartree_b = SugarTree::default();

        sugartree_a.insert(0, SugarLine::default());

        assert_eq!(
            sugartree_a.calculate_diff(&sugartree_b),
            SugarTreeDiff::LineQuantityIsDifferent(1)
        );

        sugartree_a.insert(1, SugarLine::default());

        assert_eq!(
            sugartree_a.calculate_diff(&sugartree_b),
            SugarTreeDiff::LineQuantityIsDifferent(2)
        );

        sugartree_b.insert(0, SugarLine::default());
        sugartree_b.insert(1, SugarLine::default());
        sugartree_b.insert(2, SugarLine::default());

        assert_eq!(
            sugartree_a.calculate_diff(&sugartree_b),
            SugarTreeDiff::LineQuantityIsDifferent(-1)
        );
    }

    #[test]
    fn test_sugartree_calculate_diff_exact_change() {
        let mut sugartree_a = SugarTree::default();
        let mut sugartree_b = SugarTree::default();

        sugartree_a.insert(0, SugarLine::default());
        sugartree_a.line_mut(0).unwrap().insert(Sugar {
            content: 'b',
            ..Sugar::default()
        });

        sugartree_b.insert(0, SugarLine::default());
        sugartree_b.line_mut(0).unwrap().insert(Sugar {
            content: 'b',
            ..Sugar::default()
        });

        sugartree_b.line_mut(0).unwrap().insert(Sugar {
            content: 'a',
            ..Sugar::default()
        });

        assert_eq!(
            sugartree_a.calculate_diff(&sugartree_b),
            SugarTreeDiff::LineChange(0)
        );
    }
}
