//! tmux control mode takeover.
//!
//! While a session runs, the tab that started `tmux -CC` is set aside and
//! the session's windows sit in its place, one tab each, with panes as
//! splits. When the session ends the original tab goes back where it was,
//! its terminal exactly as tmux found it.
//!
//! The panes are terminals with no PTY of their own: tmux owns the shells.
//! Output is fed to them by the reader thread of the PTY hosting the control
//! client, and their `Messenger` turns writes into `send-keys`.

use super::{
    next_rich_text_id, Context, ContextGrid, ContextManager, ContextManagerConfig,
    ContextTitle, Cursor, RenderableContent, ROUTE_ID_COUNTER,
};
use crate::ansi::CursorShape;
use crate::event::sync::FairMutex;
use crate::event::Msg;
use crate::layout::{BorderDirection, ContextDimension, TmuxDividerDrag, TmuxGridState};
use crate::messenger::Messenger;
use rio_backend::config::layout::Margin;
use rio_backend::crosswords::Crosswords;
use rio_backend::event::{EventListener, TerminalDamage, WindowId};
use rio_backend::sugarloaf::Sugarloaf;
use rio_backend::tmux::keys::{unsupported_command, KeyPress, PrefixState, TmuxKeys};
use rio_backend::tmux::session::TerminalSink;
use rio_backend::tmux::{TmuxSnapshot, TmuxWindow};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

/// A running control mode session of this window.
pub struct TmuxTakeover<T: EventListener> {
    pub(super) host_route: usize,
    /// The tab that started tmux, out of `ContextManager::contexts` for as
    /// long as the session runs. Dropping it would hang up the client.
    pub(super) host_grid: ContextGrid<T>,
    channel: corcovado::channel::Sender<Msg>,
    active_window: Option<u32>,
    /// Client size last sent to tmux.
    size: Option<(usize, usize)>,
    keys: Arc<TmuxKeys>,
    prefix: PrefixState,
}

/// What a tmux tab's panes are built from.
struct PaneFactory<'a, T: EventListener> {
    event_proxy: &'a T,
    window_id: WindowId,
    config: &'a ContextManagerConfig,
    channel: &'a corcovado::channel::Sender<Msg>,
    cursor: Cursor,
    blinking: bool,
    /// Sized for the whole tab; the layout pass narrows it down per pane.
    dimension: ContextDimension,
}

impl<T: EventListener + Clone + Send + 'static> PaneFactory<'_, T> {
    fn create(&self, pane: u32, columns: usize, lines: usize) -> Context<T> {
        let route_id = ROUTE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);

        let mut dimension = self.dimension;
        dimension.columns = columns;
        dimension.lines = lines;

        let mut terminal = Crosswords::new(
            dimension,
            CursorShape::from_char(self.cursor.content),
            self.event_proxy.clone(),
            self.window_id,
            route_id,
            self.config.scrollback_history_limit,
        );
        terminal.set_grapheme_clustering(self.config.grapheme_clustering);
        terminal.blinking_cursor = self.blinking;
        let terminal = Arc::new(FairMutex::new(terminal));

        let _ = self.channel.send(Msg::TmuxAttachPane(
            pane,
            Box::new(TerminalSink::new(
                Arc::clone(&terminal),
                self.event_proxy.clone(),
                self.window_id,
                route_id,
            )),
        ));

        Context {
            route_id,
            #[cfg(not(target_os = "windows"))]
            main_fd: Arc::new(-1),
            #[cfg(not(target_os = "windows"))]
            shell_pid: 1,
            #[cfg(not(target_os = "windows"))]
            child_terminator: teletypewriter::ChildTerminator::retired(),
            messenger: Messenger::for_tmux_pane(self.channel.clone(), pane),
            renderable_content: RenderableContent::new(self.cursor.clone()),
            terminal,
            rich_text_id: next_rich_text_id(),
            dimension,
            title: ContextTitle::default(),
            title_dirty: true,
            spawned_program: String::from("tmux"),
            tmux_size: Some((columns, lines)),
            _io_thread: None,
        }
    }

    fn create_grid(
        &self,
        host_route: usize,
        window: &TmuxWindow,
        scaled_margin: Margin,
        sugarloaf: &mut Sugarloaf,
    ) -> Option<ContextGrid<T>> {
        let &(pane, columns, lines) = window.layout.panes().first()?;

        // `ContextGrid::new` takes the tab's size from its first context.
        let mut grid = ContextGrid::new(
            self.create(pane, columns, lines),
            scaled_margin,
            self.config.split_color,
            self.config.split_active_color,
            self.config.panel,
        );
        self.apply_window(&mut grid, host_route, window, sugarloaf);
        Some(grid)
    }

    fn apply_window(
        &self,
        grid: &mut ContextGrid<T>,
        host_route: usize,
        window: &TmuxWindow,
        sugarloaf: &mut Sugarloaf,
    ) {
        let removed = grid.set_tmux_layout(
            &window.layout,
            window.active_pane,
            &mut |pane, columns, lines| self.create(pane, columns, lines),
        );
        for item in removed {
            sugarloaf.clear_image_overlays_for(item.val.rich_text_id);
            sugarloaf.remove_route_images(item.val.route_id);
        }
        grid.tmux = Some(TmuxGridState {
            host_route,
            window: window.id,
            layout: window.layout.clone(),
            active_pane: window.active_pane,
        });
        grid.custom_title = Some(window.name.clone());
        grid.apply_layout(sugarloaf);
    }
}

impl<T: EventListener + Clone + Send + 'static> ContextManager<T> {
    /// Bring the tabs in line with the session on `host_route`, taking over
    /// the host's tab the first time around.
    pub fn tmux_update(
        &mut self,
        host_route: usize,
        snapshot: &TmuxSnapshot,
        sugarloaf: &mut Sugarloaf,
    ) {
        // Only transient: closing the last window ends the session.
        if snapshot.windows.is_empty() {
            return;
        }

        let previous_route = self.current_route;
        let mut focused = self.current_grid().tmux_host() == Some(host_route);

        let started = !self.tmux.iter().any(|t| t.host_route == host_route);
        if started {
            let Some(tab) = self.tab_index_for_route(host_route) else {
                return;
            };
            focused = tab == self.current_index;
            let Some(window) = snapshot.windows.first() else {
                return;
            };
            let Some(item) = self.contexts[tab].get_by_route_id(host_route) else {
                return;
            };
            let channel = item.val.messenger.channel.clone();

            let host = &self.contexts[tab];
            let factory = PaneFactory {
                event_proxy: &self.event_proxy,
                window_id: self.window_id,
                config: &self.config,
                channel: &channel,
                cursor: host.current().cursor_from_ref(),
                blinking: host.current().renderable_content.has_blinking_enabled,
                dimension: host.grid_dimension(),
            };
            let Some(grid) =
                factory.create_grid(host_route, window, host.scaled_margin, sugarloaf)
            else {
                return;
            };

            let host_grid = std::mem::replace(&mut self.contexts[tab], grid);
            host_grid.set_all_rich_text_visibility(sugarloaf, false);
            self.tmux.push(TmuxTakeover {
                host_route,
                host_grid,
                channel,
                active_window: None,
                size: None,
                keys: Arc::clone(&snapshot.keys),
                prefix: PrefixState::default(),
            });
        }

        let Some(slot) = self.tmux_tabs(host_route).next() else {
            return;
        };

        // Lifted out and put back in tmux's order, which also covers windows
        // that moved, and tabs the user dragged elsewhere.
        let mut grids = Vec::new();
        for index in self.tmux_tabs(host_route).collect::<Vec<_>>().iter().rev() {
            grids.push(self.contexts.remove(*index));
        }
        let Some(reference) = grids.first() else {
            return;
        };

        let Some(takeover) = self.tmux.iter_mut().find(|t| t.host_route == host_route)
        else {
            return;
        };
        takeover.active_window = snapshot.active_window;
        takeover.keys = Arc::clone(&snapshot.keys);

        let factory = PaneFactory {
            event_proxy: &self.event_proxy,
            window_id: self.window_id,
            config: &self.config,
            channel: &takeover.channel,
            cursor: reference.current().cursor_from_ref(),
            blinking: reference.current().renderable_content.has_blinking_enabled,
            dimension: reference.grid_dimension(),
        };
        let scaled_margin = reference.scaled_margin;

        let mut ordered = Vec::with_capacity(snapshot.windows.len());
        for window in &snapshot.windows {
            let existing = grids
                .iter()
                .position(|grid| grid.tmux_window() == Some(window.id));
            let grid = match existing {
                Some(index) => {
                    let mut grid = grids.swap_remove(index);
                    if grid.tmux_outdated(window) {
                        factory.apply_window(&mut grid, host_route, window, sugarloaf);
                    }
                    Some(grid)
                }
                None => factory.create_grid(host_route, window, scaled_margin, sugarloaf),
            };
            ordered.extend(grid);
        }
        for grid in &grids {
            grid.remove_from_sugarloaf(sugarloaf);
        }

        for (offset, grid) in ordered.into_iter().enumerate() {
            self.contexts.insert(slot + offset, grid);
        }

        let index = if focused {
            let active = snapshot.active_window;
            self.tmux_tabs(host_route)
                .find(|index| self.contexts[*index].tmux_window() == active)
                .unwrap_or(slot)
        } else {
            self.tab_of(previous_route).unwrap_or(slot)
        };
        self.current_index = index;
        // What is about to be selected comes from tmux, so it is not news
        // to tmux. Only the user's own selections go back as commands.
        if focused {
            let shown = self.contexts[index].tmux_window();
            if let Some(takeover) =
                self.tmux.iter_mut().find(|t| t.host_route == host_route)
            {
                takeover.active_window = shown;
            }
        }
        self.sync_current_route();
        self.keep_only_active_context_visible(sugarloaf);
        self.tmux_sync_size();
    }

    /// The session on `host_route` is over: give the tab back.
    pub fn tmux_end(&mut self, host_route: usize, sugarloaf: &mut Sugarloaf) {
        let Some(position) = self.tmux.iter().position(|t| t.host_route == host_route)
        else {
            return;
        };
        let takeover = self.tmux.remove(position);
        let mut host_grid = takeover.host_grid;

        let tabs: Vec<usize> = self.tmux_tabs(host_route).collect();
        let slot = tabs.first().copied().unwrap_or(self.contexts.len());
        let focused = self.current_grid().tmux_host() == Some(host_route);
        let previous_route = self.current_route;

        // The window may have changed while the tab was away.
        if let Some(reference) = tabs.first().map(|index| &self.contexts[*index]) {
            let (width, height) = (reference.width, reference.height);
            host_grid.update_scaled_margin(reference.scaled_margin);
            host_grid.update_scale(sugarloaf.scale_factor());
            host_grid.update_line_height(reference.current().dimension.line_height);
            host_grid.resize(width, height, sugarloaf);
        }
        host_grid.update_dimensions(sugarloaf);
        for item in host_grid.contexts_mut().values_mut() {
            item.val
                .renderable_content
                .pending_update
                .set_terminal_damage(TerminalDamage::Full);
        }

        for index in tabs.iter().rev() {
            self.contexts[*index].remove_from_sugarloaf(sugarloaf);
            self.contexts.remove(*index);
        }
        self.contexts.insert(slot, host_grid);

        self.current_index = if focused {
            slot
        } else {
            self.tab_of(previous_route).unwrap_or(slot)
        };
        self.sync_current_route();
        self.keep_only_active_context_visible(sugarloaf);
    }

    /// Tell tmux how much room there is, in cells. It answers with layouts
    /// for the windows, which is what actually sizes the panes.
    pub fn tmux_sync_size(&mut self) {
        for takeover in self.tmux.iter_mut() {
            let Some(grid) = self
                .contexts
                .iter()
                .find(|grid| grid.tmux_host() == Some(takeover.host_route))
            else {
                continue;
            };

            let size = grid.tmux_client_size();
            if takeover.size != Some(size) {
                takeover.size = Some(size);
                let _ = takeover.channel.send(Msg::TmuxCommand(format!(
                    "refresh-client -C {},{}",
                    size.0, size.1
                )));
            }
        }
    }

    /// Follow a tab or split change made in Rio. tmux echoes the selection
    /// back, by which time `active_window` and `active_pane` already match.
    pub(super) fn tmux_sync_selection(&mut self) {
        let grid = &mut self.contexts[self.current_index];
        let pane = grid.current().messenger.tmux_pane();
        let Some(state) = grid.tmux.as_mut() else {
            return;
        };
        let Some(takeover) = self
            .tmux
            .iter_mut()
            .find(|t| t.host_route == state.host_route)
        else {
            return;
        };

        if takeover.active_window != Some(state.window) {
            takeover.active_window = Some(state.window);
            let _ = takeover.channel.send(Msg::TmuxCommand(format!(
                "select-window -t @{}",
                state.window
            )));
        }
        if state.active_pane != pane {
            state.active_pane = pane;
            if let Some(pane) = pane {
                let _ = takeover
                    .channel
                    .send(Msg::TmuxCommand(format!("select-pane -t %{pane}")));
            }
        }
    }

    /// Run a command on the session of the focused tab. Returns whether
    /// there is one, in which case the action belongs to tmux and the
    /// outcome comes back as a snapshot.
    fn tmux_command(&self, command: impl FnOnce(u32, u32) -> String) -> bool {
        let grid = self.current_grid();
        let Some(state) = grid.tmux.as_ref() else {
            return false;
        };
        let Some(takeover) = self.tmux.iter().find(|t| t.host_route == state.host_route)
        else {
            return false;
        };
        if let Some(pane) = grid.current().messenger.tmux_pane() {
            let _ = takeover
                .channel
                .send(Msg::TmuxCommand(command(state.window, pane)));
        }
        true
    }

    pub fn tmux_new_window(&self) -> bool {
        self.tmux_command(|_, _| String::from("new-window"))
    }

    pub(super) fn tmux_split(&self, split_down: bool) -> bool {
        let flag = if split_down { 'v' } else { 'h' };
        self.tmux_command(|_, pane| format!("split-window -{flag} -t %{pane}"))
    }

    /// Move a divider by giving the pane before it a new size in cells.
    pub fn tmux_resize_pane(&self, drag: &TmuxDividerDrag, cells: usize) {
        let flag = match drag.direction {
            BorderDirection::Vertical => 'x',
            BorderDirection::Horizontal => 'y',
        };
        let pane = drag.pane;
        self.tmux_command(|_, _| format!("resize-pane -t %{pane} -{flag} {cells}"));
    }

    pub(super) fn tmux_kill_pane(&self) -> bool {
        self.tmux_command(|_, pane| format!("kill-pane -t %{pane}"))
    }

    pub(super) fn tmux_kill_window(&self) -> bool {
        self.tmux_command(|window, _| format!("kill-window -t @{window}"))
    }

    /// Leave the session running on the server and go back to the tab it
    /// was started from.
    pub fn tmux_detach(&self) -> bool {
        self.tmux_command(|_, _| String::from("detach-client"))
    }

    /// The session of the focused tab, if it shows one.
    fn focused_takeover(&self) -> Option<&TmuxTakeover<T>> {
        let host = self.current_grid().tmux_host()?;
        self.tmux.iter().find(|t| t.host_route == host)
    }

    #[inline]
    pub fn tmux_focused(&self) -> bool {
        self.config.tmux_prefix && self.focused_takeover().is_some()
    }

    /// Run a key press, named as tmux names it, through the prefix handling
    /// of the focused tab's session.
    pub fn tmux_key(&mut self, key: &str, held: bool) -> KeyPress {
        let host = self.current_grid().tmux_host();
        let Some(takeover) = self.tmux.iter_mut().find(|t| Some(t.host_route) == host)
        else {
            return KeyPress::Pass;
        };

        let keys = Arc::clone(&takeover.keys);
        let press = keys.press(&mut takeover.prefix, key, held, Instant::now());
        if let KeyPress::Run(commands) = &press {
            let _ = takeover.channel.send(Msg::TmuxBinding(commands.clone()));
        }
        press
    }

    /// Have the focused tab's session expand the formats of a prompt.
    pub fn tmux_expand_prompt(&self, id: u64, formats: Vec<String>) {
        if let Some(takeover) = self.focused_takeover() {
            let _ = takeover.channel.send(Msg::TmuxPromptFormats(id, formats));
        }
    }

    /// Run a prompt's commands on the focused tab's session. Fails with the
    /// name of a command that can't work on a control client.
    pub fn tmux_run_prompt(&self, commands: String) -> Result<(), String> {
        if let Some(command) = unsupported_command(&commands) {
            return Err(command);
        }
        if let Some(takeover) = self.focused_takeover() {
            if !commands.trim().is_empty() {
                let _ = takeover.channel.send(Msg::TmuxPromptCommands(commands));
            }
        }
        Ok(())
    }

    /// The prefix, while the focused tab's session waits for the key after it.
    pub(super) fn tmux_armed_prefix(&self) -> Option<&str> {
        self.focused_takeover()
            .filter(|takeover| takeover.prefix.is_armed())
            .and_then(|takeover| takeover.keys.prefix())
    }

    /// Indexes of the tabs showing the session on `host_route`, ascending.
    fn tmux_tabs(&self, host_route: usize) -> impl Iterator<Item = usize> + '_ {
        self.contexts
            .iter()
            .enumerate()
            .filter(move |(_, grid)| grid.tmux_host() == Some(host_route))
            .map(|(index, _)| index)
    }

    fn tab_of(&self, route_id: usize) -> Option<usize> {
        self.contexts
            .iter()
            .position(|grid| grid.contains_route(route_id))
    }
}
