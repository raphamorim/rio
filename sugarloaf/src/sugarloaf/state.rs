use crate::sugarloaf::SugarloafLayout;
use crate::layout::{Alignment, Direction, Paragraph, LayoutContext};
use super::tree::{SugarTree, SugarTreeDiff};
use crate::sugarloaf::SpanStyle;
use crate::SugarCursorStyle;
use crate::SugarDecoration;
use crate::SugarLine;
use crate::{Content, ContentBuilder};

pub struct SugarState {
    pub current: SugarTree,
    next: SugarTree,
    content_builder: ContentBuilder,
    content: Content,
    pub render_data: Paragraph,
    layout_context: LayoutContext
}

impl Default for SugarState {
    fn default() -> Self {
        let fonts = crate::layout::FontLibrary::default();
        let layout_context = LayoutContext::new(&fonts);
        Self {
            render_data: Paragraph::new(),
            next: SugarTree::default(),
            current: SugarTree::default(),
            content_builder: ContentBuilder::default(),
            content: Content::default(),
            layout_context,
        }
    }
}

impl SugarState {
    #[inline]
    pub fn compute_changes(&mut self, ctx: &SugarloafLayout) {
        // TODO: Use layout
        self.next.width = ctx.width;
        self.next.height = ctx.height;
        self.next.margin = ctx.margin;
        self.next.scale = ctx.scale_factor;

        if !self.current.is_empty() {
            match self.current.calculate_diff(&self.next) {
                SugarTreeDiff::Equal => {
                    // Do nothing
                }
                SugarTreeDiff::WidthIsDifferent => {
                    println!("WidthIsDifferent");
                    self.current = self.next.clone();
                    self.update_data();
                    self.update_layout();
                    self.update_size();
                }
                // SugarTreeDiff::HeightIsDifferent => {
                //     println!("HeightIsDifferent");
                //     self.current = self.next.clone();
                //     self.update_data();
                //     self.update_layout();
                //     self.update_size();
                // }
                // SugarTreeDiff::ColumnsLengthIsDifferent(_) => {
                //     println!("ColumnsLengthIsDifferent");
                //     self.current = self.next.clone();
                //     self.update_data();
                //     self.update_layout();
                //     self.update_size();
                // }
                SugarTreeDiff::LineLengthIsDifferent(_) => {
                    println!("LineLengthIsDifferent");
                    self.current = self.next.clone();
                    self.update_data();
                    self.update_layout();
                    self.update_size();
                }
                SugarTreeDiff::Changes(_changes) => {
                    println!("Changes");
                    // for change in changes {
                    //     // println!("change {:?}", change);
                    //     if let Some(offs) = self.content.insert(0, change.after.content) {
                    //         // inserted = Some(offs);
                    //         println!("{:?}", offs);
                    //     }
                    // }
                    self.current = self.next.clone();
                    self.update_data();
                    self.update_layout();
                    self.update_size();
                    // println!("changes: {:?}", changes);
                }
                _ => {
                    self.current = self.next.clone();
                    self.update_data();
                    self.update_layout();
                    self.update_size();
                }
            }
        } else if !self.next.is_empty() {
            self.current = self.next.clone();
        }

        // Cleanup next
        self.next = SugarTree::default();
    }

    #[inline]
    pub fn update_data(&mut self) {
        self.content = self.content_builder.clone().build();
        self.render_data = Paragraph::default();
    }

    #[inline]
    pub fn update_layout(&mut self) {
        let mut lb =
            self.layout_context
                .builder(Direction::LeftToRight, None, self.current.scale);
        self.content.layout(&mut lb);
        self.render_data.clear();
        let start = std::time::Instant::now();
        lb.build_into(&mut self.render_data);
        let duration = start.elapsed();
        println!(
            "Time elapsed in update_layout() build_into is: {:?}",
            duration
        );
    }

    #[inline]
    pub fn update_size(&mut self) {
        // let start = std::time::Instant::now();
        self.render_data
            .break_lines()
            .break_remaining(self.current.width - (self.current.margin.x * self.current.scale), Alignment::Start);
        // let duration = start.elapsed();
        // println!(
        //     "Time elapsed in rich_text_brush.prepare() break_lines and break_remaining is: {:?}",
        //     duration
        // );
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
            // println!("char {:?} {:?}", line[i].content, line[i].repeated);

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

            // if line[i].repeated > 0 {
            //     let text = std::iter::repeat(line[i].content).take(line[i].repeated + 1).collect::<String>();
            //     self.content_builder.add_text(&text);
            // } else {
                self.content_builder.add_char(line[i].content);
            // }
            self.content_builder.leave_span();

            while span_counter > 0 {
                self.content_builder.leave_span();
                span_counter -= 1;
            }
        }
        self.content_builder.add_char('\n');
    }
}
