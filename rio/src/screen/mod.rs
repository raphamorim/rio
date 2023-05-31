mod bindings;
mod messenger;
mod state;
pub mod window;

use crate::clipboard::{Clipboard, ClipboardType};
use crate::crosswords::{
    grid::Scroll,
    pos::{Pos, Side},
    Crosswords, Mode,
};
use crate::event::sync::FairMutex;
use crate::event::{ClickState, EventProxy};
use crate::ime::Ime;
use crate::layout::Layout;
use crate::performer::Machine;
use crate::screen::bindings::{Action as Act, BindingMode, Key};
use crate::selection::{Selection, SelectionType};
use crate::tabs::TabsControl;
use colors::term::List;
use messenger::Messenger;
use state::State;
use std::borrow::Cow;
use std::error::Error;
use std::rc::Rc;
use std::sync::Arc;
use sugarloaf::Sugarloaf;
use teletypewriter::create_pty;

pub struct Screen {
    bindings: bindings::KeyBindings,
    clipboard: Clipboard,
    ignore_chars: bool,
    layout: Layout,
    pub ime: Ime,
    pub messenger: Messenger,
    pub state: State,
    sugarloaf: Sugarloaf,
    terminal: Arc<FairMutex<Crosswords<EventProxy>>>,
    #[allow(unused)]
    tabs: TabsControl,
}

impl Screen {
    pub async fn new(
        winit_window: &winit::window::Window,
        config: &Rc<config::Config>,
        event_proxy: EventProxy,
    ) -> Result<Screen, Box<dyn Error>> {
        let shell = std::env::var("SHELL")?;
        let size = winit_window.inner_size();
        let scale = winit_window.scale_factor();

        let mut layout = Layout::new(
            size.width as f32,
            size.height as f32,
            scale as f32,
            config.style.font_size,
        );
        let (columns, rows) = layout.compute();
        let pty = create_pty(&Cow::Borrowed(&shell), columns as u16, rows as u16);

        let power_preference: wgpu::PowerPreference = match config.performance {
            config::Performance::High => wgpu::PowerPreference::HighPerformance,
            config::Performance::Low => wgpu::PowerPreference::LowPower,
        };

        let sugarloaf = Sugarloaf::new(
            winit_window,
            power_preference,
            config.style.font.to_string(),
        )
        .await?;

        let state = State::new(config);

        let event_proxy_clone = event_proxy.clone();
        let mut terminal = Crosswords::new(columns, rows, event_proxy);
        terminal.cursor_shape = state.get_cursor_state().content;
        let terminal: Arc<FairMutex<Crosswords<EventProxy>>> =
            Arc::new(FairMutex::new(terminal));

        let machine = Machine::new(Arc::clone(&terminal), pty, event_proxy_clone)?;
        let channel = machine.channel();
        machine.spawn();
        let messenger = Messenger::new(channel);

        let clipboard = Clipboard::new();
        let bindings = bindings::default_key_bindings();
        let ime = Ime::new();
        let tabs = TabsControl::new();

        Ok(Screen {
            tabs,
            ime,
            sugarloaf,
            terminal,
            layout,
            messenger,
            state,
            bindings,
            clipboard,
            ignore_chars: false,
        })
    }

    #[inline]
    pub fn propagate_modifiers_state(&mut self, state: winit::event::ModifiersState) {
        self.messenger.set_modifiers(state);
    }

    #[inline]
    pub fn clipboard_get(&mut self, clipboard_type: ClipboardType) -> String {
        self.clipboard.get(clipboard_type)
    }

    pub fn input_character(&mut self, character: char) {
        if self.ime.preedit().is_some() || self.ignore_chars {
            return;
        }

        let utf8_len = character.len_utf8();
        let mut bytes = vec![0; utf8_len];
        character.encode_utf8(&mut bytes[..]);

        #[cfg(not(target_os = "macos"))]
        let alt_send_esc = true;

        #[cfg(target_os = "macos")]
        let alt_send_esc = self.state.option_as_alt;

        if alt_send_esc && self.messenger.get_modifiers().alt() && utf8_len == 1 {
            bytes.insert(0, b'\x1b');
        }

        self.messenger.send_bytes(bytes);
    }

    #[inline]
    pub fn scroll_bottom_when_cursor_not_visible(&self) {
        let mut terminal = self.terminal.lock();
        if terminal.display_offset() != 0 {
            terminal.scroll_display(Scroll::Bottom);
        }
        drop(terminal);
    }

    #[inline]
    pub fn mouse_mode(&self) -> bool {
        let mode = self.get_mode();
        mode.intersects(Mode::MOUSE_MODE) && !mode.contains(Mode::VI)
    }

    #[inline]
    pub fn display_offset(&self) -> usize {
        let mut terminal = self.terminal.lock();
        let display_offset = terminal.display_offset();
        drop(terminal);
        display_offset
    }

    pub fn get_mode(&self) -> Mode {
        let terminal = self.terminal.lock();
        let mode = terminal.mode();
        drop(terminal);
        mode
    }

    #[inline]
    #[allow(unused)]
    pub fn colors(&mut self) -> List {
        let terminal = self.terminal.lock();
        let mode = terminal.colors();
        drop(terminal);
        mode
    }

    #[inline]
    pub fn input_keycode(
        &mut self,
        virtual_keycode: Option<winit::event::VirtualKeyCode>,
        scancode: u32,
    ) {
        if self.ime.preedit().is_some() {
            return;
        }

        let mode = BindingMode::new(&self.get_mode());
        let mods = self.messenger.get_modifiers();
        let mut ignore_chars = None;

        for i in 0..self.bindings.len() {
            let binding = &self.bindings[i];

            let key = match (binding.trigger, virtual_keycode) {
                (Key::Scancode(_), _) => Key::Scancode(scancode),
                (_, Some(key)) => Key::Keycode(key),
                _ => continue,
            };

            if binding.is_triggered_by(mode.clone(), mods, &key) {
                *ignore_chars.get_or_insert(true) &= binding.action != Act::ReceiveChar;

                match &binding.action {
                    Act::Esc(s) => {
                        self.messenger.send_bytes(
                            s.replace("\r\n", "\r").replace('\n', "\r").into_bytes(),
                        );
                    }
                    Act::Paste => {
                        let content = self.clipboard.get(ClipboardType::Clipboard);
                        self.paste(&content, true);
                    }
                    Act::PasteSelection => {
                        let content = self.clipboard.get(ClipboardType::Selection);
                        self.paste(&content, true);
                    }
                    Act::Copy => {
                        self.copy_selection(ClipboardType::Clipboard);
                    }
                    Act::TabCreateNew => {
                        self.tabs.add_tab(true);
                        self.render();
                    }
                    Act::TabSwitchNext => {
                        self.tabs.switch_to_next();
                        self.render();
                    }
                    Act::ReceiveChar | Act::None => (),
                    _ => (),
                }
            }
        }

        self.ignore_chars = ignore_chars.unwrap_or(false);
    }

    pub fn copy_selection(&mut self, ty: ClipboardType) {
        let terminal = self.terminal.lock();
        let text = match terminal.selection_to_string().filter(|s| !s.is_empty()) {
            Some(text) => text,
            None => return,
        };
        drop(terminal);

        if ty == ClipboardType::Selection {
            self.clipboard.set(ClipboardType::Clipboard, text.clone());
        }
        self.clipboard.set(ty, text);
    }

    // fn on_mouse_release(&mut self, button: MouseButton) {
    //     if !self.ctx.modifiers().shift() && self.ctx.mouse_mode() {
    //         let code = match button {
    //             MouseButton::Left => 0,
    //             MouseButton::Middle => 1,
    //             MouseButton::Right => 2,
    //             // Can't properly report more than three buttons.
    //             MouseButton::Other(_) => return,
    //         };
    //         self.mouse_report(code, ElementState::Released);
    //         return;
    //     }

    //     // Trigger hints highlighted by the mouse.
    //     let hint = self.ctx.display().highlighted_hint.take();
    //     if let Some(hint) = hint.as_ref().filter(|_| button == MouseButton::Left) {
    //         self.ctx.trigger_hint(hint);
    //     }
    //     self.ctx.display().highlighted_hint = hint;

    //     let timer_id = TimerId::new(Topic::SelectionScrolling, self.ctx.window().id());
    //     self.ctx.scheduler_mut().unschedule(timer_id);

    //     if let MouseButton::Left | MouseButton::Right = button {
    //         // Copy selection on release, to prevent flooding the display server.
    //         self.ctx.copy_selection(ClipboardType::Selection);
    //     }
    // }

    pub fn clear_selection(&mut self) {
        // Clear the selection on the terminal.
        let mut terminal = self.terminal.lock();
        terminal.selection.take();
        drop(terminal);
        self.state.set_selection(None);
    }

    fn start_selection(&mut self, ty: SelectionType, point: Pos, side: Side) {
        self.copy_selection(ClipboardType::Selection);
        let mut terminal = self.terminal.lock();
        terminal.selection = Some(Selection::new(ty, point, side));
        drop(terminal);
    }

    #[allow(dead_code)]
    pub fn update_selection_scrolling(&self, _mouse_y: f64) {
        // println!("{:?}", mouse_y);
    }

    // pub fn update_selection(&mut self, mut point: Pos, side: Side) {
    pub fn update_selection(&mut self, mut point: Pos) {
        let mut terminal = self.terminal.lock();
        let mut selection = match terminal.selection.take() {
            Some(selection) => selection,
            None => return,
        };

        // Treat motion over message bar like motion over the last line.
        point.row = std::cmp::min(point.row, terminal.bottommost_line());

        // Update selection.
        // selection.update(point, side);
        selection.update(point, Side::Left);

        // Move vi cursor and expand selection.
        if terminal.mode().contains(Mode::VI) {
            terminal.vi_mode_cursor = point;
            selection.include_all();
        }

        self.state.set_selection(selection.to_range(&terminal));
        terminal.selection = Some(selection);
        drop(terminal);
    }

    #[inline]
    #[allow(unused)]
    pub fn selection_is_empty(&self) -> bool {
        let terminal = self.terminal.lock();
        let is_empty = terminal.selection.is_none();
        drop(terminal);
        is_empty
    }

    pub fn on_left_click(&mut self, point: Pos) {
        let side = self.layout.mouse.square_side;

        match self.layout.mouse.click_state {
            ClickState::Click => {
                self.clear_selection();

                // Start new empty selection.
                if self.messenger.get_modifiers().ctrl() {
                    self.start_selection(SelectionType::Block, point, side);
                } else {
                    self.start_selection(SelectionType::Simple, point, side);
                }
            }
            ClickState::DoubleClick => {
                self.start_selection(SelectionType::Semantic, point, side);
            }
            ClickState::TripleClick => {
                self.start_selection(SelectionType::Lines, point, side);
            }
            ClickState::None => (),
        };

        // Move vi mode cursor to mouse click position.
        // if self.ctx.terminal().mode().contains(TermMode::VI) && !self.ctx.search_active() {
        //     self.ctx.terminal_mut().vi_mode_cursor.point = point;
        //     self.ctx.mark_dirty();
        // }
    }

    #[inline]
    pub fn paste(&mut self, text: &str, bracketed: bool) {
        if bracketed && self.get_mode().contains(Mode::BRACKETED_PASTE) {
            self.messenger.send_bytes(b"\x1b[200~"[..].to_vec());

            // Write filtered escape sequences.
            //
            // We remove `\x1b` to ensure it's impossible for the pasted text to write the bracketed
            // paste end escape `\x1b[201~` and `\x03` since some shells incorrectly terminate
            // bracketed paste on its receival.
            let filtered = text.replace(['\x1b', '\x03'], "");
            self.messenger.send_bytes(filtered.into_bytes());

            self.messenger.send_bytes(b"\x1b[201~"[..].to_vec());
        } else {
            self.messenger
                .send_bytes(text.replace("\r\n", "\r").replace('\n', "\r").into_bytes());
        }
    }

    #[inline]
    pub fn init(&mut self, color: colors::ColorWGPU) {
        self.sugarloaf.init(color, self.layout.styles.term);
    }

    #[inline]
    pub fn render(&mut self) {
        let mut terminal = self.terminal.lock();
        let visible_rows = terminal.visible_rows();
        let cursor = terminal.cursor();
        drop(terminal);

        self.state.set_ime(self.ime.preedit());

        self.state.update(
            visible_rows,
            cursor,
            &mut self.sugarloaf,
            self.layout.styles.term,
            self.layout.styles.tabs,
            &self.tabs,
        );

        self.sugarloaf.render();
    }

    #[inline]
    pub fn scroll(&mut self, _new_scroll_x_px: f64, new_scroll_y_px: f64) {
        // let width = self.layout.width as f64;
        // let height = self.layout.height as f64;

        // if self
        //     .ctx
        //     .terminal()
        //     .mode()
        //     .contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL)
        //     && !self.ctx.modifiers().shift()
        // {
        // // let multiplier = f64::from(self.ctx.config().terminal_config.scrolling.multiplier);

        // // self.layout.mouse_mut().accumulated_scroll.x += new_scroll_x_px;//* multiplier;
        // // self.layout.mouse_mut().accumulated_scroll.y += new_scroll_y_px;// * multiplier;

        // // // The chars here are the same as for the respective arrow keys.
        // let line_cmd = if new_scroll_y_px > 0. { b'A' } else { b'B' };
        // let column_cmd = if new_scroll_x_px > 0. { b'D' } else { b'C' };

        // // let lines = (self.layout.cursor.accumulated_scroll.y / self.layout.font_size as f64).abs() as usize;
        // let lines = 1;
        // let columns = (self.layout.cursor.accumulated_scroll.x / width).abs() as usize;

        // let mut content = Vec::with_capacity(3 * (lines + columns));

        // for _ in 0..lines {
        //     content.push(0x1b);
        //     content.push(b'O');
        //     content.push(line_cmd);
        // }

        // for _ in 0..columns {
        //     content.push(0x1b);
        //     content.push(b'O');
        //     content.push(column_cmd);
        // }

        // println!("{:?} {:?} {:?} {:?}", content, lines, columns, self.layout.cursor);
        // if content.len() > 0 {
        //     self.messenger.write_to_pty(content);
        // }
        // }

        self.layout.mouse_mut().accumulated_scroll.y +=
            new_scroll_y_px * self.layout.mouse.multiplier;
        let lines = (self.layout.mouse.accumulated_scroll.y
            / self.layout.font_size as f64) as i32;

        if lines != 0 {
            let mut terminal = self.terminal.lock();
            terminal.scroll_display(Scroll::Delta(lines));
            drop(terminal);
        }
    }

    #[inline]
    pub fn layout(&mut self) -> &Layout {
        &self.layout
    }

    #[inline]
    pub fn layout_mut(&mut self) -> &mut Layout {
        &mut self.layout
    }

    #[inline]
    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) -> &mut Self {
        self.sugarloaf.resize(new_size.width, new_size.height);
        self.layout
            .set_size(new_size.width, new_size.height)
            .update();
        let (c, l) = self.layout.compute();

        let mut terminal = self.terminal.lock();
        terminal.resize::<Layout>(self.layout.columns, self.layout.rows);
        drop(terminal);

        let _ = self.messenger.send_resize(
            new_size.width as u16,
            new_size.height as u16,
            c as u16,
            l as u16,
        );
        self
    }

    pub fn set_scale(
        &mut self,
        new_scale: f32,
        new_size: winit::dpi::PhysicalSize<u32>,
    ) -> &mut Self {
        self.sugarloaf
            .resize(new_size.width, new_size.height)
            .rescale(new_scale);

        self.layout
            .set_scale(new_scale)
            .set_size(new_size.width, new_size.height)
            .update();
        self
    }
}
