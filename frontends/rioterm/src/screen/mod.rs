// MIT License
// Copyright 2022-present Raphael Amorim
//
// The functions (including comments) and logic of process_key_event, build_key_sequence, process_mouse_bindings, copy_selection, start_selection, update_selection_scrolling,
// side_by_pos, on_left_click, paste, sgr_mouse_report, mouse_report, normal_mouse_report, scroll,
// were retired from https://github.com/alacritty/alacritty/blob/c39c3c97f1a1213418c3629cc59a1d46e34070e0/alacritty/src/input.rs
// which is licensed under Apache 2.0 license.

pub mod touch;

use crate::bindings::{
    Action as Act, BindingKey, BindingMode, FontSizeAction, MouseBinding, ViAction,
};
#[cfg(target_os = "macos")]
use crate::constants::{DEADZONE_END_Y, DEADZONE_START_X, DEADZONE_START_Y};
use crate::context::{self, process_open_url, ContextManager};
use crate::crosswords::{
    grid::{Dimensions, Scroll},
    pos::{Column, Pos, Side},
    square::Hyperlink,
    vi_mode::ViMotion,
    Mode,
};
use crate::ime::Ime;
use crate::mouse::{calculate_mouse_position, Mouse};
use crate::renderer::{padding_bottom_from_config, padding_top_from_config};
use crate::selection::{Selection, SelectionType};
use crate::state;
use core::fmt::Debug;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use rio_backend::clipboard::{Clipboard, ClipboardType};
use rio_backend::config::{
    colors::term::List,
    renderer::{Backend as RendererBackend, Performance as RendererPerformance},
};
use rio_backend::event::{ClickState, EventProxy};
use rio_backend::sugarloaf::{
    layout::SugarloafLayout, Sugarloaf, SugarloafErrors, SugarloafRenderer,
    SugarloafWindow, SugarloafWindowSize,
};
use state::State;
use std::borrow::Cow;
use std::cmp::{max, min};
use std::error::Error;
use std::ffi::OsStr;
use std::rc::Rc;
use touch::TouchPurpose;
use winit::event::ElementState;
use winit::event::Modifiers;
use winit::event::MouseButton;
#[cfg(target_os = "macos")]
use winit::keyboard::ModifiersKeyState;
use winit::keyboard::{Key, KeyLocation, ModifiersState, NamedKey};
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;

/// Minimum number of pixels at the bottom/top where selection scrolling is performed.
const MIN_SELECTION_SCROLLING_HEIGHT: f32 = 5.;

/// Number of pixels for increasing the selection scrolling speed factor by one.
const SELECTION_SCROLLING_STEP: f32 = 10.;

pub struct Screen {
    bindings: crate::bindings::KeyBindings,
    mouse_bindings: Vec<MouseBinding>,
    clipboard: Clipboard,
    pub modifiers: Modifiers,
    pub mouse: Mouse,
    pub touchpurpose: TouchPurpose,
    pub ime: Ime,
    pub state: State,
    pub sugarloaf: Sugarloaf,
    pub context_manager: context::ContextManager<EventProxy>,
}

impl Screen {
    pub async fn new(
        winit_window: &winit::window::Window,
        config: &Rc<rio_backend::config::Config>,
        event_proxy: EventProxy,
        font_library: &rio_backend::sugarloaf::font::FontLibrary,
        open_url: Option<&str>,
    ) -> Result<Screen, Box<dyn Error>> {
        let size = winit_window.inner_size();
        let scale = winit_window.scale_factor();
        let raw_window_handle = winit_window.window_handle().unwrap();
        let raw_display_handle = winit_window.display_handle().unwrap();
        let window_id = winit_window.id();

        let padding_y_bottom = padding_bottom_from_config(config);
        let padding_y_top = padding_top_from_config(config);

        let sugarloaf_layout = SugarloafLayout::new(
            size.width as f32,
            size.height as f32,
            (config.padding_x, padding_y_top, padding_y_bottom),
            scale as f32,
            config.fonts.size,
            config.line_height,
        );

        let mut sugarloaf_errors: Option<SugarloafErrors> = None;

        let sugarloaf_window = SugarloafWindow {
            handle: raw_window_handle.into(),
            display: raw_display_handle.into(),
            scale: scale as f32,
            size: SugarloafWindowSize {
                width: size.width as f32,
                height: size.height as f32,
            },
        };

        let power_preference = match config.renderer.performance {
            RendererPerformance::High => wgpu::PowerPreference::HighPerformance,
            RendererPerformance::Low => wgpu::PowerPreference::LowPower,
        };

        let backend = match config.renderer.backend {
            RendererBackend::Automatic => {
                #[cfg(target_arch = "wasm32")]
                let default_backend = wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL;
                #[cfg(not(target_arch = "wasm32"))]
                let default_backend = wgpu::Backends::all();

                default_backend
            }
            RendererBackend::Vulkan => wgpu::Backends::VULKAN,
            RendererBackend::GL => wgpu::Backends::GL,
            RendererBackend::Metal => wgpu::Backends::METAL,
            RendererBackend::DX12 => wgpu::Backends::DX12,
        };

        let sugarloaf_renderer = SugarloafRenderer {
            power_preference,
            backend,
        };

        let mut sugarloaf: Sugarloaf = match Sugarloaf::new(
            sugarloaf_window,
            sugarloaf_renderer,
            font_library,
            sugarloaf_layout,
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

        let clipboard = unsafe { Clipboard::new(raw_display_handle.into()) };

        let bindings = crate::bindings::default_key_bindings(
            config.bindings.keys.to_owned(),
            config.navigation.has_navigation_key_bindings(),
            config.keyboard,
        );
        let ime = Ime::new();

        let is_collapsed = config.navigation.is_collapsed_mode();
        let is_native = config.navigation.is_native();

        let (shell, working_dir) = process_open_url(
            config.shell.to_owned(),
            config.working_dir.to_owned(),
            config.editor.to_owned(),
            open_url,
        );

        let context_manager_config = context::ContextManagerConfig {
            use_current_path: config.navigation.use_current_path,
            shell,
            working_dir,
            spawn_performer: true,
            use_fork: config.use_fork,
            is_collapsed,
            is_native,
            // When navigation is collapsed and does not contain any color rule
            // does not make sense fetch for foreground process names
            should_update_titles: !(is_collapsed
                && config.navigation.color_automation.is_empty()),
        };
        let context_manager = context::ContextManager::start(
            (&state.get_cursor_state(), config.blinking_cursor),
            event_proxy,
            window_id,
            context_manager_config,
            sugarloaf.layout_next(),
            sugarloaf_errors,
        )?;

        sugarloaf.set_background_color(state.dynamic_background);
        if let Some(image) = &config.window.background_image {
            sugarloaf.set_background_image(image);
        }
        sugarloaf.render();

        Ok(Screen {
            mouse_bindings: crate::bindings::default_mouse_bindings(),
            modifiers: Modifiers::default(),
            context_manager,
            ime,
            sugarloaf,
            mouse: Mouse::new(config.scroll.multiplier, config.scroll.divider),
            touchpurpose: TouchPurpose::default(),
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
        self.mouse.accumulated_scroll = crate::mouse::AccumulatedScroll::default();
    }

    #[inline]
    pub fn mouse_position(&self, display_offset: usize) -> Pos {
        let layout = self.sugarloaf.layout();
        calculate_mouse_position(
            &self.mouse,
            display_offset,
            layout.dimensions.scale,
            (layout.columns, layout.lines),
            layout.margin.x,
            layout.margin.top_y,
            (
                layout.dimensions.width,
                layout.dimensions.height * layout.line_height,
            ),
        )
    }

    pub fn touch_purpose(&mut self) -> &mut TouchPurpose {
        &mut self.touchpurpose
    }

    #[inline]
    #[cfg(target_os = "macos")]
    pub fn is_macos_deadzone(&self, pos_y: f64) -> bool {
        let layout = self.sugarloaf.layout();
        let scale_f64 = layout.dimensions.scale as f64;
        pos_y <= DEADZONE_START_Y * scale_f64 && pos_y >= DEADZONE_END_Y * scale_f64
    }

    #[inline]
    #[cfg(target_os = "macos")]
    pub fn is_macos_deadzone_draggable(&self, pos_x: f64) -> bool {
        let layout = self.sugarloaf.layout();
        let scale_f64 = layout.dimensions.scale as f64;
        pos_x >= DEADZONE_START_X * scale_f64
    }

    /// update_config is triggered in any configuration file update
    #[inline]
    pub fn update_config(
        &mut self,
        config: &Rc<rio_backend::config::Config>,
        current_theme: Option<winit::window::Theme>,
        _font_library: &rio_backend::sugarloaf::font::FontLibrary,
    ) {
        // if let Some(err) = self
        //     .sugarloaf
        //     .update_font(config.fonts.to_owned(), Some(db))
        // {
        //     self.context_manager
        //         .report_error_fonts_not_found(err.fonts_not_found);
        //     return;
        // }

        let padding_y_bottom = padding_bottom_from_config(config);

        let padding_y_top = padding_top_from_config(config);

        self.sugarloaf.layout_next().recalculate(
            config.fonts.size,
            config.line_height,
            config.padding_x,
            padding_y_top,
            padding_y_bottom,
        );

        self.sugarloaf.layout_next().update();
        self.state = State::new(config, current_theme);

        for context in self.ctx().contexts() {
            let mut terminal = context.terminal.lock();
            let cursor = self.state.get_cursor_state_from_ref().content;
            terminal.cursor_shape = cursor;
            terminal.default_cursor_shape = cursor;
            terminal.blinking_cursor = config.blinking_cursor;
        }

        self.mouse
            .set_multiplier_and_divider(config.scroll.multiplier, config.scroll.divider);

        self.sugarloaf
            .set_background_color(self.state.dynamic_background);
        if let Some(image) = &config.window.background_image {
            self.sugarloaf.set_background_image(image);
        }

        self.resize_all_contexts();
    }

    #[inline]
    pub fn change_font_size(&mut self, action: FontSizeAction) {
        let action: u8 = match action {
            FontSizeAction::Increase => 2,
            FontSizeAction::Decrease => 1,
            FontSizeAction::Reset => 0,
        };

        self.sugarloaf.update_font_size(action);
        self.render();

        if self.sugarloaf.dimensions_changed() {
            self.resize_all_contexts();
        };

        self.render();
        self.resize_all_contexts();
    }

    #[inline]
    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) -> &mut Self {
        self.sugarloaf.resize(new_size.width, new_size.height);
        self.resize_all_contexts();
        self
    }

    #[inline]
    pub fn set_scale(
        &mut self,
        new_scale: f32,
        new_size: winit::dpi::PhysicalSize<u32>,
    ) -> &mut Self {
        self.sugarloaf.rescale(new_scale);
        self.sugarloaf.resize(new_size.width, new_size.height);
        self.render();

        if self.sugarloaf.dimensions_changed() {
            self.resize_all_contexts();
        }

        self
    }

    #[inline]
    pub fn resize_all_contexts(&mut self) {
        // whenever a resize update happens: it will stored in
        // the next layout, so once the messenger.send_resize triggers
        // the wakeup from pty it will also trigger a sugarloaf.render()
        // and then eventually a render with the new layout computation.
        let layout = self.sugarloaf.layout_next();
        for context in self.ctx().contexts() {
            let mut terminal = context.terminal.lock();
            terminal.resize::<SugarloafLayout>(layout);
            drop(terminal);
            let _ = context.messenger.send_resize(
                layout.width as u16,
                layout.height as u16,
                layout.columns as u16,
                layout.lines as u16,
            );
        }
    }

    #[inline]
    pub fn clipboard_get(&mut self, clipboard_type: ClipboardType) -> String {
        self.clipboard.get(clipboard_type)
    }

    #[inline]
    pub fn clipboard_store(&mut self, clipboard_type: ClipboardType, content: String) {
        self.clipboard.set(clipboard_type, content);
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
        // 1. In case there is a key released event and Rio is not using kitty keyboard protocol
        // then should return drop the key processing
        // 2. In case IME has preedit then also should drop the key processing
        if !self.state.is_kitty_keyboard_enabled && key.state == ElementState::Released
            || self.ime.preedit().is_some()
        {
            return;
        }

        let mode = self.get_mode();
        let mods = self.modifiers.state();

        if self.state.is_kitty_keyboard_enabled && key.state == ElementState::Released {
            if mode.contains(Mode::KEYBOARD_REPORT_EVENT_TYPES)
                && !mode.contains(Mode::VI)
            {
                // NOTE: echoing the key back on release is how it's done in kitty/foot and
                // it's how it should be done according to the kitty author
                // https://github.com/kovidgoyal/kitty/issues/6516#issuecomment-1659454350
                let bytes: Cow<'static, [u8]> = match key.logical_key.as_ref() {
                    Key::Named(NamedKey::Tab) => [b'\t'].as_slice().into(),
                    Key::Named(NamedKey::Enter) => [b'\r'].as_slice().into(),
                    Key::Named(NamedKey::Delete) => [b'\x7f'].as_slice().into(),
                    Key::Named(NamedKey::Escape) => [b'\x1b'].as_slice().into(),
                    _ => crate::bindings::kitty_keyboard_protocol::build_key_sequence(
                        key, mods, mode,
                    )
                    .into(),
                };

                self.sugarloaf.mark_dirty();
                self.ctx_mut().current_mut().messenger.send_write(bytes);
            }

            return;
        }

        let binding_mode = BindingMode::new(&mode);
        let mut ignore_chars = None;

        for i in 0..self.bindings.len() {
            let binding = &self.bindings[i];

            // We don't want the key without modifier, because it means something else most of
            // the time. However what we want is to manually lowercase the character to account
            // for both small and capital letters on regular characters at the same time.
            let logical_key = if let Key::Character(ch) = key.logical_key.as_ref() {
                // Match `Alt` bindings without `Alt` being applied, otherwise they use the
                // composed chars, which are not intuitive to bind.
                //
                // On Windows, the `Ctrl + Alt` mangles `logical_key` to unidentified values, thus
                // preventing them from being used in bindings
                //
                // For more see https://github.com/rust-windowing/winit/issues/2945.
                if (cfg!(windows) && mods.control_key()) && mods.alt_key() {
                    key.key_without_modifiers()
                } else {
                    Key::Character(ch.to_lowercase().into())
                }
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
                    Act::Run(program) => self.exec(program.program(), program.args()),
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
                    Act::ClearSelection => {
                        self.clear_selection();
                    }
                    Act::PasteSelection => {
                        let content = self.clipboard.get(ClipboardType::Selection);
                        self.paste(&content, true);
                    }
                    Act::Copy => {
                        self.copy_selection(ClipboardType::Clipboard);
                    }
                    Act::ToggleViMode => {
                        let mut terminal =
                            self.context_manager.current_mut().terminal.lock();
                        terminal.toggle_vi_mode();
                        let has_vi_mode_enabled = terminal.mode().contains(Mode::VI);
                        drop(terminal);
                        self.state.set_vi_mode(has_vi_mode_enabled);
                    }
                    Act::ViMotion(motion) => {
                        let mut terminal =
                            self.context_manager.current_mut().terminal.lock();
                        if terminal.mode().contains(Mode::VI) {
                            terminal.vi_motion(*motion);
                        }

                        if let Some(selection) = &terminal.selection {
                            self.state.set_selection(selection.to_range(&terminal));
                        };
                        drop(terminal);
                        self.render();
                    }
                    Act::Vi(ViAction::CenterAroundViCursor) => {
                        let mut terminal =
                            self.context_manager.current_mut().terminal.lock();
                        let display_offset = terminal.display_offset() as i32;
                        let target =
                            -display_offset + terminal.grid.screen_lines() as i32 / 2 - 1;
                        let line = terminal.vi_mode_cursor.pos.row;
                        let scroll_lines = target - line.0;

                        terminal.scroll_display(Scroll::Delta(scroll_lines));
                        drop(terminal);
                    }
                    Act::Vi(ViAction::ToggleNormalSelection) => {
                        self.toggle_selection(SelectionType::Simple, Side::Left);
                    }
                    Act::Vi(ViAction::ToggleLineSelection) => {
                        self.toggle_selection(SelectionType::Lines, Side::Left);
                    }
                    Act::Vi(ViAction::ToggleBlockSelection) => {
                        self.toggle_selection(SelectionType::Block, Side::Left);
                    }
                    Act::Vi(ViAction::ToggleSemanticSelection) => {
                        self.toggle_selection(SelectionType::Semantic, Side::Left);
                    }
                    Act::ConfigEditor => {
                        self.context_manager.switch_to_settings();
                    }
                    Act::WindowCreateNew => {
                        self.context_manager.create_new_window();
                    }
                    Act::TabCreateNew => {
                        let redirect = true;

                        let layout = self.sugarloaf.layout();
                        self.context_manager.add_context(
                            redirect,
                            layout,
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
                            self.context_manager.close_current_window(false);
                        } else {
                            // Kill current context will trigger terminal.exit
                            // then RioEvent::Exit and eventually try_close_existent_tab
                            self.context_manager.kill_current_context();
                        }
                    }
                    Act::Quit => {
                        self.context_manager.quit();
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
                    Act::Scroll(delta) => {
                        let mut terminal =
                            self.context_manager.current_mut().terminal.lock();
                        terminal.scroll_display(Scroll::Delta(*delta));
                        drop(terminal);
                    }
                    Act::ClearHistory => {
                        let mut terminal =
                            self.context_manager.current_mut().terminal.lock();
                        terminal.clear_saved_history();
                        drop(terminal);
                        self.render();
                    }
                    Act::ToggleFullscreen => self.context_manager.toggle_full_screen(),
                    Act::Minimize => {
                        self.context_manager.minimize();
                    }
                    Act::Hide => {
                        self.context_manager.hide();
                    }
                    #[cfg(target_os = "macos")]
                    Act::HideOtherApplications => {
                        self.context_manager.hide_other_apps();
                    }
                    Act::SelectTab(tab_index) => {
                        self.context_manager.select_tab(*tab_index);
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

        // VI mode doesn't have inputs
        if ignore_chars.unwrap_or(false) || mode.contains(Mode::VI) {
            return;
        }

        self.sugarloaf.mark_dirty();
        let text = key.text_with_all_modifiers().unwrap_or_default();

        let bytes = if !self.state.is_kitty_keyboard_enabled {
            // If text is empty then leave without input bytes
            if text.is_empty() {
                return;
            }

            let mut bytes = Vec::with_capacity(text.len() + 1);
            if self.alt_send_esc() && text.len() == 1 {
                bytes.push(b'\x1b');
            }
            bytes.extend_from_slice(text.as_bytes());
            bytes
        } else {
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
                        && key.logical_key != Key::Named(NamedKey::Escape)));

            // Handle legacy char writing.
            if write_legacy {
                let mut bytes = Vec::with_capacity(text.len() + 1);
                if self.alt_send_esc() && text.len() == 1 {
                    bytes.push(b'\x1b');
                }

                bytes.extend_from_slice(text.as_bytes());
                bytes
            } else {
                // Otherwise we should build the key sequence for the given input.
                crate::bindings::kitty_keyboard_protocol::build_key_sequence(
                    key, mods, mode,
                )
            }
        };

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

    #[inline]
    fn start_selection(&mut self, ty: SelectionType, point: Pos, side: Side) {
        self.copy_selection(ClipboardType::Selection);
        let mut terminal = self.context_manager.current().terminal.lock();
        let selection = Selection::new(ty, point, side);
        self.state.set_selection(selection.to_range(&terminal));
        terminal.selection = Some(selection);
        drop(terminal);
    }

    #[inline]
    fn toggle_selection(&mut self, ty: SelectionType, side: Side) {
        let mut terminal = self.context_manager.current().terminal.lock();
        match &mut terminal.selection {
            Some(selection) if selection.ty == ty && !selection.is_empty() => {
                drop(terminal);
                self.clear_selection();
            }
            Some(selection) if !selection.is_empty() => {
                selection.ty = ty;
                drop(terminal);
                self.copy_selection(ClipboardType::Selection);
            }
            _ => {
                let pos = terminal.vi_mode_cursor.pos;
                drop(terminal);
                self.start_selection(ty, pos, side)
            }
        }

        let mut terminal = self.context_manager.current().terminal.lock();
        let mut selection = match terminal.selection.take() {
            Some(selection) => {
                // Make sure initial selection is not empty.
                selection
            }
            None => return,
        };

        selection.include_all();
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

    #[inline]
    pub fn search_nearest_hyperlink_from_pos(&mut self) -> bool {
        #[cfg(target_os = "macos")]
        let is_hyperlink_key_active = self.modifiers.state().super_key();

        #[cfg(not(target_os = "macos"))]
        let is_hyperlink_key_active = self.modifiers.state().alt_key();

        if !is_hyperlink_key_active {
            return false;
        }

        let mut terminal = self.context_manager.current().terminal.lock();
        let display_offset = terminal.display_offset();
        let pos = self.mouse_position(display_offset);
        let search_result = terminal.search_nearest_hyperlink_from_pos(pos);
        drop(terminal);

        if let Some(hyperlink_range) = search_result {
            self.state.set_hyperlink_range(Some(hyperlink_range));
            return true;
        }

        self.state.set_hyperlink_range(None);
        false
    }

    #[inline]
    pub fn trigger_hyperlink(&self) -> bool {
        #[cfg(target_os = "macos")]
        let is_hyperlink_key_active = self.modifiers.state().super_key();

        #[cfg(not(target_os = "macos"))]
        let is_hyperlink_key_active = self.modifiers.state().alt_key();

        if !is_hyperlink_key_active || !self.state.has_hyperlink_range() {
            return false;
        }

        let mut terminal = self.context_manager.current().terminal.lock();
        let display_offset = terminal.display_offset();
        let pos = self.mouse_position(display_offset);
        let pos_hyperlink = terminal.grid[pos].hyperlink();
        drop(terminal);

        if let Some(hyperlink) = pos_hyperlink {
            self.open_hyperlink(hyperlink);

            return true;
        }

        false
    }

    fn open_hyperlink(&self, hyperlink: Hyperlink) {
        #[cfg(not(any(target_os = "macos", windows)))]
        self.exec("xdg-open", [hyperlink.uri()]);

        #[cfg(target_os = "macos")]
        self.exec("open", [hyperlink.uri()]);

        #[cfg(windows)]
        self.exec("cmd", ["/c", "start", "", hyperlink.uri()]);
    }

    pub fn exec<I, S>(&self, program: &str, args: I)
    where
        I: IntoIterator<Item = S> + Debug + Copy,
        S: AsRef<OsStr>,
    {
        #[cfg(unix)]
        {
            let main_fd = *self.ctx().current().main_fd;
            let shell_pid = &self.ctx().current().shell_pid;
            match teletypewriter::spawn_daemon(program, args, main_fd, *shell_pid) {
                Ok(_) => log::debug!("Launched {} with args {:?}", program, args),
                Err(_) => log::warn!("Unable to launch {} with args {:?}", program, args),
            }
        }

        #[cfg(windows)]
        {
            match teletypewriter::spawn_daemon(program, args) {
                Ok(_) => log::debug!("Launched {} with args {:?}", program, args),
                Err(_) => log::warn!("Unable to launch {} with args {:?}", program, args),
            }
        }
    }

    #[inline]
    pub fn update_selection_scrolling(&mut self, mouse_y: f64) {
        let layout = self.sugarloaf.layout();
        let scale_factor = layout.dimensions.scale;
        let min_height = (MIN_SELECTION_SCROLLING_HEIGHT * scale_factor) as i32;
        let step = (SELECTION_SCROLLING_STEP * scale_factor) as f64;

        // Compute the height of the scrolling areas.
        let end_top = max(min_height, crate::constants::PADDING_Y as i32) as f64;
        let text_area_bottom =
            (crate::constants::PADDING_Y + layout.lines as f32) * layout.font_size;
        let start_bottom =
            min(layout.height as i32 - min_height, text_area_bottom as i32) as f64;

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
        let layout = self.sugarloaf.layout();
        let width = layout.dimensions.width;
        x <= (layout.margin.x + layout.columns as f32 * width) as usize
            && x > (layout.margin.x * layout.dimensions.scale) as usize
            && y <= (layout.margin.top_y * layout.dimensions.scale
                + layout.lines as f32 * layout.dimensions.height)
                as usize
            && y > layout.margin.top_y as usize
    }

    #[inline]
    pub fn side_by_pos(&self, x: usize) -> Side {
        let layout = self.sugarloaf.layout();
        let width = (layout.dimensions.width) as usize;
        let margin_x = layout.margin.x * layout.dimensions.scale;

        let cell_x = x.saturating_sub(margin_x as usize) % width;
        let half_cell_width = width / 2;

        let additional_padding = (layout.width - margin_x) % width as f32;
        let end_of_grid = layout.width - margin_x - additional_padding;

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
        if terminal.mode().contains(Mode::VI) {
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
    pub fn render_assistant(&mut self, assistant: &crate::routes::assistant::Assistant) {
        crate::routes::assistant::screen(&mut self.sugarloaf, assistant);
        self.sugarloaf.render();
    }

    #[inline]
    pub fn render_welcome(&mut self) {
        crate::routes::welcome::screen(&mut self.sugarloaf);
        self.sugarloaf.render();
    }

    #[inline]
    pub fn render_dialog(&mut self, content: &str) {
        crate::routes::dialog::screen(&mut self.sugarloaf, content);
        self.sugarloaf.render();
    }

    #[inline]
    pub fn render(&mut self) {
        // If sugarloaf does have pending updates to process then
        // should abort current render
        #[cfg(use_wa)]
        if self.sugarloaf.dimensions_changed() {
            self.resize_all_contexts();
            return;
        };

        let mut terminal = self.ctx().current().terminal.lock();
        let visible_rows = terminal.visible_rows();
        let cursor = terminal.cursor();
        let display_offset = terminal.display_offset();
        let has_blinking_enabled = terminal.blinking_cursor;
        drop(terminal);
        self.context_manager.update_titles();

        self.state.set_ime(self.ime.preedit());

        self.state.prepare_term(
            visible_rows,
            cursor,
            &mut self.sugarloaf,
            &self.context_manager,
            display_offset as i32,
            has_blinking_enabled,
        );

        self.sugarloaf.render();

        // In this case the configuration of blinking cursor is enabled
        // and the terminal also have instructions of blinking enabled
        // TODO: enable blinking for selection after adding debounce (https://github.com/raphamorim/rio/issues/437)
        if self.state.has_blinking_enabled
            && has_blinking_enabled
            && self.selection_is_empty()
        {
            self.context_manager.schedule_render(800);
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
    pub fn on_focus_change(&mut self, is_focused: bool) {
        if self.get_mode().contains(Mode::FOCUS_IN_OUT) {
            let chr = if is_focused { "I" } else { "O" };

            let msg = format!("\x1b[{}", chr);
            self.ctx_mut()
                .current_mut()
                .messenger
                .send_bytes(msg.into_bytes());
        }
    }

    #[inline]
    pub fn scroll(&mut self, new_scroll_x_px: f64, new_scroll_y_px: f64) {
        let layout = self.sugarloaf.layout();
        let width = layout.dimensions.width as f64;
        let height = layout.dimensions.height as f64;
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
            let lines = (self.mouse.accumulated_scroll.y / height).abs() as usize;

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
            self.mouse.accumulated_scroll.x +=
                (new_scroll_x_px * self.mouse.multiplier) / self.mouse.divider;
            self.mouse.accumulated_scroll.y +=
                (new_scroll_y_px * self.mouse.multiplier) / self.mouse.divider;

            // The chars here are the same as for the respective arrow keys.
            let line_cmd = if new_scroll_y_px > 0. { b'A' } else { b'B' };
            let column_cmd = if new_scroll_x_px > 0. { b'D' } else { b'C' };

            let lines = (self.mouse.accumulated_scroll.y
                / (layout.dimensions.height) as f64)
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
            self.mouse.accumulated_scroll.y +=
                (new_scroll_y_px * self.mouse.multiplier) / self.mouse.divider;
            let lines = (self.mouse.accumulated_scroll.y
                / layout.dimensions.height as f64) as i32;

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
