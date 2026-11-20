// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use rio_backend::paste;
use rio_backend::sugarloaf::text::DrawOpts;
use rio_backend::sugarloaf::Sugarloaf;

const CONFIRM: &str = "paste (y)";
const DISMISS: &str = "cancel (n)";
/// How much of the paste the dialog shows.
const PREVIEW_CHARS: usize = 60;

/// A paste held back until the user confirms it, because it could run
/// commands: see `paste-protection` and `paste-bracketed-safe`.
pub struct ConfirmPaste {
    protection: bool,
    bracketed_safe: bool,
    pending: Option<String>,
}

impl ConfirmPaste {
    pub fn new(protection: bool, bracketed_safe: bool) -> Self {
        Self {
            protection,
            bracketed_safe,
            pending: None,
        }
    }

    /// Whether pasting `text` should be confirmed first, `bracketed` being
    /// whether the program asked for bracketed paste. Bracketed, line
    /// breaks are the program's to handle when `paste-bracketed-safe` is
    /// on, and only a bracket end is asked about; otherwise any line
    /// break is too.
    pub fn needs_confirmation(&self, text: &str, bracketed: bool) -> bool {
        if !self.protection {
            return false;
        }
        if bracketed && self.bracketed_safe {
            !paste::is_safe_with(text, true)
        } else {
            !paste::is_safe(text)
        }
    }

    /// Hold `text` until it's confirmed or cancelled; a later paste
    /// replaces it.
    pub fn hold(&mut self, text: String) {
        self.pending = Some(text);
    }

    #[inline]
    pub fn is_active(&self) -> bool {
        self.pending.is_some()
    }

    /// The paste, confirmed.
    pub fn take(&mut self) -> Option<String> {
        self.pending.take()
    }

    pub fn cancel(&mut self) {
        self.pending = None;
    }

    /// What the dialog says: the bracket end if the text holds one,
    /// otherwise that its line breaks may run commands.
    fn heading(text: &str) -> &'static str {
        if !paste::is_safe_with(text, true) {
            "paste holds a bracketed paste end"
        } else {
            "paste may run commands"
        }
    }

    /// The paste's first line, control characters shown as spaces.
    fn preview(text: &str) -> String {
        let first = text
            .split(['\r', '\n'])
            .find(|line| !line.trim().is_empty())
            .unwrap_or("");
        let mut preview: String = first
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .take(PREVIEW_CHARS)
            .collect();
        if first.chars().count() > PREVIEW_CHARS {
            preview.push('…');
        }
        preview
    }

    /// `dimensions` is `(window_width, window_height, scale_factor)`,
    /// matching the other overlays' `render` signature.
    pub fn render(&self, sugarloaf: &mut Sugarloaf, dimensions: (f32, f32, f32)) {
        let Some(text) = &self.pending else {
            return;
        };

        let (width, height, scale) = dimensions;
        let win_w = width / scale;
        let win_h = height / scale;

        let heading = Self::heading(text);
        let choices = format!("  {}  /  {}", CONFIRM, DISMISS);
        let preview = Self::preview(text);
        let padding_x = 12.0;
        let padding_y = 6.0;
        let line_h = 18.0;
        let widest = (heading.chars().count() + choices.chars().count())
            .max(preview.chars().count());
        let box_w = (widest as f32 * 7.5 + padding_x * 2.0).min(win_w);
        let box_h = line_h * 2.0 + padding_y * 2.0;
        let box_x = (win_w - box_w) / 2.0;
        let box_y = (win_h - box_h) / 2.0;

        sugarloaf.rect(
            None,
            box_x,
            box_y,
            box_w,
            box_h,
            [0.0, 0.0, 0.0, 1.0],
            0.0,
            20,
        );

        let heading_opts = DrawOpts {
            font_size: 13.0,
            color: [255, 255, 255, 255],
            ..DrawOpts::default()
        };
        let gray_opts = DrawOpts {
            font_size: 13.0,
            color: [166, 166, 166, 255],
            ..DrawOpts::default()
        };

        let text_x = box_x + padding_x;
        let text_y = box_y + padding_y + 2.0;

        let ui = sugarloaf.text_mut();
        let heading_w = ui.draw(text_x, text_y, heading, &heading_opts);
        ui.draw(text_x + heading_w, text_y, &choices, &gray_opts);
        ui.draw(text_x, text_y + line_h, &preview, &gray_opts);
    }
}

#[cfg(test)]
mod tests {
    use super::ConfirmPaste;

    // As configured by default: line breaks are asked about only when
    // the paste isn't bracketed; a bracket end always is.
    #[test]
    fn asks_about_what_could_run_commands() {
        let confirm = ConfirmPaste::new(true, true);
        assert!(!confirm.needs_confirmation("one line", false));
        assert!(!confirm.needs_confirmation("one line", true));
        assert!(confirm.needs_confirmation("ls\nrm -rf ~", false));
        assert!(confirm.needs_confirmation("ls\r", false));
        assert!(!confirm.needs_confirmation("ls\nrm -rf ~", true));
        assert!(confirm.needs_confirmation("a\x1b[201~rm -rf ~\n", true));
        assert!(confirm.needs_confirmation("a\u{9b}201~rm", true));
    }

    #[test]
    fn bracketed_line_breaks_are_asked_about_when_not_trusted() {
        let confirm = ConfirmPaste::new(true, false);
        assert!(confirm.needs_confirmation("ls\nrm -rf ~", true));
        assert!(!confirm.needs_confirmation("one line", true));
    }

    #[test]
    fn nothing_is_asked_with_protection_off() {
        let confirm = ConfirmPaste::new(false, true);
        assert!(!confirm.needs_confirmation("ls\nrm -rf ~", false));
        assert!(!confirm.needs_confirmation("a\x1b[201~", true));
    }

    #[test]
    fn holds_one_paste_until_taken_or_cancelled() {
        let mut confirm = ConfirmPaste::new(true, true);
        assert!(!confirm.is_active());
        confirm.hold("first\n".into());
        confirm.hold("second\n".into());
        assert!(confirm.is_active());
        assert_eq!(confirm.take().as_deref(), Some("second\n"));
        assert!(!confirm.is_active());
        confirm.hold("third\n".into());
        confirm.cancel();
        assert_eq!(confirm.take(), None);
    }

    #[test]
    fn the_dialog_says_what_it_holds() {
        assert_eq!(
            ConfirmPaste::heading("ls\nrm -rf ~"),
            "paste may run commands"
        );
        assert_eq!(
            ConfirmPaste::heading("a\x1b[201~b"),
            "paste holds a bracketed paste end"
        );
        assert_eq!(ConfirmPaste::preview("\n  \nls\x1b -la\nrm"), "ls  -la");
        let long = "x".repeat(80);
        assert_eq!(
            ConfirmPaste::preview(&long).chars().count(),
            super::PREVIEW_CHARS + 1
        );
    }
}
