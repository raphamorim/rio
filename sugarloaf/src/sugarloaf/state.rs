use super::tree::{SugarTree, SugarTreeDiff};
use crate::sugarloaf::SpanStyle;
use crate::SugarCursorStyle;
use crate::SugarDecoration;
use crate::SugarLine;
use crate::{Content, ContentBuilder};

#[derive(Default)]
pub struct SugarState {
    current: SugarTree,
    next: SugarTree,
    content_builder: ContentBuilder,
}

// self.content = Content::builder();
// self.content.enter_span(&[
//     SpanStyle::family_list("Fira code"),
//     SpanStyle::Size(self.layout.font_size),
//     // S::features(&[("dlig", 1).into(), ("hlig", 1).into()][..]),
// ]);

impl SugarState {
    #[inline]
    pub fn content(&mut self) -> &Content {
        // if self.current.is_empty() {
        //     self.current = self.next.clone();
        //     return self.content_builder.build_ref();
        // }

        match self.current.calculate_diff(&self.next) {
            SugarTreeDiff::Equal => {
                println!("é igual");
            }
            SugarTreeDiff::ColumnsLengthIsDifferent(_) => {
                println!("ColumnsLengthIsDifferent");
                self.current = self.next.clone();
            }
            SugarTreeDiff::LineLengthIsDifferent(_) => {
                println!("LineLengthIsDifferent");
                self.current = self.next.clone();
            }
            SugarTreeDiff::Changes(changes) => {
                println!("changes: {:?}", changes);
            }
        }

        self.next = SugarTree::default();
        self.content_builder.build_ref()
    }

    #[inline]
    pub fn process_line(&mut self, line: &mut SugarLine) {
        if self.next.is_empty() {
            self.content_builder = Content::builder();
            self.content_builder.enter_span(&[
                SpanStyle::family_list("Fira code"),
                SpanStyle::Size(14.0),
                // S::features(&[("dlig", 1).into(), ("hlig", 1).into()][..]),
            ]);
        }

        self.next.insert_last(*line);

        let size = line.len;
        let underline = &[
            SpanStyle::Underline(true),
            SpanStyle::UnderlineOffset(Some(-1.)),
            SpanStyle::UnderlineSize(Some(1.)),
        ];

        // let strikethrough = &[SpanStyle::Strikethrough(true)];
        let strikethrough = &[
            SpanStyle::Underline(true),
            SpanStyle::UnderlineOffset(Some(6.)),
            SpanStyle::UnderlineSize(Some(2.)),
        ];

        // let mut content = String::from("");
        for i in 0..size {
            let mut span_counter = 0;
            if line[i].style.is_bold_italic {
                self.content_builder.enter_span(&[
                    SpanStyle::Weight(crate::layout::Weight::BOLD),
                    SpanStyle::Style(crate::layout::Style::Italic),
                ]);
                span_counter += 1;
            } else if line[i].style.is_bold {
                self.content_builder
                    .enter_span(&[SpanStyle::Weight(crate::layout::Weight::BOLD)]);
                span_counter += 1;
            } else if line[i].style.is_italic {
                self.content_builder
                    .enter_span(&[SpanStyle::Style(crate::layout::Style::Italic)]);
                span_counter += 1;
            }

            let mut has_underline_cursor = false;
            if let Some(cursor) = &line[i].cursor {
                match cursor.style {
                    SugarCursorStyle::Underline => {
                        let underline_cursor = &[
                            SpanStyle::UnderlineColor(cursor.color),
                            SpanStyle::Underline(true),
                            SpanStyle::UnderlineOffset(Some(-1.)),
                            SpanStyle::UnderlineSize(Some(2.)),
                        ];
                        self.content_builder.enter_span(underline_cursor);
                        span_counter += 1;
                        has_underline_cursor = true;
                    }
                    SugarCursorStyle::Block | SugarCursorStyle::Caret => {
                        self.content_builder
                            .enter_span(&[SpanStyle::Cursor(*cursor)]);
                        span_counter += 1;
                    }
                }
            }

            match &line[i].decoration {
                SugarDecoration::Underline => {
                    if !has_underline_cursor {
                        self.content_builder.enter_span(underline);
                        span_counter += 1;
                    }
                }
                SugarDecoration::Strikethrough => {
                    self.content_builder.enter_span(strikethrough);
                    span_counter += 1;
                }
                _ => {}
            }

            self.content_builder.enter_span(&[
                SpanStyle::Color(line[i].foreground_color),
                SpanStyle::BackgroundColor(line[i].background_color),
            ]);

            self.content_builder.add_char(line[i].content);
            self.content_builder.leave_span();

            while span_counter > 0 {
                self.content_builder.leave_span();
                span_counter -= 1;
            }
        }
        self.content_builder.add_char('\n');
    }
}
