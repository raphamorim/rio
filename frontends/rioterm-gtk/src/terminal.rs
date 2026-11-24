//! The terminal widget: one session, drawn by Sugarloaf's CPU renderer
//! into a frame GTK shows as textures, one per strip of rows, so a
//! frame that changed a few rows uploads only their strips.

use crate::input;
use crate::render::{Inputs, Renderer};
use crate::session::{Formatter, SelectionKind, Session};
use crate::shared::Shared;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, gio, glib, graphene};
use rio_vt::clipboard::ClipboardType;
use rio_vt::crosswords::grid::Scroll;
use rio_vt::crosswords::pos::Side;
use rio_vt::crosswords::CrosswordsSize;
use rio_vt::key::{KeyAction, Modifiers};
use std::rc::Rc;

/// Rows of pixels per texture.
const STRIP: u32 = 64;

glib::wrapper! {
    pub struct TerminalView(ObjectSubclass<imp::TerminalView>)
        @extends gtk::Widget,
        @implements gtk::Buildable, gtk::ConstraintTarget;
}

impl TerminalView {
    pub fn new(shared: &Rc<Shared>, working_dir: Option<String>) -> Result<Self, String> {
        let view: Self = glib::Object::new();
        view.imp().start(shared, working_dir)?;
        Ok(view)
    }

    pub fn copy(&self) {
        if let Some(text) = self.imp().session().and_then(|s| s.selection_text()) {
            WidgetExt::clipboard(self).set_text(&text);
        }
    }

    pub fn paste(&self) {
        self.paste_from(WidgetExt::clipboard(self));
    }

    pub fn paste_from_primary(&self) {
        self.paste_from(WidgetExt::primary_clipboard(self));
    }

    fn paste_from(&self, clipboard: gdk::Clipboard) {
        let view = self.downgrade();
        glib::spawn_future_local(async move {
            if let Ok(Some(text)) = clipboard.read_text_future().await {
                if let Some(view) = view.upgrade() {
                    view.paste_text(text.to_string());
                }
            }
        });
    }

    /// Paste, asking first when the text could run commands.
    pub fn paste_text(&self, text: String) {
        let imp = self.imp();
        let (Some(session), Some(shared)) = (imp.session(), imp.shared.get()) else {
            return;
        };
        let (protect, bracketed_safe) = {
            let config = shared.config.borrow();
            (config.paste_protection, config.paste_bracketed_safe)
        };
        let safe = if bracketed_safe {
            session.paste_is_safe(&text)
        } else {
            rio_vt::paste::is_safe(&text)
        };
        if !protect || safe {
            session.paste(text);
            return;
        }
        self.confirm_paste(text);
    }

    fn confirm_paste(&self, text: String) {
        let window = WidgetExt::root(self).and_downcast::<gtk::Window>();
        let preview: String = text.chars().take(400).collect();
        let view = self.downgrade();
        crate::dialog::confirm(
            window.as_ref(),
            "Paste text that may run commands?",
            &preview,
            "Paste",
            move |accepted| {
                let Some(view) = view.upgrade() else {
                    return;
                };
                if accepted {
                    if let Some(session) = view.imp().session() {
                        session.paste(text.clone());
                    }
                }
                view.grab_focus();
            },
        );
    }

    /// Text typed into the terminal, as if from the keyboard.
    pub fn type_text(&self, text: &str) {
        if let Some(session) = self.imp().session() {
            session.text(text);
        }
    }

    pub fn select_all(&self) {
        if let Some(session) = self.imp().session() {
            let lines = self.imp().cells.get().1;
            session.selection_begin(0, 0, Side::Left, SelectionKind::Simple);
            session.selection_update(
                lines.saturating_sub(1) as u16,
                u16::MAX,
                Side::Right,
            );
        }
    }

    pub fn scroll(&self, scroll: Scroll) {
        if let Some(session) = self.imp().session() {
            session.scroll(scroll);
        }
    }

    pub fn page_lines(&self) -> i32 {
        self.imp().cells.get().1 as i32
    }

    pub fn clear_history(&self) {
        if let Some(session) = self.imp().session() {
            let mut term = session.terminal.lock();
            term.clear_saved_history();
            term.mark_fully_damaged();
        }
        self.queue_draw();
    }

    /// Change the font size by `step` points, or back to the configured
    /// size with `None`.
    pub fn zoom(&self, step: Option<f32>) {
        let imp = self.imp();
        let Some(shared) = imp.shared.get() else {
            return;
        };
        let base = shared.config.borrow().fonts.size;
        if let Some(render) = imp.render.borrow_mut().as_mut() {
            let size = step.map_or(base, |step| render.font_size() + step);
            render.set_font_size(size);
        }
        imp.relayout();
    }

    pub fn working_dir(&self) -> Option<String> {
        self.imp().session()?.working_dir()
    }

    pub fn foreground_program(&self) -> Option<String> {
        self.imp().session()?.foreground_program()
    }

    pub(crate) fn ring_bell(&self) {
        let audio = self
            .imp()
            .shared
            .get()
            .map(|shared| shared.config.borrow().bell.audio)
            .unwrap_or(false);
        if audio {
            self.error_bell();
        }
        self.emit_by_name::<()>("bell", &[]);
    }

    pub(crate) fn clipboard_load(&self, kind: ClipboardType, format: Formatter) {
        let clipboard = match kind {
            ClipboardType::Clipboard => WidgetExt::clipboard(self),
            ClipboardType::Selection => WidgetExt::primary_clipboard(self),
        };
        let view = self.downgrade();
        glib::spawn_future_local(async move {
            let text = clipboard.read_text_future().await.ok().flatten();
            let Some(view) = view.upgrade() else {
                return;
            };
            if let Some(session) = view.imp().session() {
                let allowed = view.has_focus();
                let reply = match text {
                    Some(text) if allowed => format(text.as_str()).into_bytes(),
                    _ => Vec::new(),
                };
                session.reply(reply);
            }
        });
    }

    pub(crate) fn restart_blink(&self) {
        self.imp().restart_blink();
    }

    pub(crate) fn set_progress(&self, state: u8, _value: Option<u8>) {
        self.set_busy(state != 0);
    }

    pub(crate) fn notify(&self, title: &str, body: &str) {
        let Some(app) = WidgetExt::root(self)
            .and_downcast::<gtk::Window>()
            .and_then(|window| window.application())
        else {
            return;
        };
        let title = if title.is_empty() { "Rio" } else { title };
        let notification = gio::Notification::new(title);
        notification.set_body(Some(body));
        app.send_notification(None, &notification);
    }

    pub(crate) fn command_finished(
        &self,
        status: Option<i32>,
        duration: std::time::Duration,
    ) {
        use rio_backend::config::command_finished::NotifyWhen;
        let Some(shared) = self.imp().shared.get() else {
            return;
        };
        let settings = shared.config.borrow().notify_on_command_finish;
        if duration.as_millis() < settings.after as u128 {
            return;
        }
        let active = WidgetExt::root(self)
            .and_downcast::<gtk::Window>()
            .is_some_and(|window| window.is_active())
            && self.has_focus();
        match settings.when {
            NotifyWhen::Never => return,
            NotifyWhen::Unfocused if active => return,
            _ => {}
        }
        if settings.bell {
            self.error_bell();
        }
        if settings.notify {
            let body = match status {
                Some(0) | None => "Command finished".to_string(),
                Some(code) => format!("Command failed with status {code}"),
            };
            self.notify(&self.title(), &body);
        }
    }
}

mod imp {
    use super::*;
    use glib::subclass::Signal;
    use rustc_hash::FxHashSet;
    use std::cell::{Cell, OnceCell, RefCell};
    use std::sync::OnceLock;
    use sugarloaf::context::cpu::CpuFrame;

    #[derive(Default)]
    struct Strips {
        textures: Vec<gdk::Texture>,
        width: u32,
        height: u32,
        generation: u64,
    }

    #[derive(glib::Properties, Default)]
    #[properties(wrapper_type = super::TerminalView)]
    pub struct TerminalView {
        #[property(get, set)]
        title: RefCell<String>,
        /// A program reports progress (OSC 9;4).
        #[property(get, set)]
        busy: Cell<bool>,
        pub(super) shared: OnceCell<Rc<Shared>>,
        id: Cell<usize>,
        session: OnceCell<Session>,
        pub(super) render: RefCell<Option<Renderer>>,
        strips: RefCell<Strips>,
        scale: Cell<i32>,
        allocation: Cell<(i32, i32)>,
        pixels: Cell<(u32, u32)>,
        pub(super) cells: Cell<(usize, usize)>,
        cell_size: Cell<(f32, f32)>,
        origin: Cell<(f32, f32)>,
        focused: Cell<bool>,
        cursor: Cell<[u8; 3]>,
        blink_visible: Cell<bool>,
        blink: RefCell<Option<glib::SourceId>>,
        im: OnceCell<gtk::IMMulticontext>,
        in_key_press: Cell<bool>,
        committed: RefCell<Option<String>>,
        preedit: RefCell<String>,
        held: RefCell<FxHashSet<u32>>,
        button: Cell<Option<u8>>,
        selecting: Cell<bool>,
        pointer: Cell<(f64, f64)>,
        scroll: Cell<f64>,
        mods: Cell<Modifiers>,
        hints: crate::hints::MouseHints,
        hover: RefCell<Option<rio_backend::hints::HintMatch>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TerminalView {
        const NAME: &'static str = "RioTerminalView";
        type Type = super::TerminalView;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("terminal");
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for TerminalView {
        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    Signal::builder("exited").build(),
                    Signal::builder("bell").build(),
                ]
            })
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.set_focusable(true);
            obj.set_focus_on_click(true);
            obj.set_hexpand(true);
            obj.set_vexpand(true);
            obj.set_overflow(gtk::Overflow::Hidden);
            obj.set_cursor_from_name(Some("text"));
            self.scale.set(1);
            self.blink_visible.set(true);
            self.input();
        }

        fn dispose(&self) {
            if let Some(source) = self.blink.take() {
                source.remove();
            }
            if let Some(shared) = self.shared.get() {
                shared.unregister(self.id.get());
            }
        }
    }

    impl WidgetImpl for TerminalView {
        fn measure(
            &self,
            orientation: gtk::Orientation,
            _for_size: i32,
        ) -> (i32, i32, i32, i32) {
            let scale = self.scale.get().max(1) as f32;
            let (cell_w, cell_h) = self.cell_size.get();
            let (cell, count) = match orientation {
                gtk::Orientation::Horizontal => (cell_w, 80.0),
                _ => (cell_h, 24.0),
            };
            let padding = self.padding();
            let pad = match orientation {
                gtk::Orientation::Horizontal => padding[1] + padding[3],
                _ => padding[0] + padding[2],
            };
            let cell_l = (cell / scale).ceil() as i32;
            let natural = ((cell * count) / scale + pad).ceil() as i32;
            (cell_l, natural, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            self.allocation.set((width, height));
            self.relayout();
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            self.draw();
            let strips = self.strips.borrow();
            if strips.textures.is_empty() {
                return;
            }
            let scale = self.scale.get().max(1) as f32;
            let width = strips.width as f32 / scale;
            for (i, texture) in strips.textures.iter().enumerate() {
                let y = (i as u32 * STRIP) as f32 / scale;
                let height = texture.height() as f32 / scale;
                snapshot
                    .append_texture(texture, &graphene::Rect::new(0.0, y, width, height));
            }
        }

        fn realize(&self) {
            self.parent_realize();
            self.relayout();
        }
    }

    impl TerminalView {
        pub fn session(&self) -> Option<&Session> {
            self.session.get()
        }

        fn padding(&self) -> [f32; 4] {
            self.shared
                .get()
                .map(|shared| {
                    let p = shared.config.borrow().panel.padding;
                    [p.top, p.right, p.bottom, p.left]
                })
                .unwrap_or_default()
        }

        pub fn start(
            &self,
            shared: &Rc<Shared>,
            working_dir: Option<String>,
        ) -> Result<(), String> {
            let id = shared.register(&self.obj());
            self.id.set(id);
            let _ = self.shared.set(shared.clone());
            let config = shared.config.borrow();
            self.hints.set_rules(&config.hints.rules);
            let render = Renderer::new(&config, 1.0);
            let (cell_w, cell_h) = render.cell_size();
            self.cell_size.set((cell_w, cell_h));
            let cursor = config.colors.cursor;
            self.cursor
                .set([0, 1, 2].map(|i| (cursor[i] * 255.0).round() as u8));
            let size = CrosswordsSize::new_with_dimensions(
                80,
                24,
                (cell_w * 80.0) as u32,
                (cell_h * 24.0) as u32,
                cell_w as u32,
                cell_h as u32,
            );
            self.cells.set((80, 24));
            let session = Session::spawn(id, &config, working_dir, shared.sink(), size)
                .map_err(|err| err.to_string())?;
            drop(config);
            *self.render.borrow_mut() = Some(render);
            let _ = self.session.set(session);
            Ok(())
        }

        /// Fit the grid to the allocation at the display scale and the
        /// current font size.
        pub(super) fn relayout(&self) {
            let obj = self.obj();
            let scale = WidgetExt::scale_factor(&*obj).max(1);
            let rescaled = scale != self.scale.get();
            self.scale.set(scale);
            let scale = scale as f32;
            let (width, height) = self.allocation.get();
            let px_w = (width.max(1) as f32 * scale) as u32;
            let px_h = (height.max(1) as f32 * scale) as u32;
            let padding = self.padding().map(|p| p * scale);
            let [top, right, bottom, left] = padding;

            let mut render = self.render.borrow_mut();
            let Some(render) = render.as_mut() else {
                return;
            };
            render.set_scale(scale);
            let (cell_w, cell_h) = render.cell_size();
            let resized_cells = (cell_w, cell_h) != self.cell_size.get();
            self.cell_size.set((cell_w, cell_h));
            let cols = (((px_w as f32 - left - right) / cell_w).floor() as usize).max(2);
            let lines = (((px_h as f32 - top - bottom) / cell_h).floor() as usize).max(1);
            self.origin.set((left, top));
            render.resize(px_w, px_h, padding, cols, lines);
            self.pixels.set((px_w, px_h));

            if (cols, lines) != self.cells.get() || resized_cells || rescaled {
                self.cells.set((cols, lines));
                if let Some(session) = self.session.get() {
                    session.resize(CrosswordsSize::new_with_dimensions(
                        cols,
                        lines,
                        (cols as f32 * cell_w) as u32,
                        (lines as f32 * cell_h) as u32,
                        cell_w as u32,
                        cell_h as u32,
                    ));
                }
            }
            if resized_cells || rescaled {
                obj.queue_resize();
            }
            obj.queue_draw();
        }

        fn draw(&self) {
            let (Some(session), Some(shared)) = (self.session.get(), self.shared.get())
            else {
                return;
            };
            shared.awake(self.id.get());
            let (blinking, offset) = {
                let term = session.terminal.lock();
                (term.blinking_cursor, term.display_offset() as i32)
            };
            let spans = self.hover_spans(offset);
            let preedit = self.preedit.borrow();
            let mut render = self.render.borrow_mut();
            let Some(render) = render.as_mut() else {
                return;
            };
            let inputs = Inputs {
                cursor: self.cursor.get(),
                focused: self.focused.get(),
                blinking,
                blink_visible: self.blink_visible.get(),
                preedit: Some(preedit.as_str()).filter(|p| !p.is_empty()),
                spans: &spans,
            };
            if let Some(frame) = render.draw(&session.terminal, self.id.get(), &inputs) {
                self.upload(&frame);
            }
        }

        fn upload(&self, frame: &CpuFrame<'_>) {
            let mut strips = self.strips.borrow_mut();
            if frame.generation == strips.generation && !strips.textures.is_empty() {
                return;
            }
            let count = frame.height.div_ceil(STRIP) as usize;
            let whole = strips.width != frame.width
                || strips.height != frame.height
                || frame.generation != strips.generation + 1
                || strips.textures.len() != count;
            if whole {
                strips.textures = (0..count).map(|i| strip(frame, i as u32)).collect();
            } else {
                for (i, texture) in strips.textures.iter_mut().enumerate() {
                    let (y0, y1) = (i as u32 * STRIP, (i as u32 + 1) * STRIP);
                    if frame.damage.iter().any(|&(a, b)| a < y1 && b > y0) {
                        *texture = strip(frame, i as u32);
                    }
                }
            }
            strips.width = frame.width;
            strips.height = frame.height;
            strips.generation = frame.generation;
        }

        fn hover_spans(&self, offset: i32) -> Vec<crate::render::Span> {
            let hover = self.hover.borrow();
            let Some(hover) = hover.as_ref() else {
                return Vec::new();
            };
            let (cols, lines) = self.cells.get();
            let last = cols.saturating_sub(1) as u16;
            (hover.start.row.0..=hover.end.row.0)
                .filter_map(|line| {
                    let viewport = line + offset;
                    if viewport < 0 || viewport >= lines as i32 {
                        return None;
                    }
                    let lo = if line == hover.start.row.0 {
                        hover.start.col.0 as u16
                    } else {
                        0
                    };
                    let hi = if line == hover.end.row.0 {
                        hover.end.col.0 as u16
                    } else {
                        last
                    };
                    Some(crate::render::Span {
                        line: viewport as u16,
                        lo,
                        hi,
                        tag: rio_grid::HintTag::HyperlinkHover,
                    })
                })
                .collect()
        }

        /// Find the hint under the pointer with the modifiers held now.
        fn update_hover(&self) {
            let Some(session) = self.session() else {
                return;
            };
            let found = if self.selecting.get() || self.button.get().is_some() {
                None
            } else {
                let (x, y) = self.pointer.get();
                let (col, row, _) = self.cell_at(x, y);
                let term = session.terminal.lock();
                let point = rio_vt::crosswords::pos::Pos::new(
                    rio_vt::crosswords::pos::Line(
                        row as i32 - term.display_offset() as i32,
                    ),
                    rio_vt::crosswords::pos::Column(col as usize),
                );
                self.hints.at(&term, point, self.mods.get())
            };
            let changed = {
                let hover = self.hover.borrow();
                match (hover.as_ref(), found.as_ref()) {
                    (None, None) => false,
                    (Some(a), Some(b)) => {
                        a.start != b.start || a.end != b.end || a.text != b.text
                    }
                    _ => true,
                }
            };
            if !changed {
                return;
            }
            let obj = self.obj();
            obj.set_cursor_from_name(Some(if found.is_some() {
                "pointer"
            } else {
                "text"
            }));
            obj.set_tooltip_text(found.as_ref().map(|hint| hint.text.as_str()));
            self.hover.replace(found);
            obj.queue_draw();
        }

        /// Run the hovered hint's action. False when nothing is hovered.
        fn activate_hover(&self) -> bool {
            let Some(hint) = self.hover.borrow().clone() else {
                return false;
            };
            let Some(session) = self.session() else {
                return false;
            };
            let cwd = session.current_directory();
            let obj = self.obj();
            match crate::hints::outcome(&hint, cwd.as_deref()) {
                crate::hints::Outcome::Copy(text) => {
                    WidgetExt::clipboard(&*obj).set_text(&text)
                }
                crate::hints::Outcome::Paste(text) => obj.paste_text(text),
                crate::hints::Outcome::Select(start, end) => {
                    session.select_range(start, end)
                }
                crate::hints::Outcome::Open(target) => session.open(&target),
                crate::hints::Outcome::Run(program, args) => {
                    session.exec(&program, &args)
                }
            }
            true
        }

        pub(super) fn restart_blink(&self) {
            if let Some(source) = self.blink.take() {
                source.remove();
            }
            self.blink_visible.set(true);
            let Some(shared) = self.shared.get() else {
                return;
            };
            let interval = shared.config.borrow().cursor.blinking_interval.max(100);
            if !self.focused.get() {
                return;
            }
            let source = glib::timeout_add_local(
                std::time::Duration::from_millis(interval),
                glib::clone!(
                    #[weak(rename_to = this)]
                    self,
                    #[upgrade_or]
                    glib::ControlFlow::Break,
                    move || {
                        let blinking = this
                            .session()
                            .is_some_and(|s| s.terminal.lock().blinking_cursor);
                        if !blinking {
                            this.blink_visible.set(true);
                            this.blink.replace(None);
                            this.obj().queue_draw();
                            return glib::ControlFlow::Break;
                        }
                        this.blink_visible.set(!this.blink_visible.get());
                        this.obj().queue_draw();
                        glib::ControlFlow::Continue
                    }
                ),
            );
            self.blink.replace(Some(source));
        }

        fn cell_at(&self, x: f64, y: f64) -> (u16, u16, Side) {
            let scale = self.scale.get() as f64;
            let (left, top) = self.origin.get();
            let (cell_w, cell_h) = self.cell_size.get();
            let (cols, lines) = self.cells.get();
            let fx = ((x * scale - left as f64) / cell_w as f64).max(0.0);
            let fy = ((y * scale - top as f64) / cell_h as f64).max(0.0);
            let col = (fx as usize).min(cols.saturating_sub(1));
            let row = (fy as usize).min(lines.saturating_sub(1));
            let side = if fx.fract() < 0.5 {
                Side::Left
            } else {
                Side::Right
            };
            (col as u16, row as u16, side)
        }

        fn input(&self) {
            let obj = self.obj();

            let im = gtk::IMMulticontext::new();
            im.set_client_widget(Some(&*obj));
            im.connect_commit(glib::clone!(
                #[weak(rename_to = this)]
                self,
                move |_, text| {
                    if this.in_key_press.get() {
                        this.committed.replace(Some(text.to_string()));
                    } else if let Some(session) = this.session() {
                        session.text(text);
                    }
                }
            ));
            im.connect_preedit_changed(glib::clone!(
                #[weak(rename_to = this)]
                self,
                move |im| {
                    let (text, ..) = im.preedit_string();
                    this.preedit.replace(text.to_string());
                    if let Some(render) = this.render.borrow_mut().as_mut() {
                        render.invalidate();
                    }
                    this.obj().queue_draw();
                }
            ));
            let _ = self.im.set(im);

            let keys = gtk::EventControllerKey::new();
            keys.connect_key_pressed(glib::clone!(
                #[weak(rename_to = this)]
                self,
                #[upgrade_or]
                glib::Propagation::Proceed,
                move |keys, keyval, keycode, _| this.key_pressed(keys, keyval, keycode)
            ));
            keys.connect_key_released(glib::clone!(
                #[weak(rename_to = this)]
                self,
                move |keys, _, keycode, _| {
                    this.held.borrow_mut().remove(&keycode);
                    let (Some(session), Some(event)) = (this.session(), key_event(keys))
                    else {
                        return;
                    };
                    session.key(&input::key_event(&event, KeyAction::Release, None));
                }
            ));
            keys.connect_modifiers(glib::clone!(
                #[weak(rename_to = this)]
                self,
                #[upgrade_or]
                glib::Propagation::Proceed,
                move |_, state| {
                    this.mods.set(input::modifiers(state));
                    this.update_hover();
                    glib::Propagation::Proceed
                }
            ));
            obj.add_controller(keys);

            let focus = gtk::EventControllerFocus::new();
            focus.connect_enter(glib::clone!(
                #[weak(rename_to = this)]
                self,
                move |_| this.set_focused(true)
            ));
            focus.connect_leave(glib::clone!(
                #[weak(rename_to = this)]
                self,
                move |_| this.set_focused(false)
            ));
            obj.add_controller(focus);

            let click = gtk::GestureClick::new();
            click.set_button(0);
            click.connect_pressed(glib::clone!(
                #[weak(rename_to = this)]
                self,
                move |click, n_press, x, y| this.pressed(click, n_press, x, y)
            ));
            click.connect_released(glib::clone!(
                #[weak(rename_to = this)]
                self,
                move |click, _, x, y| this.released(click, x, y)
            ));
            obj.add_controller(click);

            let motion = gtk::EventControllerMotion::new();
            motion.connect_motion(glib::clone!(
                #[weak(rename_to = this)]
                self,
                move |motion, x, y| this.moved(motion.current_event_state(), x, y)
            ));
            obj.add_controller(motion);

            let scroll = gtk::EventControllerScroll::new(
                gtk::EventControllerScrollFlags::VERTICAL,
            );
            scroll.connect_scroll(glib::clone!(
                #[weak(rename_to = this)]
                self,
                #[upgrade_or]
                glib::Propagation::Proceed,
                move |scroll, _dx, dy| this.scrolled(scroll, dy)
            ));
            obj.add_controller(scroll);
        }

        fn key_pressed(
            &self,
            keys: &gtk::EventControllerKey,
            keyval: gdk::Key,
            keycode: u32,
        ) -> glib::Propagation {
            let (Some(session), Some(event)) = (self.session(), key_event(keys)) else {
                return glib::Propagation::Proceed;
            };
            if let Some(scroll) = scrollback_key(session, keyval, event.modifier_state())
            {
                session.scroll(scroll);
                return glib::Propagation::Stop;
            }
            self.in_key_press.set(true);
            let filtered = self
                .im
                .get()
                .map(|im| im.filter_keypress(&event))
                .unwrap_or(false);
            self.in_key_press.set(false);
            let committed = self.committed.take();
            if filtered && committed.is_none() {
                return glib::Propagation::Stop;
            }
            let action = if self.held.borrow_mut().insert(keycode) {
                KeyAction::Press
            } else {
                KeyAction::Repeat
            };
            let text = committed.or_else(|| input::key_text(keyval));
            if session.key(&input::key_event(&event, action, text)) {
                if self.blink.borrow().is_some() {
                    self.restart_blink();
                }
                if self.hide_pointer() {
                    self.obj().set_cursor_from_name(Some("none"));
                }
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        }

        fn hide_pointer(&self) -> bool {
            self.shared
                .get()
                .is_some_and(|shared| shared.config.borrow().hide_cursor_when_typing)
        }

        fn set_focused(&self, focused: bool) {
            self.focused.set(focused);
            if !focused {
                self.held.borrow_mut().clear();
            }
            if let Some(im) = self.im.get() {
                if focused {
                    im.focus_in();
                } else {
                    im.focus_out();
                }
            }
            if let Some(session) = self.session() {
                session.set_focus(focused);
            }
            self.restart_blink();
            self.obj().queue_draw();
        }

        fn pressed(&self, click: &gtk::GestureClick, n_press: i32, x: f64, y: f64) {
            let obj = self.obj();
            obj.grab_focus();
            let Some(session) = self.session() else {
                return;
            };
            let mods = input::modifiers(click.current_event_state());
            let button = match click.current_button() {
                1 => 0,
                2 => 1,
                3 => 2,
                _ => return,
            };
            self.pointer.set((x, y));
            self.mods.set(mods);
            self.update_hover();
            if button == 0 && self.activate_hover() {
                return;
            }
            let (col, row, side) = self.cell_at(x, y);
            self.button.set(Some(button));
            if session.mouse_button(col, row, button, true, mods) {
                self.selecting.set(false);
                return;
            }
            match button {
                0 => {
                    let kind = match n_press {
                        1 if mods.contains(Modifiers::ALT) => SelectionKind::Block,
                        1 => SelectionKind::Simple,
                        2 => SelectionKind::Semantic,
                        _ => SelectionKind::Lines,
                    };
                    session.selection_clear();
                    session.selection_begin(row, col, side, kind);
                    if n_press > 1 {
                        session.selection_update(row, col, side);
                    }
                    self.selecting.set(true);
                }
                1 => obj.paste_from(WidgetExt::primary_clipboard(&*obj)),
                _ => {}
            }
        }

        fn released(&self, click: &gtk::GestureClick, x: f64, y: f64) {
            let Some(session) = self.session() else {
                return;
            };
            let Some(button) = self.button.take() else {
                return;
            };
            let (col, row, _) = self.cell_at(x, y);
            let mods = input::modifiers(click.current_event_state());
            if self.selecting.replace(false) {
                if let Some(text) = session.selection_text() {
                    let obj = self.obj();
                    WidgetExt::primary_clipboard(&*obj).set_text(&text);
                    let copy = self
                        .shared
                        .get()
                        .is_some_and(|shared| shared.config.borrow().copy_on_select);
                    if copy {
                        WidgetExt::clipboard(&*obj).set_text(&text);
                    }
                }
                return;
            }
            session.mouse_button(col, row, button, false, mods);
        }

        fn moved(&self, state: gdk::ModifierType, x: f64, y: f64) {
            let Some(session) = self.session() else {
                return;
            };
            let moved = self.pointer.replace((x, y)) != (x, y);
            self.mods.set(input::modifiers(state));
            if moved && self.hover.borrow().is_none() {
                self.obj().set_cursor_from_name(Some("text"));
            }
            self.update_hover();
            let (col, row, side) = self.cell_at(x, y);
            if self.selecting.get() {
                session.selection_update(row, col, side);
                return;
            }
            let button = self.button.get().unwrap_or(3);
            session.mouse_motion(col, row, button, input::modifiers(state));
        }

        fn scrolled(
            &self,
            scroll: &gtk::EventControllerScroll,
            dy: f64,
        ) -> glib::Propagation {
            let Some(session) = self.session() else {
                return glib::Propagation::Proceed;
            };
            let multiplier = self
                .shared
                .get()
                .map(|shared| shared.config.borrow().scroll.multiplier)
                .unwrap_or(3.0);
            let lines_per_unit = multiplier;
            let total = self.scroll.get() + dy * lines_per_unit;
            let lines = total.trunc();
            self.scroll.set(total - lines);
            if lines != 0.0 {
                let (x, y) = self.pointer.get();
                let (col, row, _) = self.cell_at(x, y);
                let mods = input::modifiers(scroll.current_event_state());
                session.wheel(-(lines as i32), col, row, mods);
            }
            glib::Propagation::Stop
        }
    }

    fn strip(frame: &CpuFrame<'_>, index: u32) -> gdk::Texture {
        #[cfg(target_endian = "little")]
        let format = gdk::MemoryFormat::B8g8r8a8Premultiplied;
        #[cfg(target_endian = "big")]
        let format = gdk::MemoryFormat::A8r8g8b8Premultiplied;
        let width = frame.width as usize;
        let y0 = (index * STRIP).min(frame.height) as usize;
        let y1 = ((index + 1) * STRIP).min(frame.height) as usize;
        let pixels = &frame.pixels[y0 * width..y1 * width];
        let bytes = glib::Bytes::from(bytemuck::cast_slice::<u32, u8>(pixels));
        gdk::MemoryTexture::new(
            frame.width as i32,
            (y1 - y0) as i32,
            format,
            &bytes,
            width * 4,
        )
        .upcast()
    }

    /// Shift with Page Up/Down, Home and End move the scrollback, except
    /// on the alternate screen, where the program gets them.
    fn scrollback_key(
        session: &Session,
        keyval: gdk::Key,
        state: gdk::ModifierType,
    ) -> Option<Scroll> {
        let mods = state & gtk::accelerator_get_default_mod_mask();
        if mods != gdk::ModifierType::SHIFT_MASK {
            return None;
        }
        let scroll = match keyval {
            gdk::Key::Page_Up | gdk::Key::KP_Page_Up => Scroll::PageUp,
            gdk::Key::Page_Down | gdk::Key::KP_Page_Down => Scroll::PageDown,
            gdk::Key::Home | gdk::Key::KP_Home => Scroll::Top,
            gdk::Key::End | gdk::Key::KP_End => Scroll::Bottom,
            _ => return None,
        };
        let alt_screen = session
            .mode()
            .contains(rio_vt::crosswords::Mode::ALT_SCREEN);
        (!alt_screen).then_some(scroll)
    }

    fn key_event(keys: &gtk::EventControllerKey) -> Option<gdk::KeyEvent> {
        keys.current_event()?.downcast::<gdk::KeyEvent>().ok()
    }
}
