use crate::crosswords::grid::row::SemanticPrompt;
use crate::crosswords::grid::Dimensions;
use crate::crosswords::pos::{Column, Line, Pos};
use crate::crosswords::square::{ContentTag, SemanticContent, Square};
use crate::crosswords::Crosswords;
use crate::event::EventListener;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SemanticState {
    pub content: SemanticContent,
    pub clear_eol: bool,
    pub seen: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticMark {
    Prompt(SemanticPrompt),
    Input { until_eol: bool },
    Output,
    CommandEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandOutput {
    pub prompt: Option<Line>,
    pub output: Option<(Pos, Pos)>,
    pub running: bool,
    pub truncated: bool,
}

#[inline]
fn has_text(cell: Square) -> bool {
    cell.content_tag() == ContentTag::Codepoint && !matches!(cell.c(), '\0' | ' ' | '\t')
}

impl<U: EventListener> Crosswords<U> {
    pub fn semantic_prompt_seen(&self) -> bool {
        self.grid.semantic.seen
    }

    pub fn last_command_output(&self) -> Option<CommandOutput> {
        if !self.grid.semantic.seen
            || self.mode.contains(crate::crosswords::Mode::ALT_SCREEN)
        {
            return None;
        }

        let cursor_line = self.grid.cursor.pos.row;
        let current = self.prompt_start_at_or_above(cursor_line);
        if self.grid.semantic.content == SemanticContent::Output {
            let running = self.command_started_at.is_some();
            return self.command_output_from(current, cursor_line, running);
        }

        let (mut current, _) = current?;
        loop {
            let above = current - 1;
            if above < self.grid.topmost_line() {
                return None;
            }
            let prompt = self.prompt_start_at_or_above(above);
            let out = self.command_output_from(prompt, above, false)?;
            let ran = out.output.is_some()
                || prompt.is_some_and(|(line, _)| self.grid[line].command_start);
            match prompt {
                Some((line, _)) if !ran => current = line,
                _ => return Some(out),
            }
        }
    }

    pub fn command_output_at(&self, line: Line) -> Option<CommandOutput> {
        if !self.grid.semantic.seen
            || self.mode.contains(crate::crosswords::Mode::ALT_SCREEN)
        {
            return None;
        }
        if line < self.grid.topmost_line() || line > self.grid.bottommost_line() {
            return None;
        }
        let prompt = self.prompt_start_at_or_above(line);
        let current = self.prompt_start_at_or_above(self.grid.cursor.pos.row);
        let running = self.grid.semantic.content == SemanticContent::Output
            && self.command_started_at.is_some()
            && prompt.map(|p| p.0) == current.map(|p| p.0);
        self.command_output_from(prompt, line, running)
    }

    pub fn command_output_text(&self, output: &CommandOutput) -> Option<String> {
        let (start, end) = output.output?;
        Some(self.bounds_to_string(start, end))
    }

    fn command_output_from(
        &self,
        prompt: Option<(Line, bool)>,
        within: Line,
        running: bool,
    ) -> Option<CommandOutput> {
        let top = self.grid.topmost_line();
        let (scan_from, prompt_line, truncated) = match prompt {
            Some((line, complete)) => (line, Some(line), !complete),
            None if self.grid.lines_evicted() > 0 => (top, None, true),
            None => return None,
        };
        let end_line = self
            .next_prompt_start(scan_from.max(within))
            .map_or(self.grid.bottommost_line(), |next| next - 1);
        if end_line < scan_from {
            return None;
        }

        Some(CommandOutput {
            prompt: prompt_line,
            output: self.output_bounds(scan_from, end_line, prompt_line.is_some()),
            running,
            truncated,
        })
    }

    fn output_bounds(
        &self,
        from: Line,
        to: Line,
        skip_stale: bool,
    ) -> Option<(Pos, Pos)> {
        let columns = self.grid.columns();

        let first_col = if skip_stale {
            let row = &self.grid[from];
            (0..columns)
                .find(|&col| {
                    row[Column(col)].semantic_content() != SemanticContent::Output
                })
                .unwrap_or(0)
        } else {
            0
        };

        let mut start: Option<Pos> = None;
        let mut end = Pos::default();
        'rows: for line in (from.0..=to.0).map(Line) {
            let row = &self.grid[line];
            let col_start = if line == from { first_col } else { 0 };
            for col in col_start..columns {
                let cell = row[Column(col)];
                match cell.semantic_content() {
                    SemanticContent::Prompt | SemanticContent::Input => {
                        if start.is_some() {
                            break 'rows;
                        }
                    }
                    SemanticContent::Output => {
                        if has_text(cell) {
                            let pos = Pos::new(line, Column(col));
                            start.get_or_insert(pos);
                            end = pos;
                        }
                    }
                }
            }
        }
        let mut start = start?;

        let row = &self.grid[start.row];
        if (0..start.col.0)
            .all(|col| row[Column(col)].semantic_content() == SemanticContent::Output)
        {
            start.col = Column(0);
        }

        Some((start, end))
    }

    pub(crate) fn prompt_start_at_or_above(&self, line: Line) -> Option<(Line, bool)> {
        let top = self.grid.topmost_line();
        let mark = |line: Line| self.grid[line].semantic_prompt;
        let mut line = line;
        while line >= top {
            match mark(line) {
                SemanticPrompt::None => line -= 1,
                SemanticPrompt::Prompt => return Some((line, true)),
                SemanticPrompt::PromptContinuation => {
                    while line > top {
                        match mark(line - 1) {
                            SemanticPrompt::None => return Some((line, true)),
                            SemanticPrompt::Prompt => return Some((line - 1, true)),
                            SemanticPrompt::PromptContinuation => line -= 1,
                        }
                    }
                    return Some((line, self.grid.lines_evicted() == 0));
                }
            }
        }
        None
    }

    fn next_prompt_start(&self, line: Line) -> Option<Line> {
        let bottom = self.grid.bottommost_line();
        let mark = |line: Line| self.grid[line].semantic_prompt;
        let mut line = line;
        if mark(line) != SemanticPrompt::None {
            while line < bottom && mark(line + 1) == SemanticPrompt::PromptContinuation {
                line += 1;
            }
        }
        ((line.0 + 1)..=bottom.0)
            .map(Line)
            .find(|&line| mark(line) != SemanticPrompt::None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ansi::CursorShape;
    use crate::crosswords::CrosswordsSize;
    use crate::event::{VoidListener, WindowId};
    use crate::performer::handler::Processor;

    fn term(columns: usize, lines: usize, scrollback: usize) -> Crosswords<VoidListener> {
        Crosswords::new(
            CrosswordsSize::new(columns, lines),
            CursorShape::Block,
            VoidListener {},
            WindowId::from(0),
            0,
            scrollback,
        )
    }

    fn feed(cw: &mut Crosswords<VoidListener>, bytes: &str) {
        let bytes = bytes
            .replace("<A>", "\x1b]133;A\x07")
            .replace("<B>", "\x1b]133;B\x07")
            .replace("<I>", "\x1b]133;I\x07")
            .replace("<C>", "\x1b]133;C\x07")
            .replace("<D>", "\x1b]133;D;0\x07")
            .replace("<Ps>", "\x1b]133;P;k=s\x07");
        Processor::default().advance(cw, bytes.as_bytes());
    }

    fn last_text(cw: &Crosswords<VoidListener>) -> Option<String> {
        cw.command_output_text(&cw.last_command_output()?)
    }

    #[test]
    fn abandoned_prompts_do_not_hide_the_last_command() {
        let mut cw = term(30, 8, 100);
        feed(
            &mut cw,
            "<A>$ <B>ls\r\n<C>out\r\n<D><A>$ <B>half\r\n<A>$ <B>",
        );
        assert_eq!(last_text(&cw).as_deref(), Some("out"));

        let mut cw = term(30, 8, 100);
        feed(
            &mut cw,
            "<A>$ <B>ls\r\n<C>out\r\n<D><A>$ <B>\x1b[H\x1b[2J<A>$ <B>",
        );
        assert_eq!(last_text(&cw).as_deref(), Some("out"));

        let mut cw = term(30, 8, 100);
        feed(
            &mut cw,
            "<A>$ <B>ls\r\n<C>out\r\n<D><A>$ <B>true\r\n<C><D><A>$ <B>\r\n<A>$ <B>",
        );
        assert_eq!(cw.last_command_output().unwrap().output, None);
    }

    #[test]
    fn tags_cells_by_content() {
        let mut cw = term(20, 5, 100);
        feed(&mut cw, "<A>$ <B>ls\r\n<C>out");
        let row0 = &cw.grid[Line(0)];
        assert_eq!(row0[Column(0)].semantic_content(), SemanticContent::Prompt);
        assert_eq!(row0[Column(2)].semantic_content(), SemanticContent::Input);
        assert_eq!(row0[Column(5)].semantic_content(), SemanticContent::Output);
        assert_eq!(
            cw.grid[Line(1)][Column(0)].semantic_content(),
            SemanticContent::Output
        );
        assert_eq!(cw.grid[Line(0)].semantic_prompt, SemanticPrompt::Prompt);
        assert_eq!(cw.grid[Line(1)].semantic_prompt, SemanticPrompt::None);
    }

    #[test]
    fn no_shell_integration_has_no_output() {
        let mut cw = term(20, 5, 100);
        feed(&mut cw, "$ ls\r\nout\r\n$ ");
        assert!(!cw.semantic_prompt_seen());
        assert_eq!(cw.last_command_output(), None);
    }

    #[test]
    fn finished_command_at_idle_prompt() {
        let mut cw = term(20, 6, 100);
        feed(&mut cw, "<A>$ <B>ls\r\n<C>one\r\n  two\r\n<D><A>$ <B>ec");
        let out = cw.last_command_output().unwrap();
        assert_eq!(out.prompt, Some(Line(0)));
        assert!(!out.running);
        assert!(!out.truncated);
        assert_eq!(
            out.output,
            Some((Pos::new(Line(1), Column(0)), Pos::new(Line(2), Column(4))))
        );
        assert_eq!(cw.command_output_text(&out).unwrap(), "one\n  two");
    }

    #[test]
    fn running_command_reports_output_so_far() {
        let mut cw = term(20, 6, 100);
        feed(&mut cw, "<A>$ <B>make\r\n<C>");
        let out = cw.last_command_output().unwrap();
        assert!(out.running);
        assert_eq!(out.output, None);

        feed(&mut cw, "step 1\r\nstep 2");
        let out = cw.last_command_output().unwrap();
        assert!(out.running);
        assert_eq!(cw.command_output_text(&out).unwrap(), "step 1\nstep 2");

        feed(&mut cw, "\r\n<D>");
        assert!(!cw.last_command_output().unwrap().running);
    }

    #[test]
    fn command_without_output_is_distinguished() {
        let mut cw = term(20, 6, 100);
        feed(&mut cw, "<A>$ <B>true\r\n<C><D><A>$ <B>");
        let out = cw.last_command_output().unwrap();
        assert_eq!(out.prompt, Some(Line(0)));
        assert_eq!(out.output, None);
        assert_eq!(cw.grid[Line(1)].semantic_prompt, SemanticPrompt::Prompt);
    }

    #[test]
    fn previous_output_is_found_past_an_empty_command() {
        let mut cw = term(20, 8, 100);
        feed(
            &mut cw,
            "<A>$ <B>ls\r\n<C>out\r\n<D><A>$ <B>true\r\n<C><D><A>$ <B>",
        );
        let older = cw.command_output_at(Line(1)).unwrap();
        assert_eq!(older.prompt, Some(Line(0)));
        assert_eq!(cw.command_output_text(&older).unwrap(), "out");
        assert_eq!(cw.last_command_output().unwrap().output, None);
    }

    #[test]
    fn multi_line_prompt_and_wrapped_input_are_excluded() {
        let mut cw = term(10, 8, 100);
        feed(
            &mut cw,
            "<A>user@host\r\n<Ps>$ <B>echo 0123456789\r\n<C>0123456789\r\n<D><A>$ <B>",
        );
        assert_eq!(
            cw.grid[Line(1)].semantic_prompt,
            SemanticPrompt::PromptContinuation
        );
        assert_eq!(
            cw.grid[Line(2)].semantic_prompt,
            SemanticPrompt::PromptContinuation
        );
        let out = cw.last_command_output().unwrap();
        assert_eq!(out.prompt, Some(Line(0)));
        assert_eq!(cw.command_output_text(&out).unwrap(), "0123456789");
    }

    #[test]
    fn input_until_end_of_line() {
        let mut cw = term(20, 6, 100);
        feed(&mut cw, "<A>$ <I>ls\r\nout\r\n");
        assert_eq!(cw.grid.semantic.content, SemanticContent::Output);
        let out = cw.last_command_output().unwrap();
        assert!(!out.running);
        assert_eq!(cw.command_output_text(&out).unwrap(), "out");
    }

    #[test]
    fn output_on_the_command_row_skips_the_command() {
        let mut cw = term(20, 6, 100);
        feed(&mut cw, "<A>$ <B>ls<C> out\r\nmore");
        let out = cw.last_command_output().unwrap();
        assert_eq!(cw.command_output_text(&out).unwrap(), "out\nmore");
    }

    #[test]
    fn text_left_of_a_mid_line_prompt_is_not_output() {
        let mut cw = term(20, 6, 100);
        feed(&mut cw, "junk<A>$ <B>ls\r\n<C>out\r\n<D><A>$ <B>");
        let out = cw.last_command_output().unwrap();
        assert_eq!(cw.command_output_text(&out).unwrap(), "out");
    }

    #[test]
    fn output_in_scrollback() {
        let mut cw = term(20, 4, 100);
        let lines: Vec<String> = (0..12).map(|i| format!("line {i}")).collect();
        feed(
            &mut cw,
            &format!("<A>$ <B>seq\r\n<C>{}\r\n<D><A>$ <B>", lines.join("\r\n")),
        );
        let out = cw.last_command_output().unwrap();
        assert!(out.prompt.unwrap() < Line(0));
        assert!(!out.truncated);
        assert_eq!(cw.command_output_text(&out).unwrap(), lines.join("\n"));
    }

    #[test]
    fn evicted_prompt_reports_truncated_output() {
        let mut cw = term(20, 4, 5);
        let lines: Vec<String> = (0..30).map(|i| format!("line {i}")).collect();
        feed(
            &mut cw,
            &format!("<A>$ <B>seq\r\n<C>{}", lines.join("\r\n")),
        );
        let out = cw.last_command_output().unwrap();
        assert!(out.running);
        assert!(out.truncated);
        assert_eq!(out.prompt, None);
        let text = cw.command_output_text(&out).unwrap();
        assert!(text.ends_with("line 28\nline 29"));
        assert!(!text.contains("seq"));
    }

    #[test]
    fn output_survives_reflow() {
        let mut cw = term(20, 6, 100);
        feed(
            &mut cw,
            "<A>$ <B>cat\r\n<C>abcdefghijklmnop\r\nxyz\r\n<D><A>$ <B>",
        );
        cw.resize(CrosswordsSize::new(6, 6));
        let out = cw.last_command_output().unwrap();
        assert_eq!(
            cw.command_output_text(&out).unwrap(),
            "abcdefghijklmnop\nxyz"
        );
        cw.resize(CrosswordsSize::new(30, 6));
        let out = cw.last_command_output().unwrap();
        assert_eq!(
            cw.command_output_text(&out).unwrap(),
            "abcdefghijklmnop\nxyz"
        );
    }

    #[test]
    fn alternate_screen_is_not_output() {
        let mut cw = term(20, 6, 100);
        feed(&mut cw, "<A>$ <B>vim\r\n<C>before\r\n\x1b[?1049h");
        assert_eq!(cw.last_command_output(), None);
        feed(&mut cw, "editor text\r\n<A>fake<B>");
        feed(&mut cw, "\x1b[?1049l<D><A>$ <B>");
        let out = cw.last_command_output().unwrap();
        assert_eq!(cw.command_output_text(&out).unwrap(), "before");
    }

    #[test]
    fn full_reset_forgets_semantic_state() {
        let mut cw = term(20, 6, 100);
        feed(&mut cw, "<A>$ <B>ls\r\n<C>out");
        feed(&mut cw, "\x1bc");
        assert!(!cw.semantic_prompt_seen());
        assert_eq!(cw.last_command_output(), None);
    }
}
