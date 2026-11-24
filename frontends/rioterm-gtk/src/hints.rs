//! Mouse hints, as Rio has them: with a rule's modifiers held, the OSC 8
//! link or regex match under the pointer is underlined, and a click runs
//! the rule's action.

use rio_backend::config::hints::{
    Hint, HintAction, HintCommand, HintInternalAction, HintMouse,
};
use rio_backend::hints::{HintMatch, LogicalLine};
use rio_vt::crosswords::pos::{Column, Pos};
use rio_vt::crosswords::Crosswords;
use rio_vt::event::EventListener;
use rio_vt::key::Modifiers;
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Default)]
pub struct MouseHints {
    regexes: RefCell<FxHashMap<String, Option<Rc<onig::Regex>>>>,
    rules: RefCell<Vec<Rc<Hint>>>,
}

pub enum Outcome {
    Copy(String),
    Paste(String),
    Select(Pos, Pos),
    Open(String),
    Run(String, Vec<String>),
}

impl MouseHints {
    pub fn set_rules(&self, rules: &[Hint]) {
        *self.rules.borrow_mut() = rules.iter().cloned().map(Rc::new).collect();
    }

    fn regex(&self, pattern: &str) -> Option<Rc<onig::Regex>> {
        self.regexes
            .borrow_mut()
            .entry(pattern.to_string())
            .or_insert_with(|| onig::Regex::new(pattern).ok().map(Rc::new))
            .clone()
    }

    /// The hint under `point` with `mods` held, if any rule matches.
    pub fn at<U: EventListener>(
        &self,
        term: &Crosswords<U>,
        point: Pos,
        mods: Modifiers,
    ) -> Option<HintMatch> {
        if point.row.0 >= term.screen_lines() as i32 || point.col.0 >= term.columns() {
            return None;
        }
        let mut line: Option<Option<LogicalLine>> = None;
        for rule in self.rules.borrow().iter() {
            if !rule.mouse.enabled || !mods_match(&rule.mouse, mods) {
                continue;
            }
            if rule.hyperlinks {
                if let Some(found) = hyperlink_at(term, point) {
                    return Some(found);
                }
            }
            let Some(regex) = rule.regex.as_deref().and_then(|p| self.regex(p)) else {
                continue;
            };
            let line = line.get_or_insert_with(|| LogicalLine::extract(term, point));
            if let Some(found) = line
                .as_ref()
                .and_then(|line| line.match_at(term, point, &regex, rule.post_processing))
            {
                return Some(HintMatch {
                    text: found.text,
                    start: found.start,
                    end: found.end,
                    hint: rule.clone(),
                });
            }
        }
        None
    }
}

/// What a click on `hint` does.
pub fn outcome(hint: &HintMatch, cwd: Option<&std::path::Path>) -> Outcome {
    let target = || {
        rio_backend::hints::resolve_path_for_opening(&hint.text, cwd)
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|| hint.text.clone())
    };
    match &hint.hint.action {
        HintAction::Action { action } => match action {
            HintInternalAction::Copy => Outcome::Copy(hint.text.clone()),
            HintInternalAction::Paste => Outcome::Paste(hint.text.clone()),
            HintInternalAction::Select | HintInternalAction::MoveViModeCursor => {
                Outcome::Select(hint.start, hint.end)
            }
            HintInternalAction::Open => Outcome::Open(target()),
        },
        HintAction::Command { command } => match command {
            HintCommand::Simple(program) => Outcome::Run(program.clone(), vec![target()]),
            HintCommand::WithArgs { program, args } => {
                let mut args = args.clone();
                args.push(target());
                Outcome::Run(program.clone(), args)
            }
        },
    }
}

fn mods_match(mouse: &HintMouse, mods: Modifiers) -> bool {
    mouse.mods.iter().all(|name| match name.as_str() {
        "Shift" => mods.contains(Modifiers::SHIFT),
        "Control" | "Ctrl" => mods.contains(Modifiers::CTRL),
        "Alt" => mods.contains(Modifiers::ALT),
        "Super" | "Cmd" | "Command" => mods.contains(Modifiers::SUPER),
        _ => false,
    })
}

/// The OSC 8 link under `point`, spanning the cells on its row that
/// carry the same link.
fn hyperlink_at<U: EventListener>(term: &Crosswords<U>, point: Pos) -> Option<HintMatch> {
    let link = term.cell_hyperlink(point.row, point.col)?;
    let same = |col: Column| term.cell_hyperlink(point.row, col).as_ref() == Some(&link);
    let mut start = point.col;
    while start.0 > 0 && same(start - 1) {
        start -= 1;
    }
    let mut end = point.col;
    while end.0 + 1 < term.columns() && same(end + 1) {
        end += 1;
    }
    Some(HintMatch {
        text: link.uri().to_string(),
        start: Pos::new(point.row, start),
        end: Pos::new(point.row, end),
        hint: Rc::new(Hint {
            regex: None,
            hyperlinks: true,
            post_processing: false,
            persist: false,
            action: HintAction::Action {
                action: HintInternalAction::Open,
            },
            mouse: HintMouse::default(),
            binding: None,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rio_vt::crosswords::pos::Line;
    use rio_vt::crosswords::CrosswordsSize;
    use rio_vt::event::{VoidListener, WindowId};

    fn term(text: &str) -> Crosswords<VoidListener> {
        let mut term = Crosswords::new(
            CrosswordsSize::new(40, 3),
            rio_vt::ansi::CursorShape::Block,
            VoidListener,
            WindowId::from(0),
            0,
            100,
        );
        let mut parser = rio_vt::performer::handler::Processor::default();
        parser.advance(&mut term, text.as_bytes());
        term
    }

    fn hints() -> MouseHints {
        let mut rules = rio_backend::config::hints::Hints::default().rules;
        for rule in &mut rules {
            rule.mouse.mods = vec!["Alt".to_string()];
        }
        let hints = MouseHints::default();
        hints.set_rules(&rules);
        hints
    }

    fn at(row: i32, col: usize) -> Pos {
        Pos::new(Line(row), Column(col))
    }

    #[test]
    fn urls_need_the_rule_modifiers() {
        let term = term("see https://rio.dev/x, ok");
        let hints = hints();
        assert!(hints.at(&term, at(0, 8), Modifiers::empty()).is_none());
        let found = hints
            .at(&term, at(0, 8), Modifiers::ALT)
            .expect("url under alt");
        assert_eq!(found.text, "https://rio.dev/x");
        assert_eq!((found.start.col.0, found.end.col.0), (4, 20));
        assert!(hints.at(&term, at(0, 1), Modifiers::ALT).is_none());
    }

    #[test]
    fn osc8_links_span_their_cells() {
        let term = term("a \x1b]8;;https://osc.example\x1b\\link\x1b]8;;\x1b\\ b");
        let found = hints()
            .at(&term, at(0, 3), Modifiers::ALT)
            .expect("osc 8 link");
        assert_eq!(found.text, "https://osc.example");
        assert_eq!((found.start.col.0, found.end.col.0), (2, 5));
        assert!(
            matches!(outcome(&found, None), Outcome::Open(uri) if uri == "https://osc.example")
        );
    }
}
