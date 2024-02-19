// use crate::SugarKind;
// use crate::Sugar;
// use crate::SugarPosition;
use crate::fragment::Fragment;
use std::ops::Index;

#[derive(PartialEq, Default)]
pub struct TextArea {
    pub inner: Vec<Line>,
    // position: SugarPosition,
    current_line: usize,
    // pub columns: usize,
    // pub lines: usize,
}

impl TextArea {
    #[inline]
    pub fn new_line(&mut self) {
        self.inner.push(Line::default());
        self.current_line = self.inner.len() - 1;
    }

    #[inline]
    pub fn insert_on_current_line(&mut self, fragment: &Fragment) {
        self.inner[self.current_line].insert(fragment);
    }

    #[inline]
    pub fn insert_on_current_line_from_vec(&mut self, fragment_vec: &Vec<&Fragment>) {
        for fragment in fragment_vec {
            self.inner[self.current_line].insert(fragment);
        }
    }

    #[inline]
    pub fn insert_on_current_line_from_vec_owned(
        &mut self,
        fragment_vec: &Vec<Fragment>,
    ) {
        for fragment in fragment_vec {
            self.inner[self.current_line].insert(fragment);
        }
    }
}

// impl Sugar for TextArea {
//     // fn should_update(&self, other: &Self) -> bool {
//     //     self == other
//     // }
//     fn kind(&self) -> SugarKind {
//         SugarKind::TextArea
//     }
// }

/// Contains a visual representation that is hashable and comparable
/// It often represents a line of text but can also be other elements like bitmap
#[derive(Debug, Clone)]
pub struct Line {
    // hash: u64,
    // Sized arrays can take up to half of time to execute
    // https://play.rust-lang.org/?version=stable&mode=debug&edition=2018&gist=b3face22f8c64b25803fa213be6a858f

    // inner: [Fragment; SUGAR_LINE_MAX_CONTENT_SIZE],
    // pub len: usize,
    pub acc: usize,

    inner: Vec<Fragment>,
    first_non_default: usize,
    last_non_default: usize,
    non_default_count: usize,
    default_fragment: Fragment,
}

impl PartialEq for Line {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        if self.is_empty() && other.is_empty() {
            return true;
        }

        // if self.len != other.len
        let len = self.inner.len();
        if len != other.inner.len()
            || self.first_non_default != other.first_non_default
            || self.last_non_default != other.last_non_default
            || self.non_default_count != other.non_default_count
        {
            return false;
        }

        for i in 0..len {
            if self.inner[i] != other.inner[i] {
                return false;
            }
        }

        true
    }
}

impl Default for Line {
    fn default() -> Self {
        Self {
            // hash: 00000000000000,
            last_non_default: 0,
            first_non_default: 0,
            non_default_count: 0,
            inner: Vec::with_capacity(600),
            default_fragment: Fragment::default(),
            acc: 0,
            // len: 0,
        }
    }
}

impl Line {
    // #[inline]
    // pub fn insert(&mut self, fragment: &Fragment) {
    //     let previous = if self.acc > 0 { self.acc - 1 } else { 0 };

    //     if equal_without_consider_repeat(&self.inner[previous], fragment) {
    //         self.inner[previous].repeated += 1;
    //         self.len += 1;
    //         return;
    //     }

    //     self.inner[self.acc] = *fragment;

    //     if fragment != &self.default_fragment {
    //         if self.first_non_default == 0 {
    //             self.first_non_default = self.acc;
    //             self.last_non_default = self.acc;
    //         } else {
    //             self.last_non_default = self.acc;
    //         }

    //         self.non_default_count += 1;
    //     }

    //     self.acc += 1;
    //     self.len += 1;
    // }

    #[inline]
    pub fn insert(&mut self, fragment: &Fragment) {
        let len = self.inner.len();

        if len > 0 && equal_without_consider_repeat(&self.inner[len - 1], fragment) {
            self.inner[len - 1].repeated += 1;
            return;
        }

        self.inner.push(*fragment);

        if fragment != &self.default_fragment {
            if self.first_non_default == 0 {
                self.first_non_default = self.acc;
                self.last_non_default = self.acc;
            } else {
                self.last_non_default = self.acc;
            }

            self.non_default_count += 1;
        }

        self.acc += 1;
    }

    #[inline]
    pub fn insert_empty(&mut self) {
        // self.inner[self.len] = self.default_fragment;
        self.inner.push(self.default_fragment);
        self.acc += 1;
        // self.len += 1;
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
        // self.len += 1;
    }

    // #[inline]
    // fn compute_hash(&mut self) {
    // 00000000000000
    // 00000000000000 -> first non-default apparison position
    // 00000000000000 -> last non-default apparison position
    // 00000000000000 ->
    // }

    #[inline]
    pub fn is_empty(&self) -> bool {
        // if first digits are zero
        self.non_default_count == 0
    }

    #[inline]
    pub fn from_vec(&mut self, vector: &[Fragment]) {
        for element in vector.iter() {
            self.insert(element)
        }
    }
}

impl Index<usize> for Line {
    type Output = Fragment;

    fn index(&self, index: usize) -> &Self::Output {
        &self.inner[index]
    }
}

#[cfg(test)]
pub mod test {
    use super::*;

    #[test]
    fn test_fragmentelement_comparisson_exact_match() {
        let line_a = Line::default();
        let line_b = Line::default();

        assert!(line_a.is_empty());
        assert!(line_b.is_empty());
        assert_eq!(line_a, line_b);
    }

    #[test]
    fn test_fragmentelement_from_vector() {
        let mut line_a = Line::default();
        let vector = vec![
            Fragment {
                content: 't',
                ..Fragment::default()
            },
            Fragment {
                content: 'e',
                ..Fragment::default()
            },
            Fragment {
                content: 'r',
                ..Fragment::default()
            },
            Fragment {
                content: 'm',
                ..Fragment::default()
            },
        ];

        line_a.from_vec(&vector);

        assert!(!line_a.is_empty());
        assert_eq!(line_a.len(), 4);
    }

    #[test]
    fn test_fragmentelement_repetition() {
        let mut line_a = Line::default();
        let vector = vec![
            Fragment {
                content: 'a',
                ..Fragment::default()
            },
            Fragment {
                content: 'a',
                ..Fragment::default()
            },
            Fragment {
                content: 'b',
                ..Fragment::default()
            },
            Fragment {
                content: 'c',
                ..Fragment::default()
            },
            Fragment {
                content: 'd',
                ..Fragment::default()
            },
            Fragment {
                content: 'd',
                ..Fragment::default()
            },
        ];

        line_a.from_vec(&vector);

        assert!(!line_a.is_empty());
        assert_eq!(line_a.len(), 6);
        assert_eq!(line_a.acc, 4);

        let mut line_a = Line::default();
        let vector = vec![
            Fragment {
                content: 'a',
                ..Fragment::default()
            },
            Fragment {
                content: 'b',
                ..Fragment::default()
            },
            Fragment {
                content: 'c',
                ..Fragment::default()
            },
            Fragment {
                content: 'd',
                ..Fragment::default()
            },
            Fragment {
                content: 'e',
                ..Fragment::default()
            },
            Fragment {
                content: 'f',
                ..Fragment::default()
            },
        ];

        line_a.from_vec(&vector);

        assert!(!line_a.is_empty());
        assert_eq!(line_a.len(), 6);
        assert_eq!(line_a.acc, 6);

        let mut line_a = Line::default();
        let vector = vec![
            Fragment {
                content: ' ',
                ..Fragment::default()
            },
            Fragment {
                content: ' ',
                ..Fragment::default()
            },
            Fragment {
                content: ' ',
                ..Fragment::default()
            },
            Fragment {
                content: ' ',
                ..Fragment::default()
            },
            Fragment {
                content: ' ',
                ..Fragment::default()
            },
            Fragment {
                content: ' ',
                ..Fragment::default()
            },
        ];

        line_a.from_vec(&vector);

        assert!(line_a.is_empty());
        assert_eq!(line_a.len(), 6);
        assert_eq!(line_a.acc, 0);
    }

    #[test]
    fn test_fragmentelement_empty_checks() {
        let mut line_a = Line::default();
        line_a.insert_empty();
        line_a.insert_empty();
        line_a.insert_empty();

        assert!(line_a.is_empty());

        let mut line_a = Line::default();
        line_a.insert(&Fragment::default());

        assert!(line_a.is_empty());

        let mut line_a = Line::default();
        line_a.insert(&Fragment {
            content: ' ',
            ..Fragment::default()
        });

        assert!(line_a.is_empty());
    }

    #[test]
    fn test_fragmentelement_comparisson_different_len() {
        let mut line_a = Line::default();
        line_a.insert_empty();
        line_a.insert(&Fragment {
            content: 'r',
            ..Fragment::default()
        });
        let line_b = Line::default();

        assert!(!line_a.is_empty());
        assert!(line_b.is_empty());
        assert!(line_a != line_b);

        let mut line_a = Line::default();
        line_a.insert(&Fragment {
            content: ' ',
            ..Fragment::default()
        });
        line_a.insert(&Fragment {
            content: 'r',
            ..Fragment::default()
        });
        let mut line_b = Line::default();
        line_b.insert(&Fragment {
            content: 'r',
            ..Fragment::default()
        });
        line_b.insert(&Fragment {
            content: ' ',
            ..Fragment::default()
        });
        line_b.insert(&Fragment {
            content: 'i',
            ..Fragment::default()
        });
        line_b.insert(&Fragment {
            content: 'o',
            ..Fragment::default()
        });

        assert!(!line_a.is_empty());
        assert!(!line_b.is_empty());
        assert!(line_a != line_b);
    }

    #[test]
    fn test_fragmentelement_comparisson_different_match_with_same_len() {
        let mut line_a = Line::default();
        line_a.insert(&Fragment {
            content: 'o',
            ..Fragment::default()
        });
        line_a.insert(&Fragment {
            content: 'i',
            ..Fragment::default()
        });
        line_a.insert(&Fragment {
            content: 'r',
            ..Fragment::default()
        });
        let mut line_b = Line::default();
        line_b.insert(&Fragment {
            content: 'r',
            ..Fragment::default()
        });
        line_b.insert(&Fragment {
            content: 'i',
            ..Fragment::default()
        });
        line_b.insert(&Fragment {
            content: 'o',
            ..Fragment::default()
        });

        assert!(!line_a.is_empty());
        assert!(!line_b.is_empty());
        assert!(line_a != line_b);
    }
}

#[inline]
fn equal_without_consider_repeat(fragment_a: &Fragment, fragment_b: &Fragment) -> bool {
    fragment_a.content == fragment_b.content
        && fragment_a.foreground_color == fragment_b.foreground_color
        && fragment_a.background_color == fragment_b.background_color
        && fragment_a.style == fragment_b.style
        && fragment_a.decoration == fragment_b.decoration
        && fragment_a.cursor == fragment_b.cursor
}