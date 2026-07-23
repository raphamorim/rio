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
use crate::context;
use crate::context::renderable::{Cursor, RenderableContent};
use crate::context::{next_rich_text_id, process_open_url, ContextManager};
use crate::crosswords::{
    grid::{Dimensions, Scroll},
    pos::{Column, Pos, Side},
    square::Hyperlink,
    vi_mode::ViMotion,
    Mode,
};
use crate::hints::HintState;
use crate::layout::ContextDimension;
use crate::mouse::{calculate_mouse_position, Mouse};
use crate::renderer::island::{self, TabStripLayout, ISLAND_HEIGHT};
use crate::renderer::{utils::padding_top_from_config, Renderer};
use crate::screen::hint::HintMatches;
use crate::selection::{Selection, SelectionType};
use core::fmt::Debug;
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
use rio_backend::clipboard::Clipboard;
use rio_backend::clipboard::ClipboardType;
use rio_backend::config::layout::Margin;
use rio_backend::config::renderer::Backend;
use rio_backend::crosswords::pos::{Boundary, CursorState, Direction, Line};
use rio_backend::crosswords::search::RegexSearch;
use rio_backend::error::{RioError, RioErrorLevel, RioErrorType};
use rio_backend::event::{ClickState, EventProxy, SearchState};
use rio_backend::sugarloaf::{
    layout::RootStyle, Sugarloaf, SugarloafBackend, SugarloafErrors, SugarloafRenderer,
    SugarloafWindow, SugarloafWindowSize,
};
use rio_window::event::ElementState;
use rio_window::event::Modifiers;
use rio_window::event::MouseButton;
#[cfg(target_os = "macos")]
use rio_window::keyboard::ModifiersKeyState;
use rio_window::keyboard::{Key, KeyLocation, ModifiersState, NamedKey};
use rio_window::platform::modifier_supplement::KeyEventExtModifierSupplement;
use std::error::Error;
use std::ffi::OsStr;
use touch::TouchPurpose;

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
    pub triggers: crate::triggers::Triggers,
    pub renderer: Renderer,
    pub sugarloaf: Sugarloaf<'screen>,
    pub context_manager: context::ContextManager<EventProxy>,
    last_ime_cursor_pos: Option<(f32, f32)>,
    hints_config: Vec<std::rc::Rc<rio_backend::config::hints::Hint>>,
    pub resize_state: Option<crate::layout::ResizeState>,
    #[cfg(target_os = "macos")]
    pub allow_manual_dragging: bool,
    last_chrome_press: Option<ChromePress>,
    last_close_press: Option<(std::time::Instant, f32)>,
    pub grids: rustc_hash::FxHashMap<usize, rio_backend::sugarloaf::grid::GridRenderer>,
    pub grid_rasterizer: crate::grid_emit::GridGlyphRasterizer,
}

pub struct ChromePress {
    window_origin: Option<rio_window::dpi::PhysicalPosition<i32>>,
    at: std::time::Instant,
}

impl ChromePress {
    fn validates_double_click(
        &self,
        window_origin: Option<rio_window::dpi::PhysicalPosition<i32>>,
    ) -> bool {
        self.at.elapsed() <= crate::constants::MULTI_CLICK_THRESHOLD
            && self.window_origin == window_origin
    }
}

pub struct ScreenWindowProperties {
    pub size: rio_window::dpi::PhysicalSize<u32>,
    pub scale: f64,
    pub raw_window_handle: RawWindowHandle,
    pub raw_display_handle: RawDisplayHandle,
    pub window_id: rio_window::window::WindowId,
}

#[inline]
fn window_should_be_opaque(config: &rio_backend::config::Config) -> bool {
    config.window.opacity >= 1.0 && !config.window.blur.is_glass()
}

impl Screen<'_> {
    pub fn new<'screen>(
        window_properties: ScreenWindowProperties,
        config: &rio_backend::config::Config,
        event_proxy: EventProxy,
        font_library: &rio_backend::sugarloaf::font::FontLibrary,
        open_url: Option<String>,
    ) -> Result<Screen<'screen>, Box<dyn Error>> {
        let size = window_properties.size;
        let scale = window_properties.scale;
        let raw_window_handle = window_properties.raw_window_handle;
        let raw_display_handle = window_properties.raw_display_handle;
        let window_id = window_properties.window_id;

        let padding_y_top = padding_top_from_config(
            &config.navigation,
            config.margin.top,
            1,
            config.window.macos_use_unified_titlebar,
        );

        let padding_y_bottom = config.margin.bottom;
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

        let backend = if config.renderer.use_cpu {
            SugarloafBackend::Cpu
        } else {
            match config.renderer.backend {
                // `Backend::Vulkan` from the user config means the
                // native ash backend on Linux. Other OSes fall through
                // to the wgpu Vulkan path when the `wgpu` feature is
                // on; otherwise we degrade to CPU rasterizer.
                #[cfg(target_os = "linux")]
                Backend::Vulkan => SugarloafBackend::Vulkan,
                #[cfg(all(not(target_os = "linux"), feature = "wgpu"))]
                Backend::Vulkan => SugarloafBackend::Wgpu(wgpu::Backends::VULKAN),
                #[cfg(all(not(target_os = "linux"), not(feature = "wgpu")))]
                Backend::Vulkan => SugarloafBackend::Cpu,
                #[cfg(target_os = "macos")]
                Backend::Metal => SugarloafBackend::Metal,
                #[cfg(all(feature = "wgpu", target_arch = "wasm32"))]
                Backend::Webgpu => SugarloafBackend::Wgpu(
                    wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL,
                ),
                #[cfg(all(feature = "wgpu", not(target_arch = "wasm32")))]
                Backend::Webgpu => SugarloafBackend::Wgpu(wgpu::Backends::all()),
                #[cfg(not(feature = "wgpu"))]
                Backend::Webgpu => SugarloafBackend::Cpu,
            }
        };

        let sugarloaf_renderer = SugarloafRenderer {
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

        #[cfg(feature = "wgpu")]
        sugarloaf.update_filters(config.renderer.filters.as_slice());

        let mut renderer = Renderer::new(config);

        let bindings = crate::bindings::default_key_bindings(config);

        let is_native = config.navigation.is_native();

        let (shell, working_dir) = process_open_url(
            config.shell.to_owned(),
            config.working_dir.to_owned(),
            config.editor.to_owned(),
            open_url.as_deref(),
        );

        let context_manager_config = context::ContextManagerConfig {
            #[cfg(test)]
            dead_pty: false,
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
            split_active_color: config.colors.split_active,
            panel: config.panel,
            title: config.title.clone(),
            keyboard: config.keyboard,
            scrollback_history_limit: config.scrollback_history_limit,
        };

        let rich_text_id = next_rich_text_id();
        let margin = Margin::new(
            padding_y_top,
            config.margin.right,
            padding_y_bottom,
            config.margin.left,
        );
        let scaled_margin = Margin::new(
            padding_y_top * scale as f32,
            config.margin.right * scale as f32,
            padding_y_bottom * scale as f32,
            config.margin.left * scale as f32,
        );
        let (text_dimensions, cell_metrics) = sugarloaf.compute_cell_metrics(
            config.fonts.size,
            config.line_height,
            scale as f32,
        );
        let context_dimension = ContextDimension::build(
            size.width as f32,
            size.height as f32,
            text_dimensions,
            cell_metrics,
            config.line_height,
            config.fonts.size,
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
            scaled_margin,
            sugarloaf_errors,
        )?;

        sugarloaf.set_window_opaque(window_should_be_opaque(config));
        sugarloaf.set_background_color(Some(renderer.dynamic_background.1));

        if let Some(image) = &config.window.background_image {
            if let Err(message) = sugarloaf.set_background_image(image) {
                renderer.assistant.set_error(RioError {
                    level: RioErrorLevel::Warning,
                    report: RioErrorType::BackgroundImageLoadFailure(message),
                });
            }
        } else {
            sugarloaf.clear_background_image();
        }

        Ok(Screen {
            search_state: SearchState::default(),
            hint_state: HintState::new(config.hints.alphabet.clone()),
            triggers: crate::triggers::Triggers::new(&config.triggers),
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
            last_ime_cursor_pos: None,
            resize_state: None,
            #[cfg(target_os = "macos")]
            allow_manual_dragging: config.navigation.is_enabled(),
            last_chrome_press: None,
            last_close_press: None,
            grids: rustc_hash::FxHashMap::default(),
            grid_rasterizer: crate::grid_emit::GridGlyphRasterizer::new(),
        })
    }

    #[inline]
    pub fn ensure_grid(&mut self, route_id: usize, cols: u32, rows: u32) {
        use std::collections::hash_map::Entry;
        match self.grids.entry(route_id) {
            Entry::Occupied(mut e) => e.get_mut().resize(cols, rows),
            Entry::Vacant(e) => {
                e.insert(rio_backend::sugarloaf::grid::GridRenderer::new(
                    &self.sugarloaf.ctx,
                    cols,
                    rows,
                ));
            }
        }
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
    pub fn mark_dirty(&mut self) {
        self.context_manager
            .current_mut()
            .renderable_content
            .pending_update
            .set_dirty();
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
    pub fn select_current_based_on_mouse(&mut self) -> bool {
        if self
            .context_manager
            .current_grid_mut()
            .select_current_based_on_mouse(&self.mouse)
        {
            self.context_manager.select_route_from_current_grid();
            // The focusing click never reaches on_left_click, so a
            // selection left behind in the target panel would
            // drag-extend from its stale anchor; drop it on switch.
            self.clear_selection();
            return true;
        }
        false
    }

    #[inline]
    pub fn mouse_position(&self, display_offset: usize) -> Pos {
        let current_grid = self.context_manager.current_grid();
        let (context, margin) = current_grid.current_context_with_computed_dimension();
        let context_dimension = context.dimension;
        calculate_mouse_position(
            &self.mouse,
            display_offset,
            (context_dimension.columns, context_dimension.lines),
            margin.left,
            margin.top,
            (
                context_dimension.cell.cell_width,
                context_dimension.cell.cell_height,
            ),
        )
    }

    #[inline]
    pub fn touch_purpose(&mut self) -> &mut TouchPurpose {
        &mut self.touchpurpose
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
            config.margin.top,
            num_tabs,
            config.window.macos_use_unified_titlebar,
        );
        let padding_y_bottom = config.margin.bottom;

        self.triggers.rebuild(&config.triggers);

        if should_update_font_library {
            self.sugarloaf.update_font(font_library);
            // Caches keyed by font_id would serve the old font's data.
            self.grid_rasterizer.clear_font_caches();
            for grid in self.grids.values_mut() {
                grid.clear_atlas();
            }
        }
        let s = self.sugarloaf.style_mut();
        s.font_size = config.fonts.size;
        s.line_height = config.line_height;

        #[cfg(feature = "wgpu")]
        self.sugarloaf
            .update_filters(config.renderer.filters.as_slice());

        // Rebuild bindings so `[bindings]` edits live-reload like the
        // rest of the config instead of waiting for a new window.
        self.bindings = crate::bindings::default_key_bindings(config);

        // Preserve existing Island (tab state) and update its colors
        let old_island = self.renderer.island.take();
        let was_focused = self.renderer.is_window_focused;
        self.renderer = Renderer::new(config);
        self.renderer.is_window_focused = was_focused;
        if let Some(mut island) = old_island {
            island.update_colors(config.colors.tabs, config.colors.tabs_active);
            island.max_tab_width = config.navigation.max_tab_width;
            self.renderer.island = Some(island);
        }

        let scale = self.sugarloaf.scale_factor();
        for context_grid in self.context_manager.contexts_mut() {
            context_grid.update_line_height(config.line_height);

            context_grid.update_scaled_margin(Margin::new(
                padding_y_top * scale,
                config.margin.right * scale,
                padding_y_bottom * scale,
                config.margin.left * scale,
            ));

            // Update per-panel font size and line height BEFORE
            // update_dimensions — the recompute reads from these
            // fields. `rebaseline_font_size` also re-anchors the
            // "reset" target so the next change_font_size(Reset)
            // returns to the new config size.
            for current_context in context_grid.contexts_mut().values_mut() {
                let current_context = current_context.context_mut();
                current_context
                    .dimension
                    .rebaseline_font_size(config.fonts.size);
                current_context.dimension.line_height = config.line_height;
            }

            context_grid.update_dimensions(&mut self.sugarloaf);

            for current_context in context_grid.contexts_mut().values_mut() {
                let current_context = current_context.context_mut();
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

        // Re-evaluate the opaque flag — toggling `window.opacity` /
        // `window.blur` at runtime should flip the compositor mode.
        self.sugarloaf
            .set_window_opaque(window_should_be_opaque(config));

        self.sugarloaf
            .set_background_color(Some(self.renderer.dynamic_background.1));

        if let Some(image) = &config.window.background_image {
            if let Err(message) = self.sugarloaf.set_background_image(image) {
                self.renderer.assistant.set_error(RioError {
                    level: RioErrorLevel::Warning,
                    report: RioErrorType::BackgroundImageLoadFailure(message),
                });
            }
        } else {
            self.sugarloaf.clear_background_image();
        }

        self.resize_all_contexts();
    }

    #[inline]
    pub fn change_font_size(&mut self, action: FontSizeAction) {
        let dim = &mut self.context_manager.current_mut().dimension;
        let changed = match action {
            FontSizeAction::Increase => dim.increase_font_size(),
            FontSizeAction::Decrease => dim.decrease_font_size(),
            FontSizeAction::Reset => dim.reset_font_size(),
        };
        if !changed {
            return;
        }

        self.context_manager
            .current_grid_mut()
            .update_dimensions(&mut self.sugarloaf);

        self.mark_dirty();
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

        self.context_manager
            .resize_all_grids(width, height, &mut self.sugarloaf);

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

        for context_grid in self.context_manager.contexts_mut() {
            let old_scale = context_grid.current().dimension.dimension.scale.max(1.0);
            let scaled_margin = context_grid.scaled_margin;
            let unscaled_margin = Margin::new(
                scaled_margin.top / old_scale,
                scaled_margin.right / old_scale,
                scaled_margin.bottom / old_scale,
                scaled_margin.left / old_scale,
            );

            context_grid.update_scaled_margin(Margin::new(
                unscaled_margin.top * new_scale,
                unscaled_margin.right * new_scale,
                unscaled_margin.bottom * new_scale,
                unscaled_margin.left * new_scale,
            ));

            for context in context_grid.contexts_mut().values_mut() {
                context.context_mut().dimension.update_scale(new_scale);
            }

            context_grid.update_dimensions(&mut self.sugarloaf);
        }

        let width = new_size.width as f32;
        let height = new_size.height as f32;

        self.context_manager
            .resize_all_grids(width, height, &mut self.sugarloaf);
        self.mark_dirty();

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
    pub fn process_key_event(
        &mut self,
        key: &rio_window::event::KeyEvent,
        clipboard: &mut Clipboard,
    ) {
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
                    self.mark_dirty();
                    return;
                }
                rio_window::keyboard::Key::Named(
                    rio_window::keyboard::NamedKey::Backspace,
                ) => {
                    let terminal = self.context_manager.current().terminal.lock();
                    self.hint_state.keyboard_input(&*terminal, '\x08');
                    drop(terminal);
                    self.update_hint_state();
                    self.mark_dirty();
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
                    self.execute_hint_action(&hint_match, clipboard);
                    // Stop hint mode and update state with proper damage tracking
                    self.hint_state.stop();
                    self.update_hint_state();
                    self.mark_dirty();
                    return;
                }
                drop(terminal);
            }
            self.update_hint_state();
            self.mark_dirty();
            return;
        }

        let ignore_chars = self.process_key_bindings(key, &mode, mods, clipboard);
        if ignore_chars {
            return;
        }

        let text = key.text_with_all_modifiers().unwrap_or_default();

        if self.search_active() {
            for character in text.chars() {
                self.search_input(character);
            }

            self.mark_dirty();
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

        // Legacy ctrl encoding runs before trusting the platform text:
        // the OS is inconsistent about synthesizing C0 characters for
        // combos like ctrl+6 or ctrl+/ (macOS reports the plain char,
        // Windows reports nothing), so the byte is computed from the
        // kitty C0 table directly. Gated on the exact flag set that
        // makes `build_key_sequence` produce CSI u (`kitty_seq`), so
        // kitty-protocol encoding is untouched in every mode where it
        // applies.
        let kitty_seq = mode.intersects(
            Mode::REPORT_ALL_KEYS_AS_ESC
                | Mode::DISAMBIGUATE_ESC_CODES
                | Mode::REPORT_EVENT_TYPES,
        );
        let ctrl_c0 = if kitty_seq {
            None
        } else {
            crate::bindings::ctrl_seq(&key.logical_key, text, mods)
        };

        let bytes = if let Some(c0) = ctrl_c0 {
            if mods.alt_key() {
                vec![b'\x1b', c0]
            } else {
                vec![c0]
            }
        } else if build_key_sequence {
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
    pub fn process_mouse_bindings(
        &mut self,
        button: MouseButton,
        clipboard: &mut Clipboard,
    ) {
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
                let content = clipboard.get(ClipboardType::Selection);
                self.paste(&content, true);
            }
        }
    }

    pub fn process_key_bindings(
        &mut self,
        key: &rio_window::event::KeyEvent,
        mode: &Mode,
        mods: ModifiersState,
        clipboard: &mut Clipboard,
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
                // && mods.alt_key()
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
                        let content = clipboard.get(ClipboardType::Clipboard);
                        self.paste(&content, true);
                    }
                    Act::ClearSelection => {
                        self.clear_selection();
                    }
                    Act::PasteSelection => {
                        let content = clipboard.get(ClipboardType::Selection);
                        self.paste(&content, true);
                    }
                    Act::Copy => {
                        self.copy_selection(ClipboardType::Clipboard, clipboard);
                    }
                    Act::SelectAll => {
                        self.select_all();
                    }
                    Act::Hint(hint_config) => {
                        self.start_hint_mode(hint_config.clone());
                    }
                    Act::SearchForward => {
                        self.start_search(Direction::Right);
                        self.resize_top_or_bottom_line(self.ctx().len());
                        self.mark_dirty();
                    }
                    Act::SearchBackward => {
                        self.start_search(Direction::Left);
                        self.resize_top_or_bottom_line(self.ctx().len());
                        self.mark_dirty();
                    }
                    Act::Search(SearchAction::SearchConfirm) => {
                        self.confirm_search(clipboard);
                        self.resize_top_or_bottom_line(self.ctx().len());
                        self.mark_dirty();
                    }
                    Act::Search(SearchAction::SearchCancel) => {
                        self.cancel_search(clipboard);
                        self.resize_top_or_bottom_line(self.ctx().len());
                        self.mark_dirty();
                    }
                    Act::Search(SearchAction::SearchClear) => {
                        let direction = self.search_state.direction;
                        self.cancel_search(clipboard);
                        self.start_search(direction);
                        self.resize_top_or_bottom_line(self.ctx().len());
                        self.mark_dirty();
                    }
                    Act::Search(SearchAction::SearchFocusNext) => {
                        self.advance_search_origin(self.search_state.direction);
                        self.resize_top_or_bottom_line(self.ctx().len());
                        self.mark_dirty();
                    }
                    Act::Search(SearchAction::SearchFocusPrevious) => {
                        let direction = self.search_state.direction.opposite();
                        self.advance_search_origin(direction);
                        self.resize_top_or_bottom_line(self.ctx().len());
                        self.mark_dirty();
                    }
                    Act::Search(SearchAction::SearchDeleteWord) => {
                        self.search_pop_word();
                        self.mark_dirty();
                    }
                    Act::Search(SearchAction::SearchHistoryPrevious) => {
                        self.search_history_previous();
                        self.mark_dirty();
                    }
                    Act::Search(SearchAction::SearchHistoryNext) => {
                        self.search_history_next();
                        self.mark_dirty();
                    }
                    Act::ToggleViMode => {
                        let context = self.context_manager.current_mut();
                        let mut terminal = context.terminal.lock();
                        terminal.toggle_vi_mode();
                        let has_vi_mode_enabled = terminal.mode().contains(Mode::VI);
                        drop(terminal);
                        context
                            .renderable_content
                            .pending_update
                            .set_terminal_damage(
                                rio_backend::event::TerminalDamage::Full,
                            );
                        self.renderer.set_vi_mode(has_vi_mode_enabled);
                        self.mark_dirty();
                    }
                    Act::ViMotion(motion) => {
                        let context = self.context_manager.current_mut();
                        let mut terminal = context.terminal.lock();
                        if terminal.mode().contains(Mode::VI) {
                            terminal.vi_motion(*motion);
                        }

                        if let Some(selection) = &terminal.selection {
                            context.renderable_content.selection_range =
                                selection.to_range(&terminal);
                        };
                        drop(terminal);
                        context
                            .renderable_content
                            .pending_update
                            .set_terminal_damage(
                                rio_backend::event::TerminalDamage::Full,
                            );
                        self.mark_dirty();
                    }
                    Act::Vi(ViAction::CenterAroundViCursor) => {
                        let context = self.context_manager.current_mut();
                        let mut terminal = context.terminal.lock();
                        let display_offset = terminal.display_offset() as i32;
                        let target =
                            -display_offset + terminal.grid.screen_lines() as i32 / 2 - 1;
                        let line = terminal.vi_mode_cursor.pos.row;
                        let scroll_lines = target - line.0;

                        terminal.scroll_display(Scroll::Delta(scroll_lines));
                        drop(terminal);
                        context
                            .renderable_content
                            .pending_update
                            .set_terminal_damage(
                                rio_backend::event::TerminalDamage::Full,
                            );
                        self.mark_dirty();
                    }
                    Act::Vi(ViAction::ToggleNormalSelection) => {
                        self.toggle_selection(
                            SelectionType::Simple,
                            Side::Left,
                            clipboard,
                        );
                        self.context_manager
                            .current_mut()
                            .renderable_content
                            .pending_update
                            .set_terminal_damage(
                                rio_backend::event::TerminalDamage::Full,
                            );
                        self.mark_dirty();
                    }
                    Act::Vi(ViAction::ToggleLineSelection) => {
                        self.toggle_selection(
                            SelectionType::Lines,
                            Side::Left,
                            clipboard,
                        );
                        self.context_manager
                            .current_mut()
                            .renderable_content
                            .pending_update
                            .set_terminal_damage(
                                rio_backend::event::TerminalDamage::Full,
                            );
                        self.mark_dirty();
                    }
                    Act::Vi(ViAction::ToggleBlockSelection) => {
                        self.toggle_selection(
                            SelectionType::Block,
                            Side::Left,
                            clipboard,
                        );
                        self.context_manager
                            .current_mut()
                            .renderable_content
                            .pending_update
                            .set_terminal_damage(
                                rio_backend::event::TerminalDamage::Full,
                            );
                        self.mark_dirty();
                    }
                    Act::Vi(ViAction::ToggleSemanticSelection) => {
                        self.toggle_selection(
                            SelectionType::Semantic,
                            Side::Left,
                            clipboard,
                        );
                        self.context_manager
                            .current_mut()
                            .renderable_content
                            .pending_update
                            .set_terminal_damage(
                                rio_backend::event::TerminalDamage::Full,
                            );
                        self.mark_dirty();
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
                    Act::ToggleQuake => {
                        self.context_manager.toggle_quake();
                    }
                    Act::CloseCurrentSplitOrTab => {
                        self.close_split_or_tab(clipboard);
                    }
                    Act::TabCreateNew => {
                        self.create_tab(clipboard);
                    }
                    Act::TabCloseCurrent => {
                        self.close_tab(clipboard);
                    }
                    Act::TabCloseUnfocused => {
                        self.clear_selection();
                        self.cancel_search(clipboard);
                        if self.ctx().len() <= 1 {
                            return true;
                        }
                        self.context_manager.close_unfocused_tabs();
                        if let Some(ref mut island) = self.renderer.island {
                            island.dismiss_color_picker();
                        }
                        self.resize_top_or_bottom_line(1);
                        self.mark_dirty();
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
                    Act::ScrollToPrevPrompt => {
                        let current = self.context_manager.current_mut();
                        let rtid = current.rich_text_id;
                        let mut terminal = current.terminal.lock();
                        terminal.scroll_to_prompt(false);
                        drop(terminal);
                        self.renderer.scrollbar.notify_scroll(rtid);
                        self.mark_dirty();
                    }
                    Act::ScrollToNextPrompt => {
                        let current = self.context_manager.current_mut();
                        let rtid = current.rich_text_id;
                        let mut terminal = current.terminal.lock();
                        terminal.scroll_to_prompt(true);
                        drop(terminal);
                        self.renderer.scrollbar.notify_scroll(rtid);
                        self.mark_dirty();
                    }
                    Act::ScrollPageUp => {
                        // Move vi mode cursor.
                        let current = self.context_manager.current_mut();
                        let rtid = current.rich_text_id;
                        let mut terminal = current.terminal.lock();
                        let scroll_lines = terminal.grid.screen_lines() as i32;
                        terminal.vi_mode_cursor =
                            terminal.vi_mode_cursor.scroll(&terminal, scroll_lines);
                        terminal.scroll_display(Scroll::PageUp);
                        drop(terminal);
                        self.renderer.scrollbar.notify_scroll(rtid);
                        self.mark_dirty();
                    }
                    Act::ScrollPageDown => {
                        // Move vi mode cursor.
                        let current = self.context_manager.current_mut();
                        let rtid = current.rich_text_id;
                        let mut terminal = current.terminal.lock();
                        let scroll_lines = -(terminal.grid.screen_lines() as i32);

                        terminal.vi_mode_cursor =
                            terminal.vi_mode_cursor.scroll(&terminal, scroll_lines);

                        terminal.scroll_display(Scroll::PageDown);
                        drop(terminal);
                        self.renderer.scrollbar.notify_scroll(rtid);
                        self.mark_dirty();
                    }
                    Act::ScrollHalfPageUp => {
                        // Move vi mode cursor.
                        let current = self.context_manager.current_mut();
                        let rtid = current.rich_text_id;
                        let mut terminal = current.terminal.lock();
                        let scroll_lines = terminal.grid.screen_lines() as i32 / 2;

                        terminal.vi_mode_cursor =
                            terminal.vi_mode_cursor.scroll(&terminal, scroll_lines);

                        terminal.scroll_display(Scroll::Delta(scroll_lines));
                        drop(terminal);
                        self.renderer.scrollbar.notify_scroll(rtid);
                        self.mark_dirty();
                    }
                    Act::ScrollHalfPageDown => {
                        // Move vi mode cursor.
                        let current = self.context_manager.current_mut();
                        let rtid = current.rich_text_id;
                        let mut terminal = current.terminal.lock();
                        let scroll_lines = -(terminal.grid.screen_lines() as i32 / 2);

                        terminal.vi_mode_cursor =
                            terminal.vi_mode_cursor.scroll(&terminal, scroll_lines);

                        terminal.scroll_display(Scroll::Delta(scroll_lines));
                        drop(terminal);
                        self.renderer.scrollbar.notify_scroll(rtid);
                        self.mark_dirty();
                    }
                    Act::ScrollToTop => {
                        let current = self.context_manager.current_mut();
                        let rtid = current.rich_text_id;
                        let mut terminal = current.terminal.lock();
                        terminal.scroll_display(Scroll::Top);

                        let topmost_line = terminal.grid.topmost_line();
                        terminal.vi_mode_cursor.pos.row = topmost_line;
                        terminal.vi_motion(ViMotion::FirstOccupied);
                        drop(terminal);
                        self.renderer.scrollbar.notify_scroll(rtid);
                        self.mark_dirty();
                    }
                    Act::ScrollToBottom => {
                        let current = self.context_manager.current_mut();
                        let rtid = current.rich_text_id;
                        let mut terminal = current.terminal.lock();
                        terminal.scroll_display(Scroll::Bottom);

                        // Move vi mode cursor.
                        terminal.vi_mode_cursor.pos.row = terminal.grid.bottommost_line();

                        // Move to beginning twice, to always jump across linewraps.
                        terminal.vi_motion(ViMotion::FirstOccupied);
                        terminal.vi_motion(ViMotion::FirstOccupied);
                        drop(terminal);
                        self.renderer.scrollbar.notify_scroll(rtid);
                        self.mark_dirty();
                    }
                    Act::Scroll(delta) => {
                        let current = self.context_manager.current_mut();
                        let rtid = current.rich_text_id;
                        let mut terminal = current.terminal.lock();
                        terminal.scroll_display(Scroll::Delta(*delta));
                        drop(terminal);
                        self.renderer.scrollbar.notify_scroll(rtid);
                        self.mark_dirty();
                    }
                    Act::ClearHistory => {
                        let mut terminal =
                            self.context_manager.current_mut().terminal.lock();
                        terminal.clear_saved_history();
                        drop(terminal);
                        self.mark_dirty();
                    }
                    Act::ResetTriggers => {
                        self.triggers.reset();
                        self.mark_dirty();
                    }
                    Act::ToggleFullscreen => self.context_manager.toggle_full_screen(),
                    Act::ToggleAppearanceTheme => {
                        self.context_manager.toggle_appearance_theme();
                    }
                    Act::OpenCommandPalette => {
                        // One-way "open": the action never closes an
                        // already-visible palette. Users close it via
                        // Esc (handled inside the palette's own key
                        // dispatcher in `router::mod`). Idempotent —
                        // re-firing while the palette is already open
                        // must NOT wipe the user's in-progress query.
                        if !self.renderer.command_palette.is_enabled() {
                            self.renderer.command_palette.set_enabled(true);
                            self.mark_dirty();
                        }
                    }
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
                        self.cancel_search(clipboard);
                        self.context_manager.select_next_split();
                        self.mark_dirty();
                    }
                    Act::SelectPrevSplit => {
                        self.cancel_search(clipboard);
                        self.context_manager.select_prev_split();
                        self.mark_dirty();
                    }
                    Act::SelectNextSplitOrTab => {
                        self.cancel_search(clipboard);
                        self.clear_selection();
                        let old_index = self.context_manager.current_index();
                        self.context_manager.switch_to_next_split_or_tab();
                        let new_index = self.context_manager.current_index();
                        self.context_manager.switch_context_visibility(
                            &mut self.sugarloaf,
                            old_index,
                            new_index,
                        );
                        self.mark_dirty();
                    }
                    Act::SelectPrevSplitOrTab => {
                        self.cancel_search(clipboard);
                        self.clear_selection();
                        let old_index = self.context_manager.current_index();
                        self.context_manager.switch_to_prev_split_or_tab();
                        let new_index = self.context_manager.current_index();
                        self.context_manager.switch_context_visibility(
                            &mut self.sugarloaf,
                            old_index,
                            new_index,
                        );
                        self.mark_dirty();
                    }
                    Act::SelectTab(tab_index) => {
                        let old_index = self.context_manager.current_index();
                        self.context_manager.select_tab(*tab_index);
                        let new_index = self.context_manager.current_index();
                        self.context_manager.switch_context_visibility(
                            &mut self.sugarloaf,
                            old_index,
                            new_index,
                        );
                        self.cancel_search(clipboard);
                        self.mark_dirty();
                    }
                    Act::SelectLastTab => {
                        self.cancel_search(clipboard);
                        let old_index = self.context_manager.current_index();
                        self.context_manager.select_last_tab();
                        let new_index = self.context_manager.current_index();
                        self.context_manager.switch_context_visibility(
                            &mut self.sugarloaf,
                            old_index,
                            new_index,
                        );
                        self.mark_dirty();
                    }
                    Act::SelectNextTab => {
                        self.cancel_search(clipboard);
                        self.clear_selection();
                        let old_index = self.context_manager.current_index();
                        self.context_manager.switch_to_next();
                        let new_index = self.context_manager.current_index();
                        self.context_manager.switch_context_visibility(
                            &mut self.sugarloaf,
                            old_index,
                            new_index,
                        );
                        self.mark_dirty();
                    }
                    Act::MoveCurrentTabToPrev => {
                        self.cancel_search(clipboard);
                        self.clear_selection();
                        let old_index = self.context_manager.current_index();
                        self.context_manager.move_current_to_prev();
                        let new_index = self.context_manager.current_index();
                        self.context_manager.switch_context_visibility(
                            &mut self.sugarloaf,
                            old_index,
                            new_index,
                        );
                        let tab_width =
                            self.island_tab_layout(self.context_manager.len()).tab_width;
                        if let Some(ref mut island) = self.renderer.island {
                            island.remap_tab_swap(old_index, new_index, tab_width);
                        }
                        self.mark_dirty();
                    }
                    Act::MoveCurrentTabToNext => {
                        self.cancel_search(clipboard);
                        self.clear_selection();
                        let old_index = self.context_manager.current_index();
                        self.context_manager.move_current_to_next();
                        let new_index = self.context_manager.current_index();
                        self.context_manager.switch_context_visibility(
                            &mut self.sugarloaf,
                            old_index,
                            new_index,
                        );
                        let tab_width =
                            self.island_tab_layout(self.context_manager.len()).tab_width;
                        if let Some(ref mut island) = self.renderer.island {
                            island.remap_tab_swap(old_index, new_index, tab_width);
                        }
                        self.mark_dirty();
                    }
                    Act::SelectPrevTab => {
                        self.cancel_search(clipboard);
                        self.clear_selection();
                        let old_index = self.context_manager.current_index();
                        self.context_manager.switch_to_prev();
                        let new_index = self.context_manager.current_index();
                        self.context_manager.switch_context_visibility(
                            &mut self.sugarloaf,
                            old_index,
                            new_index,
                        );
                        self.mark_dirty();
                    }
                    Act::ReceiveChar | Act::None => (),
                    _ => (),
                }
            }
        }

        ignore_chars.unwrap_or(false)
    }

    pub fn split_right_with_config(&mut self, config: rio_backend::config::Config) {
        // Allocate panel id; position lands on `ContextDimension`
        // through the Taffy layout pass (`apply_taffy_layout`).
        let _ = self.renderer.margin.top
            + self.renderer.island.as_ref().map_or(0.0, |i| i.height());
        let _ = config.margin.left;
        let rich_text_id = next_rich_text_id();
        self.context_manager.split_from_config(
            rich_text_id,
            false,
            config,
            &mut self.sugarloaf,
        );

        self.mark_dirty();
    }

    pub fn split_right(&mut self) {
        let rich_text_id = next_rich_text_id();
        self.context_manager
            .split(rich_text_id, false, &mut self.sugarloaf);

        self.mark_dirty();
    }

    pub fn split_down(&mut self) {
        let rich_text_id = next_rich_text_id();
        self.context_manager
            .split(rich_text_id, true, &mut self.sugarloaf);

        self.mark_dirty();
    }

    pub fn move_divider_up(&mut self) {
        let amount = 20.0; // Default movement amount
        if self
            .context_manager
            .move_divider_up(amount, &mut self.sugarloaf)
        {
            self.mark_dirty();
        }
    }

    pub fn move_divider_down(&mut self) {
        let amount = 20.0; // Default movement amount
        if self
            .context_manager
            .move_divider_down(amount, &mut self.sugarloaf)
        {
            self.mark_dirty();
        }
    }

    pub fn move_divider_left(&mut self) {
        let amount = 40.0; // Default movement amount
        if self
            .context_manager
            .move_divider_left(amount, &mut self.sugarloaf)
        {
            self.mark_dirty();
        }
    }

    pub fn move_divider_right(&mut self) {
        let amount = 40.0; // Default movement amount
        if self
            .context_manager
            .move_divider_right(amount, &mut self.sugarloaf)
        {
            self.mark_dirty();
        }
    }

    pub fn create_tab(&mut self, clipboard: &mut Clipboard) {
        let redirect = true;

        // We resize the current tab ahead to prepare the
        // dimensions to be copied to next tab.
        let num_tabs = self.ctx().len();
        let old_index = self.context_manager.current_index();
        self.resize_top_or_bottom_line(num_tabs + 1);

        // Update the old tab's rich text positions to reflect the new margin
        // (on Linux/Windows when hide_if_single transitions from hidden to visible)
        #[cfg(not(target_os = "macos"))]
        self.context_manager.contexts_mut()[old_index]
            .update_dimensions(&mut self.sugarloaf);

        // Allocate panel id; the layout pass handles positioning via
        // `ContextDimension` once the new tab's grid is built.
        let _ = self.context_manager.current_grid().scaled_margin.left;
        let _ = self.renderer.margin.top
            + self.renderer.island.as_ref().map_or(0.0, |i| i.height());
        let rich_text_id = next_rich_text_id();
        self.context_manager.add_context(redirect, rich_text_id);
        let new_index = self.context_manager.current_index();
        self.context_manager.switch_context_visibility(
            &mut self.sugarloaf,
            old_index,
            new_index,
        );

        self.cancel_search(clipboard);
        self.mark_dirty();
    }

    pub fn close_split_or_tab(&mut self, clipboard: &mut Clipboard) {
        if self.context_manager.current_grid_len() > 1 {
            self.clear_selection();
            self.context_manager
                .remove_current_grid(&mut self.sugarloaf);
            self.mark_dirty();
        } else {
            self.close_tab(clipboard);
        }
    }

    pub fn close_tab(&mut self, clipboard: &mut Clipboard) {
        self.clear_selection();
        self.context_manager
            .close_current_context(&mut self.sugarloaf);
        if let Some(ref mut island) = self.renderer.island {
            island.dismiss_color_picker();
        }

        self.cancel_search(clipboard);
        if self.ctx().len() <= 1 {
            // Update the remaining tab's margin and position
            // (on Linux/Windows when hide_if_single transitions to hidden)
            #[cfg(not(target_os = "macos"))]
            {
                self.resize_top_or_bottom_line(1);
                self.context_manager
                    .current_grid_mut()
                    .update_dimensions(&mut self.sugarloaf);
                self.mark_dirty();
            }
            return;
        }
        let num_tabs = self.ctx().len();
        self.resize_top_or_bottom_line(num_tabs);
        self.mark_dirty();
    }

    pub fn resize_top_or_bottom_line(&mut self, num_tabs: usize) {
        let padding_y_top = padding_top_from_config(
            &self.renderer.navigation,
            self.renderer.margin.top,
            num_tabs,
            self.renderer.macos_use_unified_titlebar,
        );
        let padding_y_bottom = self.renderer.margin.bottom;

        let scale = self.sugarloaf.scale_factor();
        let scaled_top = padding_y_top * scale;
        let scaled_bottom = padding_y_bottom * scale;

        // Compare against the grid's scaled margin, the value the
        // layout actually uses. The per panel dimension margin is
        // zeroed by the taffy pass so it cannot be used as a guard.
        let current_margin = self.context_manager.current_grid().scaled_margin;
        if current_margin.top == scaled_top && current_margin.bottom == scaled_bottom {
            return;
        }

        let current_dim = self.context_manager.current().dimension;
        if current_dim.font_size <= 0.0 {
            return;
        }

        let s = self.sugarloaf.style_mut();
        s.font_size = current_dim.font_size;
        s.line_height = current_dim.line_height;

        // Every tab shares the window, so every grid needs the new
        // margin and a layout pass, not just the current one.
        for context_grid in self.context_manager.contexts_mut() {
            let margin = context_grid.scaled_margin;
            context_grid.update_scaled_margin(Margin::new(
                scaled_top,
                margin.right,
                scaled_bottom,
                margin.left,
            ));
            context_grid.update_dimensions(&mut self.sugarloaf);
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

    pub fn copy_selection(&mut self, ty: ClipboardType, clipboard: &mut Clipboard) {
        let terminal = self.context_manager.current_mut().terminal.lock();
        let text = match terminal.selection_to_string().filter(|s| !s.is_empty()) {
            Some(text) => text,
            None => return,
        };
        drop(terminal);

        clipboard.set(ty, text);
    }

    #[inline]
    pub fn select_all(&mut self) {
        let current = self.context_manager.current_mut();
        let mut terminal = current.terminal.lock();
        let start = Pos::new(terminal.grid.topmost_line(), Column(0));
        let end = Pos::new(terminal.grid.bottommost_line(), terminal.grid.last_column());
        let mut selection = Selection::new(SelectionType::Simple, start, Side::Left);
        selection.update(end, Side::Right);
        let selection_range = selection.to_range(&terminal);
        terminal.selection = Some(selection);
        drop(terminal);

        current.set_selection(selection_range);
        self.context_manager.request_render();
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
    fn start_selection(
        &mut self,
        ty: SelectionType,
        point: Pos,
        side: Side,
        clipboard: &mut Clipboard,
    ) {
        self.copy_selection(ClipboardType::Selection, clipboard);
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
    fn toggle_selection(
        &mut self,
        ty: SelectionType,
        side: Side,
        clipboard: &mut Clipboard,
    ) {
        let mut terminal = self.context_manager.current().terminal.lock();
        match &mut terminal.selection {
            Some(selection) if selection.ty == ty && !selection.is_empty() => {
                drop(terminal);
                self.clear_selection();
            }
            Some(selection) if !selection.is_empty() => {
                selection.ty = ty;
                drop(terminal);
                self.copy_selection(ClipboardType::Selection, clipboard);
            }
            _ => {
                let pos = terminal.vi_mode_cursor.pos;
                drop(terminal);
                self.start_selection(ty, pos, side, clipboard)
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
            // Mark the hint range as damaged so it gets re-rendered.
            //
            // Two damage signals are required:
            // * Terminal-side: `update_selection_damage` marks the affected
            // lines so the partial render path knows what to redraw.
            // * Renderer-side: `pending_update.set_terminal_damage(Full)`
            // ensures the render loop doesn't early-exit on
            // `!pending_update.is_dirty()`
            {
                let mut terminal = current.terminal.lock();
                let display_offset = terminal.display_offset();

                let hint_range = rio_backend::selection::SelectionRange::new(
                    hint_match.start,
                    hint_match.end,
                    false,
                );
                terminal.update_selection_damage(Some(hint_range), display_offset);
            }

            current
                .renderable_content
                .pending_update
                .set_terminal_damage(rio_backend::event::TerminalDamage::Full);
            current.renderable_content.highlighted_hint = Some(hint_match);
            true
        } else {
            if current.renderable_content.highlighted_hint.is_some() {
                let mut terminal = current.terminal.lock();
                let display_offset = terminal.display_offset();
                terminal.update_selection_damage(None, display_offset);
            }

            // Force a render so the previously-highlighted line clears.
            if had_highlight {
                current
                    .renderable_content
                    .pending_update
                    .set_terminal_damage(rio_backend::event::TerminalDamage::Full);
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
                if let Ok(regex) = onig::Regex::new(regex_pattern) {
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

        // Look up the cell's hyperlink via the per-grid extras table.
        // Cells in the same OSC 8 span share an `extras_id`, so we
        // walk left/right comparing ids (cheap u16 compare) to find
        // the span boundaries, then look up the URI once.
        let id = terminal.cell_hyperlink_id(point.row, point.col)?;

        let mut start_col = point.col;
        let mut end_col = point.col;

        while start_col > rio_backend::crosswords::pos::Column(0) {
            let prev_col = start_col - 1;
            if terminal.cell_hyperlink_id(point.row, prev_col) == Some(id) {
                start_col = prev_col;
            } else {
                break;
            }
        }
        while end_col < grid.columns() - 1 {
            let next_col = end_col + 1;
            if terminal.cell_hyperlink_id(point.row, next_col) == Some(id) {
                end_col = next_col;
            } else {
                break;
            }
        }

        let hyperlink = terminal.cell_hyperlink(point.row, point.col)?;

        // Build a synthetic hint config so the rest of the hint
        // pipeline (highlighting, click action) treats this just like
        // a regex/url match.
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

        Some(crate::hints::HintMatch {
            text: uri,
            start: rio_backend::crosswords::pos::Pos::new(point.row, start_col),
            end: rio_backend::crosswords::pos::Pos::new(point.row, end_col),
            hint: hint_config,
        })
    }

    /// Find regex match at the specified point
    fn find_regex_match_at_point(
        &self,
        terminal: &rio_backend::crosswords::Crosswords<EventProxy>,
        point: rio_backend::crosswords::pos::Pos,
        regex: &onig::Regex,
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
            line_text.push(cell.c());
        }
        let line_text = line_text.trim_end();

        // Find all matches in this line and check if point is within any of them.
        // Onig yields (byte_start, byte_end); we slice the source ourselves.
        for (start, end) in regex.find_iter(line_text) {
            let start_col = rio_backend::crosswords::pos::Column(start);
            let end_col = rio_backend::crosswords::pos::Column(end.saturating_sub(1));

            // Check if the point is within this match
            if point.col >= start_col && point.col <= end_col {
                let original_match_text = line_text[start..end].to_string();
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
                        processed_text.push(cell.c());
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

        // Look up the cell under the mouse and dispatch open_hyperlink
        // if it carries an OSC 8 link.
        let terminal = self.context_manager.current().terminal.lock();
        let display_offset = terminal.display_offset();
        let pos = self.mouse_position(display_offset);
        let pos_hyperlink = terminal.cell_hyperlink(pos.row, pos.col);
        drop(terminal);

        if let Some(hyperlink) = pos_hyperlink {
            self.open_hyperlink(hyperlink);
            return true;
        }

        false
    }

    /// Trigger hint action at mouse position
    #[inline]
    pub fn trigger_hint(&mut self, clipboard: &mut Clipboard) -> bool {
        // Take the highlighted hint
        let hint_match = self
            .context_manager
            .current_mut()
            .renderable_content
            .highlighted_hint
            .take();

        if let Some(hint_match) = hint_match {
            self.execute_hint_action(&hint_match, clipboard);
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

        // Going through `cmd /c start` re-quotes the argument and
        // mangles URLs (metacharacters plus cmd.exe batch escaping);
        // hand the URL to the default handler directly instead.
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            let uri: Vec<u16> = std::ffi::OsStr::new(&processed_uri)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let operation: Vec<u16> = "open\0".encode_utf16().collect();
            unsafe {
                windows_sys::Win32::UI::Shell::ShellExecuteW(
                    std::ptr::null_mut(),
                    operation.as_ptr(),
                    uri.as_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                    windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
                );
            }
        }
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
    /// Compute the selection scroll delta for the given mouse Y position.
    /// Returns 0 if the mouse is within the viewport, ±1 at the edges.
    /// `mouse_y` is in physical pixels (from CursorMoved position.y).
    pub fn selection_scroll_delta(&self, mouse_y: f64) -> i32 {
        let current_grid = self.context_manager.current_grid();
        let (context, margin) = current_grid.current_context_with_computed_dimension();
        let layout = context.dimension;
        // Canonical integer cell stride. line_height is already
        // baked into `cell.cell_height`; the previous code
        // multiplied by line_height again, breaking the
        // edge-of-viewport detection at line_height ≠ 1.0.
        let cell_height = layout.cell.cell_height as f64;
        let text_area_top = margin.top as f64;
        let text_area_bottom = text_area_top + layout.lines as f64 * cell_height;
        let window_height = self.sugarloaf.window_size().height as f64;

        if mouse_y < text_area_top {
            1 // scroll up (into history)
        } else if mouse_y >= window_height - cell_height && mouse_y >= text_area_bottom {
            -1 // scroll down (toward present)
        } else {
            0
        }
    }

    /// Perform one tick of selection auto-scroll.
    /// Reads mouse.raw_y to compute scroll direction.
    /// Scrolls 1 line per tick.
    pub fn selection_scroll_tick(&mut self) {
        if self.mouse.left_button_state != rio_window::event::ElementState::Pressed {
            return;
        }

        let delta = self.selection_scroll_delta(self.mouse.raw_y);
        if delta == 0 {
            return;
        }

        let mut terminal = self.context_manager.current_mut().terminal.lock();
        terminal.scroll_display(Scroll::Delta(delta));
        drop(terminal);

        // Update selection to match the new scroll position.
        let display_offset = self.display_offset();
        let point = self.mouse_position(display_offset);
        let side = self.mouse.square_side;
        self.update_selection(point, side);
    }

    #[inline]
    pub fn contains_point(&self, x: f64, y: f64) -> bool {
        let current_grid = self.context_manager.current_grid();
        let (context, margin) = current_grid.current_context_with_computed_dimension();
        let layout = context.dimension;
        // Canonical integer stride — same as the GPU paints with.
        // line_height is already baked into `cell.cell_height`; do
        // NOT multiply again here.
        let cell_w = layout.cell.cell_width as f64;
        let cell_h = layout.cell.cell_height as f64;
        let left = margin.left as f64;
        let top = margin.top as f64;
        x > left
            && x <= left + layout.columns as f64 * cell_w
            && y > top
            && y <= top + layout.lines as f64 * cell_h
    }

    #[inline]
    pub fn side_by_pos(&self, x: f64) -> Side {
        let current_grid = self.context_manager.current_grid();
        let (_, margin) = current_grid.current_context_with_computed_dimension();
        let current_context = self.context_manager.current();
        let layout = current_context.dimension;

        crate::mouse::calculate_side_by_pos(
            x,
            margin.left,
            layout.cell.cell_width,
            layout.width,
        )
    }

    #[inline]
    pub fn selection_is_empty(&self) -> bool {
        self.context_manager
            .current()
            .renderable_content
            .selection_range
            .is_none()
    }

    // return true if the click was handled by the island
    #[inline]
    pub fn handle_palette_click(&mut self, clipboard: &mut Clipboard) -> bool {
        if !self.renderer.command_palette.is_enabled() {
            return false;
        }

        let scale_factor = self.sugarloaf.scale_factor();
        let window_width = self.sugarloaf.window_size().width;
        let mouse_x = self.mouse.x as f32 / scale_factor;
        let mouse_y = self.mouse.y as f32 / scale_factor;

        match self.renderer.command_palette.hit_test(
            mouse_x,
            mouse_y,
            window_width,
            scale_factor,
        ) {
            Ok(Some(index)) => {
                // Clicked a result row — select and execute
                if let Some(action) = {
                    // Temporarily set selected index to the clicked row
                    self.renderer.command_palette.selected_index = index;
                    self.renderer.command_palette.get_selected_action()
                } {
                    self.renderer.command_palette.set_enabled(false);
                    self.execute_palette_action(action, clipboard);
                }
                self.mark_dirty();
                true
            }
            Ok(None) => {
                // Clicked inside palette but not on a result (e.g. input area)
                true
            }
            Err(()) => {
                // Clicked outside — close palette
                self.renderer.command_palette.set_enabled(false);
                self.mark_dirty();
                true
            }
        }
    }

    #[inline]
    pub fn handle_search_click(&mut self, clipboard: &mut Clipboard) -> bool {
        if !self.renderer.search.is_active() {
            return false;
        }

        let scale_factor = self.sugarloaf.scale_factor();
        let window_width = self.sugarloaf.window_size().width;
        let mouse_x = self.mouse.x as f32 / scale_factor;
        let mouse_y = self.mouse.y as f32 / scale_factor;

        match self
            .renderer
            .search
            .hit_test(mouse_x, mouse_y, window_width, scale_factor)
        {
            Ok(Some(action)) => {
                use crate::renderer::search::SearchOverlayAction;
                match action {
                    SearchOverlayAction::Next => {
                        self.advance_search_origin(self.search_state.direction);
                    }
                    SearchOverlayAction::Previous => {
                        let direction = self.search_state.direction.opposite();
                        self.advance_search_origin(direction);
                    }
                    SearchOverlayAction::Close => {
                        self.cancel_search(clipboard);
                        self.resize_top_or_bottom_line(self.ctx().len());
                    }
                }
                self.mark_dirty();
                true
            }
            Ok(None) => {
                // Clicked inside overlay but not on a button (input area)
                true
            }
            Err(()) => {
                // Clicked outside — don't close search, just pass through
                false
            }
        }
    }

    #[inline]
    pub fn handle_assistant_click(&mut self) -> bool {
        if !self.renderer.assistant.is_active() {
            return false;
        }

        let scale_factor = self.sugarloaf.scale_factor();
        let window_width = self.sugarloaf.window_size().width;
        let mouse_x = self.mouse.x as f32 / scale_factor;
        let mouse_y = self.mouse.y as f32 / scale_factor;

        match self.renderer.assistant.hit_test(
            mouse_x,
            mouse_y,
            window_width,
            scale_factor,
        ) {
            Ok(Some(action)) => {
                use crate::renderer::assistant::AssistantOverlayAction;
                match action {
                    AssistantOverlayAction::Close => {
                        self.renderer.assistant.clear();
                    }
                    AssistantOverlayAction::OpenDocs => {
                        Self::open_docs_url();
                    }
                }
                self.mark_dirty();
                true
            }
            Ok(None) => {
                // Clicked inside overlay but not on a button
                true
            }
            Err(()) => {
                // Clicked outside — close the assistant overlay
                self.renderer.assistant.clear();
                self.mark_dirty();
                true
            }
        }
    }

    fn open_docs_url() {
        let url = "https://rioterm.com/docs/config";
        #[cfg(target_os = "macos")]
        {
            let _ = std::process::Command::new("open").arg(url).spawn();
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        {
            let _ = std::process::Command::new("xdg-open").arg(url).spawn();
        }
        #[cfg(windows)]
        {
            let _ = std::process::Command::new("cmd")
                .args(["/c", "start", "", url])
                .spawn();
        }
    }

    pub fn handle_scrollbar_click(&mut self) -> bool {
        let scale_factor = self.sugarloaf.scale_factor();
        let mouse_x = self.mouse.x as f32 / scale_factor;
        let mouse_y = self.mouse.y as f32 / scale_factor;

        let grid = self.context_manager.current_grid_mut();
        let grid_margin = (grid.scaled_margin.left, grid.scaled_margin.top);

        let item = match grid.current_item() {
            Some(item) => item,
            None => return false,
        };

        let panel_rect = item.layout_rect;
        let rich_text_id = item.context().rich_text_id;

        let terminal = item.context().terminal.lock();
        let display_offset = terminal.display_offset();
        let history_size = terminal.history_size();
        let screen_lines = terminal.screen_lines();
        drop(terminal);

        if let Some((grab_offset, geom)) = self.renderer.scrollbar.hit_test(
            mouse_x,
            mouse_y,
            panel_rect,
            scale_factor,
            display_offset,
            history_size,
            screen_lines,
            grid_margin,
        ) {
            self.renderer.scrollbar.start_drag(
                rich_text_id,
                grab_offset,
                &geom,
                history_size,
            );

            // If clicked on track (not on thumb), jump-scroll to that position
            if grab_offset.is_none() {
                if let Some(new_offset) = self.renderer.scrollbar.drag_update(mouse_y) {
                    let mut terminal = self.context_manager.current_mut().terminal.lock();
                    let current = terminal.display_offset();
                    let delta = new_offset as i32 - current as i32;
                    terminal.scroll_display(Scroll::Delta(delta));
                    drop(terminal);
                }
            }
            self.mark_dirty();
            true
        } else {
            false
        }
    }

    pub fn handle_scrollbar_drag(&mut self, mouse_y: f32) -> bool {
        if !self.renderer.scrollbar.is_dragging() {
            return false;
        }

        if let Some(new_offset) = self.renderer.scrollbar.drag_update(mouse_y) {
            let mut terminal = self.context_manager.current_mut().terminal.lock();
            let current = terminal.display_offset();
            let delta = new_offset as i32 - current as i32;
            if delta != 0 {
                terminal.scroll_display(Scroll::Delta(delta));
            }
            drop(terminal);
            self.mark_dirty();
        }
        true
    }

    pub fn handle_scrollbar_release(&mut self) {
        self.renderer.scrollbar.end_drag();
    }

    pub fn is_hovering_scrollbar(&self) -> bool {
        if !self.renderer.scrollbar.is_enabled() {
            return false;
        }
        let scale_factor = self.sugarloaf.scale_factor();
        let mouse_x = self.mouse.x as f32 / scale_factor;
        let mouse_y = self.mouse.y as f32 / scale_factor;

        let grid = self.context_manager.current_grid();
        let grid_margin = (grid.scaled_margin.left, grid.scaled_margin.top);

        let item = match grid.current_item() {
            Some(item) => item,
            None => return false,
        };

        let panel_rect = item.layout_rect;

        let terminal = item.context().terminal.lock();
        let display_offset = terminal.display_offset();
        let history_size = terminal.history_size();
        let screen_lines = terminal.screen_lines();
        drop(terminal);

        self.renderer
            .scrollbar
            .hit_test(
                mouse_x,
                mouse_y,
                panel_rect,
                scale_factor,
                display_offset,
                history_size,
                screen_lines,
                grid_margin,
            )
            .is_some()
    }

    #[inline]
    fn island_tab_layout(&self, num_tabs: usize) -> TabStripLayout {
        let max_tab_width = self
            .renderer
            .island
            .as_ref()
            .map(|island| island.max_tab_width)
            .unwrap_or_else(rio_backend::config::navigation::default_max_tab_width);
        island::tab_strip_layout(
            self.sugarloaf.window_size().width,
            self.sugarloaf.scale_factor(),
            num_tabs,
            max_tab_width,
        )
    }

    #[cfg(target_os = "macos")]
    pub fn start_window_drag(&mut self, window: &rio_window::window::Window) {
        self.mouse.left_button_state = ElementState::Released;
        let _ = window.drag_window();
    }

    fn on_chrome_press(
        &mut self,
        window: &rio_window::window::Window,
        prev: Option<ChromePress>,
    ) {
        let window_origin = window.outer_position().ok();
        let double = matches!(self.mouse.click_state, ClickState::DoubleClick)
            && prev.is_some_and(|p| p.validates_double_click(window_origin));
        if double {
            let is_maximized = window.is_maximized();
            window.set_maximized(!is_maximized);
            return;
        }

        self.last_chrome_press = Some(ChromePress {
            window_origin,
            at: std::time::Instant::now(),
        });
        #[cfg(target_os = "macos")]
        if self.allow_manual_dragging {
            self.start_window_drag(window);
        }
    }

    #[inline]
    pub fn take_chrome_press(&mut self) -> Option<ChromePress> {
        self.last_chrome_press.take()
    }

    fn is_close_press_tail(&self, x_unscaled: f32) -> bool {
        const CLOSE_TAIL_SLOP: f32 = 16.0;
        self.last_close_press.is_some_and(|(at, press_x)| {
            at.elapsed() <= crate::constants::MULTI_CLICK_THRESHOLD
                && (x_unscaled - press_x).abs() <= CLOSE_TAIL_SLOP
        })
    }

    fn apply_close_hover(&mut self, hover: bool) -> bool {
        let changed = self
            .renderer
            .island
            .as_mut()
            .is_some_and(|island| island.set_close_hover(hover));
        if changed {
            self.mark_dirty();
        }
        changed
    }

    pub fn update_close_button_hover(&mut self, mouse_x: f64, mouse_y: f64) -> bool {
        let num_tabs = self.context_manager.len();
        let scale_factor = self.sugarloaf.scale_factor();

        let hovering = num_tabs > 1
            && self.renderer.navigation.island_visible(num_tabs)
            && mouse_y <= (ISLAND_HEIGHT * scale_factor) as f64
            && island::close_button_hit(
                &self.island_tab_layout(num_tabs),
                self.context_manager.current_index(),
                mouse_x as f32 / scale_factor,
            );

        self.apply_close_hover(hovering)
    }

    #[inline]
    pub fn clear_close_button_hover(&mut self) -> bool {
        self.apply_close_hover(false)
    }

    pub fn handle_island_click(
        &mut self,
        window: &rio_window::window::Window,
        clipboard: &mut Clipboard,
        is_right_click: bool,
        chrome_press: Option<ChromePress>,
    ) -> bool {
        // Only handle if navigation is enabled
        if !self.renderer.navigation.is_enabled() {
            return false;
        }

        let mouse_x = self.mouse.x;
        let mouse_y = self.mouse.y;

        let scale_factor = self.sugarloaf.scale_factor();
        let island_height_px = (ISLAND_HEIGHT * scale_factor) as f64;

        let window_width = self.sugarloaf.window_size().width;
        let num_tabs = self.context_manager.len();
        let island_visible = self.renderer.navigation.island_visible(num_tabs);

        if let Some(ref mut island) = self.renderer.island {
            if island.is_color_picker_open() {
                let consumed = island.handle_color_picker_click(
                    mouse_x as f32,
                    mouse_y as f32,
                    scale_factor,
                    window_width,
                    num_tabs,
                    &mut self.context_manager,
                );
                if consumed {
                    self.mark_dirty();
                    return true;
                }
            }
        }

        // Check if click is within island height
        if mouse_y > island_height_px {
            // Close picker if clicking outside
            if let Some(ref mut island) = self.renderer.island {
                if island.is_color_picker_open() {
                    island.close_color_picker(&mut self.context_manager);
                    self.mark_dirty();
                }
            }
            return false;
        }

        let mouse_x_unscaled = mouse_x as f32 / scale_factor;

        // Island isn't painted (hide_if_single + single tab on macOS).
        // Nothing to click on, so let the caller route the event to the
        // grid for selection / double-click maximize at the OS title bar.
        if !island_visible {
            // …unless a ×-close just hid the strip (2 tabs → 1 with
            // hide-if-single): the tail press of a double-click on the
            // × would otherwise leak into the terminal as a selection,
            // or start a window drag via the band fallback.
            if self.is_close_press_tail(mouse_x_unscaled) {
                return true;
            }
            return false;
        }

        let layout = self.island_tab_layout(num_tabs);
        let x_in_tabs = mouse_x_unscaled - layout.left_margin;

        if !is_right_click
            && self.is_close_press_tail(mouse_x_unscaled)
            && !island::close_button_hit(
                &layout,
                self.context_manager.current_index(),
                mouse_x_unscaled,
            )
        {
            return true;
        }

        if x_in_tabs < 0.0 || x_in_tabs >= layout.tabs_width {
            if !is_right_click {
                self.on_chrome_press(window, chrome_press);
            }
            return true;
        }

        // `.min` guards the float edge where x_in_tabs / tab_width
        // lands exactly on num_tabs despite x_in_tabs < tabs_width.
        let clicked_tab = ((x_in_tabs / layout.tab_width) as usize).min(num_tabs - 1);

        #[cfg(target_os = "macos")]
        if !is_right_click && self.modifiers.state().super_key() {
            if self.allow_manual_dragging {
                self.start_window_drag(window);
            }
            return true;
        }

        // Right-click or Control + left-click → toggle color picker for that tab
        if is_right_click || self.modifiers.state().control_key() {
            // Get current displayed title for the rename input
            let current_title = self
                .context_manager
                .title(clicked_tab)
                .and_then(|t| {
                    if !t.content.is_empty() {
                        Some(t.content.clone())
                    } else {
                        t.extra.as_ref().and_then(|e| {
                            if !e.program.is_empty() {
                                Some(e.program.clone())
                            } else {
                                None
                            }
                        })
                    }
                })
                .unwrap_or_else(|| String::from("~"));
            if let Some(ref mut island) = self.renderer.island {
                island.toggle_color_picker(
                    clicked_tab,
                    &current_title,
                    &mut self.context_manager,
                );
                self.mark_dirty();
            }
            return true;
        }

        if num_tabs == 1 {
            self.on_chrome_press(window, chrome_press);
            return true;
        }

        if clicked_tab == self.context_manager.current_index()
            && island::close_button_hit(&layout, clicked_tab, mouse_x_unscaled)
        {
            self.stop_hint_mode_if_active();
            self.last_close_press = Some((std::time::Instant::now(), mouse_x_unscaled));
            self.close_tab(clipboard);
            return true;
        }

        if clicked_tab != self.context_manager.current_index() {
            self.stop_hint_mode_if_active();
            self.cancel_search(clipboard);
            self.clear_selection();
            let old_index = self.context_manager.current_index();
            self.context_manager.set_current(clicked_tab);
            let new_index = self.context_manager.current_index();
            self.context_manager.switch_context_visibility(
                &mut self.sugarloaf,
                old_index,
                new_index,
            );

            self.mark_dirty();
        }

        if let Some(ref mut island) = self.renderer.island {
            if island.is_color_picker_open() {
                island.close_color_picker(&mut self.context_manager);
                self.mark_dirty();
            }
        }

        #[cfg(target_os = "macos")]
        let can_reorder = self.allow_manual_dragging;
        #[cfg(not(target_os = "macos"))]
        let can_reorder = true;
        if num_tabs > 1 && can_reorder {
            if let Some(ref mut island) = self.renderer.island {
                let tab_left = layout.left_margin + clicked_tab as f32 * layout.tab_width;
                island.start_drag(
                    clicked_tab,
                    mouse_x_unscaled - tab_left,
                    mouse_x_unscaled,
                );
            }
        }

        true
    }

    pub fn handle_tab_drag_move(&mut self, x_unscaled: f32) {
        let num_tabs = self.context_manager.len();

        // A tab closed mid-drag invalidates the armed indices.
        if num_tabs < 2 {
            if let Some(ref mut island) = self.renderer.island {
                island.cancel_drag();
            }
            return;
        }

        let layout = self.island_tab_layout(num_tabs);

        let (drag_idx, center) = match self.renderer.island.as_mut() {
            Some(island) => {
                if !island.update_drag(x_unscaled) {
                    // armed but still below the drag threshold
                    return;
                }
                match (island.drag_index(), island.drag_center(&layout)) {
                    (Some(idx), Some(center)) => (idx, center),
                    _ => return,
                }
            }
            None => return,
        };

        let old_index = self.context_manager.current_index();
        if drag_idx != old_index {
            if let Some(ref mut island) = self.renderer.island {
                island.cancel_drag();
            }
            self.mark_dirty();
            return;
        }

        let target = (((center - layout.left_margin) / layout.tab_width) as usize)
            .min(num_tabs - 1);
        if target != old_index {
            self.context_manager.move_current_tab_to(target);
            let new_index = self.context_manager.current_index();
            self.context_manager.switch_context_visibility(
                &mut self.sugarloaf,
                old_index,
                new_index,
            );
            if let Some(ref mut island) = self.renderer.island {
                island.remap_tab_move(old_index, new_index, layout.tab_width);
            }
        }
        self.mark_dirty();
    }

    pub fn handle_tab_drag_release(&mut self) -> bool {
        let num_tabs = self.context_manager.len();
        let layout = self.island_tab_layout(num_tabs);

        if let Some(ref mut island) = self.renderer.island {
            let started = island.drag_index().is_some();
            island.end_drag(&layout);
            if started {
                self.mark_dirty();
            }
            return started;
        }
        false
    }

    #[inline]
    pub fn on_left_click(&mut self, point: Pos, clipboard: &mut Clipboard) {
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
                        self.start_selection(
                            SelectionType::Block,
                            point,
                            side,
                            clipboard,
                        );
                    } else {
                        self.start_selection(
                            SelectionType::Simple,
                            point,
                            side,
                            clipboard,
                        );
                    }
                }
            }
            ClickState::DoubleClick => {
                self.start_selection(SelectionType::Semantic, point, side, clipboard);
            }
            ClickState::TripleClick => {
                self.start_selection(SelectionType::Lines, point, side, clipboard);
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

        self.mark_dirty();
    }

    #[inline]
    fn confirm_search(&mut self, clipboard: &mut Clipboard) {
        // Just cancel search when not in vi mode.
        if !self.get_mode().contains(Mode::VI) {
            self.cancel_search(clipboard);
            return;
        }

        // Force unlimited search if the previous one was interrupted.
        // let timer_id = TimerId::new(Topic::DelayedSearch, self.display.window.id());
        // if self.scheduler.scheduled(timer_id) {
        // self.goto_match(None);
        // }

        self.exit_search();
    }

    #[inline]
    fn cancel_search(&mut self, clipboard: &mut Clipboard) {
        if self.get_mode().contains(Mode::VI) {
            // Recover pre-search state in vi mode.
            self.search_reset_state();
        } else if let Some(focused_match) = &self.search_state.focused_match {
            // Create a selection for the focused match.
            let start = *focused_match.start();
            let end = *focused_match.end();
            self.start_selection(SelectionType::Simple, start, Side::Left, clipboard);
            self.update_selection(end, Side::Right);
            self.copy_selection(ClipboardType::Selection, clipboard);
        }

        self.search_state.dfas = None;
        self.exit_search();
        self.update_hint_state();
    }

    /// Cleanup the search state.
    fn exit_search(&mut self) {
        // let vi_mode = self.get_mode().contains(Mode::VI);
        // self.window().set_ime_allowed(!vi_mode);

        self.search_state.history_index = None;

        // Clear focused match.
        self.search_state.focused_match = None;

        self.mark_dirty();
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
        self.mark_dirty();
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
        self.renderer.is_window_focused = is_focused;
        if is_focused {
            self.mark_dirty();
        }
        if !is_focused {
            let rc = &mut self.context_manager.current_mut().renderable_content;
            if !rc.is_blinking_cursor_visible {
                rc.is_blinking_cursor_visible = true;
            }
            rc.last_blink_toggle = None;
            rc.pending_update
                .set_terminal_damage(rio_backend::event::TerminalDamage::CursorOnly);

            if let Some(ref mut island) = self.renderer.island {
                if island.is_dragging() {
                    island.cancel_drag();
                    self.mark_dirty();
                }
            }
            self.mouse.left_button_state = ElementState::Released;
        }

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
        let dim = self.context_manager.current().dimension.dimension;
        let width = dim.width as f64;
        let height = dim.height as f64;
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

            let lines = (self.mouse.accumulated_scroll.y / height).abs() as usize;

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
            let lines = (self.mouse.accumulated_scroll.y / height) as i32;

            if lines != 0 {
                let current = self.context_manager.current_mut();
                let rich_text_id = current.rich_text_id;
                let mut terminal = current.terminal.lock();
                terminal.scroll_display(Scroll::Delta(lines));
                drop(terminal);
                self.renderer.scrollbar.notify_scroll(rich_text_id);
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

    pub(crate) fn render_welcome(&mut self) {
        crate::router::routes::welcome::screen(
            &mut self.sugarloaf,
            &self.context_manager.current().dimension,
        );
        self.sugarloaf.render();
    }

    pub fn execute_palette_action(
        &mut self,
        action: crate::renderer::command_palette::PaletteAction,
        clipboard: &mut Clipboard,
    ) {
        use crate::renderer::command_palette::PaletteAction;
        match action {
            PaletteAction::TabCreate => self.create_tab(clipboard),
            PaletteAction::TabClose => self.close_tab(clipboard),
            PaletteAction::TabCloseUnfocused => {
                if self.ctx().len() > 1 {
                    self.context_manager.close_unfocused_tabs();
                    if let Some(ref mut island) = self.renderer.island {
                        island.dismiss_color_picker();
                    }
                    self.resize_top_or_bottom_line(1);
                }
            }
            PaletteAction::SelectNextTab => {
                self.clear_selection();
                let old = self.context_manager.current_index();
                self.context_manager.switch_to_next();
                let new = self.context_manager.current_index();
                self.context_manager.switch_context_visibility(
                    &mut self.sugarloaf,
                    old,
                    new,
                );
            }
            PaletteAction::SelectPrevTab => {
                self.clear_selection();
                let old = self.context_manager.current_index();
                self.context_manager.switch_to_prev();
                let new = self.context_manager.current_index();
                self.context_manager.switch_context_visibility(
                    &mut self.sugarloaf,
                    old,
                    new,
                );
            }
            PaletteAction::SplitRight => self.split_right(),
            PaletteAction::SplitDown => self.split_down(),
            PaletteAction::SelectNextSplit => {
                self.context_manager.select_next_split();
            }
            PaletteAction::SelectPrevSplit => {
                self.context_manager.select_prev_split();
            }
            PaletteAction::CloseCurrentSplitOrTab => self.close_split_or_tab(clipboard),
            PaletteAction::ConfigEditor => {
                self.context_manager.switch_to_settings();
            }
            PaletteAction::WindowCreateNew => {
                self.context_manager.create_new_window();
            }
            PaletteAction::IncreaseFontSize => {
                self.change_font_size(FontSizeAction::Increase);
            }
            PaletteAction::DecreaseFontSize => {
                self.change_font_size(FontSizeAction::Decrease);
            }
            PaletteAction::ResetFontSize => {
                self.change_font_size(FontSizeAction::Reset);
            }
            PaletteAction::ToggleViMode => {
                let context = self.context_manager.current_mut();
                let mut terminal = context.terminal.lock();
                terminal.toggle_vi_mode();
                drop(terminal);
                context
                    .renderable_content
                    .pending_update
                    .set_terminal_damage(rio_backend::event::TerminalDamage::Full);
            }
            PaletteAction::ToggleFullscreen => {
                self.context_manager.toggle_full_screen();
            }
            PaletteAction::ToggleAppearanceTheme => {
                self.context_manager.toggle_appearance_theme();
            }
            PaletteAction::Copy => {
                self.copy_selection(ClipboardType::Clipboard, clipboard);
            }
            PaletteAction::Paste => {
                let content = clipboard.get(ClipboardType::Clipboard);
                self.paste(&content, true);
            }
            PaletteAction::SearchForward => {
                self.start_search(Direction::Right);
            }
            PaletteAction::SearchBackward => {
                self.start_search(Direction::Left);
            }
            PaletteAction::ClearHistory => {
                let mut terminal = self.context_manager.current_mut().terminal.lock();
                terminal.clear_saved_history();
            }
            PaletteAction::ListFonts => {
                // Handled in the router: switches the palette into fonts
                // mode and keeps it open. If we land here it's either a
                // bug (router should have intercepted) or an external
                // caller firing the action directly — do nothing so the
                // palette just closes without side effects.
            }
            PaletteAction::Quit => {
                self.context_manager.quit();
            }
        }
    }

    /// Scan the focused route's new output against the configured triggers
    /// and dispatch their actions. No-op when no triggers are configured.
    fn run_triggers(&mut self) {
        if self.triggers.is_empty() {
            // Rules were all removed: drop any highlights left on the
            // focused pane so they don't drift as the text scrolls.
            self.context_manager
                .current_mut()
                .renderable_content
                .trigger_highlights = None;
            return;
        }

        // The focused context's real route id (the value get_by_route_id
        // matches), not the cached current_route which is unset until the
        // first tab switch.
        let route_id = self.context_manager.current().route_id;

        let (actions, highlights) = {
            let ctx = self.context_manager.current();
            // Previous highlights let a Partial frame rescan only damaged
            // rows and carry the rest over.
            let prev = ctx.renderable_content.trigger_highlights.as_deref();
            let terminal = ctx.terminal.lock();
            let actions = self.triggers.scan(route_id, &terminal);
            let highlights = self.triggers.highlights(&terminal, prev);
            (actions, highlights)
        };

        // `None` means no terminal content changed this frame: keep the
        // highlights already on the pane. `Some(empty)` means recompute
        // produced nothing, so clear them.
        if let Some(highlights) = highlights {
            self.context_manager
                .current_mut()
                .renderable_content
                .trigger_highlights = if highlights.is_empty() {
                None
            } else {
                Some(highlights)
            };
        }

        use crate::triggers::ResolvedAction as Action;
        use rio_backend::event::{RioEvent, TriggerEventAction as Event};
        let window_id = self.context_manager.window_id();
        let tab_index = self.context_manager.current_index();
        for action in actions {
            let event = match action {
                Action::Notify {
                    title,
                    body,
                    urgency,
                } => Event::Notify {
                    title,
                    body,
                    urgency,
                },
                Action::Run { program, args } => Event::Run { program, args },
                Action::SendText(text) => Event::SendText { text },
                Action::Coprocess {
                    program,
                    args,
                    stdin,
                } => Event::Coprocess {
                    program,
                    args,
                    stdin,
                },
                Action::TabColor(color) => {
                    self.context_manager
                        .set_custom_color(tab_index, Some(color));
                    continue;
                }
            };
            self.context_manager.event_proxy().send_event(
                RioEvent::TriggerFired {
                    route_id,
                    action: event,
                }
                .into(),
                window_id,
            );
        }
    }

    pub(crate) fn render(&mut self) -> Option<crate::context::renderable::WindowUpdate> {
        self.update_close_button_hover(self.mouse.x, self.mouse.y);
        self.run_triggers();

        let is_search_active = self.search_active();
        if is_search_active {
            if let Some(history_index) = self.search_state.history_index {
                self.renderer.set_active_search(
                    self.search_state.history.get(history_index).cloned(),
                );
            }
        } else {
            self.renderer.set_active_search(None);
        }

        if is_search_active {
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
                    .set_terminal_damage(rio_backend::event::TerminalDamage::Full);
            }
        }

        let (window_update, any_panel_dirty) = self
            .renderer
            .run(&mut self.sugarloaf, &mut self.context_manager);
        let has_animation = self.renderer.needs_redraw();
        let should_present = any_panel_dirty || has_animation;

        if self.renderer.custom_mouse_cursor {
            let scale = self.sugarloaf.scale_factor();
            crate::renderer::custom_cursor::draw(
                &mut self.sugarloaf,
                self.mouse.x as f32,
                self.mouse.y as f32,
                scale,
            );
        }

        if self.renderer.trail_cursor_enabled {
            let current_grid = self.context_manager.current_grid();
            let scaled_margin = current_grid.get_scaled_margin();

            if let Some(current_item) = current_grid.current_item() {
                let layout = current_item.val.dimension;
                // Canonical integer stride — same value the GPU
                // shader uses; line_height is already baked in.
                let cell_width = layout.cell.cell_width as f32;
                let cell_height = layout.cell.cell_height as f32;
                let scale_factor = self.sugarloaf.scale_factor();

                let panel_rect = current_item.layout_rect;
                let origin_x = panel_rect[0] + scaled_margin.left;
                let origin_y = panel_rect[1] + scaled_margin.top;

                let current = self.context_manager.current();
                let cursor = &current.renderable_content.cursor;
                let cursor_row = cursor.state.pos.row.0 as usize;
                let cursor_col = cursor.state.pos.col.0;

                // Cursor position in physical pixels.
                let cursor_px_x = origin_x + cursor_col as f32 * cell_width;
                let cursor_px_y = origin_y + cursor_row as f32 * cell_height;

                self.renderer.trail_cursor.set_route(current.route_id);
                self.renderer.trail_cursor.set_destination(
                    cursor_px_x,
                    cursor_px_y,
                    cell_width,
                    cell_height,
                );
                self.renderer.trail_cursor.animate(cell_width, cell_height);

                let cursor_color = self.renderer.named_colors.cursor;
                self.renderer.trail_cursor.draw(
                    &mut self.sugarloaf,
                    scale_factor,
                    cursor_color,
                );
            }
        }

        // Phase 2.2/2.3: per-panel CellBg + CellText emission with
        // per-row dirty gating. Iterates every panel in the active
        // grid. For each:
        // - `damage == Noop | CursorOnly` + grid not forcing full:
        // skip `write_row` entirely. Cursor state is carried
        // by `GridUniforms`, so a pure blink/move doesn't
        // touch the cell buffers.
        // - `damage == Full` | first-frame | resize:
        // rebuild every visible row.
        // - `damage == Partial(lines)`:
        // rebuild only those rows.
        // Unchanged rows keep their CellBg + CellText resident in
        // the grid's CPU state, which is re-uploaded verbatim.
        {
            struct PanelFrame {
                route_id: usize,
                layout_rect: [f32; 4],
                cols: u32,
                rows: u32,
                cell_w: f32,
                cell_h: f32,
                font_px: f32,
                visible_rows: Vec<
                    rio_backend::crosswords::grid::row::Row<
                        rio_backend::crosswords::square::Square,
                    >,
                >,
                style_table: Vec<rio_backend::crosswords::style::Style>,
                /// Snapshot of the grid's extras table — needed to hash
                /// per-cell zero-width combining codepoints into the run
                /// shape key so cells with the same base codepoint but
                /// different combining marks don't alias in the cache.
                extras:
                    rustc_hash::FxHashMap<u16, rio_backend::crosswords::square::Extras>,
                term_colors: rio_backend::config::colors::term::TermColors,
                cursor_col: u16,
                cursor_row: u16,
                cursor_visible: bool,
                /// Terminal-side cursor shape (block / underline /
                /// beam / hidden). Driven by DECSCUSR + the
                /// configured default. Mapped to a render style
                /// inside the rebuild loop.
                cursor_shape: rio_backend::ansi::CursorShape,
                /// `true` when the terminal has cursor blink
                /// enabled (DECTCEM blink mode or SGR cursor blink).
                cursor_blinking: bool,
                /// `true` for the visible half of the blink cycle.
                /// Always `true` when blink isn't enabled. Driven
                /// by `Renderer::run`'s blink toggler.
                cursor_blink_visible: bool,
                /// `true` while an IME pre-edit string is active —
                /// forces a block cursor regardless of the
                /// configured shape so the user can tell IME is
                /// taking input.
                cursor_preedit: bool,
                /// Resolved cursor color: OSC 12 wins, then config /
                /// theme `cursor`.
                /// `state.colors.cursor → config.cursor_color`
                /// resolution. Per-panel
                /// because each terminal can issue its own OSC 12.
                cursor_color: rio_backend::config::colors::ColorArray,
                is_active: bool,
                damage: rio_backend::event::TerminalDamage,
                /// Selection is per-context (`renderable_content`), not
                /// per-terminal. Grabbed alongside the grid snapshot so
                /// `build_row_bg`/`build_row_fg` can tint selected cells.
                selection: Option<rio_backend::selection::SelectionRange>,
                /// `i - display_offset = absolute Line` for the
                /// per-row selection interval check. Snapshotted at
                /// the same lock as `visible_rows` to stay consistent.
                display_offset: i32,
                /// Search-hint matches for this panel. `None` when
                /// search is inactive. Consumed alongside `selection`
                /// inside `build_row_bg` / `build_row_fg` to apply
                /// `search_match_background` / `_foreground`.
                hint_matches: Option<Vec<rio_backend::crosswords::search::Match>>,
                /// Currently-focused search match (↑/↓ navigation).
                /// Rendered with `search_focused_match_background` /
                /// `_foreground` — `.search_selected`
                /// highlight tag.
                focused_match: Option<rio_backend::crosswords::search::Match>,
                /// (start, end) of the currently-hovered hyperlink /
                /// regex hint. Only populated for the active panel.
                /// Triggers the forced underline in `emit_underlines`;
                /// no bg / fg color change.
                hovered_hyperlink: Option<(
                    rio_backend::crosswords::pos::Pos,
                    rio_backend::crosswords::pos::Pos,
                )>,
                hint_labels: Option<Vec<crate::context::renderable::HintLabel>>,
                label_style_base: Option<u16>,
                /// Trigger highlight ranges with their per-rule bg color.
                trigger_highlights:
                    Option<Vec<(rio_backend::crosswords::search::Match, [u8; 4])>>,
            }

            let (active_key, scaled_margin) = {
                let grid = self.context_manager.current_grid();
                (grid.current, grid.scaled_margin)
            };
            // Snapshot the window's focused search match before the
            // per-context borrow below. `search_state` lives on
            // `Screen`, so we can't reach for it from inside the
            // `contexts_mut` iteration.
            let search_focused_match = self.search_state.focused_match.clone();
            let mut panels: Vec<PanelFrame> = Vec::new();
            for (key, item) in self
                .context_manager
                .current_grid_mut()
                .contexts_mut()
                .iter_mut()
            {
                let ctx = &mut item.val;
                let dim = ctx.dimension;
                // Canonical integer cell stride — single source of
                // truth for paint, layout, and mouse hit-test. The
                // bg fragment shader does
                // `floor((pixel - padding) / cell_size)` and the text
                // vertex multiplies `grid_pos * cell_size`, so both
                // sides must agree on the same integer stride or
                // adjacent columns drift to 7 vs 8 px wide and seams
                // show up.
                let cell_w = dim.cell.cell_width as f32;
                let cell_h = dim.cell.cell_height as f32;
                // Per-panel font size lives on `ContextDimension` since
                // the panel-state migration; sugarloaf is no longer
                // consulted. Per-panel zoom mutates
                // `dim.scaled_font_size` directly.
                let font_px = if dim.scaled_font_size > 0.0 {
                    dim.scaled_font_size
                } else {
                    let s = self.sugarloaf.style();
                    s.font_size * s.scale_factor
                };
                // The viewport snapshot was already taken by
                // `Renderer::run` for this context: visible rows,
                // per-cell styles, extras table, term colors, and
                // display offset all live on `ctx.renderable_content`.
                // No second terminal lock and no second materialize —
                // we take ownership of the buffers via `mem::take`
                // and put them back at the end of the render pass so
                // the next frame's `Renderer::run` resumes the same
                // allocations.
                let visible_rows =
                    std::mem::take(&mut ctx.renderable_content.visible_rows);
                let mut style_table =
                    std::mem::take(&mut ctx.renderable_content.style_table);
                let extras = std::mem::take(&mut ctx.renderable_content.extras);
                let term_colors = ctx.renderable_content.term_colors;
                let display_offset = ctx.renderable_content.display_offset as i32;
                let selection = ctx.renderable_content.selection_range;
                let cursor = &ctx.renderable_content.cursor;
                // Take + reset so next frame sees fresh damage only
                // from this frame's `Renderer::run`.
                let damage = std::mem::replace(
                    &mut ctx.renderable_content.frame_damage,
                    rio_backend::event::TerminalDamage::Noop,
                );
                let hint_matches = ctx.renderable_content.hint_matches.clone();
                let trigger_highlights =
                    ctx.renderable_content.trigger_highlights.clone();
                let is_active = *key == active_key;
                // `focused_match` lives on `Screen::search_state` — it's
                // a per-window state tied to whichever panel has search
                // focus, which is the active one. Don't paint a focused
                // highlight on non-active panels even if they happen to
                // carry hint_matches.
                let focused_match = if is_active {
                    search_focused_match.clone()
                } else {
                    None
                };
                // Only the active panel can be under the mouse, so
                // hyperlink-hover state only makes sense there. Same
                // reasoning as `focused_match` above.
                let hovered_hyperlink = if is_active {
                    ctx.renderable_content
                        .highlighted_hint
                        .as_ref()
                        .map(|h| (h.start, h.end))
                } else {
                    None
                };
                let hint_labels = if is_active {
                    std::mem::take(&mut ctx.renderable_content.hint_labels)
                } else {
                    None
                };
                let label_style_base = hint_labels
                    .as_deref()
                    .filter(|labels| !labels.is_empty())
                    .map(|_| {
                        crate::grid_emit::push_hint_label_styles(
                            &mut style_table,
                            self.renderer.named_colors.hint_foreground,
                            self.renderer.named_colors.hint_background,
                        )
                    });
                let cursor_shape = cursor.state.content;
                let cursor_blinking = ctx.renderable_content.has_blinking_enabled;
                let cursor_blink_visible =
                    !cursor_blinking || ctx.renderable_content.is_blinking_cursor_visible;
                let cursor_preedit = ctx.ime.preedit().is_some();
                // OSC 12 wins; otherwise fall back to the named-color
                // theme value. `Renderer::color`'s fallback (the
                // indexed-color List) is not populated for the Cursor
                // slot — `List::fill_named` skips it — so we read
                // `named_colors.cursor` directly.
                let cursor_color = term_colors
                    [rio_backend::config::colors::NamedColor::Cursor as usize]
                    .unwrap_or(self.renderer.named_colors.cursor);
                panels.push(PanelFrame {
                    route_id: ctx.route_id,
                    layout_rect: item.layout_rect,
                    cols: ctx.renderable_content.columns.max(1) as u32,
                    rows: ctx.renderable_content.screen_lines.max(1) as u32,
                    cell_w,
                    cell_h,
                    font_px,
                    visible_rows,
                    style_table,
                    extras,
                    term_colors,
                    cursor_col: cursor.state.pos.col.0 as u16,
                    cursor_row: cursor.state.pos.row.0 as u16,
                    cursor_visible: cursor.state.is_visible(),
                    cursor_shape,
                    cursor_blinking,
                    cursor_blink_visible,
                    cursor_preedit,
                    cursor_color,
                    is_active,
                    damage,
                    selection,
                    display_offset,
                    hint_matches,
                    focused_match,
                    hovered_hyperlink,
                    hint_labels,
                    label_style_base,
                    trigger_highlights,
                });
            }

            // --- ensure every panel has a matching GridRenderer ---
            for p in &panels {
                self.ensure_grid(p.route_id, p.cols, p.rows);
            }

            // --- emit cells + build uniforms per panel ---
            let window_size = self.sugarloaf.window_size();
            let font_library = self.sugarloaf.font_library().clone();
            let bg_col = self.renderer.named_colors.background.0;
            // Same `input_colorspace` value the Metal quad pipeline
            // feeds into `Globals` — the grid shader applies the
            // matching sRGB → DisplayP3 transform so cell bg, window
            // fill, and UI overlays produce identical framebuffer
            // colors. single `load_color` path.
            let input_colorspace = self.sugarloaf.input_colorspace();

            let mut frame_grids: Vec<(
                &mut rio_backend::sugarloaf::grid::GridRenderer,
                rio_backend::sugarloaf::grid::GridUniforms,
            )> = Vec::with_capacity(panels.len());

            let rasterizer = &mut self.grid_rasterizer;
            let renderer_ref = &self.renderer;
            for (route_id, grid) in self.grids.iter_mut() {
                let Some(p) = panels.iter_mut().find(|p| p.route_id == *route_id) else {
                    continue;
                };

                // Decide which rows to rebuild.
                //
                // `force_full` short-circuits damage to "rebuild all":
                // - grid was just created or resized (CPU buffers
                // are zeroed, so whatever damage says we have to
                // do a full fill).
                // - damage == Full (the terminal explicitly asked).
                //
                // `Noop` / `CursorOnly` → no row rebuilds, uniforms
                // alone carry the frame's state change.
                //
                // `Partial(lines)` → rebuild only those row indices.
                let force_full = grid.needs_full_rebuild()
                    || matches!(p.damage, rio_backend::event::TerminalDamage::Full);

                enum RowsToRebuild {
                    None,
                    All,
                    /// Per-row decision: walk `visible_rows` and
                    /// rebuild rows whose `dirty` bit is set.
                    Dirty,
                }
                let rows_to_rebuild = if force_full {
                    RowsToRebuild::All
                } else {
                    match p.damage {
                        rio_backend::event::TerminalDamage::Full => RowsToRebuild::All,
                        rio_backend::event::TerminalDamage::Partial => {
                            RowsToRebuild::Dirty
                        }
                        rio_backend::event::TerminalDamage::CursorOnly
                        | rio_backend::event::TerminalDamage::Noop => RowsToRebuild::None,
                    }
                };

                let cols = p.cols as usize;
                let mut bg_scratch: Vec<rio_backend::sugarloaf::grid::CellBg> =
                    Vec::with_capacity(cols);
                let mut fg_scratch: Vec<rio_backend::sugarloaf::grid::CellText> =
                    Vec::with_capacity(cols);
                let mut hint_scratch: Vec<crate::grid_emit::RowHint> = Vec::new();

                // Small helper: rebuild one row into the grid's
                // buffers. Closure-style to avoid duplicating the
                // body between the `All` and `Only` branches.
                //
                // Two passes now: `build_row_bg` emits `CellBg` per
                // cell (unconditional), `build_row_fg` does run-level
                // shaping + glyph emission (macOS only). The bg pass
                // never needs shaping so it runs on all platforms;
                // the fg path is macOS-specific pending the
                // wgpu+swash port.
                let mut rebuild_row =
                    |p: &PanelFrame,
                     y: usize,
                     grid: &mut rio_backend::sugarloaf::grid::GridRenderer,
                     rasterizer: &mut crate::grid_emit::GridGlyphRasterizer| {
                        let Some(row) = p.visible_rows.get(y) else {
                            return;
                        };
                        let style_table = p.style_table.as_slice();
                        let row_sel = crate::grid_emit::row_selection_for(
                            p.selection,
                            y,
                            cols,
                            p.display_offset,
                        );
                        crate::grid_emit::row_hints_for(
                            p.hint_matches.as_deref(),
                            p.focused_match.as_ref(),
                            p.hovered_hyperlink,
                            p.trigger_highlights.as_deref(),
                            y,
                            cols,
                            p.display_offset,
                            &mut hint_scratch,
                        );
                        let label_row;
                        let row = match (p.hint_labels.as_deref(), p.label_style_base) {
                            (Some(labels), Some(style_base)) => {
                                match crate::grid_emit::overlay_hint_labels(
                                    row,
                                    labels,
                                    y,
                                    p.display_offset,
                                    style_base,
                                    &mut hint_scratch,
                                ) {
                                    Some(overlaid) => {
                                        label_row = overlaid;
                                        &label_row
                                    }
                                    None => row,
                                }
                            }
                            _ => row,
                        };
                        crate::grid_emit::build_row_bg(
                            row,
                            cols,
                            style_table,
                            renderer_ref,
                            &p.term_colors,
                            row_sel,
                            &hint_scratch,
                            &mut bg_scratch,
                        );
                        let cursor_col_for_row = if p.cursor_visible
                            && (y as u16) == p.cursor_row
                            && p.cursor_shape != rio_backend::ansi::CursorShape::Hidden
                        {
                            Some(p.cursor_col)
                        } else {
                            None
                        };
                        crate::grid_emit::build_row_fg(
                            row,
                            cols,
                            y as u16,
                            style_table,
                            &p.extras,
                            renderer_ref,
                            &p.term_colors,
                            rasterizer,
                            grid,
                            p.font_px,
                            p.cell_w,
                            p.cell_h,
                            row_sel,
                            &hint_scratch,
                            &font_library,
                            p.route_id,
                            cursor_col_for_row,
                            &mut fg_scratch,
                        );
                        grid.write_row(y as u32, &bg_scratch, &fg_scratch);
                    };

                match rows_to_rebuild {
                    RowsToRebuild::None => {
                        // Nothing to rebuild — previous frame's
                        // CellBg/CellText stay resident. The GPU
                        // pass below still runs so updated uniforms
                        // (cursor_pos moved, etc.) take effect.
                    }
                    RowsToRebuild::All => {
                        // Clear the flag before rebuilding so an
                        // atlas-full clear inside `rebuild_row` can
                        // re-set it for the recovery pass below.
                        grid.mark_full_rebuild_done();
                        #[allow(clippy::needless_range_loop)]
                        for y in 0..p.visible_rows.len() {
                            rebuild_row(p, y, grid, rasterizer);
                        }
                    }
                    RowsToRebuild::Dirty => {
                        // Walk the snapshot rows; rebuild + clear the
                        // per-row dirty bit. Set by `snapshot_visible`
                        // for rows it copied this frame; cleared here
                        // so next frame starts clean.
                        #[allow(clippy::needless_range_loop)]
                        for y in 0..p.visible_rows.len() {
                            if !p.visible_rows[y].dirty {
                                continue;
                            }
                            rebuild_row(p, y, grid, rasterizer);
                            p.visible_rows[y].dirty = false;
                        }
                    }
                }

                // Atlas-full recovery: the backend cleared the atlas
                // during the rebuild above, so rows written before the
                // clear reference stale slots. Re-emit everything.
                if grid.needs_full_rebuild() {
                    grid.mark_full_rebuild_done();
                    for y in 0..p.visible_rows.len() {
                        rebuild_row(p, y, grid, rasterizer);
                    }
                }

                // Cursor pipeline (`addCursor` /
                // `cursor.style()`):
                // 1. Decide render style with strict priority:
                // preedit > visible > focused > blink > shape.
                // 2. Some(style): build the sprite for the block
                // slot (drawn under text; the bg-tint uniforms
                // below make the bg fragment paint the block +
                // the text shader invert the underlying glyph)
                // or the tail slot (bar/underline, drawn over
                // text). None: both stay empty + zero uniforms.
                // 3. One `grid.set_cursor(block, tail)` call
                // replaces both slots. It diffs against last
                // frame and only dirties cursor buffers on
                // change — do NOT clear the slots beforehand,
                // that would dirty them every frame.
                let render_style = crate::grid_emit::cursor_render_style(
                    crate::grid_emit::CursorRenderInputs {
                        visible: p.cursor_visible,
                        focused: p.is_active,
                        blink_visible: p.cursor_blink_visible,
                        blinking: p.cursor_blinking,
                        preedit: p.cursor_preedit,
                        shape: p.cursor_shape,
                    },
                );
                let mut block_cursor: Option<rio_backend::sugarloaf::grid::CellText> =
                    None;
                let mut tail_cursor: Option<rio_backend::sugarloaf::grid::CellText> =
                    None;
                if let Some(style) = render_style {
                    let cell_w = p.cell_w.round().clamp(1.0, u32::MAX as f32) as u32;
                    let cell_h = p.cell_h.round().clamp(1.0, u32::MAX as f32) as u32;
                    let cursor_color = [
                        (p.cursor_color[0].clamp(0.0, 1.0) * 255.0) as u8,
                        (p.cursor_color[1].clamp(0.0, 1.0) * 255.0) as u8,
                        (p.cursor_color[2].clamp(0.0, 1.0) * 255.0) as u8,
                        255,
                    ];
                    if let Some((is_block, cell)) = crate::grid_emit::cursor_sprite_cell(
                        grid,
                        style,
                        p.cursor_col,
                        p.cursor_row,
                        cursor_color,
                        cell_w,
                        cell_h,
                    ) {
                        if is_block {
                            block_cursor = Some(cell);
                        } else {
                            tail_cursor = Some(cell);
                        }
                    }
                }
                grid.set_cursor(block_cursor.as_slice(), tail_cursor.as_slice());

                // Panel's grid origin in drawable-pixel space =
                // window scaled_margin + the panel's layout rect
                // offset inside the root container. Snap to integer
                // pixels so `cell_size * grid_pos + grid_padding`
                // always lands on pixel boundaries. Without this, a
                // fractional margin (e.g. Taffy layout computing
                // 10.5px offsets) shifts the whole grid half a pixel
                // and the bg fragment's
                // `floor((pixel - padding) / cell_size)` disagrees
                // with the text vertex's `cell_size * grid_pos`
                // about where cell boundaries are → visible seams.
                let panel_left = (scaled_margin.left + p.layout_rect[0]).round();
                let panel_top = (scaled_margin.top + p.layout_rect[1]).round();

                // Bg-tint uniforms fire ONLY for the active block
                // style — the bg shader paints the cursor cell in
                // `cursor_bg_color` and the text shader swaps glyph
                // fg to `cursor_color` (so the character inverts on
                // top of the block). All other styles (bar /
                // underline / hollow) draw via the sprite emitted
                // above; their bg/text stays untouched. Same gate as
                // .
                let (cursor_pos, cursor_col_u, cursor_bg_u) = if matches!(
                    render_style,
                    Some(crate::grid_emit::CursorRenderStyle::Block)
                ) {
                    (
                        [p.cursor_col as u32, p.cursor_row as u32],
                        [bg_col[0], bg_col[1], bg_col[2], bg_col[3]],
                        [p.cursor_color[0], p.cursor_color[1], p.cursor_color[2], 1.0],
                    )
                } else {
                    ([u32::MAX; 2], [0.0; 4], [0.0; 4])
                };

                let uniforms = rio_backend::sugarloaf::grid::GridUniforms {
                    projection:
                        rio_backend::sugarloaf::components::core::orthographic_projection(
                            window_size.width,
                            window_size.height,
                        ),
                    // grid_padding = (top, right, bottom, left). The
                    // bg shader only reads `.w` (left) + `.x` (top)
                    // to anchor the grid, so right/bottom can stay
                    // 0. padding_extend is 0 too — each panel's
                    // grid must stay bounded to its own rect so
                    // sibling panels / the window margin aren't
                    // painted by this grid. The full-window bg fill
                    // (re-enabled in sugarloaf's render_metal) now
                    // handles the space outside all panels.
                    grid_padding: [panel_top, 0.0, 0.0, panel_left],
                    cursor_color: cursor_col_u,
                    cursor_bg_color: cursor_bg_u,
                    cell_size: [p.cell_w, p.cell_h],
                    grid_size: [p.cols, p.rows],
                    cursor_pos,
                    _pad_cursor: [0; 2],
                    min_contrast: 0.0,
                    flags: 0,
                    padding_extend: 0,
                    input_colorspace,
                };

                frame_grids.push((grid, uniforms));
            }

            if should_present {
                if frame_grids.is_empty() {
                    self.sugarloaf.render();
                } else {
                    self.sugarloaf.render_with_grids(&mut frame_grids);
                }
            } else {
                // Nothing to draw this frame, but `Renderer::run`
                // (plus overlays, borders, scrollbars, …) already
                // pushed into sugarloaf's per-frame queues. Drain
                // them so the next presented frame doesn't
                // composite them on top of their re-pushed selves.
                self.sugarloaf.discard_frame();
            }

            // Return each panel's snapshot buffers to the matching
            // context's `renderable_content` so the next frame's
            // `Renderer::run` can reuse the existing allocations
            // (rows, per-cell styles, extras table). Closed routes
            // simply drop their PanelFrame; the context (and its
            // renderable_content) is gone too.
            for (_, item) in self
                .context_manager
                .current_grid_mut()
                .contexts_mut()
                .iter_mut()
            {
                let route_id = item.val.route_id;
                if let Some(idx) = panels.iter().position(|p| p.route_id == route_id) {
                    let p = panels.swap_remove(idx);
                    let mut style_table = p.style_table;
                    if let Some(base) = p.label_style_base {
                        style_table.truncate(base as usize);
                    }
                    item.val.renderable_content.visible_rows = p.visible_rows;
                    item.val.renderable_content.style_table = style_table;
                    item.val.renderable_content.extras = p.extras;
                    item.val.renderable_content.hint_labels = p.hint_labels;
                }
            }
            panels.clear();
        }

        // Mark as dirty if we need continuous rendering (e.g.,
        // indeterminate progress bar, trail cursor animation). UI-only
        // — terminal cells didn't change, but we want the next vsync
        // to fire a render so overlays/animations tick forward.
        if has_animation {
            self.context_manager
                .current_mut()
                .renderable_content
                .pending_update
                .set_dirty();
        }

        if let Some(wake_in) = self.renderer.scrollbar.next_wake_in() {
            self.context_manager
                .schedule_render_on_route(wake_in.as_millis() as u64);
        }

        // In case the configuration of blinking cursor is enabled
        // TODO: enable blinking for selection after adding debounce (https://github.com/raphamorim/rio/issues/437)
        if self.renderer.is_window_focused
            && self.renderer.config_has_blinking_enabled
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

        window_update
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

        let current_grid = self.context_manager.current_grid();
        let scaled_margin = current_grid.get_scaled_margin();

        let Some(current_item) = current_grid.current_item() else {
            return;
        };

        let layout = current_item.val.dimension;
        let cursor_pos = current_item.val.renderable_content.cursor.state.pos;

        // Calculate pixel position of cursor — canonical integer
        // stride (line_height already baked into cell_height).
        let cell_width = layout.cell.cell_width as f32;
        let cell_height = layout.cell.cell_height as f32;

        // Validate dimensions before calculation
        if cell_width <= 0.0 || cell_height <= 0.0 {
            tracing::warn!(
                "Invalid cell dimensions for IME cursor positioning: {}x{}",
                cell_width,
                cell_height
            );
            return;
        }

        // Panel origin: layout_rect is relative to root container,
        // add scaled_margin to get absolute screen position
        let panel_rect = current_item.layout_rect;
        let origin_x = panel_rect[0] + scaled_margin.left;
        let origin_y = panel_rect[1] + scaled_margin.top;

        // Convert grid position to pixel position
        let pixel_x =
            origin_x + (cursor_pos.col.0 as f32 * cell_width) + (cell_width * 0.5);
        let pixel_y = origin_y + (cursor_pos.row.0 as f32 * cell_height);

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

    fn stop_hint_mode_if_active(&mut self) {
        if self.hint_state.is_active() {
            self.hint_state.stop();
            self.update_hint_state();
        }
    }

    /// Process a new character for keyboard hints
    #[allow(dead_code)]
    pub fn hint_input(&mut self, c: char, clipboard: &mut Clipboard) {
        let terminal = self.context_manager.current().terminal.lock();
        if let Some(hint_match) = self.hint_state.keyboard_input(&*terminal, c) {
            drop(terminal);
            self.execute_hint_action(&hint_match, clipboard);
            // Stop hint mode and update state with proper damage tracking
            self.hint_state.stop();
            self.update_hint_state();
        } else {
            drop(terminal);
            self.update_hint_state();
        }
        self.mark_dirty();
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

        self.mark_dirty();
    }

    /// Execute the action for a selected hint
    fn execute_hint_action(
        &mut self,
        hint_match: &crate::hints::HintMatch,
        clipboard: &mut Clipboard,
    ) {
        use rio_backend::config::hints::{HintAction, HintCommand, HintInternalAction};

        match &hint_match.hint.action {
            HintAction::Action { action } => match action {
                HintInternalAction::Copy => {
                    clipboard.set(ClipboardType::Clipboard, hint_match.text.clone());
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
                    self.mark_dirty();
                }
                HintInternalAction::MoveViModeCursor => {
                    // Move vi mode cursor to hint position
                    let mut terminal = self.context_manager.current().terminal.lock();
                    terminal.vi_mode_cursor.pos = hint_match.start;
                    drop(terminal);
                    self.mark_dirty();
                }
            },
            HintAction::Command { command } => {
                // If the match looks like a local path, resolve it against
                // the terminal's OSC 7 CWD and fall back to the raw text if
                // the path doesn't exist (or the text is a URL).
                let arg_text = {
                    let cwd = &self
                        .context_manager
                        .current()
                        .terminal
                        .lock()
                        .current_directory;
                    match crate::hints::resolve_path_for_opening(
                        &hint_match.text,
                        cwd.as_deref(),
                    ) {
                        Some(resolved) => resolved.to_string_lossy().into_owned(),
                        None => hint_match.text.clone(),
                    }
                };

                match command {
                    HintCommand::Simple(program) => {
                        self.exec(program, [&arg_text]);
                    }
                    HintCommand::WithArgs { program, args } => {
                        let mut all_args = args.clone();
                        all_args.push(arg_text);
                        self.exec(program, &all_args);
                    }
                }
            }
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

            // Hint state changed (search input, label visibility,
            // match selection). The visualization changes per-cell —
            // hint highlights, label glyphs — without touching cell
            // content, so we mark each affected line dirty on the
            // live grid. The next snapshot picks them up via the
            // per-row dirty walk and sets `visible_rows[y].dirty` so
            // GPU emit re-emits those rows. Coarse fallback when we
            // can't compute affected lines: `Full`.
            {
                let current = self.context_manager.current_mut();
                let hint_labels = current.renderable_content.hint_labels.clone();
                let hint_matches = current.renderable_content.hint_matches.clone();
                let mut terminal = current.terminal.lock();
                let display_offset = terminal.display_offset();
                let screen_lines = terminal.screen_lines();

                let visible_grid_line = |line: i32| -> bool {
                    let viewport_row = line + display_offset as i32;
                    viewport_row >= 0 && (viewport_row as usize) < screen_lines
                };
                let mut dirty_lines: Vec<i32> = Vec::new();
                for label in hint_labels.iter().flatten() {
                    let line = label.position.row.0;
                    if visible_grid_line(line) {
                        dirty_lines.push(line);
                    }
                }
                if let Some(hint_matches) = &hint_matches {
                    for hint_match in hint_matches {
                        for line in hint_match.start().row.0..=hint_match.end().row.0 {
                            if visible_grid_line(line) {
                                dirty_lines.push(line);
                            }
                        }
                    }
                }
                let any = !dirty_lines.is_empty();
                for line in dirty_lines {
                    terminal.grid[rio_backend::crosswords::pos::Line(line)].dirty = true;
                }
                drop(terminal);

                current
                    .renderable_content
                    .pending_update
                    .set_terminal_damage(if any {
                        TerminalDamage::Partial
                    } else {
                        TerminalDamage::Full
                    });
            }
        } else if !self.search_active() {
            // Clear hint state only if search is not active,
            // since search also uses hint_matches for highlighting
            self.context_manager
                .current_mut()
                .renderable_content
                .hint_matches = None;
            self.context_manager
                .current_mut()
                .renderable_content
                .hint_labels = None;
            // Force full damage to clear all hint highlights
            let current = self.context_manager.current_mut();
            current
                .renderable_content
                .pending_update
                .set_terminal_damage(TerminalDamage::Full);
        }
    }

    fn update_hint_labels(&mut self) {
        use crate::context::renderable::HintLabel;

        let hint_labels = if self.hint_state.is_active() {
            let matches = self.hint_state.matches();
            let visible_labels = self.hint_state.visible_labels();

            let mut labels = Vec::new();
            for (match_index, remaining_label) in visible_labels {
                if let Some(hint_match) = matches.get(match_index) {
                    // Create labels for each character in the hint label
                    for (char_index, &label_char) in remaining_label.iter().enumerate() {
                        let position = rio_backend::crosswords::pos::Pos::new(
                            hint_match.start.row,
                            hint_match.start.col + char_index,
                        );

                        labels.push(HintLabel {
                            position,
                            label: label_char,
                            is_first: char_index == 0, // First character gets different styling
                        });
                    }
                }
            }
            Some(labels)
        } else {
            None
        };

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
                let c = indexed.square.c();
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
                let c = indexed.square.c();
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
    fn chrome_press_validates_double_click() {
        use rio_window::dpi::PhysicalPosition;
        let origin = Some(PhysicalPosition::new(10, 20));

        // Same origin, fresh → a chrome double-click.
        let fresh = ChromePress {
            window_origin: origin,
            at: std::time::Instant::now(),
        };
        assert!(fresh.validates_double_click(origin));

        // Window moved between the presses (a re-grab after a window
        // drag) → keep dragging, don't maximize.
        assert!(!fresh.validates_double_click(Some(PhysicalPosition::new(110, 20))));

        // Unreported origin on both presses (Wayland) → time guard
        // alone decides; a reported-vs-unreported mix never validates.
        let unknown = ChromePress {
            window_origin: None,
            at: std::time::Instant::now(),
        };
        assert!(unknown.validates_double_click(None));
        assert!(!unknown.validates_double_click(origin));

        // Stale press → expired even at the same origin.
        let stale = ChromePress {
            window_origin: origin,
            at: std::time::Instant::now() - crate::constants::MULTI_CLICK_THRESHOLD * 2,
        };
        assert!(!stale.validates_double_click(origin));
    }

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
