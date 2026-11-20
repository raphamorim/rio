//! The terminal widget: one librio surface, drawn by Sugarloaf's CPU
//! renderer into a frame handed to GTK as a texture.

use crate::input;
use crate::shared::Shared;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene};
use librio::{KeyAction, Modifiers, SelectionKind, Side};
use std::rc::Rc;

glib::wrapper! {
    pub struct TerminalView(ObjectSubclass<imp::TerminalView>)
        @extends gtk::Widget,
        @implements gtk::Buildable, gtk::ConstraintTarget;
}

impl TerminalView {
    /// A terminal running the configured shell.
    pub fn new(shared: &Rc<Shared>) -> Result<Self, String> {
        let view: Self = glib::Object::new();
        view.imp().start(shared)?;
        Ok(view)
    }

    pub fn copy(&self) {
        if let Some(text) = self.imp().surface().and_then(|s| s.selection_text()) {
            WidgetExt::clipboard(self).set_text(&text);
        }
    }

    pub fn paste(&self) {
        let view = self.downgrade();
        let clipboard = WidgetExt::clipboard(self);
        glib::spawn_future_local(async move {
            if let Ok(Some(text)) = clipboard.read_text_future().await {
                if let Some(view) = view.upgrade() {
                    view.paste_text(text.as_str());
                }
            }
        });
    }

    fn paste_primary(&self) {
        let view = self.downgrade();
        let clipboard = WidgetExt::primary_clipboard(self);
        glib::spawn_future_local(async move {
            if let Ok(Some(text)) = clipboard.read_text_future().await {
                if let Some(view) = view.upgrade() {
                    view.paste_text(text.as_str());
                }
            }
        });
    }

    fn paste_text(&self, text: &str) {
        if let Some(surface) = self.imp().surface() {
            surface.paste(text);
        }
    }
}

mod imp {
    use super::*;
    use glib::subclass::Signal;
    use libsugarloaf::sugarloaf::grid::GridRenderer;
    use libsugarloaf::{Sl, SurfaceInputs};
    use std::cell::{Cell, OnceCell, RefCell};
    use std::sync::OnceLock;

    /// The renderer state, created with the surface.
    pub struct Render {
        pub sl: Sl,
        pub grid: GridRenderer,
        pub state: librio::RenderState,
    }

    #[derive(glib::Properties, Default)]
    #[properties(wrapper_type = super::TerminalView)]
    pub struct TerminalView {
        /// The title the program set (OSC 0/2).
        #[property(get, set)]
        title: RefCell<String>,
        shared: OnceCell<Rc<Shared>>,
        surface: OnceCell<librio::Surface>,
        render: RefCell<Option<Render>>,
        /// The last frame as a texture, and which frame it is.
        texture: RefCell<Option<gdk::Texture>>,
        generation: Cell<u64>,
        /// Physical pixels per logical one, and the frame size in physical
        /// pixels and cells.
        scale: Cell<i32>,
        pixels: Cell<(u32, u32)>,
        cells: Cell<(u32, u32)>,
        /// Cell size and grid padding (left, top) in physical pixels.
        cell_size: Cell<(f32, f32)>,
        origin: Cell<(f32, f32)>,
        focused: Cell<bool>,
        cursor: Cell<[u8; 3]>,
        im: OnceCell<gtk::IMMulticontext>,
        /// While a key press runs through the input method: the text it
        /// commits belongs to that key.
        in_key_press: Cell<bool>,
        committed: RefCell<Option<String>>,
        preedit: RefCell<String>,
        /// The button held down (0 left, 1 middle, 2 right), and whether
        /// it is making a local selection rather than a mouse report.
        button: Cell<Option<u8>>,
        selecting: Cell<bool>,
        /// The pointer, in widget coordinates, for wheel reports.
        pointer: Cell<(f64, f64)>,
        scroll: Cell<f64>,
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
                    // The shell exited.
                    Signal::builder("exited").build(),
                    // The program rang the bell.
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
            obj.set_cursor_from_name(Some("text"));
            self.scale.set(1);
            self.input();
        }

        fn dispose(&self) {
            if let (Some(shared), Some(surface)) = (self.shared.get(), self.surface.get())
            {
                shared.unregister(surface.id());
            }
        }
    }

    impl WidgetImpl for TerminalView {
        fn measure(
            &self,
            orientation: gtk::Orientation,
            _for_size: i32,
        ) -> (i32, i32, i32, i32) {
            // At least one cell; naturally the size the shell was opened
            // at, so a new window fits 80x24.
            let scale = self.scale.get().max(1) as f32;
            let (cell_w, cell_h) = self.cell_size.get();
            let (cell, count) = match orientation {
                gtk::Orientation::Horizontal => (cell_w, 80.0),
                _ => (cell_h, 24.0),
            };
            let cell = (cell / scale).ceil() as i32;
            (cell, cell * count as i32, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            self.resize(width, height);
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            self.draw();
            if let Some(texture) = self.texture.borrow().as_ref() {
                let obj = self.obj();
                let bounds = graphene::Rect::new(
                    0.0,
                    0.0,
                    obj.width() as f32,
                    obj.height() as f32,
                );
                snapshot.append_texture(texture, &bounds);
            }
        }

        fn realize(&self) {
            self.parent_realize();
            self.rescale();
        }
    }

    impl TerminalView {
        pub fn surface(&self) -> Option<&librio::Surface> {
            self.surface.get()
        }

        pub fn start(&self, shared: &Rc<Shared>) -> Result<(), String> {
            let config = &shared.config;
            let surface = shared
                .engine
                .create_surface(&librio::SurfaceDesc {
                    shell: config.shell.program.clone(),
                    args: config.shell.args.clone(),
                    working_dir: config.working_dir.clone(),
                    scrollback: config.scrollback_history_limit,
                    ..librio::SurfaceDesc::default()
                })
                .map_err(|err| err.to_string())?;

            let mut sl = Sl::new_offscreen(
                1.0,
                1.0,
                1.0,
                config.fonts.clone(),
                config.line_height,
            );
            sl.set_colors(&config.colors);
            let cursor = config.colors.cursor;
            self.cursor
                .set([0, 1, 2].map(|i| (cursor[i] * 255.0).round() as u8));
            let metrics = sl.cell_metrics();
            self.cell_size
                .set((metrics.cell_width as f32, metrics.cell_height as f32));
            let grid = sl.new_grid(80, 24);
            let state = librio::RenderState::new(&surface);
            self.cells.set((80, 24));

            shared.register(surface.id(), &self.obj());
            *self.render.borrow_mut() = Some(Render { sl, grid, state });
            let _ = self.surface.set(surface);
            let _ = self.shared.set(shared.clone());
            Ok(())
        }

        /// Follow the display scale: cells are rasterized in physical
        /// pixels, so a new scale means new metrics and new glyphs.
        fn rescale(&self) {
            let scale = self.obj().scale_factor().max(1);
            if scale == self.scale.get() {
                return;
            }
            self.scale.set(scale);
            if let Some(render) = self.render.borrow_mut().as_mut() {
                render.sl.set_scale(scale as f32);
                render.grid.clear_atlas();
                render.grid.request_full_rebuild();
                let metrics = render.sl.cell_metrics();
                self.cell_size
                    .set((metrics.cell_width as f32, metrics.cell_height as f32));
            }
            self.obj().queue_resize();
        }

        fn resize(&self, width: i32, height: i32) {
            self.rescale();
            let scale = self.scale.get() as f32;
            let px_w = (width.max(1) as f32 * scale) as u32;
            let px_h = (height.max(1) as f32 * scale) as u32;
            let Some(shared) = self.shared.get() else {
                return;
            };
            let padding = &shared.config.panel.padding;
            let [top, right, bottom, left] =
                [padding.top, padding.right, padding.bottom, padding.left]
                    .map(|p| p * scale);
            let (cell_w, cell_h) = self.cell_size.get();
            let cols = (((px_w as f32 - left - right) / cell_w).floor() as u32).max(1);
            let rows = (((px_h as f32 - top - bottom) / cell_h).floor() as u32).max(1);
            self.origin.set((left, top));

            if let Some(render) = self.render.borrow_mut().as_mut() {
                if self.pixels.get() != (px_w, px_h) {
                    render.sl.resize(px_w, px_h);
                    render.sl.set_grid_padding([top, right, bottom, left], 0);
                    self.pixels.set((px_w, px_h));
                }
                if self.cells.get() != (cols, rows) {
                    render.grid.resize(cols, rows);
                    self.cells.set((cols, rows));
                }
            }
            if let Some(surface) = self.surface.get() {
                let clamp = |v: u32| v.min(u16::MAX as u32) as u16;
                surface.resize(clamp(cols), clamp(rows), clamp(px_w), clamp(px_h));
            }
            self.obj().queue_draw();
        }

        /// Draw the surface's current state and turn a new frame into the
        /// texture GTK shows.
        fn draw(&self) {
            let mut render = self.render.borrow_mut();
            let Some(render) = render.as_mut() else {
                return;
            };
            render.state.update();
            let preedit = self.preedit.borrow();
            render.sl.render_surface(
                &mut render.grid,
                &mut render.state,
                &SurfaceInputs {
                    cursor: self.cursor.get(),
                    focused: self.focused.get(),
                    cursor_blinking: false,
                    cursor_blink_visible: true,
                    search_spans: &[],
                    preedit: Some(preedit.as_str()).filter(|p| !p.is_empty()),
                },
            );
            let Some(frame) = render.sl.cpu_frame() else {
                return;
            };
            if frame.generation == self.generation.get()
                && self.texture.borrow().is_some()
            {
                return;
            }
            self.generation.set(frame.generation);
            // Premultiplied 0xAARRGGBB words: B, G, R, A in memory on
            // little-endian machines, A, R, G, B on big-endian ones.
            #[cfg(target_endian = "little")]
            let format = gdk::MemoryFormat::B8g8r8a8Premultiplied;
            #[cfg(target_endian = "big")]
            let format = gdk::MemoryFormat::A8r8g8b8Premultiplied;
            let bytes = glib::Bytes::from(bytemuck::cast_slice::<u32, u8>(frame.pixels));
            let texture = gdk::MemoryTexture::new(
                frame.width as i32,
                frame.height as i32,
                format,
                &bytes,
                frame.width as usize * 4,
            );
            *self.texture.borrow_mut() = Some(texture.upcast());
        }

        /// The cell under a point in widget coordinates, and which half of
        /// it the point is on.
        fn cell_at(&self, x: f64, y: f64) -> (u16, u16, Side) {
            let scale = self.scale.get() as f64;
            let (left, top) = self.origin.get();
            let (cell_w, cell_h) = self.cell_size.get();
            let (cols, rows) = self.cells.get();
            let fx = ((x * scale - left as f64) / cell_w as f64).max(0.0);
            let fy = ((y * scale - top as f64) / cell_h as f64).max(0.0);
            let col = (fx as u32).min(cols.saturating_sub(1));
            let row = (fy as u32).min(rows.saturating_sub(1));
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
                    } else if let Some(surface) = this.surface() {
                        // Committed outside a key press: a candidate
                        // picked in an input method's window.
                        surface.text(text);
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
                        render.grid.request_full_rebuild();
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
                move |keys, keyval, _keycode, _state| this.key_pressed(keys, keyval)
            ));
            keys.connect_key_released(glib::clone!(
                #[weak(rename_to = this)]
                self,
                move |keys, _keyval, _keycode, _state| {
                    let (Some(surface), Some(event)) = (this.surface(), key_event(keys))
                    else {
                        return;
                    };
                    surface.key(&input::key_event(&event, KeyAction::Release, None));
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
                move |scroll, _dx, dy| this.scrolled(scroll.current_event_state(), dy)
            ));
            obj.add_controller(scroll);
        }

        fn key_pressed(
            &self,
            keys: &gtk::EventControllerKey,
            keyval: gdk::Key,
        ) -> glib::Propagation {
            let (Some(surface), Some(event)) = (self.surface(), key_event(keys)) else {
                return glib::Propagation::Proceed;
            };
            // The input method sees the key first; whatever it commits
            // during the press is that key's text.
            self.in_key_press.set(true);
            let filtered = self
                .im
                .get()
                .map(|im| im.filter_keypress(&event))
                .unwrap_or(false);
            self.in_key_press.set(false);
            let committed = self.committed.take();
            if filtered && committed.is_none() {
                // Composing: the text comes when it commits.
                return glib::Propagation::Stop;
            }
            let text = committed.or_else(|| input::key_text(keyval));
            // GDK doesn't mark repeats, so every press is a press.
            if surface.key(&input::key_event(&event, KeyAction::Press, text)) {
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        }

        fn set_focused(&self, focused: bool) {
            self.focused.set(focused);
            if let Some(im) = self.im.get() {
                if focused {
                    im.focus_in();
                } else {
                    im.focus_out();
                }
            }
            if let Some(surface) = self.surface() {
                surface.set_focus(focused);
            }
            self.obj().queue_draw();
        }

        fn pressed(&self, click: &gtk::GestureClick, n_press: i32, x: f64, y: f64) {
            let obj = self.obj();
            obj.grab_focus();
            let Some(surface) = self.surface() else {
                return;
            };
            let mods = input::modifiers(click.current_event_state());
            let button = match click.current_button() {
                1 => 0,
                2 => 1,
                3 => 2,
                _ => return,
            };
            let (col, row, side) = self.cell_at(x, y);
            self.button.set(Some(button));
            // A program reading the mouse gets the click; shift keeps it
            // for a local selection.
            if surface.mouse_button(col, row, button, true, mods) {
                self.selecting.set(false);
                return;
            }
            match button {
                0 => {
                    let kind = match n_press {
                        1 if mods.contains(Modifiers::ALT) => SelectionKind::Block,
                        1 => SelectionKind::Simple,
                        2 => SelectionKind::Word,
                        _ => SelectionKind::Line,
                    };
                    surface.selection_clear();
                    surface.selection_begin(row as i32, col as usize, kind, side);
                    self.selecting.set(true);
                }
                1 => obj.paste_primary(),
                _ => {}
            }
        }

        fn released(&self, click: &gtk::GestureClick, x: f64, y: f64) {
            let Some(surface) = self.surface() else {
                return;
            };
            let Some(button) = self.button.take() else {
                return;
            };
            let (col, row, _) = self.cell_at(x, y);
            let mods = input::modifiers(click.current_event_state());
            if self.selecting.replace(false) {
                // Selected text goes to the primary selection, which
                // middle-click pastes.
                if let Some(text) = surface.selection_text().filter(|t| !t.is_empty()) {
                    WidgetExt::primary_clipboard(&*self.obj()).set_text(&text);
                }
                return;
            }
            surface.mouse_button(col, row, button, false, mods);
        }

        fn moved(&self, state: gdk::ModifierType, x: f64, y: f64) {
            let Some(surface) = self.surface() else {
                return;
            };
            self.pointer.set((x, y));
            let (col, row, side) = self.cell_at(x, y);
            if self.selecting.get() {
                surface.selection_update(row as i32, col as usize, side);
                return;
            }
            let button = self.button.get().unwrap_or(3);
            surface.mouse_motion(col, row, button, input::modifiers(state));
        }

        fn scrolled(&self, state: gdk::ModifierType, dy: f64) -> glib::Propagation {
            let Some(surface) = self.surface() else {
                return glib::Propagation::Proceed;
            };
            // Three lines a wheel notch; a touchpad's fractions add up.
            let total = self.scroll.get() + dy * 3.0;
            let lines = total.trunc();
            self.scroll.set(total - lines);
            if lines != 0.0 {
                // GTK's dy grows downwards; librio counts lines up.
                let (x, y) = self.pointer.get();
                let (col, row, _) = self.cell_at(x, y);
                surface.scroll_wheel(-(lines as i32), col, row, input::modifiers(state));
            }
            glib::Propagation::Stop
        }
    }

    fn key_event(keys: &gtk::EventControllerKey) -> Option<gdk::KeyEvent> {
        keys.current_event()?.downcast::<gdk::KeyEvent>().ok()
    }
}
