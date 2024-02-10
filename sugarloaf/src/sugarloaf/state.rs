use super::tree::{SugarTree, SugarTreeDiff};
use crate::layout::{Alignment, Direction, LayoutContext, Paragraph};
use crate::sugarloaf::SpanStyle;
use crate::sugarloaf::SugarloafLayout;
use crate::{Content, ContentBuilder};
use crate::{SugarCursor, SugarDecoration, SugarLine};

pub struct SugarState {
    pub current: SugarTree,
    next: SugarTree,
    content_builder: ContentBuilder,
    content: Content,
    pub latest_change: SugarTreeDiff,
    pub render_data: Paragraph,
    pub render_data_sugar: Paragraph,
    layout_context: LayoutContext,
    pub had_obtained_latest_dimensions: bool,
}

impl Default for SugarState {
    fn default() -> Self {
        let fonts = crate::layout::FontLibrary::default();
        let layout_context = LayoutContext::new(&fonts);
        Self {
            render_data: Paragraph::new(),
            render_data_sugar: Paragraph::new(),
            next: SugarTree::default(),
            current: SugarTree::default(),
            content_builder: ContentBuilder::default(),
            content: Content::default(),
            // First time computing changes should obtain sugar width/height
            latest_change: SugarTreeDiff::LayoutIsDifferent,
            layout_context,
            had_obtained_latest_dimensions: false,
        }
    }
}

impl SugarState {
    #[inline]
    pub fn compute_changes(&mut self, layout: SugarloafLayout) {
        self.next.layout = layout;

        // If sugar dimensions are empty then need to find it
        if self.current_has_empty_dimensions() {
            std::mem::swap(&mut self.current, &mut self.next);
            self.update_render_data_sugar();
            self.next = SugarTree::default();
            return;
        }

        if !self.current.is_empty() {
            self.latest_change = self.current.calculate_diff(&self.next);
            match &self.latest_change {
                SugarTreeDiff::Equal => {
                    // Do nothing
                }
                SugarTreeDiff::LayoutIsDifferent => {
                    std::mem::swap(&mut self.current, &mut self.next);
                    self.update_render_data_sugar();
                    self.update_data();
                    self.update_layout();
                    self.update_size();
                }
                // SugarTreeDiff::ColumnsLengthIsDifferent(_) => {
                //     println!("ColumnsLengthIsDifferent");
                //     std::mem::swap(&mut self.current, &mut self.next);
                //     self.update_data();
                //     self.update_layout();
                //     self.update_size();
                // }
                SugarTreeDiff::Changes(_changes) => {
                    // for change in changes {
                    //     // println!("change {:?}", change);
                    //     if let Some(offs) = self.content.insert(0, change.after.content) {
                    //         // inserted = Some(offs);
                    //         println!("{:?}", offs);
                    //     }
                    // }
                    // std::mem::swap(&mut self.current, &mut self.next);
                    std::mem::swap(&mut self.current, &mut self.next);
                    self.update_data();
                    self.update_layout();
                    self.update_size();
                    // println!("changes: {:?}", changes);
                }
                _ => {
                    std::mem::swap(&mut self.current, &mut self.next);
                    self.update_data();
                    self.update_layout();
                    self.update_size();
                }
            }
        } else if !self.next.is_empty() {
            std::mem::swap(&mut self.current, &mut self.next);
        }

        // Cleanup next
        self.next = SugarTree::default();
    }

    #[inline]
    pub fn current_has_empty_dimensions(&self) -> bool {
        self.current.layout.dimensions.width == 0.0
            || self.current.layout.dimensions.height == 0.0
    }

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
    pub fn update_layout(&mut self) {
        let mut lb = self.layout_context.builder(
            Direction::LeftToRight,
            None,
            self.current.layout.dimensions.scale,
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
    pub fn update_render_data_sugar(&mut self) {
        let mut content_builder = crate::content::Content::builder();
        content_builder.enter_span(&[
            SpanStyle::family_list("Fira code"),
            SpanStyle::Size(self.current.layout.font_size),
            // S::features(&[("dlig", 1).into(), ("hlig", 1).into()][..]),
        ]);
        content_builder.add_char(' ');
        content_builder.leave_span();

        let mut lb = self.layout_context.builder(
            Direction::LeftToRight,
            None,
            self.current.layout.dimensions.scale,
        );
        let content = content_builder.build_ref();
        content.layout(&mut lb);
        self.render_data_sugar.clear();
        lb.build_into(&mut self.render_data_sugar);

        self.render_data_sugar.break_lines().break_remaining(
            self.current.layout.width - self.current.layout.style.screen_position.0,
            Alignment::Start,
        );
    }

    #[inline]
    pub fn update_size(&mut self) {
        // let start = std::time::Instant::now();
        self.render_data.break_lines().break_remaining(
            self.current.layout.width - self.current.layout.style.screen_position.0,
            Alignment::Start,
        );
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
                SpanStyle::Size(self.current.layout.font_size),
                // S::features(&[("dlig", 1).into(), ("hlig", 1).into()][..]),
            ]);
        }

        self.next.insert_last(*line);

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
        for i in 0..line.acc {
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
            match line[i].cursor {
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

            if line[i].repeated > 0 {
                let text = std::iter::repeat(line[i].content)
                    .take(line[i].repeated + 1)
                    .collect::<String>();
                self.content_builder.add_text(&text);
            } else {
                self.content_builder.add_char(line[i].content);
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
