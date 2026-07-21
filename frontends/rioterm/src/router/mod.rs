pub mod routes;
mod window;
use crate::event::EventProxy;
use crate::renderer::session_prompt::SessionPromptKind;
use crate::router::window::{
    configure_window, create_window_builder, DEFAULT_MINIMUM_WINDOW_HEIGHT,
    DEFAULT_MINIMUM_WINDOW_WIDTH,
};
use crate::screen::{Screen, ScreenWindowProperties};
use assistant::Assistant;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use rio_backend::clipboard::Clipboard;
use rio_backend::config::Config as RioConfig;
use rio_backend::error::{RioError, RioErrorLevel, RioErrorType};

use rio_window::dpi::{PhysicalPosition, PhysicalSize};
use rio_window::event_loop::ActiveEventLoop;
use rio_window::keyboard::{Key, NamedKey};
#[cfg(not(any(target_os = "macos", windows)))]
use rio_window::platform::startup_notify::{
    self, EventLoopExtStartupNotify, WindowAttributesExtStartupNotify,
};
use rio_window::window::{Window, WindowId};
use routes::{assistant, RoutePath};
use rustc_hash::FxHashMap;
use std::time::{Duration, Instant};

// 𜱭𜱭 unicode is not available yet for all OS
// https://www.unicode.org/charts/PDF/Unicode-16.0/U160-1CC00.pdf
// #[cfg(any(target_os = "macos", target_os = "windows"))]
// const RIO_TITLE: &str = "𜱭𜱭";
// #[cfg(not(any(target_os = "macos", target_os = "windows")))]
const RIO_TITLE: &str = "▲";

pub struct Route<'a> {
    pub assistant: assistant::Assistant,
    pub path: RoutePath,
    pub window: RouteWindow<'a>,
    /// Parsed session waiting on the launch "resume?" answer.
    pub pending_session: Option<crate::session::SessionState>,
    /// Named-session binding (`rio --session <name>`): saves and
    /// quit-prompts target `sessions/<name>.json` instead of the
    /// implicit last-session slot.
    pub session_name: Option<String>,
}

impl Route<'_> {
    /// Create a performer.
    #[inline]
    pub fn new(
        assistant: assistant::Assistant,
        path: RoutePath,
        window: RouteWindow,
    ) -> Route {
        Route {
            assistant,
            path,
            window,
            pending_session: None,
            session_name: None,
        }
    }
}

impl Route<'_> {
    #[inline]
    pub fn request_redraw(&mut self) {
        self.window.winit_window.request_redraw();
    }

    /// Mark the active context dirty (UI-only) and request a redraw
    /// at the next vsync. Used by overlay input paths (command palette,
    /// assistant, island rename) where the UI changed but terminal
    /// cells didn't. `set_dirty` passes `Renderer::run`'s per-context
    /// gate; the inner damage match hits
    /// `(None, None) => TerminalDamage::Noop` so rows don't rebuild,
    /// and the overlay itself is drawn unconditionally after the loop.
    #[inline]
    pub fn request_overlay_redraw(&mut self) {
        self.window
            .screen
            .ctx_mut()
            .current_mut()
            .renderable_content
            .pending_update
            .set_dirty();
        self.request_redraw();
    }

    #[inline]
    pub fn begin_render(&mut self) {
        self.window.render_timestamp = Instant::now();
    }

    #[inline]
    pub fn update_config(
        &mut self,
        config: &RioConfig,
        db: &rio_backend::sugarloaf::font::FontLibrary,
        should_update_font: bool,
    ) {
        self.window
            .screen
            .update_config(config, db, should_update_font);
    }

    #[inline]
    #[allow(unused_variables)]
    pub fn set_window_subtitle(&mut self, subtitle: &str) {
        #[cfg(target_os = "macos")]
        self.window.winit_window.set_subtitle(subtitle);
    }

    #[inline]
    pub fn set_window_title(&mut self, title: &str) {
        self.window.winit_window.set_title(title);
    }

    #[inline]
    pub fn report_error(&mut self, error: &RioError) {
        if error.report == RioErrorType::ConfigurationNotFound {
            self.path = RoutePath::Welcome;
            return;
        }

        self.assistant.set(error.to_owned());
        self.window
            .screen
            .renderer
            .assistant
            .set_error(error.to_owned());
    }

    #[inline]
    pub fn clear_errors(&mut self) {
        self.assistant.clear();
        self.window.screen.renderer.assistant.clear();
        self.path = RoutePath::Terminal;
    }

    #[inline]
    pub fn confirm_quit(&mut self) {
        self.window.screen.renderer.confirm_quit.set_active(true);
        self.request_overlay_redraw();
    }

    #[inline]
    pub fn quit(&mut self) {
        use rio_backend::config::session::SessionRestore;
        match self.window.screen.renderer.session_restore {
            SessionRestore::Prompt => {
                self.window.screen.renderer.confirm_quit.set_active(false);
                self.window
                    .screen
                    .renderer
                    .session_prompt
                    .set_active(Some(SessionPromptKind::SaveOnExit));
                self.request_overlay_redraw();
                return;
            }
            SessionRestore::Always => self.save_session(),
            SessionRestore::Never => {}
        }
        std::process::exit(0);
    }

    /// Target file for this window's session saves: the bound name
    /// (CLI/save-as) or the implicit last-session slot.
    fn session_path(&self) -> std::path::PathBuf {
        match &self.session_name {
            Some(name) => {
                let _ = std::fs::create_dir_all(rio_backend::config::sessions_dir_path());
                rio_backend::config::session_named_path(name)
            }
            None => rio_backend::config::session_file_path(),
        }
    }

    /// Persist this window's tabs/splits/CWDs/scrollback to disk.
    pub fn save_session(&mut self) {
        let max = self.window.screen.renderer.session_max_scrollback;
        let state = crate::session::SessionState {
            version: crate::session::SESSION_VERSION,
            windows: vec![crate::session::capture_window(
                self.window.screen.ctx(),
                max,
                &self.window.winit_window,
            )],
        };
        if let Err(err) = state.save(&self.session_path()) {
            tracing::warn!("session save failed: {err}");
        }
    }

    /// Palette "Save Session As": bind this window to `name` and save.
    pub fn save_session_as(&mut self, name: &str) {
        let name = crate::session::sanitize_name(name);
        if name.is_empty() {
            return;
        }
        self.session_name = Some(name);
        self.save_session();
    }

    /// Palette "Restore Session": append a named session's tabs to
    /// this window. Named sessions are workspaces — never consumed.
    pub fn restore_session_named(&mut self, name: &str) {
        let path = rio_backend::config::session_named_path(name);
        if let Some(state) = crate::session::SessionState::load(&path) {
            self.session_name = Some(name.to_string());
            self.restore_session_inner(state, false);
        }
    }

    /// Offer to resume `state` (activates the launch prompt).
    pub fn prompt_session_resume(&mut self, state: crate::session::SessionState) {
        self.pending_session = Some(state);
        self.window
            .screen
            .renderer
            .session_prompt
            .set_active(Some(SessionPromptKind::ResumeOnLaunch));
        self.request_overlay_redraw();
    }

    /// Rebuild tabs/splits/scrollback from a saved session, replacing
    /// the window's default tab (launch-time restore).
    pub fn restore_session(&mut self, state: crate::session::SessionState) {
        self.restore_session_inner(state, true);
    }

    fn restore_session_inner(
        &mut self,
        state: crate::session::SessionState,
        replace: bool,
    ) {
        let Some(win) = state.windows.into_iter().next() else {
            return;
        };
        if win.tabs.is_empty() {
            return;
        }
        if replace && win.size.0 > 0 && win.size.1 > 0 {
            // When the platform applies the size immediately (Wayland),
            // no Resized event follows — run the resize pipeline here so
            // the layout and the tabs built below use the final size.
            if let Some(applied) = self
                .window
                .winit_window
                .request_inner_size(PhysicalSize::new(win.size.0, win.size.1))
            {
                if applied.width > 0 && applied.height > 0 {
                    self.window.screen.resize(applied);
                }
            }
        }
        // No-op on Wayland: the compositor owns placement.
        if let (true, Some((x, y))) = (replace, win.position) {
            self.window
                .winit_window
                .set_outer_position(PhysicalPosition::new(x, y));
        }
        let screen = &mut self.window.screen;
        for tab in &win.tabs {
            let first = tab.layout.first_leaf();

            // Mirror Screen::create_tab: grow the top margin before the
            // tab exists (hide_if_single -> visible transition) and
            // reposition the previous tab's rich texts.
            let num_tabs = screen.ctx().len();
            let old_index = screen.context_manager.current_index();
            screen.resize_top_or_bottom_line(num_tabs + 1);
            #[cfg(not(target_os = "macos"))]
            screen.context_manager.contexts_mut()[old_index]
                .update_dimensions(&mut screen.sugarloaf);

            screen.ctx_mut().add_context_with_dir(
                crate::context::next_rich_text_id(),
                first.cwd.clone(),
            );
            let new_index = screen.context_manager.current_index();
            screen.context_manager.switch_context_visibility(
                &mut screen.sugarloaf,
                old_index,
                new_index,
            );
            screen.ctx_mut().current_grid_mut().custom_title = tab.custom_title.clone();
            crate::session::restore_tab_layout(
                &mut screen.context_manager,
                &tab.layout,
                &mut screen.sugarloaf,
            );
        }
        if replace {
            // Drop the default tab the window opened with; the restored
            // tabs then sit at their saved indices.
            screen.ctx_mut().select_tab(0);
            screen
                .context_manager
                .close_current_context(&mut screen.sugarloaf);
            let num_tabs = screen.ctx().len();
            screen.resize_top_or_bottom_line(num_tabs);
            screen
                .ctx_mut()
                .select_tab(win.active_tab.min(win.tabs.len().saturating_sub(1)));
        }
        screen.resize_all_contexts();

        // The implicit last-session slot is consumed once restored;
        // quitting offers a fresh save, so a stale copy must not
        // re-prompt next launch. Named sessions persist.
        if replace && self.session_name.is_none() {
            crate::session::SessionState::discard(
                &rio_backend::config::session_file_path(),
            );
        }
        self.request_redraw();
    }

    #[inline]
    pub fn has_key_wait(
        &mut self,
        key_event: &rio_window::event::KeyEvent,
        clipboard: &mut Clipboard,
    ) -> bool {
        use rio_window::event::ElementState;

        // Handle island color picker / rename input
        if let Some(ref mut island) = self.window.screen.renderer.island {
            if island.is_color_picker_open() {
                let consumed = island.handle_rename_input(
                    key_event,
                    &mut self.window.screen.context_manager,
                );
                if consumed {
                    self.request_overlay_redraw();
                    return true;
                }
            }
        }

        // Handle command palette input first (works in all routes)
        if self.window.screen.renderer.command_palette.is_enabled() {
            if key_event.state == ElementState::Pressed {
                match &key_event.logical_key {
                    Key::Named(NamedKey::Escape) => {
                        self.window
                            .screen
                            .renderer
                            .command_palette
                            .set_enabled(false);
                        self.request_overlay_redraw();
                    }
                    Key::Named(NamedKey::ArrowUp) => {
                        self.window
                            .screen
                            .renderer
                            .command_palette
                            .move_selection_up();
                        self.request_overlay_redraw();
                    }
                    Key::Named(NamedKey::ArrowDown) => {
                        self.window
                            .screen
                            .renderer
                            .command_palette
                            .move_selection_down();
                        self.request_overlay_redraw();
                    }
                    Key::Named(NamedKey::Tab) => {
                        self.window
                            .screen
                            .renderer
                            .command_palette
                            .move_selection_down();
                        self.request_overlay_redraw();
                    }
                    Key::Named(NamedKey::Enter) => {
                        // Snapshot what the palette wants to do FIRST,
                        // before taking a mut-borrow on it, so we can
                        // freely call other `self.window.screen.*`
                        // methods in the match arms without tripping
                        // the borrow checker on nested disjoint borrows.
                        let selected_font = self
                            .window
                            .screen
                            .renderer
                            .command_palette
                            .get_selected_font();
                        let selected_action = self
                            .window
                            .screen
                            .renderer
                            .command_palette
                            .get_selected_action();
                        use crate::renderer::command_palette::PaletteAction;

                        // Sessions-mode Enter: save-as or restore the
                        // picked name, then close the palette.
                        if let Some((name, saving)) = self
                            .window
                            .screen
                            .renderer
                            .command_palette
                            .get_selected_session()
                        {
                            self.window
                                .screen
                                .renderer
                                .command_palette
                                .set_enabled(false);
                            if saving {
                                self.window
                                    .screen
                                    .context_manager
                                    .request_save_session_as(name);
                            } else {
                                self.window
                                    .screen
                                    .context_manager
                                    .request_restore_session_named(name);
                            }
                            self.request_overlay_redraw();
                            return true;
                        }

                        // Fonts-mode Enter: copy the family name to
                        // the system clipboard and close. The copy
                        // icon on each row advertises this.
                        if let Some(font) = selected_font {
                            clipboard.set(
                                rio_backend::clipboard::ClipboardType::Clipboard,
                                font,
                            );
                            self.window
                                .screen
                                .renderer
                                .command_palette
                                .set_enabled(false);
                            self.request_overlay_redraw();
                            return true;
                        }

                        match selected_action {
                            // `ListFonts` stays inside the palette —
                            // swap the palette's contents from the
                            // command list to the registered font
                            // family names and keep it open.
                            Some(PaletteAction::ListFonts) => {
                                let fonts =
                                    self.window.screen.sugarloaf.font_family_names();
                                self.window
                                    .screen
                                    .renderer
                                    .command_palette
                                    .enter_fonts_mode(fonts);
                            }
                            // Sessions pickers stay inside the palette,
                            // swapping to the saved-session list.
                            Some(
                                action @ (PaletteAction::SaveSessionAs
                                | PaletteAction::RestoreSessionPicker),
                            ) => {
                                let names = crate::session::list_sessions();
                                self.window
                                    .screen
                                    .renderer
                                    .command_palette
                                    .enter_sessions_mode(
                                        names,
                                        action == PaletteAction::SaveSessionAs,
                                    );
                            }
                            // Any other command is a one-shot: close
                            // the palette first, then dispatch.
                            Some(action) => {
                                self.window
                                    .screen
                                    .renderer
                                    .command_palette
                                    .set_enabled(false);
                                self.window
                                    .screen
                                    .execute_palette_action(action, clipboard);
                            }
                            // No match at all — Enter just closes.
                            None => {
                                self.window
                                    .screen
                                    .renderer
                                    .command_palette
                                    .set_enabled(false);
                            }
                        }
                        self.request_overlay_redraw();
                    }
                    Key::Named(NamedKey::Backspace) => {
                        let current_query =
                            self.window.screen.renderer.command_palette.query.clone();
                        if !current_query.is_empty() {
                            let mut chars = current_query.chars().collect::<Vec<_>>();
                            chars.pop();
                            self.window
                                .screen
                                .renderer
                                .command_palette
                                .set_query(chars.into_iter().collect());
                            self.request_overlay_redraw();
                        }
                    }
                    _ => {
                        if let Some(text) = key_event.text.as_ref() {
                            // Filter out control characters
                            let text_str = text.as_str();
                            if !text_str.is_empty()
                                && text_str.chars().all(|c| !c.is_control())
                            {
                                let current_query = self
                                    .window
                                    .screen
                                    .renderer
                                    .command_palette
                                    .query
                                    .clone();
                                self.window
                                    .screen
                                    .renderer
                                    .command_palette
                                    .set_query(format!("{}{}", current_query, text_str));
                                self.request_overlay_redraw();
                            }
                        }
                    }
                }
            }
            return true; // Block all input when command palette is active
        }

        if self.window.screen.renderer.confirm_quit.is_active() {
            if key_event.state == rio_window::event::ElementState::Pressed {
                match &key_event.logical_key {
                    Key::Character(c) if c.as_str() == "n" || c.as_str() == "N" => {
                        self.window.screen.renderer.confirm_quit.set_active(false);
                        self.request_overlay_redraw();
                    }
                    Key::Named(NamedKey::Escape) => {
                        self.window.screen.renderer.confirm_quit.set_active(false);
                        self.request_overlay_redraw();
                    }
                    Key::Character(c) if c.as_str() == "y" || c.as_str() == "Y" => {
                        self.window.screen.renderer.confirm_quit.set_active(false);
                        self.quit();
                        return true;
                    }
                    _ => {}
                }
            }
            return true;
        }

        if let Some(kind) = self.window.screen.renderer.session_prompt.kind() {
            if key_event.state == rio_window::event::ElementState::Pressed {
                match (&key_event.logical_key, kind) {
                    (Key::Character(c), SessionPromptKind::SaveOnExit)
                        if c.as_str() == "y" || c.as_str() == "Y" =>
                    {
                        self.save_session();
                        std::process::exit(0);
                    }
                    (Key::Character(c), SessionPromptKind::SaveOnExit)
                        if c.as_str() == "n" || c.as_str() == "N" =>
                    {
                        std::process::exit(0);
                    }
                    (Key::Character(c), SessionPromptKind::ResumeOnLaunch)
                        if c.as_str() == "y" || c.as_str() == "Y" =>
                    {
                        self.window.screen.renderer.session_prompt.set_active(None);
                        if let Some(state) = self.pending_session.take() {
                            self.restore_session(state);
                        }
                        self.request_overlay_redraw();
                    }
                    (Key::Character(c), SessionPromptKind::ResumeOnLaunch)
                        if c.as_str() == "n" || c.as_str() == "N" =>
                    {
                        self.window.screen.renderer.session_prompt.set_active(None);
                        self.pending_session = None;
                        crate::session::SessionState::discard(
                            &rio_backend::config::session_file_path(),
                        );
                        self.request_overlay_redraw();
                    }
                    // Esc cancels: on exit it aborts the quit, on
                    // launch it keeps the file for next time.
                    (Key::Named(NamedKey::Escape), _) => {
                        self.window.screen.renderer.session_prompt.set_active(None);
                        self.pending_session = None;
                        self.request_overlay_redraw();
                    }
                    _ => {}
                }
            }
            return true;
        }

        if self.path == RoutePath::Terminal {
            return false;
        }

        let is_enter = key_event.logical_key == Key::Named(NamedKey::Enter);

        // Handle assistant overlay dismiss
        if self.window.screen.renderer.assistant.is_active() {
            if is_enter {
                self.assistant.clear();
                self.window.screen.renderer.assistant.clear();
                self.request_overlay_redraw();
            }
            return true;
        }

        if self.path == RoutePath::Welcome && is_enter {
            rio_backend::config::create_config_file(None);
            self.path = RoutePath::Terminal;
        }

        false
    }
}

pub struct Router<'a> {
    pub routes: FxHashMap<WindowId, Route<'a>>,
    propagated_report: Option<RioError>,
    pub font_library: Box<rio_backend::sugarloaf::font::FontLibrary>,
    pub config_route: Option<WindowId>,
    pub clipboard: Clipboard,
    current_tab_id: u64,
}

impl Router<'_> {
    pub fn new<'b>(
        fonts: rio_backend::sugarloaf::font::SugarloafFonts,
        clipboard: Clipboard,
    ) -> Router<'b> {
        let (font_library, fonts_not_found) =
            rio_backend::sugarloaf::font::FontLibrary::new(fonts);

        let mut propagated_report = None;

        if let Some(err) = fonts_not_found {
            propagated_report = Some(RioError {
                report: RioErrorType::FontsNotFound(err.fonts_not_found),
                level: RioErrorLevel::Warning,
            });
        }

        Router {
            routes: FxHashMap::default(),
            propagated_report,
            config_route: None,
            font_library: Box::new(font_library),
            clipboard,
            current_tab_id: 0,
        }
    }

    #[inline]
    pub fn propagate_error_to_next_route(&mut self, error: RioError) {
        self.propagated_report = Some(error);
    }

    #[inline]
    pub fn update_titles(&mut self) {
        for route in self.routes.values_mut() {
            if route.window.is_focused {
                route.window.screen.context_manager.update_titles();
            }
        }
    }

    #[inline]
    pub fn get_focused_route(&self) -> Option<WindowId> {
        self.routes
            .iter()
            .find_map(|(key, val)| {
                if val.window.winit_window.has_focus() {
                    Some(key)
                } else {
                    None
                }
            })
            .copied()
    }

    pub fn open_config_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        event_proxy: EventProxy,
        config: &RioConfig,
    ) {
        // In case configuration window does exists already
        if let Some(route_id) = self.config_route {
            if let Some(route) = self.routes.get(&route_id) {
                route.window.winit_window.focus_window();
                return;
            }
        }

        let current_config: RioConfig = config.clone();
        let editor = config.editor.clone();
        let mut args = editor.args;
        args.push(
            rio_backend::config::config_file_path()
                .display()
                .to_string(),
        );
        let new_config = RioConfig {
            shell: rio_backend::config::Shell {
                program: editor.program,
                args,
            },
            ..current_config
        };

        let window = RouteWindow::from_target(
            event_loop,
            event_proxy,
            &new_config,
            &self.font_library,
            "Rio Settings",
            None,
            None,
            None,
        );
        let id = window.winit_window.id();
        let route = Route::new(Assistant::new(), RoutePath::Terminal, window);
        self.routes.insert(id, route);
        self.config_route = Some(id);
    }

    pub fn open_config_split(&mut self, config: &RioConfig) {
        let current_config: RioConfig = config.clone();
        let editor = config.editor.clone();
        let mut args = editor.args;
        args.push(
            rio_backend::config::config_file_path()
                .display()
                .to_string(),
        );
        let new_config = RioConfig {
            shell: rio_backend::config::Shell {
                program: editor.program,
                args,
            },
            ..current_config
        };

        let window_id = match self.get_focused_route() {
            Some(window_id) => window_id,
            None => return,
        };

        let route = match self.routes.get_mut(&window_id) {
            Some(window) => window,
            None => return,
        };

        route.window.screen.split_right_with_config(new_config);
    }

    #[inline]
    pub fn create_window<'a>(
        &'a mut self,
        event_loop: &'a ActiveEventLoop,
        event_proxy: EventProxy,
        config: &'a rio_backend::config::Config,
        open_url: Option<String>,
        app_id: Option<&str>,
    ) {
        let tab_id = if config.navigation.is_native() {
            let id = self.current_tab_id;
            self.current_tab_id = self.current_tab_id.wrapping_add(1);
            Some(id.to_string())
        } else {
            None
        };

        let window = RouteWindow::from_target(
            event_loop,
            event_proxy,
            config,
            &self.font_library,
            RIO_TITLE,
            tab_id.as_deref(),
            open_url,
            app_id,
        );
        let id = window.winit_window.id();

        let mut route = Route {
            window,
            path: RoutePath::Terminal,
            assistant: Assistant::new(),
            pending_session: None,
            session_name: None,
        };

        if let Some(err) = &self.propagated_report {
            route.report_error(err);
            self.propagated_report = None;
        }

        self.routes.insert(id, route);
    }

    #[cfg(target_os = "macos")]
    #[inline]
    pub fn create_native_tab<'a>(
        &'a mut self,
        event_loop: &'a ActiveEventLoop,
        event_proxy: EventProxy,
        config: &'a rio_backend::config::Config,
        tab_id: Option<&str>,
        open_url: Option<String>,
    ) {
        let window = RouteWindow::from_target(
            event_loop,
            event_proxy,
            config,
            &self.font_library,
            RIO_TITLE,
            tab_id,
            open_url,
            None,
        );
        self.routes.insert(
            window.winit_window.id(),
            Route {
                window,
                path: RoutePath::Terminal,
                assistant: Assistant::new(),
                pending_session: None,
                session_name: None,
            },
        );
    }
}

pub struct RouteWindow<'a> {
    pub is_focused: bool,
    pub is_occluded: bool,
    pub needs_render_after_occlusion: bool,
    pub render_timestamp: Instant,
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub vblank_interval: Duration,
    pub winit_window: Window,
    pub screen: Screen<'a>,
}

impl<'a> RouteWindow<'a> {
    pub fn configure_window(&mut self, config: &rio_backend::config::Config) {
        configure_window(&self.winit_window, config);
    }

    pub fn wait_until(&self) -> Option<Duration> {
        // If we need to render after occlusion, render immediately
        if self.needs_render_after_occlusion {
            return None;
        }

        // On macOS, CVDisplayLink handles VSync synchronization automatically,
        // so we don't need software-based frame timing calculations
        #[cfg(target_os = "macos")]
        {
            None
        }

        #[cfg(not(target_os = "macos"))]
        {
            let now = Instant::now();
            let elapsed = now.duration_since(self.render_timestamp);
            let vblank = self.vblank_interval;

            // Calculate how many complete frames have elapsed
            let frames_elapsed = elapsed.as_nanos() / vblank.as_nanos();

            // Calculate when the next frame should occur
            let next_frame_time = self.render_timestamp
                + Duration::from_nanos(
                    (frames_elapsed + 1) as u64 * vblank.as_nanos() as u64,
                );

            if next_frame_time > now {
                // Return the time to wait until the next ideal frame time
                Some(next_frame_time.duration_since(now))
            } else {
                // We've missed the target frame time, render immediately
                None
            }
        }
    }

    // TODO: Use it whenever animated cursor is done
    // pub fn request_animation_frame(&mut self) {
    //     if self.config.renderer.strategy.is_event_based() {
    //         // Schedule a render for the next frame time
    //         let route_id = self.window.screen.ctx().current_route();
    //         let timer_id = TimerId::new(Topic::RenderRoute, route_id);
    //         let event = EventPayload::new(
    //             RioEventType::Rio(RioEvent::RenderRoute(route_id)),
    //             self.window.winit_window.id(),
    //         );

    //         // Always schedule at the next vblank interval
    //         self.scheduler.schedule(event, self.window.vblank_interval, false, timer_id);
    //     } else {
    //         // For game loop rendering, the standard redraw is fine
    //         self.request_redraw();
    //     }
    // }

    #[inline]
    pub fn update_vblank_interval(&mut self) {
        // On macOS, CVDisplayLink handles VSync synchronization automatically,
        // so we don't need to calculate vblank intervals
        #[cfg(not(target_os = "macos"))]
        {
            // Always update vblank interval based on monitor refresh rate
            // Get the display refresh rate, default to 60Hz if unavailable
            let refresh_rate_hz = self
                .winit_window
                .current_monitor()
                .and_then(|monitor| monitor.refresh_rate_millihertz())
                .unwrap_or(60_000) as f64
                / 1000.0; // Convert millihertz to Hz

            // Calculate frame time in microseconds (1,000,000 µs / refresh_rate)
            let frame_time_us = (1_000_000.0 / refresh_rate_hz) as u64;
            self.vblank_interval = Duration::from_micros(frame_time_us);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_target<'b>(
        event_loop: &'b ActiveEventLoop,
        event_proxy: EventProxy,
        config: &'b RioConfig,
        font_library: &rio_backend::sugarloaf::font::FontLibrary,
        window_name: &str,
        tab_id: Option<&str>,
        open_url: Option<String>,
        app_id: Option<&str>,
    ) -> RouteWindow<'a> {
        #[allow(unused_mut)]
        let mut window_builder =
            create_window_builder(window_name, config, tab_id, app_id);

        #[cfg(not(any(target_os = "macos", windows)))]
        if let Some(token) = event_loop.read_token_from_env() {
            tracing::debug!("Activating window with token: {token:?}");
            window_builder = window_builder.with_activation_token(token);

            // Remove the token from the env.
            startup_notify::reset_activation_token_env();
        }

        let winit_window = event_loop.create_window(window_builder).unwrap();
        configure_window(&winit_window, config);

        let properties = ScreenWindowProperties {
            size: winit_window.inner_size(),
            scale: winit_window.scale_factor(),
            raw_window_handle: winit_window.window_handle().unwrap().into(),
            raw_display_handle: winit_window.display_handle().unwrap().into(),
            window_id: winit_window.id(),
        };

        let screen = Screen::new(properties, config, event_proxy, font_library, open_url)
            .expect("Screen not created");

        if config.window.columns.is_some() || config.window.rows.is_some() {
            let (physical_width, physical_height) = compute_window_size_from_grid(
                config.window.columns,
                config.window.rows,
                &config.panel,
                &screen.ctx().current().dimension,
                winit_window.inner_size(),
            );
            let _ = winit_window.request_inner_size(PhysicalSize {
                width: physical_width,
                height: physical_height,
            });
            if let Some(pos) =
                centered_position(event_loop, physical_width, physical_height)
            {
                winit_window.set_outer_position(pos);
            }
        }

        #[cfg(target_os = "windows")]
        {
            // On windows cloak (hide) the window initially, we later reveal it after the first draw.
            // This is a workaround to hide the "white flash" that occurs during application startup.
            use rio_window::platform::windows::WindowExtWindows;
            winit_window.set_cloaked(false);
        }

        // Get the display refresh rate and convert to frame interval
        // On macOS, CVDisplayLink handles VSync synchronization automatically,
        // so we don't need to calculate vblank intervals
        #[cfg(target_os = "macos")]
        let monitor_vblank_interval = Duration::from_micros(16667); // Placeholder value, not used

        #[cfg(not(target_os = "macos"))]
        let monitor_vblank_interval = {
            let monitor_refresh_rate_hz = winit_window
                .current_monitor()
                .and_then(|monitor| monitor.refresh_rate_millihertz())
                .unwrap_or(60_000) as f64
                / 1000.0;

            // Convert to microseconds for precise frame timing
            let frame_time_us = (1_000_000.0 / monitor_refresh_rate_hz) as u64;
            Duration::from_micros(frame_time_us)
        };

        Self {
            vblank_interval: monitor_vblank_interval,
            render_timestamp: Instant::now(),
            is_focused: true,
            is_occluded: false,
            needs_render_after_occlusion: false,
            winit_window,
            screen,
        }
    }
}

fn centered_position(
    event_loop: &ActiveEventLoop,
    width: u32,
    height: u32,
) -> Option<PhysicalPosition<i32>> {
    let monitor = event_loop.primary_monitor()?;
    let monitor_size = monitor.size();
    let monitor_pos = monitor.position();
    let x = monitor_pos.x + (monitor_size.width as i32 - width as i32) / 2;
    let y = monitor_pos.y + (monitor_size.height as i32 - height as i32) / 2;
    Some(PhysicalPosition::new(x, y))
}

fn compute_window_size_from_grid(
    columns: Option<u16>,
    rows: Option<u16>,
    panel: &rio_backend::config::layout::Panel,
    dim: &crate::layout::ContextDimension,
    window_size: PhysicalSize<u32>,
) -> (u32, u32) {
    let scale = dim.dimension.scale;
    let scale_u32 = scale.round().max(1.0) as u32;

    let physical_width = match columns {
        Some(columns) if columns > 0 => {
            let margin = (dim.margin.left + dim.margin.right) * scale;
            let panel_edge = (panel.padding.left
                + panel.padding.right
                + panel.margin.left
                + panel.margin.right)
                * scale;
            let raw =
                columns as u32 * dim.cell.cell_width + margin as u32 + panel_edge as u32;
            raw.next_multiple_of(scale_u32)
        }
        _ => window_size.width,
    };

    let physical_height = match rows {
        Some(rows) if rows > 0 => {
            let margin = (dim.margin.top + dim.margin.bottom) * scale;
            let panel_edge = (panel.padding.top
                + panel.padding.bottom
                + panel.margin.top
                + panel.margin.bottom)
                * scale;
            let raw =
                rows as u32 * dim.cell.cell_height + margin as u32 + panel_edge as u32;
            raw.next_multiple_of(scale_u32)
        }
        _ => window_size.height,
    };

    let min_w = (DEFAULT_MINIMUM_WINDOW_WIDTH as f32 * scale).ceil() as u32;
    let min_h = (DEFAULT_MINIMUM_WINDOW_HEIGHT as f32 * scale).ceil() as u32;

    (physical_width.max(min_w), physical_height.max(min_h))
}

#[cfg(test)]
mod grid_size_tests {
    use super::*;
    use rio_backend::config::layout::{Margin, Panel};
    use rio_backend::sugarloaf::layout::TextDimensions;

    fn make_dim(
        width: f32,
        height: f32,
        scale: f32,
        margin: Margin,
    ) -> crate::layout::ContextDimension {
        crate::layout::ContextDimension {
            dimension: TextDimensions {
                width,
                height,
                scale,
            },
            // Canonical cell stride matches the dims so router math
            // (`cols * cell_width`) lines up with what the test
            // labels imply.
            cell: rio_backend::sugarloaf::layout::CellMetrics {
                cell_width: width.round().max(1.0) as u32,
                cell_height: height.round().max(1.0) as u32,
                cell_baseline: 0,
                face_width: width as f64,
                face_height: height as f64,
                face_y: 0.0,
            },
            margin,
            ..Default::default()
        }
    }

    fn win(w: u32, h: u32) -> PhysicalSize<u32> {
        PhysicalSize {
            width: w,
            height: h,
        }
    }

    fn panel_zero() -> Panel {
        Panel {
            padding: Margin::all(0.0),
            margin: Margin::all(0.0),
            ..Default::default()
        }
    }

    #[test]
    fn applies_only_columns_override() {
        let dim = make_dim(10.0, 20.0, 2.0, Margin::all(0.0));
        // 80 * 10.0 = 800, next_multiple_of(2) = 800; height stays at window size
        assert_eq!(
            compute_window_size_from_grid(
                Some(80),
                None,
                &panel_zero(),
                &dim,
                win(1000, 600)
            ),
            (800, 600)
        );
    }

    #[test]
    fn applies_only_rows_override() {
        let dim = make_dim(10.0, 20.0, 2.0, Margin::all(0.0));
        // 24 * 20.0 = 480, next_multiple_of(2) = 480; width stays at window size
        assert_eq!(
            compute_window_size_from_grid(
                None,
                Some(24),
                &panel_zero(),
                &dim,
                win(1000, 600)
            ),
            (1000, 480)
        );
    }

    #[test]
    fn applies_both_overrides() {
        let dim = make_dim(10.0, 20.0, 1.0, Margin::all(0.0));
        assert_eq!(
            compute_window_size_from_grid(
                Some(100),
                Some(40),
                &panel_zero(),
                &dim,
                win(500, 300)
            ),
            (1000, 800)
        );
    }

    #[test]
    fn ignores_zero_overrides_and_keeps_window_size() {
        let dim = make_dim(10.0, 20.0, 2.0, Margin::all(0.0));
        assert_eq!(
            compute_window_size_from_grid(
                Some(0),
                Some(0),
                &panel_zero(),
                &dim,
                win(1000, 600)
            ),
            (1000, 600)
        );
    }

    #[test]
    fn rounds_up_on_hidpi() {
        let dim = make_dim(16.41, 33.0, 2.0, Margin::all(0.0));
        // Canonical cell stride: round(16.41) = 16, round(33.0) = 33.
        // 80 * 16 = 1280, next_multiple_of(2) = 1280
        // 24 * 33 = 792,  next_multiple_of(2) = 792
        assert_eq!(
            compute_window_size_from_grid(
                Some(80),
                Some(24),
                &panel_zero(),
                &dim,
                win(1000, 600)
            ),
            (1280, 792)
        );
    }

    #[test]
    fn includes_terminal_and_panel_margins() {
        let panel = Panel {
            padding: Margin::new(3.0, 2.0, 4.0, 1.0),
            margin: Margin::new(7.0, 6.0, 8.0, 5.0),
            ..Default::default()
        };
        let dim = make_dim(10.0, 20.0, 1.0, Margin::new(4.0, 3.0, 5.0, 2.0));
        assert_eq!(
            compute_window_size_from_grid(Some(10), Some(5), &panel, &dim, win(500, 300)),
            (300, 200)
        );
    }

    #[test]
    fn never_goes_under_minimum() {
        let dim = make_dim(1.0, 1.0, 1.0, Margin::all(0.0));
        assert_eq!(
            compute_window_size_from_grid(
                Some(1),
                Some(1),
                &panel_zero(),
                &dim,
                win(50, 50)
            ),
            (300, 200)
        );
    }
}
