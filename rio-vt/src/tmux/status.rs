//! tmux's status line.
//!
//! tmux draws no status line for a control client, but subscriptions report
//! what it would have drawn, and report it again whenever that changes. The
//! line is `status-format`, so it holds whatever the user set up, in tmux's
//! own style markup. This reads that markup, leaving the frontend coloured
//! text to draw.
//!
//! The window list is left out: the tabs already show those windows.

/// Names the subscription for the line itself.
pub const LINE_SUBSCRIPTION: &str = "rio-status-line";
/// Names the subscription for what surrounds it: see [`StatusBar::parse`].
pub const BAR_SUBSCRIPTION: &str = "rio-status-bar";

/// Ask tmux to keep us up to date. tmux older than 3.2 refuses, and such a
/// session goes without a status line.
pub const SUBSCRIBE: [&str; 2] = [
    "refresh-client -B 'rio-status-line::#{T:status-format[0]}'",
    "refresh-client -B 'rio-status-bar::#{status} #{status-position} #{E:status-style}'",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StatusColor {
    /// The terminal's own foreground or background.
    #[default]
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatusStyle {
    pub fg: StatusColor,
    pub bg: StatusColor,
    pub bold: bool,
    pub italic: bool,
    pub reverse: bool,
}

impl StatusStyle {
    /// Apply a style's fields, each relative to `default`.
    fn apply(&mut self, tokens: &str, default: &StatusStyle) {
        for token in tokens.split([',', ' ']) {
            self.apply_token(token, default);
        }
    }

    fn apply_token(&mut self, token: &str, default: &StatusStyle) {
        if let Some(color) = token.strip_prefix("fg=") {
            self.fg = parse_color(color, default.fg).unwrap_or(self.fg);
        } else if let Some(color) = token.strip_prefix("bg=") {
            self.bg = parse_color(color, default.bg).unwrap_or(self.bg);
        } else {
            match token {
                "default" => *self = *default,
                "none" => {
                    self.bold = false;
                    self.italic = false;
                    self.reverse = false;
                }
                "bold" | "bright" => self.bold = true,
                "nobold" | "nobright" => self.bold = false,
                "italics" => self.italic = true,
                "noitalics" => self.italic = false,
                "reverse" => self.reverse = true,
                "noreverse" => self.reverse = false,
                _ => (),
            }
        }
    }
}

/// `default` is what `default` means here, which is not always the terminal's
/// colour. `None` for what isn't a colour.
fn parse_color(name: &str, default: StatusColor) -> Option<StatusColor> {
    const NAMES: [&str; 8] = [
        "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
    ];

    let position = |name: &str| NAMES.iter().position(|known| *known == name);
    if name == "default" {
        Some(default)
    } else if name == "terminal" {
        Some(StatusColor::Default)
    } else if let Some(index) = position(name) {
        Some(StatusColor::Indexed(index as u8))
    } else if let Some(index) = name.strip_prefix("bright").and_then(position) {
        Some(StatusColor::Indexed(index as u8 + 8))
    } else if let Some(hex) = name.strip_prefix('#') {
        let hex = hex.get(..6).filter(|_| hex.len() == 6)?;
        let channel = |at: usize| u8::from_str_radix(hex.get(at..at + 2)?, 16).ok();
        Some(StatusColor::Rgb(channel(0)?, channel(2)?, channel(4)?))
    } else {
        name.strip_prefix("colour")
            .or_else(|| name.strip_prefix("color"))?
            .parse()
            .ok()
            .map(StatusColor::Indexed)
    }
}

/// What surrounds the line, from `<status> <status-position> <status-style>`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatusBar {
    pub visible: bool,
    pub top: bool,
    pub style: StatusStyle,
}

impl StatusBar {
    pub fn parse(value: &str) -> StatusBar {
        let mut fields = value.splitn(3, ' ');
        let visible = fields.next().is_some_and(|status| status != "off");
        let top = fields.next() == Some("top");
        let mut style = StatusStyle::default();
        if let Some(tokens) = fields.next() {
            style.apply(tokens, &StatusStyle::default());
        }
        StatusBar {
            visible,
            top,
            style,
        }
    }
}

/// Text in one style.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusRun {
    pub text: String,
    pub style: StatusStyle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Align {
    Left,
    Centre,
    Right,
}

/// The status line as the frontend should display it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TmuxStatus {
    pub bar: StatusBar,
    pub left: Vec<StatusRun>,
    pub centre: Vec<StatusRun>,
    pub right: Vec<StatusRun>,
}

impl TmuxStatus {
    /// Read an expanded `status-format`.
    pub fn parse(bar: StatusBar, line: &str) -> TmuxStatus {
        let mut parser = LineParser {
            status: TmuxStatus {
                bar,
                ..TmuxStatus::default()
            },
            align: Align::Left,
            in_list: false,
            style: bar.style,
            default: bar.style,
            text: String::new(),
        };

        let mut rest = line;
        while let Some(hash) = rest.find('#') {
            parser.text.push_str(&rest[..hash]);
            rest = &rest[hash..];
            if let Some(after) = rest.strip_prefix("##") {
                parser.text.push('#');
                rest = after;
            } else if let Some((markup, after)) =
                rest.strip_prefix("#[").and_then(|r| r.split_once(']'))
            {
                parser.flush();
                parser.markup(markup);
                rest = after;
            } else {
                parser.text.push('#');
                rest = &rest[1..];
            }
        }
        parser.text.push_str(rest);
        parser.flush();
        parser.status
    }
}

struct LineParser {
    status: TmuxStatus,
    align: Align,
    in_list: bool,
    style: StatusStyle,
    default: StatusStyle,
    text: String,
}

impl LineParser {
    fn flush(&mut self) {
        let text = std::mem::take(&mut self.text);
        if text.is_empty() || self.in_list {
            return;
        }
        let runs = match self.align {
            Align::Left => &mut self.status.left,
            Align::Centre => &mut self.status.centre,
            Align::Right => &mut self.status.right,
        };
        match runs.last_mut() {
            Some(run) if run.style == self.style => run.text.push_str(&text),
            _ => runs.push(StatusRun {
                text,
                style: self.style,
            }),
        }
    }

    fn markup(&mut self, markup: &str) {
        for token in markup.split([',', ' ']) {
            match token {
                "align=left" => self.align = Align::Left,
                "align=centre" | "align=absolute-centre" => self.align = Align::Centre,
                "align=right" => self.align = Align::Right,
                "nolist" => self.in_list = false,
                "push-default" => self.default = self.style,
                "pop-default" => self.default = self.status.bar.style,
                _ if token.starts_with("list=") => self.in_list = true,
                _ => {
                    let default = self.default;
                    self.style.apply_token(token, &default);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What tmux 3.2a reports with its default configuration.
    const DEFAULT_LINE: &str = "#[align=left range=left default]#[push-default][s] \
        #[pop-default]#[norange default]#[list=on align=left]#[list=left-marker]<\
        #[list=right-marker]>#[list=on]#[range=window|0 list=focus default]\
        #[push-default]0:bash*#[pop-default]#[norange list=on default]\
        #[nolist align=right range=right default]#[push-default]\"host\" 14:23 06-Nov-26\
        #[pop-default]#[norange default]";

    fn green_bar() -> StatusBar {
        StatusBar::parse("on bottom bg=green,fg=black")
    }

    fn texts(runs: &[StatusRun]) -> Vec<&str> {
        runs.iter().map(|run| run.text.as_str()).collect()
    }

    #[test]
    fn the_bar_comes_from_tmux_options() {
        let bar = green_bar();
        assert!(bar.visible);
        assert!(!bar.top);
        assert_eq!(bar.style.bg, StatusColor::Indexed(2));
        assert_eq!(bar.style.fg, StatusColor::Indexed(0));

        assert!(StatusBar::parse("2 top default").top);
        assert!(StatusBar::parse("2 top default").visible);
        assert!(!StatusBar::parse("off bottom default").visible);
    }

    #[test]
    fn the_default_line_has_its_sides_but_not_the_window_list() {
        let status = TmuxStatus::parse(green_bar(), DEFAULT_LINE);
        assert_eq!(texts(&status.left), ["[s] "]);
        assert!(status.centre.is_empty());
        assert_eq!(texts(&status.right), ["\"host\" 14:23 06-Nov-26"]);
        assert_eq!(status.left[0].style, green_bar().style);
    }

    #[test]
    fn styles_change_along_the_line() {
        let line = "a#[fg=colour196,bold]b#[fg=#102030,nobold,italics]c#[default]d";
        let status = TmuxStatus::parse(green_bar(), line);
        let base = green_bar().style;
        assert_eq!(
            status.left,
            [
                StatusRun {
                    text: "a".into(),
                    style: base
                },
                StatusRun {
                    text: "b".into(),
                    style: StatusStyle {
                        fg: StatusColor::Indexed(196),
                        bold: true,
                        ..base
                    }
                },
                StatusRun {
                    text: "c".into(),
                    style: StatusStyle {
                        fg: StatusColor::Rgb(0x10, 0x20, 0x30),
                        italic: true,
                        ..base
                    }
                },
                StatusRun {
                    text: "d".into(),
                    style: base
                },
            ]
        );
    }

    #[test]
    fn default_follows_push_and_pop() {
        let line = "#[fg=red]#[push-default]#[fg=blue]#[default]a\
                    #[pop-default]#[default]b";
        let status = TmuxStatus::parse(green_bar(), line);
        assert_eq!(status.left[0].style.fg, StatusColor::Indexed(1));
        assert_eq!(status.left[1].style.fg, StatusColor::Indexed(0));
    }

    #[test]
    fn a_default_colour_is_the_bars_and_terminal_is_the_terminals() {
        let status = TmuxStatus::parse(green_bar(), "#[fg=red,fg=default]a");
        assert_eq!(status.left[0].style.fg, StatusColor::Indexed(0));
        let status = TmuxStatus::parse(green_bar(), "#[bg=terminal]a");
        assert_eq!(status.left[0].style.bg, StatusColor::Default);
    }

    #[test]
    fn text_goes_where_it_is_aligned() {
        let line = "l#[align=centre]c#[align=absolute-centre]d#[align=right]r";
        let status = TmuxStatus::parse(green_bar(), line);
        assert_eq!(texts(&status.left), ["l"]);
        assert_eq!(texts(&status.centre), ["cd"]);
        assert_eq!(texts(&status.right), ["r"]);
    }

    #[test]
    fn hashes_can_be_literal() {
        let status = TmuxStatus::parse(green_bar(), "##[a] #b #[fg=red");
        assert_eq!(texts(&status.left), ["#[a] #b #[fg=red"]);
    }

    #[test]
    fn what_is_not_a_colour_changes_nothing() {
        for name in [
            "#12",
            "#gggggg",
            "#é12345",
            "colourful",
            "colour999",
            "mauve",
        ] {
            assert_eq!(parse_color(name, StatusColor::Default), None, "{name}");
        }
        assert_eq!(
            parse_color("brightred", StatusColor::Default),
            Some(StatusColor::Indexed(9))
        );
    }

    #[test]
    fn multibyte_text_survives() {
        let status = TmuxStatus::parse(green_bar(), "é#[fg=red]→ ✓");
        assert_eq!(texts(&status.left), ["é", "→ ✓"]);
    }
}
