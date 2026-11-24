//! The terminal widget: one session, drawn by Sugarloaf's CPU renderer
//! into a frame GTK shows as textures, one per terminal row (and the
//! padding around them): a frame that changed a few rows uploads only
//! theirs, and a scroll moves the textures of the rows it kept.

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

glib::wrapper! {
    pub struct TerminalView(ObjectSubclass<imp::TerminalView>)
        @extends gtk::Widget,
        @implements gtk::Buildable, gtk::ConstraintTarget;
}

impl TerminalView {
    pub fn new(shared: &Rc<Shared>, working_dir: Option<String>) -> Result<Self, String> {
        let view: Self = glib::Object::new();
        view.imp().start(shared, working_dir)?;
        view.refresh_title();
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

    /// Search for `pattern`, moving to the next older match (`up`) or
    /// newer one. A new pattern starts from the newest match. Returns the
    /// focused match's position from the newest, and the match count.
    pub fn search(&self, pattern: &str, up: Option<bool>) -> (usize, usize) {
        let imp = self.imp();
        let Some(session) = imp.session() else {
            return (0, 0);
        };
        let mut search = imp.search.borrow_mut();
        if search.pattern != pattern {
            search.pattern = pattern.to_string();
            search.matches = session.search(pattern, 10_000);
            search.focused = search.matches.len().checked_sub(1);
        } else if let (Some(focused), Some(up)) = (search.focused, up) {
            let count = search.matches.len();
            search.focused = Some(if up {
                (focused + count - 1) % count
            } else {
                (focused + 1) % count
            });
        }
        if let Some(&(start, _)) = search.focused.and_then(|i| search.matches.get(i)) {
            session.reveal(start.row);
        }
        let count = search.matches.len();
        let position = search.focused.map_or(0, |i| count - i);
        drop(search);
        self.queue_draw();
        (position, count)
    }

    pub fn search_clear(&self) {
        *self.imp().search.borrow_mut() = Default::default();
        self.queue_draw();
    }

    pub fn shared(&self) -> Option<Rc<Shared>> {
        self.imp().shared.get().cloned()
    }

    /// The title a program set (OSC 0/2).
    pub(crate) fn set_program_title(&self, title: String) {
        self.imp().program_title.replace(title);
        self.refresh_title();
    }

    /// The title from the config's template (`title.content`).
    pub(crate) fn refresh_title(&self) {
        let imp = self.imp();
        let (Some(shared), Some(session)) = (imp.shared.get(), imp.session()) else {
            return;
        };
        let config = shared.config.borrow();
        let directory = session.current_directory();
        let (columns, lines) = imp.cells.get();
        let title = rio_backend::config::title::render(
            &config.title.content,
            &rio_backend::config::title::TitleVars {
                title: &imp.program_title.borrow(),
                program: &session.shell,
                directory: directory.as_deref(),
                columns,
                lines,
            },
        );
        let title = if title.trim().is_empty() {
            config.title.placeholder.clone().unwrap_or_default()
        } else {
            title
        };
        drop(config);
        if title != self.title() {
            self.set_title(title);
        }
    }

    /// Grows each time a terminal gets focus: the highest in a tab is
    /// the one it last focused.
    pub fn focus_order(&self) -> u64 {
        self.imp().focus_order.get()
    }

    pub fn toggle_vi(&self) {
        if let Some(session) = self.imp().session() {
            session.vi_toggle();
        }
    }

    /// Label the matches of the config's hint rule `rule`.
    pub fn start_hints(&self, rule: usize) {
        if let Some(session) = self.imp().session() {
            self.imp().start_hints(session, rule);
        }
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

    /// Take the shared config as it is now.
    pub(crate) fn apply_config(&self) {
        let imp = self.imp();
        let Some(shared) = imp.shared.get() else {
            return;
        };
        let config = shared.config.borrow();
        imp.hints.set_rules(&config.hints.rules);
        let cursor = config.colors.cursor;
        imp.cursor
            .set([0, 1, 2].map(|i| (cursor[i] * 255.0).round() as u8));
        if let Some(render) = imp.render.borrow_mut().as_mut() {
            render.reconfigure(&config);
        }
        if let Some(session) = imp.session() {
            session.reconfigure(&config);
        }
        drop(config);
        imp.relayout();
        imp.restart_blink();
        self.refresh_title();
        self.queue_resize();
    }

    pub(crate) fn apply_graphics(&self, queues: rio_vt::ansi::graphics::UpdateQueues) {
        if let Some(render) = self.imp().render.borrow_mut().as_mut() {
            render.apply_graphics(queues);
        }
        self.queue_draw();
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
    pub(super) struct Search {
        pub pattern: String,
        pub matches: Vec<(rio_vt::crosswords::pos::Pos, rio_vt::crosswords::pos::Pos)>,
        pub focused: Option<usize>,
    }

    #[derive(Default)]
    struct Strips {
        /// Strip boundaries in pixel rows, top to bottom.
        edges: Vec<u32>,
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
        pub(super) cursor: Cell<[u8; 3]>,
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
        pub(super) hints: crate::hints::MouseHints,
        hover: RefCell<Option<rio_backend::hints::HintMatch>>,
        pub(super) search: RefCell<Search>,
        pub(super) focus_order: Cell<u64>,
        pub(super) program_title: RefCell<String>,
        scrollbar: OnceCell<gtk::Scrollbar>,
        im_cursor: Cell<(i32, usize)>,
        scrollbar_hide: RefCell<Option<glib::SourceId>>,
        /// The scrollback position the scrollbar shows: lines of history
        /// and how far up the view is.
        scrolled: Cell<(usize, usize)>,
        syncing_scrollbar: Cell<bool>,
        /// Keyboard hint mode: labeled matches to pick by typing.
        hint_mode: RefCell<Option<rio_backend::hints::HintState>>,
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
                    Signal::builder("focused").build(),
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
            self.scrollbar();
        }

        fn dispose(&self) {
            if let Some(source) = self.blink.take() {
                source.remove();
            }
            if let Some(source) = self.scrollbar_hide.take() {
                source.remove();
            }
            while let Some(child) = self.obj().first_child() {
                child.unparent();
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
            let mut child = self.obj().first_child();
            while let Some(widget) = child {
                child = widget.next_sibling();
                if let Some(popover) = widget.downcast_ref::<gtk::Popover>() {
                    popover.present();
                }
            }
            if let Some(scrollbar) = self.scrollbar.get() {
                let (_, natural, _, _) =
                    scrollbar.measure(gtk::Orientation::Horizontal, -1);
                scrollbar.size_allocate(
                    &gtk::Allocation::new(width - natural, 0, natural, height),
                    -1,
                );
            }
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            self.draw();
            let strips = self.strips.borrow();
            if strips.textures.is_empty() {
                return;
            }
            let scale = self.scale.get().max(1) as f32;
            let width = strips.width as f32 / scale;
            for (texture, &y) in strips.textures.iter().zip(&strips.edges) {
                let y = y as f32 / scale;
                let height = texture.height() as f32 / scale;
                snapshot
                    .append_texture(texture, &graphene::Rect::new(0.0, y, width, height));
            }
            drop(strips);
            if let Some(scrollbar) = self.scrollbar.get().filter(|s| s.width() > 0) {
                self.obj().snapshot_child(scrollbar, snapshot);
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
                let obj = obj.clone();
                glib::idle_add_local_once(move || obj.refresh_title());
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
            let (blinking, offset, history, cursor) = {
                let term = session.terminal.lock();
                let cursor = term.grid.cursor.pos;
                (
                    term.blinking_cursor,
                    term.display_offset() as i32,
                    term.history_size(),
                    (cursor.row.0 + term.display_offset() as i32, cursor.col.0),
                )
            };
            self.place_input_method(cursor);
            if self.scrolled.get() != (history, offset as usize) {
                let obj = self.obj().clone();
                glib::idle_add_local_once(move || {
                    obj.imp().sync_scrollbar(history, offset as usize)
                });
            }
            let spans = self.hover_spans(offset);
            let labels = self.hint_labels();
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
                labels: &labels,
            };
            if let Some(frame) = render.draw(&session.terminal, self.id.get(), &inputs) {
                self.upload(&frame);
            }
        }

        /// Tell the input method where the cursor is, for its candidate
        /// window.
        fn place_input_method(&self, (line, col): (i32, usize)) {
            if self.im_cursor.replace((line, col)) == (line, col) {
                return;
            }
            let Some(im) = self.im.get() else {
                return;
            };
            let scale = self.scale.get().max(1) as f32;
            let (left, top) = self.origin.get();
            let (cell_w, cell_h) = self.cell_size.get();
            let rect = gdk::Rectangle::new(
                ((left + col as f32 * cell_w) / scale) as i32,
                ((top + line as f32 * cell_h) / scale) as i32,
                (cell_w / scale).ceil() as i32,
                (cell_h / scale).ceil() as i32,
            );
            im.set_cursor_location(&rect);
        }

        fn scrollbar(&self) {
            let adjustment = gtk::Adjustment::new(0.0, 0.0, 1.0, 1.0, 1.0, 1.0);
            let scrollbar =
                gtk::Scrollbar::new(gtk::Orientation::Vertical, Some(&adjustment));
            scrollbar.add_css_class("overlay-indicator");
            scrollbar.set_cursor_from_name(Some("default"));
            scrollbar.set_opacity(0.0);
            scrollbar.set_parent(&*self.obj());
            adjustment.connect_value_changed(glib::clone!(
                #[weak(rename_to = this)]
                self,
                move |adjustment| {
                    if this.syncing_scrollbar.get() {
                        return;
                    }
                    let Some(session) = this.session() else {
                        return;
                    };
                    let (history, offset) = this.scrolled.get();
                    let wanted = history as f64 - adjustment.value().round();
                    let delta = wanted as i32 - offset as i32;
                    if delta != 0 {
                        session.scroll(Scroll::Delta(delta));
                    }
                    this.reveal_scrollbar();
                }
            ));
            let _ = self.scrollbar.set(scrollbar);
        }

        /// Show where the view is in the scrollback, and briefly the
        /// scrollbar itself when that moved.
        fn sync_scrollbar(&self, history: usize, offset: usize) {
            let Some(scrollbar) = self.scrollbar.get() else {
                return;
            };
            let enabled = self
                .shared
                .get()
                .is_some_and(|shared| shared.config.borrow().enable_scroll_bar);
            let visible = enabled && history > 0;
            if scrollbar.is_visible() != visible {
                scrollbar.set_visible(visible);
                self.obj().queue_allocate();
            }
            let last = self.scrolled.replace((history, offset));
            if last == (history, offset) {
                return;
            }
            let lines = self.cells.get().1 as f64;
            let adjustment = scrollbar.adjustment();
            self.syncing_scrollbar.set(true);
            adjustment.configure(
                (history - offset.min(history)) as f64,
                0.0,
                history as f64 + lines,
                1.0,
                lines,
                lines,
            );
            self.syncing_scrollbar.set(false);
            if last.1 != offset {
                self.reveal_scrollbar();
            }
        }

        fn reveal_scrollbar(&self) {
            let Some(scrollbar) = self.scrollbar.get() else {
                return;
            };
            scrollbar.set_opacity(1.0);
            if let Some(source) = self.scrollbar_hide.take() {
                source.remove();
            }
            let source = glib::timeout_add_local_once(
                std::time::Duration::from_millis(2000),
                glib::clone!(
                    #[weak(rename_to = this)]
                    self,
                    move || {
                        this.scrollbar_hide.replace(None);
                        if let Some(scrollbar) = this.scrollbar.get() {
                            scrollbar.set_opacity(0.0);
                        }
                    }
                ),
            );
            self.scrollbar_hide.replace(Some(source));
        }

        /// Strip boundaries for a frame `height` pixels tall: the top
        /// padding, then runs of rows, and what is left below them. A run
        /// is one row, or as many as it takes to end on a whole logical
        /// pixel, so no texture edge falls between pixels.
        fn strip_edges(&self, height: u32) -> Vec<u32> {
            let scale = self.scale.get().max(1) as u32;
            let (_, top) = self.origin.get();
            let (_, cell_h) = self.cell_size.get();
            let lines = self.cells.get().1 as u32;
            let mut edges = vec![0];
            let top = top.round() as u32;
            if cell_h.fract() == 0.0 && cell_h >= 1.0 && top.is_multiple_of(scale) {
                let cell_h = cell_h as u32;
                let rows = (1..=scale)
                    .find(|k| (k * cell_h).is_multiple_of(scale))
                    .unwrap_or(scale);
                let mut row = 0;
                while row <= lines {
                    let y = top + row * cell_h;
                    if y > 0 && y < height {
                        edges.push(y);
                    }
                    row += rows;
                }
            }
            edges.dedup();
            edges.push(height);
            edges
        }

        fn upload(&self, frame: &CpuFrame<'_>) {
            let mut strips = self.strips.borrow_mut();
            if frame.generation == strips.generation && !strips.textures.is_empty() {
                return;
            }
            let edges = self.strip_edges(frame.height);
            let whole = strips.width != frame.width
                || strips.height != frame.height
                || frame.generation != strips.generation + 1
                || strips.edges != edges;
            let bands: Vec<(u32, u32)> = edges.windows(2).map(|e| (e[0], e[1])).collect();
            if whole {
                strips.textures = bands.iter().map(|&band| strip(frame, band)).collect();
            } else {
                let old = std::mem::take(&mut strips.textures);
                let index = |y: u32| bands.iter().position(|&(y0, _)| y0 == y);
                strips.textures = bands
                    .iter()
                    .enumerate()
                    .map(|(i, &(y0, y1))| {
                        let damaged = frame.damage.iter().any(|&(a, b)| a < y1 && b > y0);
                        let source = match frame.moved {
                            Some((from, to, count)) if y0 >= to && y1 <= to + count => {
                                index(y0 + from - to)
                                    .filter(|&j| bands[j].1 - bands[j].0 == y1 - y0)
                            }
                            Some((_, to, count)) if y0 < to + count && y1 > to => None,
                            _ => Some(i),
                        };
                        match source {
                            Some(j) if !damaged => old[j].clone(),
                            _ => strip(frame, (y0, y1)),
                        }
                    })
                    .collect();
            }
            strips.edges = edges;
            strips.width = frame.width;
            strips.height = frame.height;
            strips.generation = frame.generation;
        }

        /// Highlighted runs on screen: search matches and the hovered
        /// hint.
        fn hover_spans(&self, offset: i32) -> Vec<crate::render::Span> {
            use rio_grid::HintTag;
            let (cols, lines) = self.cells.get();
            let mut spans = Vec::new();
            let mut push = |start: rio_vt::crosswords::pos::Pos,
                            end: rio_vt::crosswords::pos::Pos,
                            tag: HintTag| {
                let first = start.row.0.max(-offset);
                let last = end.row.0.min(lines as i32 - 1 - offset);
                for line in first..=last {
                    let lo = if line == start.row.0 {
                        start.col.0 as u16
                    } else {
                        0
                    };
                    let hi = if line == end.row.0 {
                        end.col.0 as u16
                    } else {
                        cols.saturating_sub(1) as u16
                    };
                    spans.push(crate::render::Span {
                        line: (line + offset) as u16,
                        lo,
                        hi,
                        tag,
                    });
                }
            };
            {
                let search = self.search.borrow();
                let top = -offset;
                let bottom = lines as i32 - 1 - offset;
                let first = search.matches.partition_point(|(_, end)| end.row.0 < top);
                for (i, &(start, end)) in search.matches[first..].iter().enumerate() {
                    if start.row.0 > bottom {
                        break;
                    }
                    let tag = if search.focused == Some(first + i) {
                        HintTag::Focused
                    } else {
                        HintTag::Match
                    };
                    push(start, end, tag);
                }
            }
            if let Some(hover) = self.hover.borrow().as_ref() {
                push(hover.start, hover.end, HintTag::HyperlinkHover);
            }
            if let Some(state) = self.hint_mode.borrow().as_ref() {
                for hint in state.matches() {
                    push(hint.start, hint.end, HintTag::Match);
                }
            }
            spans
        }

        fn hint_labels(&self) -> Vec<rio_grid::HintLabel> {
            let state = self.hint_mode.borrow();
            let Some(state) = state.as_ref() else {
                return Vec::new();
            };
            let matches = state.matches();
            let mut labels = Vec::new();
            for (index, remaining) in state.visible_labels() {
                let Some(hint) = matches.get(index) else {
                    continue;
                };
                for (i, &label) in remaining.iter().enumerate() {
                    labels.push(rio_grid::HintLabel {
                        position: rio_vt::crosswords::pos::Pos::new(
                            hint.start.row,
                            hint.start.col + i,
                        ),
                        label,
                        is_first: i == 0,
                    });
                }
            }
            labels
        }

        /// Label the matches of a hint rule, to pick one by typing.
        pub(super) fn start_hints(&self, session: &Session, rule: usize) {
            let Some(shared) = self.shared.get() else {
                return;
            };
            let config = shared.config.borrow();
            let Some(rule) = config.hints.rules.get(rule) else {
                return;
            };
            let mut state =
                rio_backend::hints::HintState::new(config.hints.alphabet.clone());
            state.start(Rc::new(rule.clone()));
            state.update_matches(&session.terminal.lock());
            drop(config);
            if state.matches().is_empty() {
                return;
            }
            self.hint_mode.replace(Some(state));
            self.obj().queue_draw();
        }

        /// A key while hints are labeled: picks, narrows or cancels.
        fn hint_key(&self, session: &Session, keyval: gdk::Key) -> glib::Propagation {
            let mut mode = self.hint_mode.borrow_mut();
            let Some(state) = mode.as_mut() else {
                return glib::Propagation::Proceed;
            };
            let picked = match keyval {
                gdk::Key::Escape => {
                    *mode = None;
                    None
                }
                gdk::Key::BackSpace => {
                    state.keyboard_input(&session.terminal.lock(), '\x08')
                }
                _ => match keyval.to_unicode().filter(|c| !c.is_control()) {
                    Some(c) => state.keyboard_input(&session.terminal.lock(), c),
                    None => None,
                },
            };
            if picked.is_some() {
                *mode = None;
            }
            drop(mode);
            if let Some(hint) = picked {
                self.run_hint(session, &hint);
            }
            self.obj().queue_draw();
            glib::Propagation::Stop
        }

        fn run_hint(&self, session: &Session, hint: &rio_backend::hints::HintMatch) {
            let cwd = session.current_directory();
            let obj = self.obj();
            match crate::hints::outcome(hint, cwd.as_deref()) {
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
            self.run_hint(session, &hint);
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

            let drop = gtk::DropTarget::new(glib::Type::INVALID, gdk::DragAction::COPY);
            drop.set_types(&[gdk::FileList::static_type(), glib::Type::STRING]);
            drop.connect_drop(glib::clone!(
                #[weak(rename_to = this)]
                self,
                #[upgrade_or]
                false,
                move |_, value, _, _| {
                    let text = if let Ok(files) = value.get::<gdk::FileList>() {
                        files
                            .files()
                            .iter()
                            .filter_map(|file| file.path())
                            .map(|path| {
                                rio_vt::paste::shell_escape(&path.to_string_lossy()) + " "
                            })
                            .collect::<String>()
                    } else if let Ok(text) = value.get::<String>() {
                        text
                    } else {
                        return false;
                    };
                    this.obj().grab_focus();
                    this.obj().paste_text(text);
                    true
                }
            ));
            obj.add_controller(drop);

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
            if self.hint_mode.borrow().is_some() {
                return self.hint_key(session, keyval);
            }
            let mods = event.modifier_state() & gtk::accelerator_get_default_mod_mask();
            if crate::vi::is_toggle(keyval, mods) {
                session.vi_toggle();
                return glib::Propagation::Stop;
            }
            let lines = self.cells.get().1 as i32;
            match crate::vi::key(session, keyval, mods, lines) {
                crate::vi::Outcome::Unhandled => {}
                crate::vi::Outcome::Handled => return glib::Propagation::Stop,
                crate::vi::Outcome::Copy => {
                    self.obj().copy();
                    session.selection_clear();
                    return glib::Propagation::Stop;
                }
                crate::vi::Outcome::Find => {
                    let _ = self.obj().activate_action("win.find", None);
                    return glib::Propagation::Stop;
                }
            }
            if self.run_binding(session, keyval, event.modifier_state()) {
                return glib::Propagation::Stop;
            }
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

        /// Run the config's binding for this key, if it has one. False
        /// when the key is left to the terminal.
        fn run_binding(
            &self,
            session: &Session,
            keyval: gdk::Key,
            state: gdk::ModifierType,
        ) -> bool {
            use crate::bindings::{Act, Scroll as By};
            let Some(shared) = self.shared.get() else {
                return false;
            };
            let key = crate::bindings::normalize(keyval);
            let mods = state & gtk::accelerator_get_default_mod_mask();
            let bindings = shared.bindings.borrow();
            if bindings.is_empty() {
                return false;
            }
            let mode = session.mode();
            let Some(act) = bindings
                .iter()
                .rev()
                .find(|binding| binding.matches(key, mods, mode))
                .map(|binding| binding.act.clone())
            else {
                return false;
            };
            drop(bindings);
            let obj = self.obj();
            let lines = self.cells.get().1 as i32;
            match act {
                Act::PassThrough => return false,
                Act::Action(detailed) => {
                    if let Ok((name, target)) =
                        gio::Action::parse_detailed_name(&detailed)
                    {
                        let _ = obj.activate_action(&name, target.as_ref());
                    }
                }
                Act::Esc(bytes) => session.write(bytes.into_bytes()),
                Act::Run(program, args) => session.exec(&program, &args),
                Act::ClearHistory => obj.clear_history(),
                Act::ToggleTheme => shared.toggle_theme(),
                Act::Hint(rule) => self.start_hints(session, rule),
                Act::Scroll(by) => match by {
                    By::Lines(n) => session.scroll(Scroll::Delta(n)),
                    By::Pages(n) => session.scroll(Scroll::Delta(n * lines)),
                    By::HalfPages(n) => session.scroll(Scroll::Delta(n * lines / 2)),
                    By::Top => session.scroll(Scroll::Top),
                    By::Bottom => session.scroll(Scroll::Bottom),
                    By::Prompt(forward) => session.scroll_to_prompt(forward),
                },
            }
            true
        }

        fn hide_pointer(&self) -> bool {
            self.shared
                .get()
                .is_some_and(|shared| shared.config.borrow().hide_cursor_when_typing)
        }

        fn set_focused(&self, focused: bool) {
            use std::sync::atomic::{AtomicU64, Ordering};
            static FOCUS: AtomicU64 = AtomicU64::new(1);
            if focused {
                self.focus_order.set(FOCUS.fetch_add(1, Ordering::Relaxed));
            }
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
            if focused {
                self.obj().emit_by_name::<()>("focused", &[]);
            }
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
                _ => self.context_menu(x, y),
            }
        }

        /// The right-click menu: clipboard, splits and tabs.
        fn context_menu(&self, x: f64, y: f64) {
            let menu = gio::Menu::new();
            let edit = gio::Menu::new();
            edit.append(Some("Copy"), Some("win.copy"));
            edit.append(Some("Paste"), Some("win.paste"));
            edit.append(Some("Select All"), Some("win.select-all"));
            edit.append(Some("Find…"), Some("win.find"));
            menu.append_section(None, &edit);
            let layout = gio::Menu::new();
            layout.append(Some("Split Right"), Some("win.split-right"));
            layout.append(Some("Split Down"), Some("win.split-down"));
            layout.append(Some("New Tab"), Some("win.new-tab"));
            layout.append(Some("Close"), Some("win.close"));
            menu.append_section(None, &layout);
            let more = gio::Menu::new();
            more.append(Some("Open Config"), Some("win.open-config"));
            menu.append_section(None, &more);

            let obj = self.obj();
            let popover = gtk::PopoverMenu::from_model(Some(&menu));
            popover.set_parent(&*obj);
            popover.set_has_arrow(false);
            popover.set_halign(gtk::Align::Start);
            popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
            popover.connect_closed(|popover| {
                let popover = popover.clone();
                glib::idle_add_local_once(move || popover.unparent());
            });
            popover.popup();
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

    fn strip(frame: &CpuFrame<'_>, (y0, y1): (u32, u32)) -> gdk::Texture {
        #[cfg(target_endian = "little")]
        let format = gdk::MemoryFormat::B8g8r8a8Premultiplied;
        #[cfg(target_endian = "big")]
        let format = gdk::MemoryFormat::A8r8g8b8Premultiplied;
        let width = frame.width as usize;
        let (y0, y1) = (y0 as usize, y1 as usize);
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
