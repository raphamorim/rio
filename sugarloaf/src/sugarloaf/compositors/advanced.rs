use crate::layout::{Alignment, Direction, LayoutContext, Paragraph};
use crate::sugarloaf::{tree::SugarTree, SpanStyle};
use crate::{Content, ContentBuilder, SugarCursor, SugarDecoration, SugarLine};

pub struct Advanced {
    pub render_data: Paragraph,
    pub render_data_sugar: Paragraph,
    content_builder: ContentBuilder,
    content: Content,
    layout_context: LayoutContext,
}

impl Default for Advanced {
    fn default() -> Self {
        let fonts = crate::layout::FontLibrary::default();
        let layout_context = LayoutContext::new(&fonts);
        Self {
            layout_context,
            content_builder: ContentBuilder::default(),
            content: Content::default(),
            render_data: Paragraph::new(),
            render_data_sugar: Paragraph::new(),
        }
    }
}

impl Advanced {
    pub fn reset(&mut self) {}
    pub fn clean(&mut self) {}

    #[inline]
    pub fn update_data(&mut self) {
        // Used for quick testings
        // self.content = build_simple_content();
        // self.content = build_complex_content();
        // self.content = build_terminal_content();
        // self.content = self.content_builder.clone().build();
        self.render_data = Paragraph::default();
    }

    #[inline]
    pub fn update_layout(&mut self, tree: &SugarTree) {
        let mut lb = self.layout_context.builder(
            Direction::LeftToRight,
            None,
            tree.layout.dimensions.scale,
        );
        let content = self.content_builder.clone().build();
        content.layout(&mut lb);
        self.render_data.clear();
        // let start = std::time::Instant::now();
        lb.build_into(&mut self.render_data);
        // let duration = start.elapsed();
        // println!(
        //     "Time elapsed in update_layout() build_into is: {:?}",
        //     duration
        // );
    }

    #[inline]
    pub fn calculate_dimensions(&mut self, tree: &SugarTree) {
        let mut content_builder = crate::content::Content::builder();
        content_builder.enter_span(&[
            SpanStyle::family_list("Fira code"),
            SpanStyle::Size(tree.layout.font_size),
            // S::features(&[("dlig", 1).into(), ("hlig", 1).into()][..]),
        ]);
        content_builder.add_char(' ');
        content_builder.leave_span();

        let mut lb = self.layout_context.builder(
            Direction::LeftToRight,
            None,
            tree.layout.dimensions.scale,
        );
        let content = content_builder.build_ref();
        content.layout(&mut lb);
        self.render_data_sugar.clear();
        lb.build_into(&mut self.render_data_sugar);

        self.render_data_sugar.break_lines().break_remaining(
            tree.layout.width - tree.layout.style.screen_position.0,
            Alignment::Start,
        );
    }

    #[inline]
    pub fn update_size(&mut self, tree: &SugarTree) {
        // let start = std::time::Instant::now();
        self.render_data.break_lines().break_remaining(
            tree.layout.width - tree.layout.style.screen_position.0,
            Alignment::Start,
        );
        // let duration = start.elapsed();
        // println!(
        //     "Time elapsed in rich_text_brush.prepare() break_lines and break_remaining is: {:?}",
        //     duration
        // );
    }

    #[inline]
    pub fn update_tree_with_block(
        &mut self,
        block: &mut SugarLine,
        tree: &mut SugarTree,
    ) {
        if tree.is_empty() {
            self.content_builder = Content::builder();
            self.content_builder.enter_span(&[
                SpanStyle::family_list("Fira code"),
                SpanStyle::Size(tree.layout.font_size),
                // S::features(&[("dlig", 1).into(), ("hlig", 1).into()][..]),
            ]);
        }

        tree.insert_last(*block);

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
        for i in 0..block.acc {
            // println!("char {:?} {:?}", block[i].content, block[i].repeated);

            let mut span_counter = 0;
            if block[i].style.is_bold_italic {
                self.content_builder.enter_span(&[
                    SpanStyle::Weight(crate::layout::Weight::BOLD),
                    SpanStyle::Style(crate::layout::Style::Italic),
                ]);
                span_counter += 1;
            } else if block[i].style.is_bold {
                self.content_builder
                    .enter_span(&[SpanStyle::Weight(crate::layout::Weight::BOLD)]);
                span_counter += 1;
            } else if block[i].style.is_italic {
                self.content_builder
                    .enter_span(&[SpanStyle::Style(crate::layout::Style::Italic)]);
                span_counter += 1;
            }

            let mut has_underline_cursor = false;
            match block[i].cursor {
                SugarCursor::Underline(cursor_color) => {
                    let underline_cursor = &[
                        SpanStyle::UnderlineColor(cursor_color),
                        SpanStyle::Underline(true),
                        SpanStyle::UnderlineOffset(Some(-1.)),
                        SpanStyle::UnderlineSize(Some(2.)),
                    ];
                    self.content_builder.enter_span(underline_cursor);
                    span_counter += 1;
                    has_underline_cursor = true;
                }
                SugarCursor::Block(cursor_color) => {
                    self.content_builder.enter_span(&[SpanStyle::Cursor(
                        SugarCursor::Block(cursor_color),
                    )]);
                    span_counter += 1;
                }
                SugarCursor::Caret(cursor_color) => {
                    self.content_builder.enter_span(&[SpanStyle::Cursor(
                        SugarCursor::Caret(cursor_color),
                    )]);
                    span_counter += 1;
                }
                _ => {}
            }

            match &block[i].decoration {
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
                SpanStyle::Color(block[i].foreground_color),
                SpanStyle::BackgroundColor(block[i].background_color),
            ]);

            if block[i].repeated > 0 {
                let text = std::iter::repeat(block[i].content)
                    .take(block[i].repeated + 1)
                    .collect::<String>();
                self.content_builder.add_text(&text);
            } else {
                self.content_builder.add_char(block[i].content);
            }
            self.content_builder.leave_span();

            while span_counter > 0 {
                self.content_builder.leave_span();
                span_counter -= 1;
            }
        }
        self.content_builder.add_char('\n');
    }
}

#[allow(unused)]
fn build_simple_content() -> crate::content::Content {
    use crate::layout::*;
    let mut db = crate::content::Content::builder();

    use SpanStyle as S;

    db.enter_span(&[S::Size(14.)]);
    db.add_text("Rio terminal -> is back\n");
    db.add_text("Second paragraph\n");
    db.leave_span();
    db.build()
}

#[allow(unused)]
fn build_complex_content() -> crate::content::Content {
    use crate::layout::*;
    let mut db = crate::content::Content::builder();

    use SpanStyle as S;

    let underline = &[
        S::Underline(true),
        S::UnderlineOffset(Some(-1.)),
        S::UnderlineSize(Some(1.)),
    ];

    db.enter_span(&[
        S::family_list("Victor Mono, times, georgia, serif"),
        S::Size(14.),
        S::features(&[("dlig", 1).into(), ("hlig", 1).into()][..]),
    ]);
    db.enter_span(&[S::Weight(Weight::BOLD)]);
    db.enter_span(&[S::Size(20.)]);
    db.enter_span(&[S::Color([0.5, 0.5, 0.5, 1.0])]);
    db.add_text("Rio is back");
    db.leave_span();
    db.leave_span();
    db.enter_span(&[S::Size(40.), S::Color([0.5, 1.0, 0.5, 1.0])]);
    db.add_text("Rio terminal\n");
    db.leave_span();
    db.leave_span();
    db.enter_span(&[S::LineSpacing(1.2)]);
    db.enter_span(&[S::family_list("fira code, serif"), S::Size(22.)]);
    db.add_text("❯ According >= to Wikipedia, the foremost expert on any subject,\n\n");
    db.leave_span();
    db.enter_span(&[S::Weight(Weight::BOLD)]);
    db.add_text("Typography");
    db.leave_span();
    db.add_text(" is the ");
    db.enter_span(&[S::Style(Style::Italic)]);
    db.add_text("art and technique");
    db.leave_span();
    db.add_text(" of arranging type to make ");
    db.enter_span(underline);
    db.add_text("written language");
    db.leave_span();
    db.add_text(" ");
    db.enter_span(underline);
    db.add_text("legible");
    db.leave_span();
    db.add_text(", ");
    db.enter_span(underline);
    db.add_text("readable");
    db.leave_span();
    db.add_text(" and ");
    db.enter_span(underline);
    db.add_text("appealing");
    db.leave_span();
    db.enter_span(&[S::LineSpacing(1.)]);
    db.add_text(
        " Furthermore, العربية نص جميل. द क्विक ब्राउन फ़ॉक्स jumps over the lazy 🐕.\n\n",
    );
    db.leave_span();
    db.enter_span(&[S::family_list("verdana, sans-serif"), S::LineSpacing(1.)]);
    db.add_text("A true ");
    db.enter_span(&[S::Size(48.)]);
    db.add_text("🕵🏽‍♀️");
    db.leave_span();
    db.add_text(" will spot the tricky selection in this BiDi text: ");
    db.enter_span(&[S::Size(22.)]);
    db.add_text("ניפגש ב09:35 בחוף הים");
    db.add_text("\nABC🕵🏽‍♀️🕵🏽‍♀️🕵🏽‍♀️🕵🏽‍♀️🕵🏽‍♀️🕵🏽‍♀️🕵🏽‍♀️");
    db.leave_span();
    db.build()
}

#[allow(unused)]
fn build_terminal_content() -> crate::content::Content {
    use crate::layout::*;
    let mut db = crate::content::Content::builder();

    use SpanStyle as S;

    let underline = &[
        S::Underline(true),
        S::UnderlineOffset(Some(-1.)),
        S::UnderlineSize(Some(1.)),
    ];

    for i in 0..20 {
        db.enter_span(&[
            S::family_list("Fira code, times, georgia, serif"),
            S::Size(24.),
            // S::features(&[("dlig", 1).into(), ("hlig", 1).into()][..]),
        ]);
        db.enter_span(&[
            S::Weight(Weight::BOLD),
            S::BackgroundColor([0.0, 1.0, 1.0, 1.0]),
            S::Color([1.0, 0.5, 0.5, 1.0]),
        ]);
        db.add_char('R');
        db.leave_span();
        // should return to span
        db.enter_span(&[
            S::Color([0.0, 1.0, 0.0, 1.0]),
            S::BackgroundColor([1.0, 1.0, 0.0, 1.0]),
        ]);
        db.add_text("iiii");
        db.leave_span();
        db.enter_span(&[
            S::Weight(Weight::NORMAL),
            S::Style(Style::Italic),
            S::Color([0.0, 1.0, 1.0, 1.0]),
            // S::Size(20.),
        ]);
        db.add_char('o');
        db.leave_span();
        db.add_char('+');
        db.add_text(" 🌊🌊🌊🌊");
        for x in 0..5 {
            db.add_char(' ');
        }
        db.add_text("---> ->");
        db.add_text("-> 🥶");
        db.break_line();
        db.leave_span();
        db.leave_span();
    }
    // db.break_line();
    // db.enter_span(&[S::Color([1.0, 1.0, 1.0, 1.0])]);
    // db.add_text("terminal");
    // db.leave_span();
    // db.add_text("\n");
    // db.enter_span(&[S::Weight(Weight::BOLD)]);
    // db.add_text("t");
    // db.add_text("e");
    // db.add_text("r");
    // db.add_text("m");
    // db.add_text(" ");
    // db.enter_span(underline);
    // db.add_text("\n");
    // db.enter_span(&[S::Color([0.0, 1.0, 1.0, 1.0])]);
    // db.add_text("n");
    db.build()
}
