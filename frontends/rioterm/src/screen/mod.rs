// MIT License
// Copyright 2022-present Raphael Amorim
//
// The functions (including comments) and logic of process_key_event, build_key_sequence, process_mouse_bindings, copy_selection, start_selection, update_selection_scrolling,
// side_by_pos, on_left_click, paste, sgr_mouse_report, mouse_report, normal_mouse_report, scroll,
// were retired from https://github.com/alacritty/alacritty/blob/c39c3c97f1a1213418c3629cc59a1d46e34070e0/alacritty/src/input.rs
// which is licensed under Apache 2.0 license.

pub mod hint;
pub mod touch;

use crate::bindings::kitty_keyboard::build_key_sequence;
use crate::bindings::{
    Action as Act, BindingKey, BindingMode, FontSizeAction, MouseBinding, SearchAction,
    ViAction,
};
#[cfg(target_os = "macos")]
use crate::constants::{DEADZONE_END_Y, DEADZONE_START_Y};
use crate::context::grid::{ContextDimension, Delta};
use crate::context::renderable::{Cursor, RenderableContent};
use crate::context::{self, process_open_url, ContextManager};
use crate::crosswords::{
    grid::{Dimensions, Scroll},
    pos::{Column, Pos, Side},
    square::Hyperlink,
    vi_mode::ViMotion,
    Mode,
};
use crate::hints::HintState;
use crate::mouse::{calculate_mouse_position, Mouse};
use crate::renderer::{
    utils::{padding_bottom_from_config, padding_top_from_config},
    Renderer,
};
use crate::screen::hint::HintMatches;
use crate::selection::{Selection, SelectionType};
use core::fmt::Debug;
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
use rio_backend::clipboard::Clipboard;
use rio_backend::clipboard::ClipboardType;
use rio_backend::config::renderer::{
    Backend as RendererBackend, Performance as RendererPerformance,
};
use rio_backend::crosswords::pos::{Boundary, CursorState, Direction, Line};
use rio_backend::crosswords::search::RegexSearch;
use rio_backend::event::{ClickState, EventProxy, SearchState};
use rio_backend::sugarloaf::{
    layout::RootStyle, Sugarloaf, SugarloafErrors, SugarloafRenderer, SugarloafWindow,
    SugarloafWindowSize,
};
use rio_window::event::ElementState;
use rio_window::event::Modifiers;
use rio_window::event::MouseButton;
#[cfg(target_os = "macos")]
use rio_window::keyboard::ModifiersKeyState;
use rio_window::keyboard::{Key, KeyLocation, ModifiersState, NamedKey};
use rio_window::platform::modifier_supplement::KeyEventExtModifierSupplement;
use std::cell::RefCell;
use std::cmp::{max, min};
use std::error::Error;
use std::ffi::OsStr;
use std::rc::Rc;
use touch::TouchPurpose;

/// Minimum number of pixels at the bottom/top where selection scrolling is performed.
const MIN_SELECTION_SCROLLING_HEIGHT: f32 = 5.;

/// Number of pixels for increasing the selection scrolling speed factor by one.
const SELECTION_SCROLLING_STEP: f32 = 10.;

/// Maximum number of lines for the blocking search while still typing the search regex.
const MAX_SEARCH_WHILE_TYPING: Option<usize> = Some(1000);

/// Maximum number of search terms stored in the history.
const MAX_SEARCH_HISTORY_SIZE: usize = 255;

pub struct Screen<'screen> {
    bindings: crate::bindings::KeyBindings,
    mouse_bindings: Vec<MouseBinding>,
    pub modifiers: Modifiers,
    pub mouse: Mouse,
    pub touchpurpose: TouchPurpose,
    pub search_state: SearchState,
    pub hint_state: HintState,
    pub renderer: Renderer,
    pub sugarloaf: Sugarloaf<'screen>,
    pub context_manager: context::ContextManager<EventProxy>,
    pub clipboard: Rc<RefCell<Clipboard>>,
    last_ime_cursor_pos: Option<(f32, f32)>,
    hints_config: Vec<std::rc::Rc<rio_backend::config::hints::Hint>>,
}

pub struct ScreenWindowProperties {
    pub size: rio_window::dpi::PhysicalSize<u32>,
    pub scale: f64,
    pub raw_window_handle: RawWindowHandle,
    pub raw_display_handle: RawDisplayHandle,
    pub window_id: rio_window::window::WindowId,
}

impl Screen<'_> {
    pub fn new<'screen>(
        window_properties: ScreenWindowProperties,
        config: &rio_backend::config::Config,
        event_proxy: EventProxy,
        font_library: &rio_backend::sugarloaf::font::FontLibrary,
        open_url: Option<String>,
        clipboard: Rc<RefCell<Clipboard>>,
    ) -> Result<Screen<'screen>, Box<dyn Error>> {
        let size = window_properties.size;
        let scale = window_properties.scale;
        let raw_window_handle = window_properties.raw_window_handle;
        let raw_display_handle = window_properties.raw_display_handle;
        let window_id = window_properties.window_id;

        let padding_y_top = padding_top_from_config(
            &config.navigation,
            config.padding_y[0],
            1,
            config.window.macos_use_unified_titlebar,
        );

        let padding_y_bottom =
            padding_bottom_from_config(&config.navigation, config.padding_y[1], 1, false);
        let sugarloaf_layout =
            RootStyle::new(scale as f32, config.fonts.size, config.line_height);

        let mut sugarloaf_errors: Option<SugarloafErrors> = None;

        let sugarloaf_window = SugarloafWindow {
            handle: raw_window_handle,
            display: raw_display_handle,
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
            font_features: config.fonts.features.clone(),
            colorspace: config.window.colorspace.to_sugarloaf_colorspace(),
        };

        let mut sugarloaf: Sugarloaf = match Sugarloaf::new(
            sugarloaf_window,
            sugarloaf_renderer,
            font_library,
            sugarloaf_layout,
        ) {
            Ok(instance) => instance,
            Err(instance_with_errors) => {
                sugarloaf_errors = Some(instance_with_errors.errors);
                instance_with_errors.instance
            }
        };

        sugarloaf.update_filters(config.renderer.filters.as_slice());

        let renderer = Renderer::new(config, font_library);

        let bindings = crate::bindings::default_key_bindings(config);

        let is_native = config.navigation.is_native();

        let (shell, working_dir) = process_open_url(
            config.shell.to_owned(),
            config.working_dir.to_owned(),
            config.editor.to_owned(),
            open_url.as_deref(),
        );

        let context_manager_config = context::ContextManagerConfig {
            cwd: config.navigation.current_working_directory,
            shell,
            working_dir,
            spawn_performer: true,
            #[cfg(not(target_os = "windows"))]
            use_fork: config.use_fork,
            is_native,
            // When navigation does not contain any color rule
            // does not make sense fetch for foreground process names/path
            should_update_title_extra: !config.navigation.color_automation.is_empty(),
            split_color: config.colors.split,
            title: config.title.clone(),
            keyboard: config.keyboard,
        };

        let rich_text_id = sugarloaf.create_rich_text();

        let margin = Delta {
            x: config.padding_x,
            top_y: padding_y_top,
            bottom_y: padding_y_bottom,
        };
        let context_dimension = ContextDimension::build(
            size.width as f32,
            size.height as f32,
            sugarloaf.get_rich_text_dimensions(&rich_text_id),
            config.line_height,
            margin,
        );

        let cursor = Cursor {
            content: config.cursor.shape.into(),
            content_ref: config.cursor.shape.into(),
            state: CursorState::new(config.cursor.shape.into()),
            is_ime_enabled: false,
        };

        let context_manager = context::ContextManager::start(
            // config.cursor.blinking
            (&cursor, config.cursor.blinking),
            event_proxy,
            window_id,
            0,
            rich_text_id,
            context_manager_config,
            context_dimension,
            margin,
            sugarloaf_errors,
        )?;

        if cfg!(target_os = "macos") {
            sugarloaf.set_background_color(None);
        } else {
            sugarloaf.set_background_color(Some(renderer.dynamic_background.1));
        }

        if let Some(image) = &config.window.background_image {
            sugarloaf.set_background_image(image);
        }
        sugarloaf.render();

        Ok(Screen {
            search_state: SearchState::default(),
            hint_state: HintState::new(config.hints.alphabet.clone()),
            hints_config: config
                .hints
                .rules
                .iter()
                .map(|h| std::rc::Rc::new(h.clone()))
                .collect(),
            mouse_bindings: crate::bindings::default_mouse_bindings(),
            modifiers: Modifiers::default(),
            context_manager,
            sugarloaf,
            mouse: Mouse::new(config.scroll.multiplier, config.scroll.divider),
            touchpurpose: TouchPurpose::default(),
            renderer,
            bindings,
            clipboard,
            last_ime_cursor_pos: None,
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
    pub fn search_active(&self) -> bool {
        self.search_state.history_index.is_some()
    }

    #[inline]
    pub fn reset_mouse(&mut self) {
        self.mouse.accumulated_scroll = crate::mouse::AccumulatedScroll::default();
    }

    #[inline]
    pub fn select_current_based_on_mouse(&mut self) {
        if self
            .context_manager
            .current_grid_mut()
            .select_current_based_on_mouse(&self.mouse)
        {
            self.context_manager.select_route_from_current_grid();
        }
    }

    #[inline]
    pub fn mouse_position(&self, display_offset: usize) -> Pos {
        let current_grid = self.context_manager.current_grid();
        let (context, margin) = current_grid.current_context_with_computed_dimension();
        let context_dimension = context.dimension;
        let style = self.sugarloaf.style();
        calculate_mouse_position(
            &self.mouse,
            display_offset,
            style.scale_factor,
            (context_dimension.columns, context_dimension.lines),
            margin.x,
            margin.top_y,
            (
                context_dimension.dimension.width,
                context_dimension.dimension.height * style.line_height,
            ),
        )
    }

    #[inline]
    pub fn touch_purpose(&mut self) -> &mut TouchPurpose {
        &mut self.touchpurpose
    }

    #[inline]
    #[cfg(target_os = "macos")]
    pub fn is_macos_deadzone(&self, pos_y: f64) -> bool {
        let layout = self
            .sugarloaf
            .rich_text_layout(&self.context_manager.current().rich_text_id);
        let scale_f64 = layout.dimensions.scale as f64;
        pos_y <= DEADZONE_START_Y * scale_f64 && pos_y >= DEADZONE_END_Y * scale_f64
    }

    /// update_config is triggered in any configuration file update
    #[inline]
    pub fn update_config(
        &mut self,
        config: &rio_backend::config::Config,
        font_library: &rio_backend::sugarloaf::font::FontLibrary,
        should_update_font_library: bool,
    ) {
        let num_tabs = self.ctx().len();
        let padding_y_top = padding_top_from_config(
            &config.navigation,
            config.padding_y[0],
            num_tabs,
            config.window.macos_use_unified_titlebar,
        );
        let padding_y_bottom = padding_bottom_from_config(
            &config.navigation,
            config.padding_y[1],
            num_tabs,
            self.search_active(),
        );

        if should_update_font_library {
            self.sugarloaf.update_font(font_library);
        }
        let s = self.sugarloaf.style_mut();
        s.font_size = config.fonts.size;
        s.line_height = config.line_height;

        self.sugarloaf
            .update_filters(config.renderer.filters.as_slice());
        self.renderer = Renderer::new(config, font_library);

        for context_grid in self.context_manager.contexts_mut() {
            context_grid.update_line_height(config.line_height);

            context_grid.update_margin((
                config.padding_x,
                padding_y_top,
                padding_y_bottom,
            ));

            context_grid.update_dimensions(&self.sugarloaf);

            for current_context in context_grid.contexts_mut().values_mut() {
                let current_context = current_context.context_mut();
                self.sugarloaf.set_rich_text_line_height(
                    &current_context.rich_text_id,
                    current_context.dimension.line_height,
                );

                let mut terminal = current_context.terminal.lock();
                current_context.renderable_content =
                    RenderableContent::from_cursor_config(&config.cursor);
                let shape = config.cursor.shape;
                terminal.cursor_shape = shape;
                terminal.default_cursor_shape = shape;
                terminal.blinking_cursor = config.cursor.blinking;
                drop(terminal);
            }
        }

        self.mouse
            .set_multiplier_and_divider(config.scroll.multiplier, config.scroll.divider);

        // Update keyboard config in context manager
        self.context_manager.config.keyboard = config.keyboard;

        if cfg!(target_os = "macos") {
            self.sugarloaf.set_background_color(None);
        } else {
            self.sugarloaf
                .set_background_color(Some(self.renderer.dynamic_background.1));
        }

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

        self.sugarloaf.set_rich_text_font_size_based_on_action(
            &self.context_manager.current().rich_text_id,
            action,
        );

        self.context_manager
            .current_grid_mut()
            .update_dimensions(&self.sugarloaf);

        self.render();
        self.resize_all_contexts();
    }

    #[inline]
    pub fn resize(&mut self, new_size: rio_window::dpi::PhysicalSize<u32>) -> &mut Self {
        if self
            .context_manager
            .current()
            .renderable_content
            .selection_range
            .is_some()
        {
            self.clear_selection();
        }
        self.sugarloaf.resize(new_size.width, new_size.height);
        let width = new_size.width as f32;
        let height = new_size.height as f32;

        for context_grid in self.context_manager.contexts_mut() {
            context_grid.resize(width, height);
        }

        self
    }

    #[inline]
    pub fn set_scale(
        &mut self,
        new_scale: f32,
        new_size: rio_window::dpi::PhysicalSize<u32>,
    ) -> &mut Self {
        self.sugarloaf.rescale(new_scale);
        self.sugarloaf.resize(new_size.width, new_size.height);
        self.render();
        self.resize_all_contexts();
        self.context_manager
            .current_grid_mut()
            .update_dimensions(&self.sugarloaf);
        let width = new_size.width as f32;
        let height = new_size.height as f32;

        for context_grid in self.context_manager.contexts_mut() {
            context_grid.resize(width, height);
        }

        self
    }

    #[inline]
    pub fn resize_all_contexts(&mut self) {
        // whenever a resize update happens: it will stored in
        // the next layout, so once the messenger.send_resize triggers
        // the wakeup from pty it will also trigger a sugarloaf.render()
        // and then eventually a render with the new layout computation.
        for context_grid in self.context_manager.contexts_mut() {
            for context in context_grid.contexts_mut().values_mut() {
                let ctx = context.context_mut();
                let mut terminal = ctx.terminal.lock();
                terminal.resize::<ContextDimension>(ctx.dimension);
                drop(terminal);
                let winsize = crate::renderer::utils::terminal_dimensions(&ctx.dimension);
                let _ = ctx.messenger.send_resize(winsize);
            }
        }
    }

    #[inline]
    pub fn scroll_bottom_when_cursor_not_visible(&mut self) {
        let mut terminal = self.ctx_mut().current_mut().terminal.lock();
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
        let terminal = self.ctx().current().terminal.lock();
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
    pub fn process_key_event(&mut self, key: &rio_window::event::KeyEvent) {
        if self.context_manager.current().ime.preedit().is_some() {
            return;
        }

        let mode = self.get_mode();
        let mods = self.modifiers.state();

        if key.state == ElementState::Released {
            if !mode.contains(Mode::REPORT_EVENT_TYPES)
                || mode.contains(Mode::VI)
                || self.search_active()
                || self.hint_state.is_active()
            {
                return;
            }

            // Mask `Alt` modifier from input when we won't send esc.
            let text = key.text_with_all_modifiers().unwrap_or_default();
            let mods = if self.alt_send_esc(key, text) {
                mods
            } else {
                mods & !ModifiersState::ALT
            };

            let bytes = match key.logical_key.as_ref() {
                Key::Named(NamedKey::Enter)
                | Key::Named(NamedKey::Tab)
                | Key::Named(NamedKey::Backspace)
                    if !mode.contains(Mode::REPORT_ALL_KEYS_AS_ESC) =>
                {
                    return
                }
                _ => build_key_sequence(key, mods, mode),
            };

            self.ctx_mut().current_mut().messenger.send_write(bytes);

            return;
        }

        // All key bindings are disabled while a hint is being selected (like Alacritty)
        if self.hint_state.is_active() {
            // Handle special keys first
            match key.logical_key {
                rio_window::keyboard::Key::Named(
                    rio_window::keyboard::NamedKey::Escape,
                ) => {
                    self.hint_state.stop();
                    self.update_hint_state();
                    self.render();
                    return;
                }
                rio_window::keyboard::Key::Named(
                    rio_window::keyboard::NamedKey::Backspace,
                ) => {
                    let terminal = self.context_manager.current().terminal.lock();
                    self.hint_state.keyboard_input(&*terminal, '\x08');
                    drop(terminal);
                    self.update_hint_state();
                    self.render();
                    return;
                }
                _ => {}
            }

            // Handle text input
            let text = key.text_with_all_modifiers().unwrap_or_default();
            for character in text.chars() {
                let terminal = self.context_manager.current().terminal.lock();
                if let Some(hint_match) =
                    self.hint_state.keyboard_input(&*terminal, character)
                {
                    drop(terminal);
                    self.execute_hint_action(&hint_match);
                    // Stop hint mode and update state with proper damage tracking
                    self.hint_state.stop();
                    self.update_hint_state();
                    self.render();
                    return;
                }
                drop(terminal);
            }
            self.update_hint_state();
            self.render();
            return;
        }

        let ignore_chars = self.process_key_bindings(key, &mode, mods);
        if ignore_chars {
            return;
        }

        let text = key.text_with_all_modifiers().unwrap_or_default();

        if self.search_active() {
            for character in text.chars() {
                self.search_input(character);
            }

            self.render();
            return;
        }

        // Vi mode on its own doesn't have any input, the search input was done before.
        if mode.contains(Mode::VI) {
            return;
        }

        // Mask `Alt` modifier from input when we won't send esc.
        let mods = if self.alt_send_esc(key, text) {
            mods
        } else {
            mods & !ModifiersState::ALT
        };

        let build_key_sequence = Self::should_build_sequence(key, text, mode, mods);

        let bytes = if build_key_sequence {
            crate::bindings::kitty_keyboard::build_key_sequence(key, mods, mode)
        } else {
            let mut bytes = Vec::with_capacity(text.len() + 1);
            if mods.alt_key() {
                bytes.push(b'\x1b');
            }

            bytes.extend_from_slice(text.as_bytes());
            bytes
        };

        if !bytes.is_empty() {
            self.scroll_bottom_when_cursor_not_visible();
            self.clear_selection();

            self.ctx_mut().current_mut().messenger.send_write(bytes);
        }
    }

    /// Check whether we should try to build escape sequence for the [`KeyEvent`].
    fn should_build_sequence(
        key: &rio_window::event::KeyEvent,
        text: &str,
        mode: Mode,
        mods: ModifiersState,
    ) -> bool {
        if mode.contains(Mode::REPORT_ALL_KEYS_AS_ESC) {
            return true;
        }

        let disambiguate = mode.contains(Mode::DISAMBIGUATE_ESC_CODES)
            && (key.logical_key == Key::Named(NamedKey::Escape)
                || key.location == KeyLocation::Numpad
                || (!mods.is_empty()
                    && (mods != ModifiersState::SHIFT
                        || matches!(
                            key.logical_key,
                            Key::Named(NamedKey::Tab)
                                | Key::Named(NamedKey::Enter)
                                | Key::Named(NamedKey::Backspace)
                        ))));

        match key.logical_key {
            _ if disambiguate => true,
            // Exclude all the named keys unless they have textual representation.
            Key::Named(named) => named.to_text().is_none(),
            _ => text.is_empty(),
        }
    }

    #[inline]
    pub fn process_mouse_bindings(&mut self, button: MouseButton) {
        let mode = self.get_mode();
        let binding_mode = BindingMode::new(&mode, self.search_active());
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
                let content = self.clipboard.borrow_mut().get(ClipboardType::Selection);
                self.paste(&content, true);
            }
        }
    }

    pub fn process_key_bindings(
        &mut self,
        key: &rio_window::event::KeyEvent,
        mode: &Mode,
        mods: ModifiersState,
    ) -> bool {
        let search_active = self.search_active();
        let binding_mode = BindingMode::new(mode, search_active);
        let mut ignore_chars = None;

        for i in 0..self.bindings.len() {
            let binding = &self.bindings[i];
            let trigger = &binding.trigger;
            let action = binding.action.clone();

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
                // if (cfg!(target_os = "macos") || (cfg!(windows) && mods.control_key()))
                //     && mods.alt_key()
                if (mods.shift_key() || mods.alt_key())
                    || mods.alt_key() && (cfg!(windows) && mods.control_key())
                {
                    key.key_without_modifiers()
                } else {
                    Key::Character(ch.to_lowercase().into())
                }
            } else {
                key.logical_key.clone()
            };

            let key_match = match (&trigger, logical_key) {
                (BindingKey::Scancode(_), _) => BindingKey::Scancode(key.physical_key),
                (_, code) => BindingKey::Keycode {
                    key: code,
                    location: key.location,
                },
            };

            if binding.is_triggered_by(binding_mode.to_owned(), mods, &key_match) {
                *ignore_chars.get_or_insert(true) &= action != Act::ReceiveChar;

                match &action {
                    Act::Run(program) => self.exec(program.program(), program.args()),
                    Act::Esc(s) => {
                        self.paste(s, false);
                    }
                    Act::Paste => {
                        let content =
                            self.clipboard.borrow_mut().get(ClipboardType::Clipboard);
                        self.paste(&content, true);
                    }
                    Act::ClearSelection => {
                        self.clear_selection();
                    }
                    Act::PasteSelection => {
                        let content =
                            self.clipboard.borrow_mut().get(ClipboardType::Selection);
                        self.paste(&content, true);
                    }
                    Act::Copy => {
                        self.copy_selection(ClipboardType::Clipboard);
                    }
                    Act::Hint(hint_config) => {
                        self.start_hint_mode(hint_config.clone());
                    }
                    Act::SearchForward => {
                        self.start_search(Direction::Right);
                        self.resize_top_or_bottom_line(self.ctx().len());
                        self.render();
                    }
                    Act::SearchBackward => {
                        self.start_search(Direction::Left);
                        self.resize_top_or_bottom_line(self.ctx().len());
                        self.render();
                    }
                    Act::Search(SearchAction::SearchConfirm) => {
                        self.confirm_search();
                        self.resize_top_or_bottom_line(self.ctx().len());
                        self.render();
                    }
                    Act::Search(SearchAction::SearchCancel) => {
                        self.cancel_search();
                        self.resize_top_or_bottom_line(self.ctx().len());
                        self.render();
                    }
                    Act::Search(SearchAction::SearchClear) => {
                        let direction = self.search_state.direction;
                        self.cancel_search();
                        self.start_search(direction);
                        self.resize_top_or_bottom_line(self.ctx().len());
                        self.render();
                    }
                    Act::Search(SearchAction::SearchFocusNext) => {
                        self.advance_search_origin(self.search_state.direction);
                        self.resize_top_or_bottom_line(self.ctx().len());
                        self.render();
                    }
                    Act::Search(SearchAction::SearchFocusPrevious) => {
                        let direction = self.search_state.direction.opposite();
                        self.advance_search_origin(direction);
                        self.resize_top_or_bottom_line(self.ctx().len());
                        self.render();
                    }
                    Act::Search(SearchAction::SearchDeleteWord) => {
                        self.search_pop_word();
                        self.render();
                    }
                    Act::Search(SearchAction::SearchHistoryPrevious) => {
                        self.search_history_previous();
                        self.render();
                    }
                    Act::Search(SearchAction::SearchHistoryNext) => {
                        self.search_history_next();
                        self.render();
                    }
                    Act::ToggleViMode => {
                        let mut terminal =
                            self.context_manager.current_mut().terminal.lock();
                        terminal.toggle_vi_mode();
                        let has_vi_mode_enabled = terminal.mode().contains(Mode::VI);
                        drop(terminal);
                        self.renderer.set_vi_mode(has_vi_mode_enabled);
                        self.render();
                    }
                    Act::ViMotion(motion) => {
                        let current_context = self.context_manager.current_mut();
                        let mut terminal = current_context.terminal.lock();
                        if terminal.mode().contains(Mode::VI) {
                            terminal.vi_motion(*motion);
                        }

                        if let Some(selection) = &terminal.selection {
                            current_context.renderable_content.selection_range =
                                selection.to_range(&terminal);
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
                        self.render();
                    }
                    Act::Vi(ViAction::ToggleLineSelection) => {
                        self.toggle_selection(SelectionType::Lines, Side::Left);
                        self.render();
                    }
                    Act::Vi(ViAction::ToggleBlockSelection) => {
                        self.toggle_selection(SelectionType::Block, Side::Left);
                        self.render();
                    }
                    Act::Vi(ViAction::ToggleSemanticSelection) => {
                        self.toggle_selection(SelectionType::Semantic, Side::Left);
                        self.render();
                    }
                    Act::SplitRight => {
                        self.split_right();
                    }
                    Act::SplitDown => {
                        self.split_down();
                    }
                    Act::MoveDividerUp => {
                        // User wants divider to move up visually, which means expanding the bottom split
                        self.move_divider_down();
                    }
                    Act::MoveDividerDown => {
                        // User wants divider to move down visually, which means expanding the top split
                        self.move_divider_up();
                    }
                    Act::MoveDividerLeft => {
                        self.move_divider_left();
                    }
                    Act::MoveDividerRight => {
                        self.move_divider_right();
                    }
                    Act::ConfigEditor => {
                        self.context_manager.switch_to_settings();
                    }
                    Act::WindowCreateNew => {
                        self.context_manager.create_new_window();
                    }
                    Act::CloseCurrentSplitOrTab => {
                        self.close_split_or_tab();
                    }
                    Act::TabCreateNew => {
                        self.create_tab();
                    }
                    Act::TabCloseCurrent => {
                        self.close_tab();
                    }
                    Act::TabCloseUnfocused => {
                        self.clear_selection();
                        self.cancel_search();
                        if self.ctx().len() <= 1 {
                            return true;
                        }
                        self.context_manager.close_unfocused_tabs();
                        self.resize_top_or_bottom_line(1);
                        self.render();
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
                        self.render();
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
                        self.render();
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
                        self.render();
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
                        self.render();
                    }
                    Act::ScrollToTop => {
                        let mut terminal =
                            self.context_manager.current_mut().terminal.lock();
                        terminal.scroll_display(Scroll::Top);

                        let topmost_line = terminal.grid.topmost_line();
                        terminal.vi_mode_cursor.pos.row = topmost_line;
                        terminal.vi_motion(ViMotion::FirstOccupied);
                        drop(terminal);
                        self.render();
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
                        self.render();
                    }
                    Act::Scroll(delta) => {
                        let mut terminal =
                            self.context_manager.current_mut().terminal.lock();
                        terminal.scroll_display(Scroll::Delta(*delta));
                        drop(terminal);
                        self.render();
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
                    Act::SelectNextSplit => {
                        self.cancel_search();
                        self.context_manager.select_next_split();
                        self.render();
                    }
                    Act::SelectPrevSplit => {
                        self.cancel_search();
                        self.context_manager.select_prev_split();
                        self.render();
                    }
                    Act::SelectNextSplitOrTab => {
                        self.cancel_search();
                        self.clear_selection();
                        self.context_manager.switch_to_next_split_or_tab();
                        self.render();
                    }
                    Act::SelectPrevSplitOrTab => {
                        self.cancel_search();
                        self.clear_selection();
                        self.context_manager.switch_to_prev_split_or_tab();
                        self.render();
                    }
                    Act::SelectTab(tab_index) => {
                        self.context_manager.select_tab(*tab_index);
                        self.cancel_search();
                        self.render();
                    }
                    Act::SelectLastTab => {
                        self.cancel_search();
                        self.context_manager.select_last_tab();
                        self.render();
                    }
                    Act::SelectNextTab => {
                        self.cancel_search();
                        self.clear_selection();
                        self.context_manager.switch_to_next();
                        self.render();
                    }
                    Act::MoveCurrentTabToPrev => {
                        self.cancel_search();
                        self.clear_selection();
                        self.context_manager.move_current_to_prev();
                        self.render();
                    }
                    Act::MoveCurrentTabToNext => {
                        self.cancel_search();
                        self.clear_selection();
                        self.context_manager.move_current_to_next();
                        self.render();
                    }
                    Act::SelectPrevTab => {
                        self.cancel_search();
                        self.clear_selection();
                        self.context_manager.switch_to_prev();
                        self.render();
                    }
                    Act::ReceiveChar | Act::None => (),
                    _ => (),
                }
            }
        }

        ignore_chars.unwrap_or(false)
    }

    pub fn split_right_with_config(&mut self, config: rio_backend::config::Config) {
        let rich_text_id = self.sugarloaf.create_rich_text();
        self.context_manager
            .split_from_config(rich_text_id, false, config);

        self.render();
    }

    pub fn split_right(&mut self) {
        let rich_text_id = self.sugarloaf.create_rich_text();
        self.context_manager.split(rich_text_id, false);

        self.render();
    }

    pub fn split_down(&mut self) {
        let rich_text_id = self.sugarloaf.create_rich_text();
        self.context_manager.split(rich_text_id, true);

        self.render();
    }

    pub fn move_divider_up(&mut self) {
        let amount = 20.0; // Default movement amount
        if self.context_manager.move_divider_up(amount) {
            self.render();
        }
    }

    pub fn move_divider_down(&mut self) {
        let amount = 20.0; // Default movement amount
        if self.context_manager.move_divider_down(amount) {
            self.render();
        }
    }

    pub fn move_divider_left(&mut self) {
        let amount = 40.0; // Default movement amount
        if self.context_manager.move_divider_left(amount) {
            self.render();
        }
    }

    pub fn move_divider_right(&mut self) {
        let amount = 40.0; // Default movement amount
        if self.context_manager.move_divider_right(amount) {
            self.render();
        }
    }

    pub fn create_tab(&mut self) {
        let redirect = true;

        // We resize the current tab ahead to prepare the
        // dimensions to be copied to next tab.
        let num_tabs = self.ctx().len();
        self.resize_top_or_bottom_line(num_tabs + 1);

        let rich_text_id = self.sugarloaf.create_rich_text();
        self.context_manager.add_context(redirect, rich_text_id);

        self.cancel_search();
        self.render();
    }

    pub fn close_split_or_tab(&mut self) {
        if self.context_manager.current_grid_len() > 1 {
            self.clear_selection();
            self.context_manager.remove_current_grid();
            self.render();
        } else {
            self.close_tab();
        }
    }

    pub fn close_tab(&mut self) {
        self.clear_selection();
        self.context_manager.close_current_context();

        self.cancel_search();
        if self.ctx().len() <= 1 {
            return;
        }

        let num_tabs = self.ctx().len().wrapping_sub(1);
        self.resize_top_or_bottom_line(num_tabs);
        self.render();
    }

    pub fn resize_top_or_bottom_line(&mut self, num_tabs: usize) {
        let layout = self.context_manager.current().dimension;
        let previous_margin = layout.margin;
        let padding_y_top = padding_top_from_config(
            &self.renderer.navigation.navigation,
            self.renderer.navigation.padding_y[0],
            num_tabs,
            self.renderer.macos_use_unified_titlebar,
        );
        let padding_y_bottom = padding_bottom_from_config(
            &self.renderer.navigation.navigation,
            self.renderer.navigation.padding_y[1],
            num_tabs,
            self.search_active(),
        );

        if previous_margin.top_y != padding_y_top
            || previous_margin.bottom_y != padding_y_bottom
        {
            let layout = self
                .sugarloaf
                .rich_text_layout(&self.context_manager.current().rich_text_id);
            let s = self.sugarloaf.style_mut();
            s.font_size = layout.font_size;
            s.line_height = layout.line_height;

            let d = self.context_manager.current_grid_mut();
            d.update_margin((d.margin.x, padding_y_top, padding_y_bottom));
            self.resize_all_contexts();
        }
    }

    #[inline]
    fn search_pop_word(&mut self) {
        if let Some(regex) = self.search_state.regex_mut() {
            *regex = regex.trim_end().to_owned();
            regex.truncate(regex.rfind(' ').map_or(0, |i| i + 1));
            self.update_search();
        }
    }

    /// Go to the previous regex in the search history.
    #[inline]
    fn search_history_previous(&mut self) {
        let index = match &mut self.search_state.history_index {
            None => return,
            Some(index) if *index + 1 >= self.search_state.history.len() => return,
            Some(index) => index,
        };

        *index += 1;
        self.update_search();
    }

    /// Go to the previous regex in the search history.
    #[inline]
    fn search_history_next(&mut self) {
        let index = match &mut self.search_state.history_index {
            Some(0) | None => return,
            Some(index) => index,
        };

        *index -= 1;
        self.update_search();
    }

    #[inline]
    fn advance_search_origin(&mut self, direction: Direction) {
        // Use focused match as new search origin if available.
        if let Some(focused_match) = &self.search_state.focused_match {
            let mut terminal = self.context_manager.current_mut().terminal.lock();
            let new_origin = match direction {
                Direction::Right => {
                    focused_match.end().add(&*terminal, Boundary::None, 1)
                }
                Direction::Left => {
                    focused_match.start().sub(&*terminal, Boundary::None, 1)
                }
            };

            terminal.scroll_to_pos(new_origin);
            drop(terminal);

            self.search_state.display_offset_delta = 0;
            self.search_state.origin = new_origin;
        }

        // Search for the next match using the supplied direction.
        let search_direction =
            std::mem::replace(&mut self.search_state.direction, direction);
        self.goto_match(None);
        self.search_state.direction = search_direction;

        // If we found a match, we set the search origin right in front of it to make sure that
        // after modifications to the regex the search is started without moving the focused match
        // around.
        let focused_match = match &self.search_state.focused_match {
            Some(focused_match) => focused_match,
            None => return,
        };

        // Set new origin to the left/right of the match, depending on search direction.
        let new_origin = match self.search_state.direction {
            Direction::Right => *focused_match.start(),
            Direction::Left => *focused_match.end(),
        };

        let mut terminal = self.context_manager.current_mut().terminal.lock();

        // Store the search origin with display offset by checking how far we need to scroll to it.
        let old_display_offset = terminal.display_offset() as i32;
        terminal.scroll_to_pos(new_origin);
        let new_display_offset = terminal.display_offset() as i32;
        self.search_state.display_offset_delta = new_display_offset - old_display_offset;

        // Store origin and scroll back to the match.
        terminal.scroll_display(Scroll::Delta(-self.search_state.display_offset_delta));
        drop(terminal);
        self.search_state.origin = new_origin;
    }

    /// Whether we should send `ESC` due to `Alt` being pressed.
    fn alt_send_esc(&mut self, key: &rio_window::event::KeyEvent, text: &str) -> bool {
        #[cfg(not(target_os = "macos"))]
        let alt_send_esc = self.modifiers.state().alt_key();

        #[cfg(target_os = "macos")]
        let alt_send_esc = {
            let option_as_alt = &self.renderer.option_as_alt;
            self.modifiers.state().alt_key()
                && (option_as_alt == "both"
                    || (option_as_alt == "left"
                        && self.modifiers.lalt_state() == ModifiersKeyState::Pressed)
                    || (option_as_alt == "right"
                        && self.modifiers.ralt_state() == ModifiersKeyState::Pressed))
        };

        match key.logical_key {
            Key::Named(named) => {
                if named.to_text().is_some() {
                    alt_send_esc
                } else {
                    // Treat `Alt` as modifier for named keys without text, like ArrowUp.
                    self.modifiers.state().alt_key()
                }
            }
            _ => alt_send_esc && text.chars().count() == 1,
        }
    }

    pub fn copy_selection(&mut self, ty: ClipboardType) {
        let terminal = self.context_manager.current_mut().terminal.lock();
        let text = match terminal.selection_to_string().filter(|s| !s.is_empty()) {
            Some(text) => text,
            None => return,
        };
        drop(terminal);

        if ty == ClipboardType::Selection {
            self.clipboard
                .borrow_mut()
                .set(ClipboardType::Clipboard, text.clone());
        }
        self.clipboard.borrow_mut().set(ty, text);
    }

    #[inline]
    pub fn clear_selection(&mut self) {
        // Clear the selection on the terminal.
        let mut terminal = self.context_manager.current_mut().terminal.lock();
        terminal.selection.take();
        drop(terminal);
        self.context_manager.current_mut().set_selection(None);
    }

    #[inline]
    fn start_selection(&mut self, ty: SelectionType, point: Pos, side: Side) {
        self.copy_selection(ClipboardType::Selection);
        let current = self.context_manager.current_mut();
        let mut terminal = current.terminal.lock();
        let selection = Selection::new(ty, point, side);
        let selection_range = selection.to_range(&terminal);
        terminal.selection = Some(selection);
        drop(terminal);

        // Use set_selection to trigger render
        current.set_selection(selection_range);

        // Request render to ensure it shows immediately
        self.context_manager.request_render();
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

        let current = self.context_manager.current_mut();
        let mut terminal = current.terminal.lock();
        let mut selection = match terminal.selection.take() {
            Some(selection) => {
                // Make sure initial selection is not empty.
                selection
            }
            None => return,
        };

        selection.include_all();
        current.renderable_content.selection_range = selection.to_range(&terminal);
        terminal.selection = Some(selection);
        drop(terminal);
    }

    #[inline]
    pub fn update_selection(&mut self, mut pos: Pos, side: Side) {
        let is_search_active = self.search_active();
        let current = self.context_manager.current_mut();
        let mut terminal = current.terminal.lock();
        let mut selection = match terminal.selection.take() {
            Some(selection) => selection,
            None => return,
        };

        // Treat motion over message bar like motion over the last line.
        pos.row = std::cmp::min(pos.row, terminal.bottommost_line());

        // Update selection.
        selection.update(pos, side);

        // Move vi cursor and expand selection.
        if terminal.mode().contains(Mode::VI) && !is_search_active {
            terminal.vi_mode_cursor.pos = pos;
            selection.include_all();
        }

        let selection_range = selection.to_range(&terminal);
        terminal.selection = Some(selection);
        drop(terminal);

        // Use set_selection to trigger render
        current.set_selection(selection_range);

        // Request render to ensure it shows immediately
        self.context_manager.request_render();
    }

    #[inline]
    /// Update hint highlighting based on mouse position and modifiers
    pub fn update_highlighted_hints(&mut self) -> bool {
        // Check if any hint configuration has matching modifiers
        let should_highlight = self.hints_config.iter().any(|hint_config| {
            hint_config.mouse.enabled && self.modifiers_match(&hint_config.mouse.mods)
        });

        let had_highlight = self
            .context_manager
            .current()
            .renderable_content
            .highlighted_hint
            .is_some();

        if !should_highlight {
            let current = self.context_manager.current_mut();

            // Clear any previous hint damage
            if current.renderable_content.highlighted_hint.is_some() {
                let mut terminal = current.terminal.lock();
                let display_offset = terminal.display_offset();
                terminal.update_selection_damage(None, display_offset);
            }

            current.renderable_content.highlighted_hint = None;
            return had_highlight;
        }

        let terminal = self.context_manager.current().terminal.lock();
        let display_offset = terminal.display_offset();
        let mouse_point = self.mouse_position(display_offset);

        // Find hint at mouse position
        let highlighted_hint =
            self.find_hint_at_point(&terminal, mouse_point, self.modifiers.state());
        drop(terminal);

        let current = self.context_manager.current_mut();

        if let Some(hint_match) = highlighted_hint {
            // Mark the hint range as damaged so it gets re-rendered
            {
                let mut terminal = current.terminal.lock();
                let display_offset = terminal.display_offset();

                // Create a temporary selection range for damage tracking
                let hint_range = rio_backend::selection::SelectionRange::new(
                    hint_match.start,
                    hint_match.end,
                    false,
                );
                terminal.update_selection_damage(Some(hint_range), display_offset);
            }

            current.renderable_content.highlighted_hint = Some(hint_match);
            true
        } else {
            // Clear any previous hint damage
            if current.renderable_content.highlighted_hint.is_some() {
                let mut terminal = current.terminal.lock();
                let display_offset = terminal.display_offset();
                terminal.update_selection_damage(None, display_offset);
            }

            current.renderable_content.highlighted_hint = None;
            had_highlight
        }
    }

    /// Check if current modifiers match the required modifiers
    fn modifiers_match(&self, required_mods: &[String]) -> bool {
        if required_mods.is_empty() {
            return true;
        }

        let current_mods = self.modifiers.state();

        for required_mod in required_mods {
            let matches = match required_mod.as_str() {
                "Shift" => current_mods.shift_key(),
                "Control" | "Ctrl" => current_mods.control_key(),
                "Alt" => current_mods.alt_key(),
                "Super" | "Cmd" | "Command" => current_mods.super_key(),
                _ => false,
            };

            if !matches {
                return false;
            }
        }

        true
    }

    /// Find hint at the specified point
    fn find_hint_at_point(
        &self,
        terminal: &rio_backend::crosswords::Crosswords<EventProxy>,
        point: rio_backend::crosswords::pos::Pos,
        _modifiers: rio_window::keyboard::ModifiersState,
    ) -> Option<crate::hints::HintMatch> {
        // Check each enabled hint configuration
        for hint_config in &self.hints_config {
            // Check if mouse highlighting is enabled for this hint
            if !hint_config.mouse.enabled {
                continue;
            }

            // Check if current modifiers match the required modifiers for this hint
            if !self.modifiers_match(&hint_config.mouse.mods) {
                continue;
            }

            // Check hyperlinks if enabled
            if hint_config.hyperlinks {
                if let Some(hyperlink_match) =
                    self.find_hyperlink_at_point(terminal, point)
                {
                    return Some(hyperlink_match);
                }
            }

            // Check regex patterns if specified
            if let Some(regex_pattern) = &hint_config.regex {
                if let Ok(regex) = regex::Regex::new(regex_pattern) {
                    if let Some(regex_match) = self.find_regex_match_at_point(
                        terminal,
                        point,
                        &regex,
                        hint_config.clone(),
                    ) {
                        return Some(regex_match);
                    }
                }
            }
        }

        None
    }

    /// Find hyperlink at the specified point
    fn find_hyperlink_at_point(
        &self,
        terminal: &rio_backend::crosswords::Crosswords<EventProxy>,
        point: rio_backend::crosswords::pos::Pos,
    ) -> Option<crate::hints::HintMatch> {
        let grid = &terminal.grid;

        // Check if the point is within grid bounds
        if point.row >= grid.total_lines() as i32 || point.col.0 >= grid.columns() {
            return None;
        }

        let cell = &grid[point.row][point.col];
        if let Some(hyperlink) = cell.hyperlink() {
            // Find the extent of this hyperlink
            let mut start_col = point.col;
            let mut end_col = point.col;

            // Scan backward to find start
            while start_col > rio_backend::crosswords::pos::Column(0) {
                let prev_col = start_col - 1;
                let prev_cell = &grid[point.row][prev_col];
                if prev_cell.hyperlink().as_ref() == Some(&hyperlink) {
                    start_col = prev_col;
                } else {
                    break;
                }
            }

            // Scan forward to find end
            while end_col < grid.columns() - 1 {
                let next_col = end_col + 1;
                let next_cell = &grid[point.row][next_col];
                if next_cell.hyperlink().as_ref() == Some(&hyperlink) {
                    end_col = next_col;
                } else {
                    break;
                }
            }

            // Create a dummy hint config for hyperlinks
            let hint_config = std::rc::Rc::new(rio_backend::config::hints::Hint {
                regex: None,
                hyperlinks: true,
                post_processing: true,
                persist: false,
                action: rio_backend::config::hints::HintAction::Command {
                    command: rio_backend::config::hints::HintCommand::Simple(
                        "xdg-open".to_string(),
                    ),
                },
                mouse: rio_backend::config::hints::HintMouse::default(),
                binding: None,
            });

            let mut uri = hyperlink.uri().to_string();
            if hint_config.post_processing {
                uri = post_process_hyperlink_uri(&uri);
            }

            return Some(crate::hints::HintMatch {
                text: uri,
                start: rio_backend::crosswords::pos::Pos::new(point.row, start_col),
                end: rio_backend::crosswords::pos::Pos::new(point.row, end_col),
                hint: hint_config,
            });
        }

        None
    }

    /// Find regex match at the specified point
    fn find_regex_match_at_point(
        &self,
        terminal: &rio_backend::crosswords::Crosswords<EventProxy>,
        point: rio_backend::crosswords::pos::Pos,
        regex: &regex::Regex,
        hint_config: std::rc::Rc<rio_backend::config::hints::Hint>,
    ) -> Option<crate::hints::HintMatch> {
        let grid = &terminal.grid;

        // Check if the point is within grid bounds
        if point.row >= grid.total_lines() as i32 || point.col.0 >= grid.columns() {
            return None;
        }

        // Extract text from the line
        let mut line_text = String::new();
        for col in 0..grid.columns() {
            let cell = &grid[point.row][rio_backend::crosswords::pos::Column(col)];
            line_text.push(cell.c);
        }
        let line_text = line_text.trim_end();

        // Find all matches in this line and check if point is within any of them
        for mat in regex.find_iter(line_text) {
            let start_col = rio_backend::crosswords::pos::Column(mat.start());
            let end_col =
                rio_backend::crosswords::pos::Column(mat.end().saturating_sub(1));

            // Check if the point is within this match
            if point.col >= start_col && point.col <= end_col {
                let original_match_text = mat.as_str().to_string();
                let mut match_text = original_match_text.clone();

                // Apply grid-based post-processing
                let (processed_start, processed_end) = if hint_config.post_processing {
                    self.hint_post_processing(
                        terminal,
                        start_col,
                        end_col,
                        rio_backend::crosswords::pos::Line(point.row.0),
                    )
                    .unwrap_or((start_col, end_col))
                } else {
                    (start_col, end_col)
                };

                // Extract the processed text
                if hint_config.post_processing {
                    let mut processed_text = String::new();
                    for col in processed_start.0..=processed_end.0 {
                        let cell =
                            &grid[point.row][rio_backend::crosswords::pos::Column(col)];
                        processed_text.push(cell.c);
                    }
                    match_text = processed_text.trim_end().to_string();
                }

                return Some(crate::hints::HintMatch {
                    text: match_text,
                    start: rio_backend::crosswords::pos::Pos::new(
                        point.row,
                        processed_start,
                    ),
                    end: rio_backend::crosswords::pos::Pos::new(point.row, processed_end),
                    hint: hint_config,
                });
            }
        }

        None
    }

    #[inline]
    pub fn trigger_hyperlink(&self) -> bool {
        // Check if any hyperlink hint configuration has the required modifiers active
        let mut is_hyperlink_key_active = false;
        for hint_config in &self.hints_config {
            if hint_config.hyperlinks && self.modifiers_match(&hint_config.mouse.mods) {
                is_hyperlink_key_active = true;
                break;
            }
        }

        if !is_hyperlink_key_active
            || !self.context_manager.current().has_hyperlink_range()
        {
            return false;
        }

        let terminal = self.context_manager.current().terminal.lock();
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

    /// Trigger hint action at mouse position
    #[inline]
    pub fn trigger_hint(&mut self) -> bool {
        // Take the highlighted hint
        let hint_match = self
            .context_manager
            .current_mut()
            .renderable_content
            .highlighted_hint
            .take();

        if let Some(hint_match) = hint_match {
            self.execute_hint_action(&hint_match);
            true
        } else {
            false
        }
    }

    fn open_hyperlink(&self, hyperlink: Hyperlink) {
        // Apply post-processing to remove trailing delimiters and handle uneven brackets
        let processed_uri = post_process_hyperlink_uri(hyperlink.uri());

        #[cfg(not(any(target_os = "macos", windows)))]
        self.exec("xdg-open", [&processed_uri]);

        #[cfg(target_os = "macos")]
        self.exec("open", [&processed_uri]);

        #[cfg(windows)]
        self.exec("cmd", ["/c", "start", "", &processed_uri]);
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
                Ok(_) => tracing::debug!("Launched {} with args {:?}", program, args),
                Err(_) => {
                    tracing::warn!("Unable to launch {} with args {:?}", program, args)
                }
            }
        }

        #[cfg(windows)]
        {
            match teletypewriter::spawn_daemon(program, args) {
                Ok(_) => tracing::debug!("Launched {} with args {:?}", program, args),
                Err(_) => {
                    tracing::warn!("Unable to launch {} with args {:?}", program, args)
                }
            }
        }
    }

    #[inline]
    pub fn update_selection_scrolling(&mut self, mouse_y: f64) {
        let current_context = self.context_manager.current();
        let layout = current_context.dimension;
        let sugarloaf_layout = self
            .sugarloaf
            .rich_text_layout(&current_context.rich_text_id);
        let scale_factor = layout.dimension.scale;
        let min_height = (MIN_SELECTION_SCROLLING_HEIGHT * scale_factor) as i32;
        let step = (SELECTION_SCROLLING_STEP * scale_factor) as f64;

        // Compute the height of the scrolling areas.
        let end_top = max(min_height, crate::constants::PADDING_Y as i32) as f64;
        let text_area_bottom = (crate::constants::PADDING_Y + layout.lines as f32)
            * sugarloaf_layout.font_size;
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

        let mut terminal = self.context_manager.current_mut().terminal.lock();
        terminal.scroll_display(Scroll::Delta((delta / step) as i32));
        drop(terminal);
    }

    #[inline]
    pub fn contains_point(&self, x: usize, y: usize) -> bool {
        let current_context = self.context_manager.current();
        let layout = current_context.dimension;
        let width = layout.dimension.width;
        x <= (layout.margin.x + layout.columns as f32 * width) as usize
            && x > (layout.margin.x * layout.dimension.scale) as usize
            && y <= (layout.margin.top_y * layout.dimension.scale
                + layout.lines as f32 * layout.dimension.height)
                as usize
            && y > layout.margin.top_y as usize
    }

    #[inline]
    pub fn side_by_pos(&self, x: usize) -> Side {
        let current_context = self.context_manager.current();
        let layout = current_context.dimension;
        let width = (layout.dimension.width) as usize;
        let margin_x = layout.margin.x * layout.dimension.scale;

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
        self.context_manager
            .current()
            .renderable_content
            .selection_range
            .is_none()
    }

    #[inline]
    pub fn on_left_click(&mut self, point: Pos) {
        let side = self.mouse.square_side;

        match self.mouse.click_state {
            ClickState::Click => {
                // If Shift is pressed and there's an existing selection, expand it
                if self.modifiers.state().shift_key() && !self.selection_is_empty() {
                    self.update_selection(point, side);
                } else {
                    self.clear_selection();

                    // Start new empty selection.
                    if self.modifiers.state().control_key() {
                        self.start_selection(SelectionType::Block, point, side);
                    } else {
                        self.start_selection(SelectionType::Simple, point, side);
                    }
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
        let mut terminal = self.context_manager.current_mut().terminal.lock();
        if terminal.mode().contains(Mode::VI) {
            terminal.vi_mode_cursor.pos = point;
        }
        drop(terminal);
    }

    #[inline]
    fn start_search(&mut self, direction: Direction) {
        // Only create new history entry if the previous regex wasn't empty.
        if self
            .search_state
            .history
            .front()
            .is_none_or(|regex| !regex.is_empty())
        {
            self.search_state.history.push_front(String::new());
            self.search_state.history.truncate(MAX_SEARCH_HISTORY_SIZE);
        }

        self.search_state.history_index = Some(0);
        self.search_state.direction = direction;
        self.search_state.focused_match = None;

        // Store original search position as origin and reset location.
        if self.get_mode().contains(Mode::VI) {
            let terminal = self.context_manager.current().terminal.lock();
            self.search_state.origin = terminal.vi_mode_cursor.pos;
            self.search_state.display_offset_delta = 0;

            // Adjust origin for content moving upward on search start.
            if terminal.grid.cursor.pos.row + 1 == terminal.screen_lines() {
                self.search_state.origin.row -= 1;
            }
            drop(terminal);
        } else {
            let terminal = self.context_manager.current().terminal.lock();
            let viewport_top = Line(-(terminal.grid.display_offset() as i32)) - 1;
            let viewport_bottom = viewport_top + terminal.bottommost_line();
            let last_column = terminal.last_column();
            self.search_state.origin = match direction {
                Direction::Right => Pos::new(viewport_top, Column(0)),
                Direction::Left => Pos::new(viewport_bottom, last_column),
            };
            drop(terminal);
        }

        // Enable IME so we can input into the search bar with it if we were in Vi mode.
        // self.window().set_ime_allowed(true);

        self.render();
    }

    #[inline]
    fn confirm_search(&mut self) {
        // Just cancel search when not in vi mode.
        if !self.get_mode().contains(Mode::VI) {
            self.cancel_search();
            return;
        }

        // Force unlimited search if the previous one was interrupted.
        // let timer_id = TimerId::new(Topic::DelayedSearch, self.display.window.id());
        // if self.scheduler.scheduled(timer_id) {
        //     self.goto_match(None);
        // }

        self.exit_search();
    }

    #[inline]
    fn cancel_search(&mut self) {
        if self.get_mode().contains(Mode::VI) {
            // Recover pre-search state in vi mode.
            self.search_reset_state();
        } else if let Some(focused_match) = &self.search_state.focused_match {
            // Create a selection for the focused match.
            let start = *focused_match.start();
            let end = *focused_match.end();
            self.start_selection(SelectionType::Simple, start, Side::Left);
            self.update_selection(end, Side::Right);
            self.copy_selection(ClipboardType::Selection);
        }

        self.search_state.dfas = None;

        self.exit_search();
    }

    /// Cleanup the search state.
    fn exit_search(&mut self) {
        // let vi_mode = self.get_mode().contains(Mode::VI);
        // self.window().set_ime_allowed(!vi_mode);

        self.search_state.history_index = None;

        // Clear focused match.
        self.search_state.focused_match = None;

        self.render();
    }

    #[inline]
    fn search_input(&mut self, c: char) {
        match self.search_state.history_index {
            Some(0) => (),
            // When currently in history, replace active regex with history on change.
            Some(index) => {
                self.search_state.history[0] = self.search_state.history[index].clone();
                self.search_state.history_index = Some(0);
            }
            None => return,
        }
        let regex = &mut self.search_state.history[0];

        match c {
            // Handle backspace/ctrl+h.
            '\x08' | '\x7f' => {
                let _ = regex.pop();
            }
            // Add ascii and unicode text.
            ' '..='~' | '\u{a0}'..='\u{10ffff}' => regex.push(c),
            // Ignore non-printable characters.
            _ => return,
        }

        let mode = self.get_mode();
        if !mode.contains(Mode::VI) {
            // Clear selection so we do not obstruct any matches.
            self.context_manager.current_mut().set_selection(None);
        }

        self.update_search();
        self.render();
    }

    fn update_search(&mut self) {
        let regex = match self.search_state.regex() {
            Some(regex) => regex,
            None => return,
        };

        if regex.is_empty() {
            // Stop search if there's nothing to search for.
            self.search_reset_state();
            self.search_state.dfas = None;
        } else {
            // Create search dfas for the new regex string.
            self.search_state.dfas = RegexSearch::new(regex).ok();

            // Update search highlighting.
            self.goto_match(MAX_SEARCH_WHILE_TYPING);
        }
    }

    /// Reset terminal to the state before search was started.
    fn search_reset_state(&mut self) {
        // Unschedule pending timers.
        // let timer_id = TimerId::new(Topic::DelayedSearch, self.display.window.id());
        // self.scheduler.unschedule(timer_id);

        // Clear focused match.
        self.search_state.focused_match = None;

        // The viewport reset logic is only needed for vi mode, since without it our origin is
        // always at the current display offset instead of at the vi cursor position which we need
        // to recover to.
        let mode = self.get_mode();
        if !mode.contains(Mode::VI) {
            return;
        }

        // Reset display offset and cursor position.
        {
            let mut terminal = self.context_manager.current_mut().terminal.lock();
            terminal.vi_mode_cursor.pos = self.search_state.origin;
            terminal
                .scroll_display(Scroll::Delta(self.search_state.display_offset_delta));
            drop(terminal);
        }
        self.search_state.display_offset_delta = 0;
    }

    /// Jump to the first regex match from the search origin.
    fn goto_match(&mut self, mut limit: Option<usize>) {
        let dfas = match &mut self.search_state.dfas {
            Some(dfas) => dfas,
            None => return,
        };

        let mut should_reset_search_state = false;

        // Jump to the next match.
        {
            let mut terminal = self.context_manager.current_mut().terminal.lock();
            // Limit search only when enough lines are available to run into the limit.
            limit = limit.filter(|&limit| limit <= terminal.total_lines());

            let direction = self.search_state.direction;
            let clamped_origin = self
                .search_state
                .origin
                .grid_clamp(&*terminal, Boundary::Grid);
            match terminal.search_next(dfas, clamped_origin, direction, Side::Left, limit)
            {
                Some(regex_match) => {
                    let old_offset = terminal.display_offset() as i32;
                    if terminal.mode().contains(Mode::VI) {
                        // Move vi cursor to the start of the match.
                        terminal.vi_goto_pos(*regex_match.start());
                    } else {
                        // Select the match when vi mode is not active.
                        terminal.scroll_to_pos(*regex_match.start());
                    }

                    // Update the focused match.
                    self.search_state.focused_match = Some(regex_match);

                    // Store number of lines the viewport had to be moved.
                    let display_offset = terminal.display_offset();
                    self.search_state.display_offset_delta +=
                        old_offset - display_offset as i32;

                    // Since we found a result, we require no delayed re-search.
                    // let timer_id = TimerId::new(Topic::DelayedSearch, self.display.window.id());
                    // self.scheduler.unschedule(timer_id);
                }
                // Reset viewport only when we know there is no match, to prevent unnecessary jumping.
                None if limit.is_none() => {
                    should_reset_search_state = true;
                }
                None => {
                    // Schedule delayed search if we ran into our search limit.
                    // let timer_id = TimerId::new(Topic::DelayedSearch, self.display.window.id());
                    // if !self.scheduler.scheduled(timer_id) {
                    // let event = Event::new(EventType::SearchNext, self.display.window.id());
                    // self.scheduler.schedule(event, TYPING_SEARCH_DELAY, false, timer_id);
                    // }

                    // Clear focused match.
                    self.search_state.focused_match = None;
                }
            }
            drop(terminal);
        }

        if should_reset_search_state {
            self.search_reset_state();
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
            .send_write(msg.into_bytes());
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
        let terminal = self.ctx().current().terminal.lock();
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

        self.ctx_mut().current_mut().messenger.send_write(msg);
    }

    #[inline]
    pub fn on_focus_change(&mut self, is_focused: bool) {
        if self.get_mode().contains(Mode::FOCUS_IN_OUT) {
            let chr = if is_focused { "I" } else { "O" };

            let msg = format!("\x1b[{chr}");
            self.ctx_mut()
                .current_mut()
                .messenger
                .send_write(msg.into_bytes());
        }
    }

    #[inline]
    pub fn scroll(&mut self, new_scroll_x_px: f64, new_scroll_y_px: f64) {
        let layout = self
            .sugarloaf
            .rich_text_layout(&self.context_manager.current().rich_text_id);
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
                self.ctx_mut().current_mut().messenger.send_write(content);
            }
        } else {
            self.mouse.accumulated_scroll.y +=
                (new_scroll_y_px * self.mouse.multiplier) / self.mouse.divider;
            let lines = (self.mouse.accumulated_scroll.y
                / layout.dimensions.height as f64) as i32;

            if lines != 0 {
                let mut terminal = self.context_manager.current_mut().terminal.lock();
                terminal.scroll_display(Scroll::Delta(lines));
                drop(terminal);
            }
        }

        self.mouse.accumulated_scroll.x %= width;
        self.mouse.accumulated_scroll.y %= height;
    }

    #[inline]
    pub fn paste(&mut self, text: &str, bracketed: bool) {
        if self.search_active() {
            for c in text.chars() {
                self.search_input(c);
            }
        } else if bracketed && self.get_mode().contains(Mode::BRACKETED_PASTE) {
            self.scroll_bottom_when_cursor_not_visible();
            self.clear_selection();

            self.ctx_mut()
                .current_mut()
                .messenger
                .send_write(&b"\x1b[200~"[..]);

            // Write filtered escape sequences.
            //
            // We remove `\x1b` to ensure it's impossible for the pasted text to write the bracketed
            // paste end escape `\x1b[201~` and `\x03` since some shells incorrectly terminate
            // bracketed paste on its receival.
            let filtered = text.replace(['\x1b', '\x03'], "");
            self.ctx_mut()
                .current_mut()
                .messenger
                .send_write(filtered.into_bytes());

            self.ctx_mut()
                .current_mut()
                .messenger
                .send_write(&b"\x1b[201~"[..]);
        } else {
            let payload = if bracketed {
                // In non-bracketed (ie: normal) mode, terminal applications cannot distinguish
                // pasted data from keystrokes.
                //
                // In theory, we should construct the keystrokes needed to produce the data we are
                // pasting... since that's neither practical nor sensible (and probably an
                // impossible task to solve in a general way), we'll just replace line breaks
                // (windows and unix style) with a single carriage return (\r, which is what the
                // Enter key produces).
                text.replace("\r\n", "\r").replace('\n', "\r").into_bytes()
            } else {
                // When we explicitly disable bracketed paste don't manipulate with the input,
                // so we pass user input as is.
                text.to_owned().into_bytes()
            };

            self.ctx_mut().current_mut().messenger.send_write(payload);
        }
    }

    pub fn render_assistant(
        &mut self,
        assistant: &crate::router::routes::assistant::Assistant,
    ) {
        self.sugarloaf.clear();
        crate::router::routes::assistant::screen(
            &mut self.sugarloaf,
            &self.context_manager.current().dimension,
            assistant,
        );
        self.sugarloaf.render();
    }

    pub fn render_welcome(&mut self) {
        self.sugarloaf.clear();
        crate::router::routes::welcome::screen(
            &mut self.sugarloaf,
            &self.context_manager.current().dimension,
        );
        self.sugarloaf.render();
    }

    pub fn render_dialog(&mut self, content: &str, confirm: &str, close: &str) {
        self.sugarloaf.clear();
        crate::router::routes::dialog::screen(
            &mut self.sugarloaf,
            &self.context_manager.current().dimension,
            content,
            confirm,
            close,
        );
        self.sugarloaf.render();
    }

    pub fn render(&mut self) {
        // let screen_render_start = std::time::Instant::now();
        let is_search_active = self.search_active();
        if is_search_active {
            if let Some(history_index) = self.search_state.history_index {
                self.renderer.set_active_search(
                    self.search_state.history.get(history_index).cloned(),
                );
            }

            // Update search hints in renderable content
            let terminal = self.context_manager.current().terminal.lock();
            let hints = self
                .search_state
                .dfas_mut()
                .map(|dfas| HintMatches::visible_regex_matches(&terminal, dfas));
            drop(terminal);

            self.context_manager
                .current_mut()
                .renderable_content
                .hint_matches = hints.map(|h| h.iter().cloned().collect());

            // Force invalidation for search with full damage
            {
                let current = self.context_manager.current_mut();
                current
                    .renderable_content
                    .pending_update
                    .set_ui_damage(rio_backend::event::TerminalDamage::Full);
            }
        }

        // let renderer_run_start = std::time::Instant::now();
        self.renderer.run(
            &mut self.sugarloaf,
            &mut self.context_manager,
            &self.search_state.focused_match,
        );
        // In case the configuration of blinking cursor is enabled
        // and the terminal also have instructions of blinking enabled
        // TODO: enable blinking for selection after adding debounce (https://github.com/raphamorim/rio/issues/437)
        if self.renderer.config_has_blinking_enabled
            && self.selection_is_empty()
            && self
                .context_manager
                .current()
                .renderable_content
                .has_blinking_enabled
        {
            self.context_manager
                .blink_cursor(self.renderer.config_blinking_interval);
        }

        // let _screen_render_duration = screen_render_start.elapsed();
        // if self.renderer.enable_performance_logging {
        // println!(
        //     "[PERF] Screen render() total: {:?}\n",
        //     screen_render_duration
        // );
        // }
    }

    /// Update IME cursor position based on terminal cursor position
    /// This should be called after rendering to ensure cursor position is current
    pub fn update_ime_cursor_position_if_needed(
        &mut self,
        window: &rio_window::window::Window,
    ) {
        // Check if IME cursor positioning is enabled in config
        if !self.context_manager.config.keyboard.ime_cursor_positioning {
            return;
        }

        let current_context = self.context_manager.current();
        let terminal = current_context.terminal.lock();
        let cursor_pos = terminal.grid.cursor.pos;
        let layout = current_context.dimension;
        drop(terminal);

        // Calculate pixel position of cursor
        let cell_width = layout.dimension.width;
        let cell_height = layout.dimension.height;
        let margin_x = layout.margin.x * layout.dimension.scale;
        let margin_y = layout.margin.top_y * layout.dimension.scale;

        // Validate dimensions before calculation
        if cell_width <= 0.0 || cell_height <= 0.0 {
            tracing::warn!(
                "Invalid cell dimensions for IME cursor positioning: {}x{}",
                cell_width,
                cell_height
            );
            return;
        }

        // Convert grid position to pixel position, centering horizontally in the cell
        let pixel_x =
            margin_x + (cursor_pos.col.0 as f32 * cell_width) + (cell_width * 0.5);
        let pixel_y =
            margin_y + (cursor_pos.row.0 as f32 * cell_height * layout.line_height);

        // Validate final coordinates
        if pixel_x.is_nan() || pixel_y.is_nan() || pixel_x < 0.0 || pixel_y < 0.0 {
            tracing::warn!("Invalid IME cursor coordinates: ({}, {})", pixel_x, pixel_y);
            return;
        }

        // Check if position has changed significantly to avoid unnecessary updates
        if let Some((last_x, last_y)) = self.last_ime_cursor_pos {
            if (pixel_x - last_x).abs() < 1.0 && (pixel_y - last_y).abs() < 1.0 {
                return; // Position hasn't changed significantly
            }
        }

        // Update last position
        self.last_ime_cursor_pos = Some((pixel_x, pixel_y));

        // Set IME cursor area
        window.set_ime_cursor_area(
            rio_window::dpi::PhysicalPosition::new(pixel_x as f64, pixel_y as f64),
            rio_window::dpi::PhysicalSize::new(cell_width as f64, cell_height as f64),
        );
    }

    /// Process a new character for keyboard hints
    #[allow(dead_code)]
    pub fn hint_input(&mut self, c: char) {
        let terminal = self.context_manager.current().terminal.lock();
        if let Some(hint_match) = self.hint_state.keyboard_input(&*terminal, c) {
            drop(terminal);
            self.execute_hint_action(&hint_match);
            // Stop hint mode and update state with proper damage tracking
            self.hint_state.stop();
            self.update_hint_state();
        } else {
            drop(terminal);
            self.update_hint_state();
        }
        self.render();
    }

    /// Start hint mode with the given hint configuration
    pub fn start_hint_mode(
        &mut self,
        hint: std::rc::Rc<rio_backend::config::hints::Hint>,
    ) {
        self.hint_state.start(hint);
        let terminal = self.context_manager.current().terminal.lock();
        self.hint_state.update_matches(&*terminal);
        drop(terminal);

        // Update hint state and trigger damage tracking
        self.update_hint_state();

        self.render();
    }

    /// Execute the action for a selected hint
    fn execute_hint_action(&mut self, hint_match: &crate::hints::HintMatch) {
        use rio_backend::config::hints::{HintAction, HintCommand, HintInternalAction};

        match &hint_match.hint.action {
            HintAction::Action { action } => match action {
                HintInternalAction::Copy => {
                    self.clipboard
                        .borrow_mut()
                        .set(ClipboardType::Clipboard, hint_match.text.clone());
                }
                HintInternalAction::Paste => {
                    self.paste(&hint_match.text, true);
                }
                HintInternalAction::Select => {
                    // Set selection to the hint match
                    let selection = rio_backend::selection::SelectionRange::new(
                        hint_match.start,
                        hint_match.end,
                        false, // not a block selection
                    );
                    self.context_manager
                        .current_mut()
                        .set_selection(Some(selection));
                    self.render();
                }
                HintInternalAction::MoveViModeCursor => {
                    // Move vi mode cursor to hint position
                    let mut terminal = self.context_manager.current().terminal.lock();
                    terminal.vi_mode_cursor.pos = hint_match.start;
                    drop(terminal);
                    self.render();
                }
            },
            HintAction::Command { command } => match command {
                HintCommand::Simple(program) => {
                    self.exec(program, [&hint_match.text]);
                }
                HintCommand::WithArgs { program, args } => {
                    let mut all_args = args.clone();
                    all_args.push(hint_match.text.clone());
                    self.exec(program, &all_args);
                }
            },
        }
    }

    /// Update hint state and trigger appropriate damage tracking
    pub fn update_hint_state(&mut self) {
        use rio_backend::event::TerminalDamage;

        if self.hint_state.is_active() {
            // Update hint labels
            self.update_hint_labels();

            // Update hint matches in renderable content
            let matches: Vec<rio_backend::crosswords::search::Match> = self
                .hint_state
                .matches()
                .iter()
                .map(|hint_match| hint_match.start..=hint_match.end)
                .collect();
            self.context_manager
                .current_mut()
                .renderable_content
                .hint_matches = Some(matches);

            // Mark lines with hint labels as damaged
            let mut damaged_lines = std::collections::BTreeSet::new();
            {
                let current = &self.context_manager.current();
                let hint_labels = &current.renderable_content.hint_labels;
                let terminal = current.terminal.lock();
                let display_offset = terminal.display_offset();
                let screen_lines = terminal.screen_lines();
                drop(terminal);

                if !hint_labels.is_empty() {
                    // Collect all lines that have hint labels
                    for label in hint_labels {
                        let line = label.position.row.0 - display_offset as i32;
                        if line >= 0 && (line as usize) < screen_lines {
                            damaged_lines.insert(
                                rio_backend::crosswords::LineDamage::new(
                                    line as usize,
                                    true,
                                ),
                            );
                        }
                    }
                }

                // Also damage lines with hint matches
                if let Some(hint_matches) = &current.renderable_content.hint_matches {
                    for hint_match in hint_matches {
                        let start_line = hint_match.start().row.0 - display_offset as i32;
                        let end_line = hint_match.end().row.0 - display_offset as i32;

                        for line in start_line..=end_line {
                            if line >= 0 && (line as usize) < screen_lines {
                                damaged_lines.insert(
                                    rio_backend::crosswords::LineDamage::new(
                                        line as usize,
                                        true,
                                    ),
                                );
                            }
                        }
                    }
                }
            }

            let current = self.context_manager.current_mut();
            if !damaged_lines.is_empty() {
                current
                    .renderable_content
                    .pending_update
                    .set_ui_damage(TerminalDamage::Partial(damaged_lines));
            } else {
                // Force full damage if no specific lines (for hint highlights)
                current
                    .renderable_content
                    .pending_update
                    .set_ui_damage(TerminalDamage::Full);
            }
        } else {
            // Clear hint state
            self.context_manager
                .current_mut()
                .renderable_content
                .hint_matches = None;
            self.context_manager
                .current_mut()
                .renderable_content
                .hint_labels
                .clear();
            // Force full damage to clear all hint highlights
            let current = self.context_manager.current_mut();
            current
                .renderable_content
                .pending_update
                .set_ui_damage(TerminalDamage::Full);
        }
    }

    fn update_hint_labels(&mut self) {
        use crate::context::renderable::HintLabel;

        let mut hint_labels = Vec::new();

        if self.hint_state.is_active() {
            let matches = self.hint_state.matches();
            let visible_labels = self.hint_state.visible_labels();

            for (match_index, remaining_label) in visible_labels {
                if let Some(hint_match) = matches.get(match_index) {
                    // Create labels for each character in the hint label
                    for (char_index, &label_char) in remaining_label.iter().enumerate() {
                        let position = rio_backend::crosswords::pos::Pos::new(
                            hint_match.start.row,
                            hint_match.start.col + char_index,
                        );

                        hint_labels.push(HintLabel {
                            position,
                            label: vec![label_char],
                            is_first: char_index == 0, // First character gets different styling
                        });
                    }
                }
            }
        }

        self.context_manager
            .current_mut()
            .renderable_content
            .hint_labels = hint_labels;
    }

    /// Apply grid-based hint post-processing.
    ///
    /// This iterates through the terminal grid character by character and adjusts
    /// the match bounds based on bracket balance and trailing delimiters.
    fn hint_post_processing(
        &self,
        terminal: &rio_backend::crosswords::Crosswords<EventProxy>,
        start_col: rio_backend::crosswords::pos::Column,
        end_col: rio_backend::crosswords::pos::Column,
        row: rio_backend::crosswords::pos::Line,
    ) -> Option<(
        rio_backend::crosswords::pos::Column,
        rio_backend::crosswords::pos::Column,
    )> {
        use rio_backend::crosswords::grid::BidirectionalIterator;

        let grid = &terminal.grid;
        let start_pos = rio_backend::crosswords::pos::Pos::new(row, start_col);
        let end_pos = rio_backend::crosswords::pos::Pos::new(row, end_col);

        let mut iter = grid.iter_from(start_pos);
        let mut current_pos = start_pos;
        let mut open_parents = 0;
        let mut open_brackets = 0;

        // First pass: handle uneven brackets/parentheses
        while current_pos <= end_pos {
            if let Some(indexed) = iter.next() {
                let c = indexed.square.c;
                current_pos = indexed.pos;

                match c {
                    '(' => open_parents += 1,
                    '[' => open_brackets += 1,
                    ')' => {
                        if open_parents == 0 {
                            // Unmatched closing parenthesis, truncate here
                            if iter.prev().is_some() {
                                return Some((start_col, iter.pos().col));
                            }
                            break;
                        } else {
                            open_parents -= 1;
                        }
                    }
                    ']' => {
                        if open_brackets == 0 {
                            // Unmatched closing bracket, truncate here
                            if iter.prev().is_some() {
                                return Some((start_col, iter.pos().col));
                            }
                            break;
                        } else {
                            open_brackets -= 1;
                        }
                    }
                    _ => (),
                }

                if current_pos == end_pos {
                    break;
                }
            } else {
                break;
            }
        }

        // Second pass: remove trailing delimiters
        let mut final_end = end_pos;
        let mut iter = grid.iter_from(end_pos);

        while final_end > start_pos {
            if let Some(indexed) = iter.next() {
                let c = indexed.square.c;
                if !matches!(c, '.' | ',' | ':' | ';' | '?' | '!' | '(' | '[' | '\'') {
                    break;
                }

                if let Some(prev_indexed) = iter.prev() {
                    final_end = prev_indexed.pos;
                    if iter.prev().is_some() {
                        // Move iterator back one more position for next iteration
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        Some((start_col, final_end.col))
    }
}

/// Apply post-processing to hyperlink URIs to remove trailing delimiters and handle uneven brackets.
fn post_process_hyperlink_uri(uri: &str) -> String {
    let chars: Vec<char> = uri.chars().collect();
    if chars.is_empty() {
        return String::new();
    }

    let mut end_idx = chars.len() - 1;
    let mut open_parents = 0;
    let mut open_brackets = 0;

    // First pass: handle uneven brackets/parentheses
    for (i, &c) in chars.iter().enumerate() {
        match c {
            '(' => open_parents += 1,
            '[' => open_brackets += 1,
            ')' => {
                if open_parents == 0 {
                    // Unmatched closing parenthesis, truncate here
                    end_idx = i.saturating_sub(1);
                    break;
                } else {
                    open_parents -= 1;
                }
            }
            ']' => {
                if open_brackets == 0 {
                    // Unmatched closing bracket, truncate here
                    end_idx = i.saturating_sub(1);
                    break;
                } else {
                    open_brackets -= 1;
                }
            }
            _ => (),
        }
    }

    // Second pass: remove trailing delimiters
    while end_idx > 0 {
        match chars[end_idx] {
            '.' | ',' | ':' | ';' | '?' | '!' | '(' | '[' | '\'' => {
                end_idx = end_idx.saturating_sub(1);
            }
            _ => break,
        }
    }

    chars.into_iter().take(end_idx + 1).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_post_process_hyperlink_uri() {
        // Test removing trailing parenthesis
        assert_eq!(
            post_process_hyperlink_uri("https://example.com)"),
            "https://example.com"
        );

        // Test removing trailing comma
        assert_eq!(
            post_process_hyperlink_uri("https://example.com,"),
            "https://example.com"
        );

        // Test removing trailing period
        assert_eq!(
            post_process_hyperlink_uri("https://example.com."),
            "https://example.com"
        );

        // Test handling balanced parentheses (should keep them)
        assert_eq!(
            post_process_hyperlink_uri("https://example.com/path(with)parens"),
            "https://example.com/path(with)parens"
        );

        // Test handling unbalanced parentheses
        assert_eq!(
            post_process_hyperlink_uri("https://example.com/path)"),
            "https://example.com/path"
        );

        // Test handling multiple trailing delimiters
        assert_eq!(
            post_process_hyperlink_uri("https://example.com.'),"),
            "https://example.com"
        );

        // Test markdown-style URLs
        assert_eq!(
            post_process_hyperlink_uri("https://example.com)"),
            "https://example.com"
        );

        // Test handling unbalanced brackets
        assert_eq!(
            post_process_hyperlink_uri("https://example.com/path]"),
            "https://example.com/path"
        );

        // Test balanced brackets (should keep them)
        assert_eq!(
            post_process_hyperlink_uri("https://example.com/path[with]brackets"),
            "https://example.com/path[with]brackets"
        );
    }
}
