mod bindings;
mod constants;
mod context;
mod messenger;
mod mouse;
mod navigation;
mod state;
pub mod window;

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
#[cfg(target_os = "macos")]
use crate::screen::constants::{DEADZONE_END_Y, DEADZONE_START_X, DEADZONE_START_Y};
use crate::screen::{
    bindings::{Action as Act, BindingKey, BindingMode, FontSizeAction},
    context::ContextManager,
    mouse::Mouse,
};
use crate::selection::{Selection, SelectionType};
use colors::term::List;
use messenger::Messenger;
use state::State;
use std::cmp::max;
use std::cmp::min;
use std::error::Error;
use std::rc::Rc;
use sugarloaf::{layout::SugarloafLayout, Sugarloaf};
use winit::event::ElementState;
use winit::keyboard::{Key, ModifiersState};
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
    clipboard: Clipboard,
    pub modifiers: ModifiersState,
    pub mouse: Mouse,
    pub ime: Ime,
    pub state: State,
    pub sugarloaf: Sugarloaf,
    context_manager: context::ContextManager<EventProxy>,
}

impl Screen {
    pub async fn new(
        winit_window: &winit::window::Window,
        config: &Rc<config::Config>,
        event_proxy: EventProxy,
        native_tab_id: Option<String>,
    ) -> Result<Screen, Box<dyn Error>> {
        let size = winit_window.inner_size();
        let scale = winit_window.scale_factor();
        // let raw_window_handle = winit_window.raw_window_handle();
        let raw_display_handle = winit_window.raw_display_handle();
        let window_id = winit_window.id();

        let power_preference: wgpu::PowerPreference = match config.performance {
            config::Performance::High => wgpu::PowerPreference::HighPerformance,
            config::Performance::Low => wgpu::PowerPreference::LowPower,
        };

        let mut padding_y_bottom = 0.0;
        if config.navigation.is_placed_on_bottom() {
            padding_y_bottom += config.font_size
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
            config.font_size,
            config.line_height,
            (MIN_COLUMNS, MIN_LINES),
        );

        let sugarloaf = Sugarloaf::new(
            winit_window,
            power_preference,
            config.font.to_string(),
            sugarloaf_layout,
        )
        .await?;

        let state = State::new(config);

        let clipboard = unsafe { Clipboard::new(raw_display_handle) };

        let bindings = bindings::default_key_bindings(config.bindings.keys.clone());
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
        let context_manager = context::ContextManager::start(
            (sugarloaf.layout.width_u32, sugarloaf.layout.height_u32),
            (sugarloaf.layout.columns, sugarloaf.layout.lines),
            state.get_cursor_state(),
            event_proxy,
            window_id,
            context_manager_config,
        )?;

        Ok(Screen {
            modifiers: ModifiersState::default(),
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
    pub fn set_modifiers(&mut self, modifiers: ModifiersState) {
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
        let col_fac = (layout.sugarwidth * self.sugarloaf.layout.scale_factor) as usize;

        let col = self.mouse.x.saturating_sub(
            (layout.margin.x * 2. * self.sugarloaf.layout.scale_factor) as usize,
        ) / col_fac;
        let col = std::cmp::min(Column(col), Column(layout.columns));

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
        let padding_y_top = constants::PADDING_Y;
        if tab_num > 1 {
            self.sugarloaf
                .layout
                .set_top_y_for_native_tabs((padding_y_top * 2.0) + 2.0);
        } else {
            self.sugarloaf
                .layout
                .set_top_y_for_native_tabs(padding_y_top + 2.0);
        }

        let width = self.sugarloaf.layout.width_u32 as u16;
        let height = self.sugarloaf.layout.height_u32 as u16;
        let columns = self.sugarloaf.layout.columns;
        let lines = self.sugarloaf.layout.lines;
        self.resize_all_contexts(width, height, columns, lines);
    }

    /// update_config is triggered in any configuration file update
    #[inline]
    pub fn update_config(&mut self, config: &Rc<config::Config>) {
        let mut padding_y_bottom = 0.0;
        if config.navigation.is_placed_on_bottom() {
            padding_y_bottom += config.font_size
        }

        let mut padding_y_top = constants::PADDING_Y;

        #[cfg(not(target_os = "macos"))]
        {
            if config.navigation.is_placed_on_top() {
                padding_y_top = constants::PADDING_Y_WITH_TAB_ON_TOP;
            }
        }

        if config.navigation.is_native() {
            padding_y_top += 2.0;
        }

        self.sugarloaf.layout.recalculate(
            config.font_size,
            config.line_height,
            config.padding_x,
            (padding_y_top, padding_y_bottom),
        );
        self.sugarloaf.update_font(config.font.to_string());
        self.sugarloaf.layout.update();
        self.state = State::new(config);

        for context in self.ctx().contexts() {
            let mut terminal = context.terminal.lock();
            terminal.cursor_shape = self.state.get_cursor_state_from_ref().content;
        }

        let width = self.sugarloaf.layout.width_u32 as u16;
        let height = self.sugarloaf.layout.height_u32 as u16;
        let columns = self.sugarloaf.layout.columns;
        let lines = self.sugarloaf.layout.lines;
        self.resize_all_contexts(width, height, columns, lines);

        self.init(config.colors.background.1);
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

    pub fn input_str(&mut self, text: &str) {
        self.scroll_bottom_when_cursor_not_visible();
        self.clear_selection();

        #[cfg(not(target_os = "macos"))]
        let alt_send_esc = true;

        #[cfg(target_os = "macos")]
        let alt_send_esc = self.state.option_as_alt;

        let mut bytes = Vec::with_capacity(text.len() + 1);
        if text.len() == 1 && alt_send_esc && self.modifiers.alt_key() {
            bytes.push(b'\x1b');
        }
        bytes.extend_from_slice(text.as_bytes());

        self.ctx_mut().current_mut().messenger.send_bytes(bytes);
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

    pub fn process_key_event(&mut self, key: &winit::event::KeyEvent) {
        if self.ime.preedit().is_some() {
            return;
        }

        let mode = BindingMode::new(&self.get_mode());
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

            if binding.is_triggered_by(mode.clone(), self.modifiers, &key) {
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
                        self.context_manager.create_config_editor();
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
                            self.state.get_cursor_state_from_ref(),
                        );

                        self.render();
                    }
                    Act::TabSwitchNext => {
                        self.clear_selection();
                        self.context_manager.switch_to_next();
                        self.render();
                    }
                    Act::TabSwitchPrev => {
                        self.clear_selection();
                        self.context_manager.switch_to_prev();
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
                    Act::ReceiveChar | Act::None => (),
                    _ => (),
                }
            }
        }

        if ignore_chars.unwrap_or(false) {
            return;
        }

        let text = key.text_with_all_modifiers().unwrap_or_default();
        if self.get_mode().contains(Mode::VI) || text.is_empty() {
            return;
        }

        self.input_str(text);
    }

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
                if self.modifiers.control_key() {
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
    pub fn init(&mut self, color: colors::ColorWGPU) {
        let initial_columns = self.sugarloaf.layout.columns;
        self.sugarloaf
            .set_background_color(color)
            .calculate_bounds();

        if self.sugarloaf.layout.columns != initial_columns {
            let width = self.sugarloaf.layout.width_u32 as u16;
            let height = self.sugarloaf.layout.height_u32 as u16;
            let columns = self.sugarloaf.layout.columns;
            let lines = self.sugarloaf.layout.lines;
            self.resize_all_contexts(width, height, columns, lines);
        }
    }

    #[inline]
    pub fn render(&mut self) {
        let mut terminal = self.ctx().current().terminal.lock();
        let visible_rows = terminal.visible_rows();
        let cursor = terminal.cursor();
        let display_offset = terminal.display_offset();
        drop(terminal);
        self.context_manager.update_titles();

        self.state.set_ime(self.ime.preedit());

        self.state.update(
            visible_rows,
            cursor,
            &mut self.sugarloaf,
            &self.context_manager,
            display_offset as i32,
        );

        self.sugarloaf.render();
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
        if self.modifiers.shift_key() {
            mods += 4;
        }
        if self.modifiers.alt_key() {
            mods += 8;
        }
        if self.modifiers.control_key() {
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
            && !self.modifiers.shift_key()
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
