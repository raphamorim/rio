use rio_backend::sugarloaf::text::DrawOpts;
use rio_backend::sugarloaf::Sugarloaf;

const CONFIRM: &str = "yes (y)";
const DISMISS: &str = "no (n)";

#[derive(Clone, Copy, PartialEq)]
pub enum SessionPromptKind {
    SaveOnExit,
    ResumeOnLaunch,
}

/// Session save/resume overlay; same shape as `ConfirmQuit`, but which
/// action the keys gate depends on the active kind.
#[derive(Default)]
pub struct SessionPrompt {
    active: Option<SessionPromptKind>,
    /// Transient "session saved" flash; cleared by a scheduler event.
    saved_notice: bool,
}

impl SessionPrompt {
    #[inline]
    pub fn is_active(&self) -> bool {
        self.active.is_some()
    }

    #[inline]
    pub fn kind(&self) -> Option<SessionPromptKind> {
        self.active
    }

    #[inline]
    pub fn set_active(&mut self, kind: Option<SessionPromptKind>) {
        self.active = kind;
    }

    #[inline]
    pub fn set_saved_notice(&mut self, on: bool) {
        self.saved_notice = on;
    }

    /// `dimensions` is `(window_width, window_height, scale_factor)`,
    /// matching the other overlays' `render` signature.
    pub fn render(&self, sugarloaf: &mut Sugarloaf, dimensions: (f32, f32, f32)) {
        if self.saved_notice {
            self.render_saved_notice(sugarloaf, dimensions);
        }
        let Some(kind) = self.active else {
            return;
        };
        let heading = match kind {
            SessionPromptKind::SaveOnExit => "save session?",
            SessionPromptKind::ResumeOnLaunch => "resume last session?",
        };

        let (width, height, scale) = dimensions;
        let win_w = width / scale;
        let win_h = height / scale;

        let full_text = format!("{}  {}  /  {}", heading, CONFIRM, DISMISS);
        let padding_x = 12.0;
        let padding_y = 6.0;
        let text_h = 16.0;
        let box_w = full_text.len() as f32 * 7.5 + padding_x * 2.0;
        let box_h = text_h + padding_y * 2.0;
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
        ui.draw(
            text_x + heading_w,
            text_y,
            &format!("  {CONFIRM}  /  {DISMISS}"),
            &gray_opts,
        );
    }

    fn render_saved_notice(
        &self,
        sugarloaf: &mut Sugarloaf,
        dimensions: (f32, f32, f32),
    ) {
        const NOTICE: &str = "session saved";
        let (width, _, scale) = dimensions;
        let win_w = width / scale;

        let padding_x = 10.0;
        let padding_y = 5.0;
        let box_w = NOTICE.len() as f32 * 7.5 + padding_x * 2.0;
        let box_h = 16.0 + padding_y * 2.0;
        let box_x = win_w - box_w - 12.0;
        let box_y = 40.0;

        sugarloaf.rect(
            None,
            box_x,
            box_y,
            box_w,
            box_h,
            [0.0, 0.0, 0.0, 0.9],
            0.0,
            20,
        );
        let opts = DrawOpts {
            font_size: 13.0,
            color: [140, 220, 140, 255],
            ..DrawOpts::default()
        };
        sugarloaf.text_mut().draw(
            box_x + padding_x,
            box_y + padding_y + 2.0,
            NOTICE,
            &opts,
        );
    }
}
