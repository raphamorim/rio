mod bindings;
mod constants;
mod context;
mod messenger;
mod mouse;
mod navigation;
mod state;
pub mod window;

use crate::crosswords::vi_mode::ViMotion;
use crate::screen::bindings::MouseBinding;
use std::borrow::Cow;
use winit::event::KeyEvent;
use winit::event::Modifiers;
use winit::event::MouseButton;
use winit::window::raw_window_handle::HasRawDisplayHandle;
// use winit::window::raw_window_handle::HasRawWindowHandle;
use crate::clipboard::{Clipboard, ClipboardType};
use crate::crosswords::grid::Dimensions;
use crate::crosswords::pos::{Column, Line};
use crate::crosswords::{
    grid::Scroll,
    pos::{Pos, Side},
    Crosswords, Mode, MIN_COLUMNS, MIN_LINES,
};
use crate::event::{ClickState, EventProxy};
use crate::ime::Ime;
use crate::router;
#[cfg(target_os = "macos")]
use crate::screen::constants::{DEADZONE_END_Y, DEADZONE_START_X, DEADZONE_START_Y};
use crate::screen::{
    bindings::{Action as Act, BindingKey, BindingMode, FontSizeAction},
    context::ContextManager,
    mouse::Mouse,
};
use crate::selection::{Selection, SelectionType};
use messenger::Messenger;
use rio_config::colors::{term::List, ColorWGPU};
use state::State;
use std::cmp::max;
use std::cmp::min;
use std::error::Error;
use std::rc::Rc;
use sugarloaf::{layout::SugarloafLayout, Sugarloaf, SugarloafErrors};
use winit::event::ElementState;
#[cfg(target_os = "macos")]
use winit::keyboard::ModifiersKeyState;
use winit::keyboard::{Key, KeyLocation, ModifiersState};
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;

/// Minimum number of pixels at the bottom/top where selection scrolling is performed.
const MIN_SELECTION_SCROLLING_HEIGHT: f32 = 5.;

/// Number of pixels for increasing the selection scrolling speed factor by one.
const SELECTION_SCROLLING_STEP: f32 = 10.;

impl Dimensions for SugarloafLayout {
    #[inline]
    fn columns(&self) -> usize {
        self.columns
    }

    #[inline]
    fn screen_lines(&self) -> usize {
        self.lines
    }

    #[inline]
    fn total_lines(&self) -> usize {
        self.screen_lines()
    }
}

pub struct Screen {
    bindings: bindings::KeyBindings,
    mouse_bindings: Vec<MouseBinding>,
    clipboard: Clipboard,
    pub modifiers: Modifiers,
    pub mouse: Mouse,
    pub ime: Ime,
    pub state: State,
    pub sugarloaf: Sugarloaf,
    pub context_manager: context::ContextManager<EventProxy>,
}

impl Screen {
    pub async fn new(
        winit_window: &winit::window::Window,
        config: &Rc<rio_config::Config>,
        event_proxy: EventProxy,
        font_database: &sugarloaf::font::loader::Database,
        native_tab_id: Option<String>,
    ) -> Result<Screen, Box<dyn Error>> {
        let size = winit_window.inner_size();
        let scale = winit_window.scale_factor();
        // let raw_window_handle = winit_window.raw_window_handle();
        let raw_display_handle = winit_window.raw_display_handle();
        let window_id = winit_window.id();

        let power_preference: wgpu::PowerPreference = match config.performance {
            rio_config::Performance::High => wgpu::PowerPreference::HighPerformance,
            rio_config::Performance::Low => wgpu::PowerPreference::LowPower,
        };

        let mut padding_y_bottom = 0.0;
        if config.navigation.is_placed_on_bottom() {
            padding_y_bottom += config.fonts.size
        }

        let mut padding_y_top = constants::PADDING_Y;

        #[cfg(not(target_os = "macos"))]
        {
            if config.navigation.is_placed_on_top() {
                padding_y_top = constants::PADDING_Y_WITH_TAB_ON_TOP;
            }
        }

        if config.navigation.is_native() {
            if native_tab_id.is_some() {
                padding_y_top *= 2.0;
            }

            padding_y_top += 2.0;
        }

        let sugarloaf_layout = SugarloafLayout::new(
            size.width as f32,
            size.height as f32,
            (config.padding_x, padding_y_top, padding_y_bottom),
            scale as f32,
            config.fonts.size,
            config.line_height,
            (MIN_COLUMNS, MIN_LINES),
        );

        let mut sugarloaf_errors: Option<SugarloafErrors> = None;
        let sugarloaf: Sugarloaf = match Sugarloaf::new(
            winit_window,
            power_preference,
            config.fonts.to_owned(),
            sugarloaf_layout,
            Some(font_database),
        )
        .await
        {
            Ok(instance) => instance,
            Err(instance_with_errors) => {
                sugarloaf_errors = Some(instance_with_errors.errors);
                instance_with_errors.instance
            }
        };

        let state = State::new(config, winit_window.theme());

        let clipboard = unsafe { Clipboard::new(raw_display_handle) };

        let bindings = bindings::default_key_bindings(
            config.bindings.keys.clone(),
            config.navigation.is_plain(),
        );
        let ime = Ime::new();

        let is_collapsed = config.navigation.is_collapsed_mode();
        let is_native = config.navigation.is_native();
        let context_manager_config = context::ContextManagerConfig {
            use_current_path: config.navigation.use_current_path,
            shell: config.shell.clone(),
            spawn_performer: true,
            use_fork: config.use_fork,
            working_dir: config.working_dir.clone(),
            is_collapsed,
            is_native,
            // When navigation is collapsed and does not contain any color rule
            // does not make sense fetch for foreground process names
            should_update_titles: !(is_collapsed
                || is_native && config.navigation.color_automation.is_empty()),
        };
        // let default_cursor_style = CursorStyle {
        //     shape: state.get_cursor_state(),
        //     blinking: config.blinking_cursor,
        // };
        let context_manager = context::ContextManager::start(
            (sugarloaf.layout.width_u32, sugarloaf.layout.height_u32),
            (sugarloaf.layout.columns, sugarloaf.layout.lines),
            (&state.get_cursor_state(), config.blinking_cursor),
            event_proxy,
            window_id,
            context_manager_config,
            sugarloaf_errors,
        )?;

        Ok(Screen {
            mouse_bindings: bindings::default_mouse_bindings(),
            modifiers: Modifiers::default(),
            context_manager,
            ime,
            sugarloaf,
            mouse: Mouse::default(),
            state,
            bindings,
            clipboard,
        })
    }

    #[inline]
    pub fn ctx(&self) -> &ContextManager<EventProxy> {
        &self.context_manager
    }

    #[inline]
    pub fn ctx_mut(&mut self) -> &mut ContextManager<EventProxy> {
        &mut self.context_manager
    }

    #[inline]
    pub fn set_modifiers(&mut self, modifiers: Modifiers) {
        self.modifiers = modifiers;
    }

    #[inline]
    pub fn reset_mouse(&mut self) {
        self.mouse.accumulated_scroll = mouse::AccumulatedScroll::default();
    }

    #[inline]
    pub fn mouse_position(&self, display_offset: usize) -> Pos {
        let layout = &self.sugarloaf.layout;
        let line_fac =
            ((layout.sugarheight) * self.sugarloaf.layout.scale_factor) as usize;

        let mouse_x = self.mouse.x + layout.margin.x as usize;
        let col = mouse_x
            / (layout.sugarwidth.floor() * self.sugarloaf.layout.scale_factor) as usize;
        // TODO: Refactor
        let col = col.saturating_sub(1);
        let col = col.saturating_sub(1);
        let col = std::cmp::min(Column(col), Column(layout.columns));

        // println!("{:?}", self.mouse.x);
        // println!("{:?}", layout.sugarwidth);
        // println!("{:?}", col);

        let line = self.mouse.y.saturating_sub(
            (layout.margin.top_y * 2. * self.sugarloaf.layout.scale_factor) as usize,
        ) / line_fac;
        let calc_line = std::cmp::min(line, layout.lines - 1);
        let line = Line(calc_line as i32) - (display_offset);

        Pos::new(line, col)
    }

    #[inline]
    #[cfg(target_os = "macos")]
    pub fn is_macos_deadzone(&self, pos_y: f64) -> bool {
        let scale_f64 = self.sugarloaf.layout.scale_factor as f64;
        pos_y <= DEADZONE_START_Y * scale_f64 && pos_y >= DEADZONE_END_Y * scale_f64
    }

    #[inline]
    #[cfg(target_os = "macos")]
    pub fn is_macos_deadzone_draggable(&self, pos_x: f64) -> bool {
        let scale_f64 = self.sugarloaf.layout.scale_factor as f64;
        pos_x >= DEADZONE_START_X * scale_f64
    }

    #[inline]
    #[cfg(target_os = "macos")]
    pub fn update_top_y_for_native_tabs(&mut self, tab_num: usize) {
        if !self.context_manager.config.is_native {
            return;
        }

        let expected = if tab_num > 1 {
            constants::PADDING_Y_WITH_MANY_NATIVE_TAB
        } else {
            constants::PADDING_Y_WITH_SINGLE_NATIVE_TAB
        };

        if self.sugarloaf.layout.margin.top_y == expected {
            return;
        }

        self.sugarloaf.layout.set_top_y_for_native_tabs(expected);

        let width = self.sugarloaf.layout.width_u32 as u16;
        let height = self.sugarloaf.layout.height_u32 as u16;
        let columns = self.sugarloaf.layout.columns;
        let lines = self.sugarloaf.layout.lines;
        self.resize_all_contexts(width, height, columns, lines);
    }

    /// update_config is triggered in any configuration file update
    #[inline]
    pub fn update_config(
        &mut self,
        config: &Rc<rio_config::Config>,
        current_theme: Option<winit::window::Theme>,
        db: &sugarloaf::font::loader::Database,
    ) {
        if let Some(err) = self
            .sugarloaf
            .update_font(config.fonts.to_owned(), Some(db))
        {
            self.context_manager
                .report_error_fonts_not_found(err.fonts_not_found);
            return;
        }

        let mut padding_y_bottom = 0.0;
        if config.navigation.is_placed_on_bottom() {
            padding_y_bottom += config.fonts.size
        }

        self.sugarloaf.layout.recalculate(
            config.fonts.size,
            config.line_height,
            config.padding_x,
            padding_y_bottom,
        );

        self.sugarloaf.layout.update();
        self.state = State::new(config, current_theme);

        for context in self.ctx().contexts() {
            let mut terminal = context.terminal.lock();
            terminal.cursor_shape = self.state.get_cursor_state_from_ref().content;
            terminal.blinking_cursor = config.blinking_cursor;
        }

        let width = self.sugarloaf.layout.width_u32 as u16;
        let height = self.sugarloaf.layout.height_u32 as u16;
        let columns = self.sugarloaf.layout.columns;
        let lines = self.sugarloaf.layout.lines;
        self.resize_all_contexts(width, height, columns, lines);

        self.init(
            self.state.named_colors.background.1,
            config.background.mode.is_image(),
            &config.background.image,
        );
    }

    #[inline]
    pub fn change_font_size(&mut self, action: FontSizeAction) {
        let should_update = match action {
            FontSizeAction::Increase => self.sugarloaf.layout.increase_font_size(),
            FontSizeAction::Decrease => self.sugarloaf.layout.decrease_font_size(),
            FontSizeAction::Reset => self.sugarloaf.layout.reset_font_size(),
        };

        if !should_update {
            return;
        }
        self.sugarloaf.layout.update();
        self.sugarloaf.calculate_bounds();

        let width = self.sugarloaf.layout.width_u32 as u16;
        let height = self.sugarloaf.layout.height_u32 as u16;
        let columns = self.sugarloaf.layout.columns;
        let lines = self.sugarloaf.layout.lines;
        self.resize_all_contexts(width, height, columns, lines);
    }

    #[inline]
    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) -> &mut Self {
        self.sugarloaf.resize(new_size.width, new_size.height);

        self.resize_all_contexts(
            new_size.width as u16,
            new_size.height as u16,
            self.sugarloaf.layout.columns,
            self.sugarloaf.layout.lines,
        );
        self
    }

    #[inline]
    pub fn set_scale(
        &mut self,
        new_scale: f32,
        new_size: winit::dpi::PhysicalSize<u32>,
    ) -> &mut Self {
        self.sugarloaf
            .rescale(new_scale)
            .resize(new_size.width, new_size.height)
            .calculate_bounds();

        self
    }

    #[inline]
    pub fn resize_all_contexts(
        &mut self,
        width: u16,
        height: u16,
        columns: usize,
        lines: usize,
    ) {
        for context in self.ctx().contexts() {
            let mut terminal = context.terminal.lock();
            terminal.resize::<SugarloafLayout>(columns, lines);
            drop(terminal);
            let _ = context.messenger.send_resize(
                width,
                height,
                columns as u16,
                lines as u16,
            );
        }
    }

    #[inline]
    pub fn clipboard_get(&mut self, clipboard_type: ClipboardType) -> String {
        self.clipboard.get(clipboard_type)
    }

    #[inline]
    pub fn scroll_bottom_when_cursor_not_visible(&mut self) {
        let mut terminal = self.ctx_mut().current().terminal.lock();
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
        let mut terminal = self.ctx().current().terminal.lock();
        let display_offset = terminal.display_offset();
        drop(terminal);
        display_offset
    }

    #[inline]
    pub fn get_mode(&self) -> Mode {
        let terminal = self.ctx().current().terminal.lock();
        let mode = terminal.mode();
        drop(terminal);
        mode
    }

    #[inline]
    #[allow(unused)]
    pub fn colors(&mut self) -> List {
        let terminal = self.ctx().current().terminal.lock();
        let mode = terminal.colors();
        drop(terminal);
        mode
    }

    #[inline]
    pub fn process_key_event(&mut self, key: &winit::event::KeyEvent) {
        if self.ime.preedit().is_some() {
            return;
        }

        let mode = self.get_mode();
        let mods = self.modifiers.state();

        if key.state == ElementState::Released {
            if mode.contains(Mode::KEYBOARD_REPORT_EVENT_TYPES)
                && !mode.contains(Mode::VI)
            {
                // NOTE: echoing the key back on release is how it's done in kitty/foot and
                // it's how it should be done according to the kitty author
                // https://github.com/kovidgoyal/kitty/issues/6516#issuecomment-1659454350
                let bytes: Cow<'static, [u8]> = match key.logical_key.as_ref() {
                    Key::Tab => [b'\t'].as_slice().into(),
                    Key::Enter => [b'\r'].as_slice().into(),
                    Key::Delete => [b'\x7f'].as_slice().into(),
                    Key::Escape => [b'\x1b'].as_slice().into(),
                    _ => self.build_key_sequence(key.to_owned(), mods, mode).into(),
                };

                self.ctx_mut().current_mut().messenger.send_write(bytes);
            }

            return;
        }

        let binding_mode = BindingMode::new(&mode);
        let mut ignore_chars = None;

        for i in 0..self.bindings.len() {
            let binding = &self.bindings[i];

            // When the logical key is some named key, use it, otherwise fallback to
            // key without modifiers to account for bindings.
            let logical_key = if matches!(key.logical_key, Key::Character(_)) {
                key.key_without_modifiers()
            } else {
                key.logical_key.clone()
            };

            let key = match (&binding.trigger, logical_key) {
                (BindingKey::Scancode(_), _) => BindingKey::Scancode(key.physical_key),
                (_, code) => BindingKey::Keycode {
                    key: code,
                    location: key.location,
                },
            };

            if binding.is_triggered_by(binding_mode.to_owned(), mods, &key) {
                *ignore_chars.get_or_insert(true) &= binding.action != Act::ReceiveChar;

                match &binding.action {
                    Act::Esc(s) => {
                        let current_context = self.context_manager.current_mut();
                        self.state.set_selection(None);
                        let mut terminal = current_context.terminal.lock();
                        terminal.selection.take();
                        terminal.scroll_display(Scroll::Bottom);
                        drop(terminal);
                        current_context.messenger.send_bytes(s.clone().into_bytes());
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
                    Act::ViMotion(motion) => {
                        let mut terminal =
                            self.context_manager.current_mut().terminal.lock();
                        terminal.vi_motion(*motion);
                        drop(terminal);
                    }
                    Act::ConfigEditor => {
                        self.context_manager.switch_to_settings();
                    }
                    Act::WindowCreateNew => {
                        self.context_manager.create_new_window();
                    }
                    Act::TabCreateNew => {
                        let redirect = true;

                        self.context_manager.add_context(
                            redirect,
                            (
                                self.sugarloaf.layout.width_u32,
                                self.sugarloaf.layout.height_u32,
                            ),
                            (self.sugarloaf.layout.columns, self.sugarloaf.layout.lines),
                            (
                                &self.state.get_cursor_state_from_ref(),
                                self.state.has_blinking_enabled,
                            ),
                        );

                        self.render();
                    }
                    Act::TabCloseCurrent => {
                        self.clear_selection();

                        if self.context_manager.config.is_native {
                            self.context_manager.close_current_window();
                        } else {
                            // Kill current context will trigger terminal.exit
                            // then RioEvent::Exit and eventually try_close_existent_tab
                            self.context_manager.kill_current_context();
                        }
                    }
                    Act::Quit => {
                        // TODO: Add it in event system
                        std::process::exit(0);
                    }
                    Act::IncreaseFontSize => {
                        self.change_font_size(FontSizeAction::Increase);
                    }
                    Act::DecreaseFontSize => {
                        self.change_font_size(FontSizeAction::Decrease);
                    }
                    Act::ResetFontSize => {
                        self.change_font_size(FontSizeAction::Reset);
                    }
                    Act::ScrollPageUp => {
                        // Move vi mode cursor.
                        let mut terminal =
                            self.context_manager.current_mut().terminal.lock();
                        let scroll_lines = terminal.grid.screen_lines() as i32;
                        terminal.vi_mode_cursor =
                            terminal.vi_mode_cursor.scroll(&terminal, scroll_lines);
                        terminal.scroll_display(Scroll::PageUp);
                        drop(terminal);
                    }
                    Act::ScrollPageDown => {
                        // Move vi mode cursor.
                        let mut terminal =
                            self.context_manager.current_mut().terminal.lock();
                        let scroll_lines = -(terminal.grid.screen_lines() as i32);

                        terminal.vi_mode_cursor =
                            terminal.vi_mode_cursor.scroll(&terminal, scroll_lines);

                        terminal.scroll_display(Scroll::PageDown);
                        drop(terminal);
                    }
                    Act::ScrollHalfPageUp => {
                        // Move vi mode cursor.
                        let mut terminal =
                            self.context_manager.current_mut().terminal.lock();
                        let scroll_lines = terminal.grid.screen_lines() as i32 / 2;

                        terminal.vi_mode_cursor =
                            terminal.vi_mode_cursor.scroll(&terminal, scroll_lines);

                        terminal.scroll_display(Scroll::Delta(scroll_lines));
                        drop(terminal);
                    }
                    Act::ScrollHalfPageDown => {
                        // Move vi mode cursor.
                        let mut terminal =
                            self.context_manager.current_mut().terminal.lock();
                        let scroll_lines = -(terminal.grid.screen_lines() as i32 / 2);

                        terminal.vi_mode_cursor =
                            terminal.vi_mode_cursor.scroll(&terminal, scroll_lines);

                        terminal.scroll_display(Scroll::Delta(scroll_lines));
                        drop(terminal);
                    }
                    Act::ScrollToTop => {
                        let mut terminal =
                            self.context_manager.current_mut().terminal.lock();
                        terminal.scroll_display(Scroll::Top);

                        let topmost_line = terminal.grid.topmost_line();
                        terminal.vi_mode_cursor.pos.row = topmost_line;
                        terminal.vi_motion(ViMotion::FirstOccupied);
                        drop(terminal);
                    }
                    Act::ScrollToBottom => {
                        let mut terminal =
                            self.context_manager.current_mut().terminal.lock();
                        terminal.scroll_display(Scroll::Bottom);

                        // Move vi mode cursor.
                        terminal.vi_mode_cursor.pos.row = terminal.grid.bottommost_line();

                        // Move to beginning twice, to always jump across linewraps.
                        terminal.vi_motion(ViMotion::FirstOccupied);
                        terminal.vi_motion(ViMotion::FirstOccupied);
                        drop(terminal);
                    }
                    Act::ScrollLineUp => {
                        let mut terminal =
                            self.context_manager.current_mut().terminal.lock();
                        terminal.scroll_display(Scroll::Delta(1));
                        drop(terminal);
                    }
                    Act::ScrollLineDown => {
                        let mut terminal =
                            self.context_manager.current_mut().terminal.lock();
                        terminal.scroll_display(Scroll::Delta(-1));
                        drop(terminal);
                    }
                    Act::ClearHistory => {
                        let mut terminal =
                            self.context_manager.current_mut().terminal.lock();
                        terminal.clear_saved_history();
                        drop(terminal);

                        self.render();
                    }
                    Act::Minimize => {
                        self.context_manager.minimize();
                    }
                    Act::Hide => {
                        self.context_manager.hide();
                    }
                    Act::SelectTab1 => {
                        self.context_manager.select_tab(0);
                    }
                    Act::SelectTab2 => {
                        self.context_manager.select_tab(1);
                    }
                    Act::SelectTab3 => {
                        self.context_manager.select_tab(2);
                    }
                    Act::SelectTab4 => {
                        self.context_manager.select_tab(3);
                    }
                    Act::SelectTab5 => {
                        self.context_manager.select_tab(4);
                    }
                    Act::SelectTab6 => {
                        self.context_manager.select_tab(5);
                    }
                    Act::SelectTab7 => {
                        self.context_manager.select_tab(6);
                    }
                    Act::SelectTab8 => {
                        self.context_manager.select_tab(7);
                    }
                    Act::SelectTab9 => {
                        self.context_manager.select_tab(8);
                    }
                    Act::SelectLastTab => {
                        self.context_manager.select_last_tab();
                    }
                    Act::SelectNextTab => {
                        self.clear_selection();
                        self.context_manager.switch_to_next();
                        self.render();
                    }
                    Act::SelectPrevTab => {
                        self.clear_selection();
                        self.context_manager.switch_to_prev();
                        self.render();
                    }
                    Act::ReceiveChar | Act::None => (),
                    _ => (),
                }
            }
        }

        if ignore_chars.unwrap_or(false) || mode.contains(Mode::VI) {
            return;
        }

        let text = key.text_with_all_modifiers().unwrap_or_default();

        // We use legacy input when we have associated text with
        // the given key and we have one of the following situations:
        //
        // 1. No keyboard input protocol is enabled.
        // 2. Mode is KEYBOARD_DISAMBIGUATE_ESC_CODES, but we have text + empty or Shift
        //    modifiers and the location of the key is not on the numpad, and it's not an `Esc`.
        let write_legacy = !mode.contains(Mode::KEYBOARD_REPORT_ALL_KEYS_AS_ESC)
            && !text.is_empty()
            && (!mode.contains(Mode::KEYBOARD_DISAMBIGUATE_ESC_CODES)
                || (mode.contains(Mode::KEYBOARD_DISAMBIGUATE_ESC_CODES)
                    && (mods.is_empty() || mods == ModifiersState::SHIFT)
                    && key.location != KeyLocation::Numpad
                    // Special case escape here.
                    && key.logical_key != Key::Escape));

        // Handle legacy char writing.
        let bytes = if write_legacy {
            let mut bytes = Vec::with_capacity(text.len() + 1);
            if self.alt_send_esc() && text.len() == 1 {
                bytes.push(b'\x1b');
            }

            bytes.extend_from_slice(text.as_bytes());
            bytes
        } else {
            // Otherwise we should build the key sequence for the given input.
            self.build_key_sequence(key.to_owned(), mods, mode)
        };

        // Write only when we have something to write.
        if !bytes.is_empty() {
            self.scroll_bottom_when_cursor_not_visible();
            self.clear_selection();

            self.ctx_mut().current_mut().messenger.send_bytes(bytes);
        }
    }

    #[inline]
    pub fn process_mouse_bindings(&mut self, button: MouseButton) {
        let mode = self.get_mode();
        let binding_mode = BindingMode::new(&mode);
        let mouse_mode = self.mouse_mode();
        let mods = self.modifiers.state();

        for i in 0..self.mouse_bindings.len() {
            let mut binding = self.mouse_bindings[i].clone();

            // Require shift for all modifiers when mouse mode is active.
            if mouse_mode {
                binding.mods |= ModifiersState::SHIFT;
            }

            if binding.is_triggered_by(binding_mode.to_owned(), mods, &button)
                && binding.action == Act::PasteSelection
            {
                let content = self.clipboard.get(ClipboardType::Selection);
                self.paste(&content, true);
            }
        }
    }

    /// Whether we should send `ESC` due to `Alt` being pressed.
    #[cfg(not(target_os = "macos"))]
    #[inline]
    fn alt_send_esc(&mut self) -> bool {
        self.modifiers.state().alt_key()
    }

    #[cfg(target_os = "macos")]
    #[inline]
    fn alt_send_esc(&mut self) -> bool {
        self.modifiers.state().alt_key()
            && (self.state.option_as_alt == *"both"
                || (self.state.option_as_alt == *"left"
                    && self.modifiers.lalt_state() == ModifiersKeyState::Pressed)
                || (self.state.option_as_alt == *"right"
                    && self.modifiers.ralt_state() == ModifiersKeyState::Pressed))
    }

    #[inline(never)]
    fn build_key_sequence(
        &mut self,
        key: KeyEvent,
        mods: ModifiersState,
        mode: Mode,
    ) -> Vec<u8> {
        let mut modifiers = 0;
        if mods.shift_key() {
            modifiers |= 0b0001;
        }

        if mods.alt_key() {
            modifiers |= 0b0010;
        }

        if mods.control_key() {
            modifiers |= 0b0100;
        }

        if mods.super_key() {
            modifiers |= 0b1000;
        }

        // The `1` must be added to result.
        modifiers += 1;

        let named_csi_u = mode.intersects(
            Mode::KEYBOARD_REPORT_ALL_KEYS_AS_ESC
                | Mode::KEYBOARD_DISAMBIGUATE_ESC_CODES
                | Mode::KEYBOARD_REPORT_EVENT_TYPES,
        );
        // Send CSI u for numpad
        let csi_u_numpad = key.location == KeyLocation::Numpad && named_csi_u;
        let encode_all = mode.contains(Mode::KEYBOARD_REPORT_ALL_KEYS_AS_ESC);
        let send_event_type = mode.contains(Mode::KEYBOARD_REPORT_EVENT_TYPES)
            && (key.repeat || key.state == ElementState::Released);

        let (codepoint, suffix): (Cow<'static, str>, char) = match key
            .logical_key
            .as_ref()
        {
            // Special case numpad.
            Key::Character("0") if csi_u_numpad => ("57399".into(), 'u'),
            Key::Character("1") if csi_u_numpad => ("57400".into(), 'u'),
            Key::Character("2") if csi_u_numpad => ("57401".into(), 'u'),
            Key::Character("3") if csi_u_numpad => ("57402".into(), 'u'),
            Key::Character("4") if csi_u_numpad => ("57403".into(), 'u'),
            Key::Character("5") if csi_u_numpad => ("57404".into(), 'u'),
            Key::Character("6") if csi_u_numpad => ("57405".into(), 'u'),
            Key::Character("7") if csi_u_numpad => ("57406".into(), 'u'),
            Key::Character("8") if csi_u_numpad => ("57407".into(), 'u'),
            Key::Character("9") if csi_u_numpad => ("57408".into(), 'u'),
            Key::Character(".") if csi_u_numpad => ("57409".into(), 'u'),
            Key::Character("/") if csi_u_numpad => ("57410".into(), 'u'),
            Key::Character("*") if csi_u_numpad => ("57411".into(), 'u'),
            Key::Character("-") if csi_u_numpad => ("57412".into(), 'u'),
            Key::Character("+") if csi_u_numpad => ("57413".into(), 'u'),
            Key::Enter if csi_u_numpad => ("57414".into(), 'u'),
            Key::Character("=") if csi_u_numpad => ("57415".into(), 'u'),
            // KP_SEPARATOR if csi_u_numpad => ("57415".into(), 'u'),
            Key::ArrowLeft if csi_u_numpad => ("57417".into(), 'u'),
            Key::ArrowRight if csi_u_numpad => ("57418".into(), 'u'),
            Key::ArrowUp if csi_u_numpad => ("57419".into(), 'u'),
            Key::ArrowDown if csi_u_numpad => ("57420".into(), 'u'),
            Key::PageUp if csi_u_numpad => ("57421".into(), 'u'),
            Key::PageDown if csi_u_numpad => ("57422".into(), 'u'),
            Key::Home if csi_u_numpad => ("57423".into(), 'u'),
            Key::End if csi_u_numpad => ("57424".into(), 'u'),
            Key::Insert if csi_u_numpad => ("57425".into(), 'u'),
            Key::Delete if csi_u_numpad => ("57426".into(), 'u'),
            // KP_BEGIN if csi_u_numpad => ("57427".into(), 'u'),
            // Handle common keys.
            Key::ArrowLeft if mods.is_empty() && !send_event_type => ("".into(), 'D'),
            Key::ArrowLeft => ("1".into(), 'D'),
            Key::ArrowRight if mods.is_empty() && !send_event_type => ("".into(), 'C'),
            Key::ArrowRight => ("1".into(), 'C'),
            Key::ArrowUp if mods.is_empty() && !send_event_type => ("".into(), 'A'),
            Key::ArrowUp => ("1".into(), 'A'),
            Key::ArrowDown if mods.is_empty() && !send_event_type => ("".into(), 'B'),
            Key::ArrowDown => ("1".into(), 'B'),
            Key::Home if mods.is_empty() && !send_event_type => ("".into(), 'H'),
            Key::Home => ("1".into(), 'H'),
            Key::End if mods.is_empty() && !send_event_type => ("".into(), 'F'),
            Key::End => ("1".into(), 'F'),
            Key::PageUp => ("5".into(), '~'),
            Key::PageDown => ("6".into(), '~'),
            Key::Insert => ("2".into(), '~'),
            Key::Delete => ("3".into(), '~'),
            Key::F1 if mods.is_empty() && named_csi_u && !send_event_type => {
                ("".into(), 'P')
            }
            Key::F1 if !mods.is_empty() || send_event_type => ("1".into(), 'P'),
            Key::F2 if mods.is_empty() && named_csi_u && !send_event_type => {
                ("".into(), 'Q')
            }
            Key::F2 if !mods.is_empty() || send_event_type => ("1".into(), 'Q'),
            // F3 diverges from alacritty's terminfo for CSI u modes.
            Key::F3 if named_csi_u => ("13".into(), '~'),
            Key::F3 if !mods.is_empty() => ("1".into(), 'R'),
            Key::F4 if mods.is_empty() && named_csi_u && !send_event_type => {
                ("".into(), 'S')
            }
            Key::F4 if !mods.is_empty() || send_event_type => ("1".into(), 'S'),
            Key::F5 => ("15".into(), '~'),
            Key::F6 => ("17".into(), '~'),
            Key::F7 => ("18".into(), '~'),
            Key::F8 => ("19".into(), '~'),
            Key::F9 => ("20".into(), '~'),
            Key::F10 => ("21".into(), '~'),
            Key::F11 => ("23".into(), '~'),
            Key::F12 => ("24".into(), '~'),
            // These keys are enabled regardless of mode and reported with the CSI u.
            Key::F13 => ("57376".into(), 'u'),
            Key::F14 => ("57377".into(), 'u'),
            Key::F15 => ("57378".into(), 'u'),
            Key::F16 => ("57379".into(), 'u'),
            Key::F17 => ("57380".into(), 'u'),
            Key::F18 => ("57381".into(), 'u'),
            Key::F19 => ("57382".into(), 'u'),
            Key::F20 => ("57383".into(), 'u'),
            Key::F21 => ("57384".into(), 'u'),
            Key::F22 => ("57385".into(), 'u'),
            Key::F23 => ("57386".into(), 'u'),
            Key::F24 => ("57387".into(), 'u'),
            Key::F25 => ("57388".into(), 'u'),
            Key::F26 => ("57389".into(), 'u'),
            Key::F27 => ("57390".into(), 'u'),
            Key::F28 => ("57391".into(), 'u'),
            Key::F29 => ("57392".into(), 'u'),
            Key::F30 => ("57393".into(), 'u'),
            Key::F31 => ("57394".into(), 'u'),
            Key::F32 => ("57395".into(), 'u'),
            Key::F33 => ("57396".into(), 'u'),
            Key::F34 => ("57397".into(), 'u'),
            Key::F35 => ("57398".into(), 'u'),
            Key::ScrollLock => ("57359".into(), 'u'),
            Key::PrintScreen => ("57361".into(), 'u'),
            Key::Pause => ("57362".into(), 'u'),
            Key::ContextMenu => ("57363".into(), 'u'),
            Key::MediaPlay => ("57428".into(), 'u'),
            Key::MediaPause => ("57429".into(), 'u'),
            Key::MediaPlayPause => ("57430".into(), 'u'),
            // Key::MediaReverse => ("57431".into(), 'u'),
            Key::MediaStop => ("57432".into(), 'u'),
            Key::MediaFastForward => ("57433".into(), 'u'),
            Key::MediaRewind => ("57434".into(), 'u'),
            Key::MediaTrackNext => ("57435".into(), 'u'),
            Key::MediaTrackPrevious => ("57436".into(), 'u'),
            Key::MediaRecord => ("57437".into(), 'u'),
            Key::AudioVolumeDown => ("57438".into(), 'u'),
            Key::AudioVolumeUp => ("57439".into(), 'u'),
            Key::AudioVolumeMute => ("57440".into(), 'u'),
            Key::Escape if named_csi_u => ("27".into(), 'u'),
            // Keys which are reported only when all key must be reported
            Key::CapsLock if encode_all => ("57358".into(), 'u'),
            Key::NumLock if encode_all => ("57360".into(), 'u'),
            // Left mods.
            Key::Shift if key.location == KeyLocation::Left && encode_all => {
                ("57441".into(), 'u')
            }
            Key::Control if key.location == KeyLocation::Left && encode_all => {
                ("57442".into(), 'u')
            }
            Key::Alt if key.location == KeyLocation::Left && encode_all => {
                ("57443".into(), 'u')
            }
            Key::Super if key.location == KeyLocation::Left && encode_all => {
                ("57444".into(), 'u')
            }
            Key::Hyper if key.location == KeyLocation::Left && encode_all => {
                ("57445".into(), 'u')
            }
            Key::Meta if key.location == KeyLocation::Left && encode_all => {
                ("57446".into(), 'u')
            }
            // Right mods.
            Key::Shift if key.location == KeyLocation::Right && encode_all => {
                ("57447".into(), 'u')
            }
            Key::Control if key.location == KeyLocation::Right && encode_all => {
                ("57448".into(), 'u')
            }
            Key::Alt if key.location == KeyLocation::Right && encode_all => {
                ("57449".into(), 'u')
            }
            Key::Super if key.location == KeyLocation::Right && encode_all => {
                ("57450".into(), 'u')
            }
            Key::Hyper if key.location == KeyLocation::Right && encode_all => {
                ("57451".into(), 'u')
            }
            Key::Meta if key.location == KeyLocation::Right && encode_all => {
                ("57452".into(), 'u')
            }

            Key::Enter if encode_all => ("13".into(), 'u'),
            Key::Tab if encode_all => ("9".into(), 'u'),
            Key::Backspace if encode_all => ("127".into(), 'u'),
            // When the character key ended up being a text, like when compose was done.
            Key::Character(c) if encode_all && c.chars().count() > 1 => ("0".into(), 'u'),
            Key::Character(c) => {
                let character = c.chars().next().unwrap();
                let base_character = character.to_lowercase().next().unwrap();

                let codepoint = u32::from(character);
                let base_codepoint = u32::from(base_character);

                let payload = if mode.contains(Mode::KEYBOARD_REPORT_ALTERNATE_KEYS)
                    && codepoint != base_codepoint
                {
                    format!("{codepoint}:{base_codepoint}")
                } else {
                    codepoint.to_string()
                };

                (payload.into(), 'u')
            }
            // In case we have text attached to the key, but we don't have a
            // matching logical key with the text, likely due to winit not being
            // able to map it.
            _ if encode_all && key.text.is_some() => ("0".into(), 'u'),
            _ => return Vec::new(),
        };

        let mut payload = format!("\x1b[{codepoint}");

        // Add modifiers information. Check for text to push `;`.
        if send_event_type
            || modifiers > 1
            || (mode.contains(Mode::KEYBOARD_REPORT_ASSOCIATED_TEXT)
                && key.text.is_some())
        {
            payload.push_str(&format!(";{modifiers}"));
        }

        // Push event types. The `Press` is default, so we don't have to push it.
        if send_event_type {
            payload.push(':');
            let event_type = match key.state {
                _ if key.repeat => '2',
                ElementState::Pressed => '1',
                ElementState::Released => '3',
            };
            payload.push(event_type);
        }

        if mode.contains(Mode::KEYBOARD_REPORT_ASSOCIATED_TEXT)
            && key.state != ElementState::Released
        {
            if let Some(text) = key.text {
                let mut codepoints = text.chars().map(u32::from);
                if let Some(codepoint) = codepoints.next() {
                    payload.push_str(&format!(";{codepoint}"));
                }
                // Push the rest of the chars.
                for codepoint in codepoints {
                    payload.push_str(&format!(":{codepoint}"));
                }
            }
        }

        // Terminate the sequence.
        payload.push(suffix);

        payload.into_bytes()
    }

    #[inline]
    pub fn try_close_existent_tab(&mut self) -> bool {
        if self.context_manager.len() > 1 {
            self.context_manager.close_context();
            return true;
        }

        false
    }

    pub fn copy_selection(&mut self, ty: ClipboardType) {
        let terminal = self.ctx().current().terminal.lock();
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

    #[inline]
    pub fn clear_selection(&mut self) {
        // Clear the selection on the terminal.
        let mut terminal = self.ctx().current().terminal.lock();
        terminal.selection.take();
        drop(terminal);
        self.state.set_selection(None);
    }

    fn start_selection(&mut self, ty: SelectionType, point: Pos, side: Side) {
        self.copy_selection(ClipboardType::Selection);
        let mut terminal = self.context_manager.current().terminal.lock();
        let selection = Selection::new(ty, point, side);
        self.state.set_selection(selection.to_range(&terminal));
        terminal.selection = Some(selection);
        drop(terminal);
    }

    #[inline]
    pub fn update_selection(&mut self, mut pos: Pos, side: Side) {
        let mut terminal = self.context_manager.current().terminal.lock();
        let mut selection = match terminal.selection.take() {
            Some(selection) => selection,
            None => return,
        };

        // Treat motion over message bar like motion over the last line.
        pos.row = std::cmp::min(pos.row, terminal.bottommost_line());

        // Update selection.
        selection.update(pos, side);

        // Move vi cursor and expand selection.
        if terminal.mode().contains(Mode::VI) {
            terminal.vi_mode_cursor.pos = pos;
            selection.include_all();
        }

        self.state.set_selection(selection.to_range(&terminal));
        terminal.selection = Some(selection);
        drop(terminal);
    }

    // TODO: Exec
    // #[allow(unused)]
    // pub fn exec<I, S>(&self, program: &str, args: I)
    // where
    //     I: IntoIterator<Item = S> + Debug + Copy,
    //     S: AsRef<OsStr>,
    // {
    // Example:
    // #[cfg(not(any(target_os = "macos", windows)))]
    // let action = HintAction::Command(Program::Just(String::from("xdg-open")));
    // #[cfg(target_os = "macos")]
    // let action = HintAction::Command(Program::Just(String::from("open")));
    // #[cfg(windows)]
    // let action = HintAction::Command(Program::WithArgs {
    //     program: String::from("cmd"),
    //     args: vec!["/c".to_string(), "start".to_string(), "".to_string()],
    // });

    // Early implementation
    // let main_fd = *self.ctx().current().main_fd;
    // let shell_pid = &self.ctx().current().shell_pid;
    // match teletypewriter::spawn_daemon(program, args, main_fd, *shell_pid) {
    //     Ok(_) => log::debug!("Launched {} with args {:?}", program, args),
    //     Err(_) => log::warn!("Unable to launch {} with args {:?}", program, args),
    // }
    // std::process::exit(10);
    // echo $?
    // }

    #[inline]
    pub fn update_selection_scrolling(&mut self, mouse_y: f64) {
        let scale_factor = self.sugarloaf.layout.scale_factor;
        let min_height = (MIN_SELECTION_SCROLLING_HEIGHT * scale_factor) as i32;
        let step = (SELECTION_SCROLLING_STEP * scale_factor) as f64;

        // Compute the height of the scrolling areas.
        let end_top = max(min_height, constants::PADDING_Y as i32) as f64;
        let text_area_bottom = (constants::PADDING_Y
            + self.sugarloaf.layout.lines as f32)
            * self.sugarloaf.layout.font_size;
        let start_bottom = min(
            self.sugarloaf.layout.height as i32 - min_height,
            text_area_bottom as i32,
        ) as f64;

        // Get distance from closest window boundary.
        let delta = if mouse_y < end_top {
            end_top - mouse_y + step
        } else if mouse_y >= start_bottom {
            start_bottom - mouse_y - step
        } else {
            return;
        };

        let mut terminal = self.ctx().current().terminal.lock();
        terminal.scroll_display(Scroll::Delta((delta / step) as i32));
        drop(terminal);
    }

    #[inline]
    pub fn contains_point(&self, x: usize, y: usize) -> bool {
        let width = self.sugarloaf.layout.style.text_scale / 2.0;
        x <= (self.sugarloaf.layout.margin.x
            + self.sugarloaf.layout.columns as f32 * width) as usize
            && x > self.sugarloaf.layout.margin.x as usize
            && y <= (self.sugarloaf.layout.margin.top_y
                + self.sugarloaf.layout.lines as f32 * self.sugarloaf.layout.font_size)
                as usize
            && y > self.sugarloaf.layout.margin.top_y as usize
    }

    #[inline]
    pub fn side_by_pos(&self, x: usize) -> Side {
        let width = (self.sugarloaf.layout.style.text_scale / 2.0) as usize;

        let cell_x = x.saturating_sub(self.sugarloaf.layout.margin.x as usize) % width;
        let half_cell_width = width / 2;

        let additional_padding = (self.sugarloaf.layout.width
            - self.sugarloaf.layout.margin.x * 2.)
            % width as f32;
        let end_of_grid = self.sugarloaf.layout.width
            - self.sugarloaf.layout.margin.x
            - additional_padding;

        if cell_x > half_cell_width
            // Edge case when mouse leaves the window.
            || x as f32 >= end_of_grid
        {
            Side::Right
        } else {
            Side::Left
        }
    }

    #[inline]
    pub fn selection_is_empty(&self) -> bool {
        // let terminal = self.context_manager.current().terminal.lock();
        // let is_empty = terminal.selection.is_none();
        // drop(terminal);
        self.state.selection_range.is_none()
    }

    #[inline]
    pub fn on_left_click(&mut self, point: Pos) {
        let side = self.mouse.square_side;

        match self.mouse.click_state {
            ClickState::Click => {
                self.clear_selection();

                // Start new empty selection.
                if self.modifiers.state().control_key() {
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
        let mut terminal = self.ctx().current().terminal.lock();
        let mode = terminal.mode();
        if mode.contains(Mode::VI) {
            terminal.vi_mode_cursor.pos = point;
        }
        drop(terminal);
    }

    #[inline]
    pub fn paste(&mut self, text: &str, bracketed: bool) {
        if bracketed && self.get_mode().contains(Mode::BRACKETED_PASTE) {
            self.ctx_mut()
                .current_mut()
                .messenger
                .send_bytes(b"\x1b[200~"[..].to_vec());

            // Write filtered escape sequences.
            //
            // We remove `\x1b` to ensure it's impossible for the pasted text to write the bracketed
            // paste end escape `\x1b[201~` and `\x03` since some shells incorrectly terminate
            // bracketed paste on its receival.
            let filtered = text.replace(['\x1b', '\x03'], "");
            self.ctx_mut()
                .current_mut()
                .messenger
                .send_bytes(filtered.into_bytes());

            self.ctx_mut()
                .current_mut()
                .messenger
                .send_bytes(b"\x1b[201~"[..].to_vec());
        } else {
            self.ctx_mut()
                .current_mut()
                .messenger
                .send_bytes(text.replace("\r\n", "\r").replace('\n', "\r").into_bytes());
        }
    }

    #[inline]
    pub fn init(
        &mut self,
        color: ColorWGPU,
        use_image_as_background: bool,
        background_image_opt: &Option<sugarloaf::core::ImageProperties>,
    ) {
        let initial_columns = self.sugarloaf.layout.columns;

        self.sugarloaf.set_background_color(color);
        if use_image_as_background {
            if let Some(background_image) = background_image_opt {
                self.sugarloaf.set_background_image(background_image);
            }
        }

        self.sugarloaf.calculate_bounds();

        if self.sugarloaf.layout.columns != initial_columns {
            let width = self.sugarloaf.layout.width_u32 as u16;
            let height = self.sugarloaf.layout.height_u32 as u16;
            let columns = self.sugarloaf.layout.columns;
            let lines = self.sugarloaf.layout.lines;
            self.resize_all_contexts(width, height, columns, lines);
        }
    }

    #[inline]
    pub fn render_settings(&mut self, settings: &router::settings::Settings) {
        self.state.prepare_settings(&mut self.sugarloaf, settings);
        self.sugarloaf.render();
    }

    #[inline]
    pub fn render_assistant(&mut self, assistant: &router::assistant::Assistant) {
        self.state.prepare_assistant(&mut self.sugarloaf, assistant);
        self.sugarloaf.render();
    }

    #[inline]
    pub fn render_welcome(&mut self) {
        self.state.prepare_welcome(&mut self.sugarloaf);
        self.sugarloaf.render();
    }

    #[inline]
    pub fn render(&mut self) {
        let mut terminal = self.ctx().current().terminal.lock();
        let visible_rows = terminal.visible_rows();
        let cursor = terminal.cursor();
        let display_offset = terminal.display_offset();
        let terminal_has_blinking_enabled = terminal.blinking_cursor;
        drop(terminal);
        self.context_manager.update_titles();

        self.state.set_ime(self.ime.preedit());

        self.state.prepare_term(
            visible_rows,
            cursor,
            &mut self.sugarloaf,
            &self.context_manager,
            display_offset as i32,
            terminal_has_blinking_enabled,
        );

        self.sugarloaf.render();

        // In this case the configuration of blinking cursor is enabled
        // and the terminal also have instructions of blinking enabled
        if self.state.has_blinking_enabled && terminal_has_blinking_enabled {
            self.context_manager.schedule_cursor_blinking_render();
        }
    }

    fn sgr_mouse_report(&mut self, pos: Pos, button: u8, state: ElementState) {
        let c = match state {
            ElementState::Pressed => 'M',
            ElementState::Released => 'm',
        };

        let msg = format!("\x1b[<{};{};{}{}", button, pos.col + 1, pos.row + 1, c);
        self.ctx_mut()
            .current_mut()
            .messenger
            .send_bytes(msg.into_bytes());
    }

    #[inline]
    pub fn has_mouse_motion_and_drag(&mut self) -> bool {
        self.get_mode()
            .intersects(Mode::MOUSE_MOTION | Mode::MOUSE_DRAG)
    }

    #[inline]
    pub fn has_mouse_motion(&mut self) -> bool {
        self.get_mode().intersects(Mode::MOUSE_MOTION)
    }

    #[inline]
    pub fn mouse_report(&mut self, button: u8, state: ElementState) {
        let mut terminal = self.ctx().current().terminal.lock();
        let display_offset = terminal.display_offset();
        let mode = terminal.mode();
        drop(terminal);

        let pos = self.mouse_position(display_offset);

        // Assure the mouse pos is not in the scrollback.
        if pos.row < 0 {
            return;
        }

        // Calculate modifiers value.
        let mut mods = 0;
        let mod_state = self.modifiers.state();
        if mod_state.shift_key() {
            mods += 4;
        }
        if mod_state.alt_key() {
            mods += 8;
        }
        if mod_state.control_key() {
            mods += 16;
        }

        // Report mouse events.
        if mode.contains(Mode::SGR_MOUSE) {
            self.sgr_mouse_report(pos, button + mods, state);
        } else if let ElementState::Released = state {
            self.normal_mouse_report(pos, 3 + mods);
        } else {
            self.normal_mouse_report(pos, button + mods);
        }
    }

    #[inline]
    fn normal_mouse_report(&mut self, position: Pos, button: u8) {
        let Pos { row, col } = position;
        let utf8 = self.get_mode().contains(Mode::UTF8_MOUSE);

        let max_point = if utf8 { 2015 } else { 223 };

        if row >= max_point || col >= max_point {
            return;
        }

        let mut msg = vec![b'\x1b', b'[', b'M', 32 + button];

        let mouse_pos_encode = |pos: usize| -> Vec<u8> {
            let pos = 32 + 1 + pos;
            let first = 0xC0 + pos / 64;
            let second = 0x80 + (pos & 63);
            vec![first as u8, second as u8]
        };

        if utf8 && col >= Column(95) {
            msg.append(&mut mouse_pos_encode(col.0));
        } else {
            msg.push(32 + 1 + col.0 as u8);
        }

        if utf8 && row >= 95 {
            msg.append(&mut mouse_pos_encode(row.0 as usize));
        } else {
            msg.push(32 + 1 + row.0 as u8);
        }

        self.ctx_mut().current_mut().messenger.send_bytes(msg);
    }

    #[inline]
    pub fn scroll(&mut self, new_scroll_x_px: f64, new_scroll_y_px: f64) {
        let width = self.sugarloaf.layout.width as f64;
        let height = self.sugarloaf.layout.height as f64;
        let mode = self.get_mode();

        const MOUSE_WHEEL_UP: u8 = 64;
        const MOUSE_WHEEL_DOWN: u8 = 65;
        const MOUSE_WHEEL_LEFT: u8 = 66;
        const MOUSE_WHEEL_RIGHT: u8 = 67;

        if mode.intersects(Mode::MOUSE_MODE) && !mode.contains(Mode::VI) {
            self.mouse.accumulated_scroll.x += new_scroll_x_px;
            self.mouse.accumulated_scroll.y += new_scroll_y_px;

            let code = if new_scroll_y_px > 0. {
                MOUSE_WHEEL_UP
            } else {
                MOUSE_WHEEL_DOWN
            };
            let lines = (self.mouse.accumulated_scroll.y
                / (self.sugarloaf.layout.font_size * self.sugarloaf.layout.scale_factor)
                    as f64)
                .abs() as usize;

            for _ in 0..lines {
                self.mouse_report(code, ElementState::Pressed);
            }

            let code = if new_scroll_x_px > 0. {
                MOUSE_WHEEL_LEFT
            } else {
                MOUSE_WHEEL_RIGHT
            };
            let columns = (self.mouse.accumulated_scroll.x / width).abs() as usize;

            for _ in 0..columns {
                self.mouse_report(code, ElementState::Pressed);
            }
        } else if mode.contains(Mode::ALT_SCREEN | Mode::ALTERNATE_SCROLL)
            && !self.modifiers.state().shift_key()
        {
            self.mouse.accumulated_scroll.x += new_scroll_x_px;
            self.mouse.accumulated_scroll.y += new_scroll_y_px;

            // // The chars here are the same as for the respective arrow keys.
            let line_cmd = if new_scroll_y_px > 0. { b'A' } else { b'B' };
            let column_cmd = if new_scroll_x_px > 0. { b'D' } else { b'C' };

            let lines = (self.mouse.accumulated_scroll.y
                / (self.sugarloaf.layout.font_size * self.sugarloaf.layout.scale_factor)
                    as f64)
                .abs() as usize;
            let columns = (self.mouse.accumulated_scroll.x / width).abs() as usize;

            let mut content = Vec::with_capacity(3 * (lines + columns));

            for _ in 0..lines {
                content.push(0x1b);
                content.push(b'O');
                content.push(line_cmd);
            }

            for _ in 0..columns {
                content.push(0x1b);
                content.push(b'O');
                content.push(column_cmd);
            }

            if !content.is_empty() {
                self.ctx_mut().current_mut().messenger.send_bytes(content);
            }
        } else {
            self.mouse.accumulated_scroll.y += new_scroll_y_px * self.mouse.multiplier;
            let lines = (self.mouse.accumulated_scroll.y
                / self.sugarloaf.layout.font_size as f64) as i32;

            if lines != 0 {
                let mut terminal = self.ctx().current().terminal.lock();
                terminal.scroll_display(Scroll::Delta(lines));
                drop(terminal);
            }
        }

        self.mouse.accumulated_scroll.x %= width;
        self.mouse.accumulated_scroll.y %= height;
    }
}
