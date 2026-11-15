pub mod renderable;
pub mod title;

use crate::ansi::mode::{NamedPrivateMode, PrivateMode};
use crate::ansi::CursorShape;
use crate::ansi::{KeyboardModes, KeyboardModesApplyBehavior};
use crate::context::title::{update_title, ContextTitle};
use crate::event::sync::TerminalLock;
use crate::event::{Msg, RioEvent};
pub use crate::layout::{ContextDimension, ContextGrid, ContextGridItem};
use crate::messenger::Messenger;
use crate::performer::handler::Handler;
use crate::performer::{self, Machine};
use renderable::Cursor;
use renderable::RenderableContent;
use rio_backend::config::layout::Margin;
use rio_backend::config::Shell;
use smallvec::{smallvec, SmallVec};

use crate::layout::{BorderDirection, GridId, PanelImages, ResizeState};
use rio_backend::crosswords::pos::{CharsetIndex, StandardCharset};
use rio_backend::crosswords::{Crosswords, MIN_COLUMNS, MIN_LINES};
use rio_backend::error::{RioError, RioErrorLevel, RioErrorType};
use rio_backend::event::EventListener;
use rio_backend::event::WindowId;
use rio_backend::selection::SelectionRange;
use rio_backend::sugarloaf::{font::SugarloafFont, Rect, SugarloafErrors};
use std::error::Error;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;

// Global atomic counter for generating unique route IDs
static ROUTE_ID_COUNTER: AtomicUsize = AtomicUsize::new(1);

/// A route id no pane has had before. Starts at one: zero is the
/// manager's "no route yet" sentinel.
pub fn next_route_id() -> usize {
    ROUTE_ID_COUNTER.fetch_add(1, Ordering::SeqCst)
}

// Global atomic counter for generating unique rich text IDs
static RICH_TEXT_ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Generate a unique rich text ID for terminal contexts
pub fn next_rich_text_id() -> usize {
    RICH_TEXT_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
}

#[cfg(target_os = "windows")]
use teletypewriter::create_pty;
#[cfg(not(target_os = "windows"))]
use teletypewriter::{create_pty_with_fork, create_pty_with_spawn};

pub struct Context<T: EventListener> {
    pub route_id: usize,
    pub terminal: Arc<TerminalLock<Crosswords<T>>>,
    pub renderable_content: RenderableContent,
    pub messenger: Messenger,
    #[cfg(not(target_os = "windows"))]
    pub main_fd: Arc<i32>,
    #[cfg(not(target_os = "windows"))]
    pub shell_pid: u32,
    #[cfg(not(target_os = "windows"))]
    child_terminator: teletypewriter::ChildTerminator,
    pub rich_text_id: usize,
    pub dimension: ContextDimension,
    pub title: ContextTitle,
    /// An OSC title change arrived while this pane was hidden: the
    /// render was skipped and must run when the pane surfaces.
    pub title_dirty: bool,
    /// Display name of the command this pane spawned (the configured
    /// shell or program), fixed for the pane's lifetime. Fills
    /// `{{ program }}` and the empty-title strip fallback without ever
    /// inspecting the foreground process.
    pub spawned_program: String,
    /// The command exited and the pane is held open until a key press.
    pub exited: bool,
    _io_thread: Option<JoinHandle<(Machine<teletypewriter::Pty, T>, performer::State)>>,
}

impl<T: rio_backend::event::EventListener> Drop for Context<T> {
    fn drop(&mut self) {
        // Shutdown the terminal's PTY.
        let _ = self.messenger.channel.send(Msg::Shutdown);
        // Also hang up synchronously: quit paths call process::exit
        // right after dropping routes, before the reader thread can run
        // its shutdown escalation. The handle is a no-op once the child
        // was reaped, so no stale PID is ever signaled.
        #[cfg(not(target_os = "windows"))]
        let _ = self.child_terminator.hangup();
    }
}

impl<T: EventListener> Context<T> {
    #[inline]
    pub fn set_selection(&mut self, selection_range: Option<SelectionRange>) {
        let old_selection = self.renderable_content.selection_range;
        let has_updated = old_selection != selection_range;

        if has_updated {
            // Selection affects terminal line rendering, so use terminal damage
            self.renderable_content
                .pending_update
                .set_terminal_damage(rio_backend::event::TerminalDamage::Full);
        }

        self.renderable_content.selection_range = selection_range;
    }

    #[inline]
    pub fn cursor_from_ref(&self) -> Cursor {
        Cursor {
            state: self.renderable_content.cursor.state.new_from_self(),
            content: self.renderable_content.cursor.content,
        }
    }
}

#[derive(Clone, Default)]
pub struct ContextManagerConfig {
    /// Build contexts without spawning a PTY (see
    /// `create_dead_context`). Unit tests fork one real `$SHELL` per
    /// context otherwise, which is slow and flaky under the parallel
    /// test runner (fork failures surface as random test panics).
    #[cfg(test)]
    pub dead_pty: bool,
    pub shell: Shell,
    #[cfg(not(target_os = "windows"))]
    pub use_fork: bool,
    pub shell_integration: bool,
    pub working_dir: Option<String>,
    pub spawn_performer: bool,
    pub cwd: bool,
    pub is_native: bool,
    pub split_color: [f32; 4],
    pub split_active_color: [f32; 4],
    pub panel: rio_backend::config::layout::Panel,
    pub title: rio_backend::config::title::Title,
    pub keyboard: rio_backend::config::keyboard::Keyboard,
    pub scrollback_history_limit: usize,
    pub grapheme_clustering: bool,
    /// The configured palette, handed to every terminal so it answers
    /// color queries itself.
    pub colors: rio_backend::config::colors::term::List,
}

const DEFAULT_CONTEXT_CAPACITY: usize = 28;

/// A divider drag: the grid it started on, by its id since tab indices
/// shift when tabs close, the shape that grid had, and where the drag
/// started.
#[derive(Debug, Clone, Copy)]
struct BorderDrag {
    grid: GridId,
    shape: u64,
    state: ResizeState,
}

/// One pointer move of a divider drag: the divider's direction, and
/// whether the grid it moves is the one on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DragStep {
    pub direction: BorderDirection,
    pub visible: bool,
}

pub struct ContextManager<T: EventListener> {
    contexts: SmallVec<[ContextGrid<T>; DEFAULT_CONTEXT_CAPACITY]>,
    current_index: usize,
    current_route: usize,
    /// Window focus as the platform last reported it. With the active
    /// pane it decides which pane, if any, a program should consider
    /// focused.
    window_focused: bool,
    /// The pane last told it has focus (CSI I), and so owed a CSI O.
    focused_route: Option<usize>,
    /// A divider drag in progress, pinned to the grid it started on.
    border_drag: Option<BorderDrag>,
    #[allow(unused)]
    capacity: usize,
    event_proxy: T,
    window_id: WindowId,
    pub config: ContextManagerConfig,
}

/// Display name for the command a pane spawns: the configured program,
/// else the user's shell (what the PTY spawn itself falls back to),
/// reduced to its basename.
fn spawned_program_name(config: &ContextManagerConfig) -> String {
    let program = match config.shell.program.as_deref() {
        Some(program) if !program.is_empty() => program.to_string(),
        _ => {
            #[cfg(unix)]
            {
                teletypewriter::default_shell_program()
            }
            #[cfg(not(unix))]
            {
                String::from("powershell")
            }
        }
    };
    std::path::Path::new(&program)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or(program)
}

pub fn create_dead_context<T: rio_backend::event::EventListener>(
    event_proxy: T,
    window_id: WindowId,
    route_id: usize,
    rich_text_id: usize,
    dimension: ContextDimension,
) -> Context<T> {
    let terminal = Crosswords::new(
        dimension,
        CursorShape::Block,
        event_proxy,
        window_id,
        route_id,
        // Dead context never sees new input — no scrollback needed.
        0,
    );
    let terminal: Arc<TerminalLock<Crosswords<T>>> =
        Arc::new(TerminalLock::new(terminal));
    let (sender, _receiver) = corcovado::channel::channel();

    Context {
        route_id,
        #[cfg(not(target_os = "windows"))]
        main_fd: Arc::new(-1),
        #[cfg(not(target_os = "windows"))]
        shell_pid: 1,
        #[cfg(not(target_os = "windows"))]
        child_terminator: teletypewriter::ChildTerminator::retired(),
        messenger: Messenger::new(sender),
        renderable_content: RenderableContent::new(Cursor::default()),
        terminal,
        rich_text_id,
        dimension,
        title: ContextTitle::default(),
        title_dirty: false,
        spawned_program: String::new(),
        exited: false,
        _io_thread: None,
    }
}

#[cfg(test)]
pub fn create_mock_context<
    T: rio_backend::event::EventListener + Clone + std::marker::Send + 'static,
>(
    event_proxy: T,
    window_id: WindowId,
    rich_text_id: usize,
    dimension: ContextDimension,
) -> Context<T> {
    let config = ContextManagerConfig {
        dead_pty: true,
        ..ContextManagerConfig::default()
    };
    ContextManager::create_context(
        (&Cursor::default(), false),
        event_proxy.clone(),
        window_id,
        rich_text_id,
        dimension,
        &config,
    )
    .unwrap()
}

/// Where `current_index` lands after removing the tab at `removed`:
/// the focused tab falls back to its left neighbor, and a background
/// removal shifts the focused index left only when the removed tab
/// sat before it. Pure, so the arithmetic the tmux SIGHUP crash
/// hinged on stays testable without a GPU.
#[inline]
fn current_index_after_tab_removal(current: usize, removed: usize) -> usize {
    if removed <= current {
        current.saturating_sub(1)
    } else {
        current
    }
}

impl<T: EventListener + Clone + std::marker::Send + 'static> ContextManager<T> {
    #[inline]
    fn create_context(
        cursor_state: (&Cursor, bool),
        event_proxy: T,
        window_id: WindowId,
        rich_text_id: usize,
        dimension: ContextDimension,
        config: &ContextManagerConfig,
    ) -> Result<Context<T>, Box<dyn Error>> {
        let route_id = next_route_id();

        #[cfg(test)]
        if config.dead_pty {
            return Ok(create_dead_context(
                event_proxy,
                window_id,
                route_id,
                rich_text_id,
                dimension,
            ));
        }

        let cols: u16 = dimension.columns.try_into().unwrap_or(MIN_COLUMNS as u16);
        let rows: u16 = dimension.lines.try_into().unwrap_or(MIN_LINES as u16);
        #[cfg(not(target_os = "windows"))]
        let initial_winsize = crate::renderer::utils::terminal_dimensions(&dimension);

        let mut terminal = Crosswords::new(
            dimension,
            CursorShape::from_char(cursor_state.0.content),
            event_proxy.clone(),
            window_id,
            route_id,
            config.scrollback_history_limit,
        );
        terminal.set_grapheme_clustering(config.grapheme_clustering);
        terminal.set_default_colors(config.colors);
        // The application answers `q;grapheme=` by shaping the cluster
        // with the grid rasterizer (`GlyphProtocolGraphemeQuery`).
        terminal.set_glyph_grapheme_query(true);
        terminal.blinking_cursor = cursor_state.1;
        let terminal: Arc<TerminalLock<Crosswords<T>>> =
            Arc::new(TerminalLock::new(terminal));

        let integration = if config.shell_integration {
            crate::shell_integration::prepare(
                config.shell.program.as_deref(),
                &config.shell.args,
            )
        } else {
            crate::shell_integration::SpawnIntegration::default()
        };
        let (shell_program, shell_args) = match &integration.command {
            Some((program, args)) => (program.as_deref(), args.as_slice()),
            None => (
                config.shell.program.as_deref(),
                config.shell.args.as_slice(),
            ),
        };

        let pty;
        #[cfg(not(target_os = "windows"))]
        {
            if config.use_fork {
                tracing::info!("rio -> teletypewriter: create_pty_with_fork");
                pty = match create_pty_with_fork(
                    shell_program,
                    shell_args,
                    &integration.env,
                    cols,
                    rows,
                    initial_winsize.width,
                    initial_winsize.height,
                ) {
                    Ok(created_pty) => created_pty,
                    Err(err) => {
                        tracing::error!("{err:?}");
                        return Err(Box::new(err));
                    }
                }
            } else {
                tracing::info!("rio -> teletypewriter: create_pty_with_spawn");
                pty = match create_pty_with_spawn(
                    shell_program,
                    shell_args.to_vec(),
                    &config.working_dir,
                    (!integration.env.is_empty()).then(|| integration.env.clone()),
                    cols,
                    rows,
                    initial_winsize.width,
                    initial_winsize.height,
                ) {
                    Ok(created_pty) => created_pty,
                    Err(err) => {
                        tracing::error!("{err:?}");
                        return Err(Box::new(err));
                    }
                }
            };
        }

        #[cfg(not(target_os = "windows"))]
        let main_fd = pty.child.id.clone();
        #[cfg(not(target_os = "windows"))]
        let shell_pid = *pty.child.pid.clone() as u32;
        #[cfg(not(target_os = "windows"))]
        let child_terminator = pty.child.terminator();

        #[cfg(target_os = "windows")]
        {
            pty = match create_pty(
                shell_program,
                shell_args.to_vec(),
                &config.working_dir,
                (!integration.env.is_empty()).then(|| integration.env.clone()),
                cols,
                rows,
            ) {
                Ok(created_pty) => created_pty,
                Err(err) => {
                    tracing::error!("{err:?}");
                    return Err(Box::new(err));
                }
            }
        }

        let machine = Machine::new(
            Arc::clone(&terminal),
            pty,
            event_proxy.clone(),
            window_id,
            route_id,
        )?;
        let channel = machine.channel();
        let io_thread = if config.spawn_performer {
            Some(machine.spawn())
        } else {
            None
        };

        let messenger = Messenger::new(channel);

        let mut context = Context {
            route_id,
            #[cfg(not(target_os = "windows"))]
            main_fd,
            #[cfg(not(target_os = "windows"))]
            shell_pid,
            #[cfg(not(target_os = "windows"))]
            child_terminator,
            messenger,
            terminal,
            rich_text_id,
            renderable_content: RenderableContent::new(cursor_state.0.clone()),
            dimension,
            title: ContextTitle::default(),
            title_dirty: false,
            spawned_program: spawned_program_name(config),
            exited: false,
            _io_thread: io_thread,
        };
        context.title = ContextTitle {
            content: update_title(&config.title.content, &context, None),
        };
        Ok(context)
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        cursor_state: (&Cursor, bool),
        event_proxy: T,
        window_id: WindowId,
        rich_text_id: usize,
        ctx_config: ContextManagerConfig,
        size: ContextDimension,
        scaled_margin: Margin,
        sugarloaf_errors: Option<SugarloafErrors>,
    ) -> Result<Self, Box<dyn Error>> {
        let initial_context = match ContextManager::create_context(
            cursor_state,
            event_proxy.clone(),
            window_id,
            rich_text_id,
            size,
            &ctx_config,
        ) {
            Ok(context) => context,
            Err(err_message) => {
                tracing::error!("{:?}", err_message);

                event_proxy.send_event(
                    RioEvent::ReportToAssistant(RioError {
                        report: RioErrorType::InitializationError(
                            err_message.to_string(),
                        ),
                        level: RioErrorLevel::Error,
                    }),
                    window_id,
                );

                // A dead pane is still a pane: it takes a minted route
                // like any other, so the grid named after it is unique.
                create_dead_context(
                    event_proxy.clone(),
                    window_id,
                    next_route_id(),
                    0,
                    ContextDimension::default(),
                )
            }
        };

        // Sugarloaf has found errors and context need to notify it for the user
        if let Some(errors) = sugarloaf_errors {
            if !errors.fonts_not_found.is_empty() {
                event_proxy.send_event(
                    RioEvent::ReportToAssistant({
                        RioError {
                            report: RioErrorType::FontsNotFound(errors.fonts_not_found),
                            level: RioErrorLevel::Warning,
                        }
                    }),
                    window_id,
                );
            }
        }

        let mut manager = ContextManager {
            current_index: 0,
            current_route: 0,
            window_focused: false,
            focused_route: None,
            border_drag: None,
            contexts: smallvec![ContextGrid::new(
                initial_context,
                scaled_margin,
                ctx_config.split_color,
                ctx_config.split_active_color,
                ctx_config.panel,
            )],
            capacity: DEFAULT_CONTEXT_CAPACITY,
            event_proxy,
            window_id,
            config: ctx_config,
        };
        // The native titlebar starts as the placeholder; one poke makes
        // it converge on the displayed title even for shells that never
        // emit an OSC title or OSC 7.
        manager.sync_window_title();
        Ok(manager)
    }

    #[cfg(test)]
    pub fn start_with_capacity(
        capacity: usize,
        event_proxy: T,
        window_id: WindowId,
    ) -> Result<Self, Box<dyn Error>> {
        let config = ContextManagerConfig {
            #[cfg(test)]
            dead_pty: true,
            ..ContextManagerConfig::default()
        };
        let initial_context = ContextManager::create_context(
            (&Cursor::default(), false),
            event_proxy.clone(),
            window_id,
            0,
            ContextDimension::default(),
            &config,
        )?;

        Ok(ContextManager {
            current_index: 0,
            current_route: 0,
            window_focused: false,
            focused_route: None,
            border_drag: None,
            contexts: smallvec![ContextGrid::new(
                initial_context,
                Margin::default(),
                config.split_color,
                config.split_active_color,
                config.panel,
            )],
            capacity,
            event_proxy,
            window_id,
            config,
        })
    }

    /// Holds the exited pane open with a note instead of closing it.
    /// Returns false when the pane is gone or was already held, so the
    /// close goes ahead.
    pub fn hold_exited(&mut self, route_id: usize) -> bool {
        let Some(item) = self.get_by_route_id(route_id) else {
            return false;
        };
        let context = item.context_mut();
        if context.exited {
            return false;
        }
        context.exited = true;
        let left_vi_mode = {
            let mut terminal = context.terminal.lock();
            let in_vi_mode = terminal.mode().contains(rio_backend::crosswords::Mode::VI);
            if in_vi_mode {
                terminal.toggle_vi_mode();
            }
            terminal.terminal_attribute(rio_backend::crosswords::attr::Attr::Reset);
            terminal.configure_charset(CharsetIndex::G0, StandardCharset::Ascii);
            terminal.set_active_charset(CharsetIndex::G0);
            terminal.carriage_return();
            terminal.linefeed();
            terminal.input_str("Process exited. Press any key to close the terminal.");
            for mode in [
                NamedPrivateMode::ShowCursor,
                NamedPrivateMode::ReportX10MouseClicks,
                NamedPrivateMode::ReportMouseClicks,
                NamedPrivateMode::ReportCellMouseMotion,
                NamedPrivateMode::ReportAllMouseMotion,
                NamedPrivateMode::ReportFocusInOut,
            ] {
                terminal.unset_private_mode(PrivateMode::Named(mode));
            }
            Handler::set_keyboard_mode(
                &mut *terminal,
                KeyboardModes::empty(),
                KeyboardModesApplyBehavior::Replace,
            );
            in_vi_mode
        };
        if left_vi_mode {
            context
                .renderable_content
                .pending_update
                .set_terminal_damage(rio_backend::event::TerminalDamage::Full);
        }
        context.renderable_content.pending_update.set_dirty();
        true
    }

    /// Closes the focused pane if it is held after its command exited.
    pub fn close_if_exited(&mut self) -> bool {
        if !self.current().exited {
            return false;
        }
        self.event_proxy
            .send_event(RioEvent::CloseTerminal(self.current_route), self.window_id);
        true
    }

    #[inline]
    pub fn should_close_context_manager(
        &mut self,
        route_id: usize,
        sugarloaf: &mut impl PanelImages,
    ) -> bool {
        // Called when terminal.exit() fires: a tab-close action (the
        // context is already gone, nothing found below, return false)
        // or a PTY that exited on its own. The exiting PTY can live
        // ANYWHERE: a background tab, or a background split of any
        // tab (`tmux new -X` SIGHUPs a shell the user is not even
        // looking at), so the search covers every panel of every tab
        // rather than assuming the focused one.
        let Some(tab_index) = self.tab_index_for_route(route_id) else {
            return self.contexts.is_empty();
        };

        // A split dies: remove just that panel, keep the tab. When
        // the tab is focused and its focused panel was the one that
        // died, the sibling selection becomes the current route.
        if self.contexts[tab_index].len() > 1 {
            if let Some(removed) = self.contexts[tab_index].remove_by_route(route_id) {
                removed.release(sugarloaf);
            }
            if tab_index == self.current_index {
                self.sync_current_route();
            }
            return false;
        }

        // A whole tab dies.
        self.contexts[tab_index].remove_from_sugarloaf(sugarloaf);
        self.contexts.remove(tab_index);

        if self.contexts.is_empty() {
            return true;
        }

        // Removing a tab shifts every index after it. Adjusting only
        // when the focused tab itself died leaves `current_index`
        // past the end when a background tab exits first (the tmux
        // SIGHUP crash: len 1, index 1).
        let new_index = current_index_after_tab_removal(self.current_index, tab_index);
        if tab_index == self.current_index {
            self.set_current(new_index);
        } else {
            self.current_index = new_index;
        }

        self.keep_only_active_context_visible(sugarloaf);
        false
    }

    #[inline]
    pub fn request_render(&mut self) {
        self.event_proxy
            .send_event(RioEvent::RenderRoute(self.current_route), self.window_id);
    }

    #[inline]
    pub fn blink_cursor(&mut self, scheduled_time: u64) {
        // PrepareRender will force a render for any route that is focused on window
        // PrepareRenderOnRoute only call render function for specific route ids.
        self.event_proxy.send_event(
            RioEvent::BlinkCursor(scheduled_time, self.current_route),
            self.window_id,
        );
    }

    #[inline]
    pub fn schedule_render_on_route(&mut self, millis: u64) {
        self.event_proxy.send_event(
            RioEvent::PrepareRenderOnRoute(millis, self.current_route),
            self.window_id,
        );
    }

    #[inline]
    pub fn report_error_fonts_not_found(&mut self, fonts_not_found: Vec<SugarloafFont>) {
        if !fonts_not_found.is_empty() {
            self.event_proxy.send_event(
                RioEvent::ReportToAssistant({
                    RioError {
                        report: RioErrorType::FontsNotFound(fonts_not_found),
                        level: RioErrorLevel::Warning,
                    }
                }),
                self.window_id,
            );
        }
    }

    #[inline]
    pub fn create_new_window(&self) {
        self.event_proxy
            .send_event(RioEvent::CreateWindow, self.window_id);
    }

    #[inline]
    pub fn toggle_quake(&self) {
        self.event_proxy
            .send_event(RioEvent::ToggleQuake, self.window_id);
    }

    #[inline]
    pub fn close_unfocused_tabs(&mut self, sugarloaf: &mut impl PanelImages) {
        let current_route_id = self.current().route_id;
        self.contexts.retain(|ctx| {
            let keep = ctx.current().route_id == current_route_id;
            if !keep {
                ctx.remove_from_sugarloaf(sugarloaf);
            }
            keep
        });
        self.current_route = self.contexts[0].current().route_id;
        self.set_current(0);
    }

    #[inline]
    pub fn set_last_typing(&mut self) {
        self.current_mut().renderable_content.last_typing = Some(Instant::now());
    }

    #[inline]
    pub fn select_next_split(&mut self) {
        self.contexts[self.current_index].select_next_split();
        self.sync_current_route();
    }

    #[inline]
    pub fn select_prev_split(&mut self) {
        self.contexts[self.current_index].select_prev_split();
        self.sync_current_route();
    }

    #[inline]
    pub fn switch_to_next_split_or_tab(&mut self) {
        if self.contexts[self.current_index].select_next_split_no_loop() {
            self.sync_current_route();
            return;
        }
        if !self.step_tab(true) {
            return;
        }
        // Make sure first split is selected - get the root key
        let current_tab = &mut self.contexts[self.current_index];
        if let Some(root) = current_tab.root {
            current_tab.current = root;
        }
        self.sync_current_route();
    }

    #[inline]
    pub fn switch_to_prev_split_or_tab(&mut self) {
        if self.contexts[self.current_index].select_prev_split_no_loop() {
            self.sync_current_route();
            return;
        }
        if !self.step_tab(false) {
            return;
        }
        // Make sure last split is selected - get the last key in order
        let current_tab = &mut self.contexts[self.current_index];
        let ordered_keys = current_tab.get_ordered_keys();
        if let Some(&last_key) = ordered_keys.last() {
            current_tab.current = last_key;
        }
        self.sync_current_route();
    }

    #[inline]
    pub fn move_divider_up(&mut self, amount: f32) -> bool {
        self.contexts[self.current_index].move_divider_up(amount)
    }

    #[inline]
    pub fn move_divider_down(&mut self, amount: f32) -> bool {
        self.contexts[self.current_index].move_divider_down(amount)
    }

    #[inline]
    pub fn move_divider_left(&mut self, amount: f32) -> bool {
        self.contexts[self.current_index].move_divider_left(amount)
    }

    #[inline]
    pub fn move_divider_right(&mut self, amount: f32) -> bool {
        self.contexts[self.current_index].move_divider_right(amount)
    }

    #[inline]
    pub fn select_tab(&mut self, tab_index: usize) {
        if self.config.is_native {
            self.event_proxy
                .send_event(RioEvent::SelectNativeTabByIndex(tab_index), self.window_id);
            return;
        }

        self.set_current(tab_index);
    }

    #[inline]
    pub fn toggle_full_screen(&mut self) {
        self.event_proxy
            .send_event(RioEvent::ToggleFullScreen, self.window_id);
    }

    #[inline]
    pub fn toggle_appearance_theme(&mut self) {
        self.event_proxy
            .send_event(RioEvent::ToggleAppearanceTheme, self.window_id);
    }

    #[inline]
    pub fn minimize(&mut self) {
        self.event_proxy
            .send_event(RioEvent::Minimize(true), self.window_id);
    }

    #[inline]
    pub fn hide(&mut self) {
        self.event_proxy.send_event(RioEvent::Hide, self.window_id);
    }

    #[inline]
    pub fn quit(&mut self) {
        self.event_proxy.send_event(RioEvent::Quit, self.window_id);
    }

    #[cfg(target_os = "macos")]
    #[inline]
    pub fn hide_other_apps(&mut self) {
        self.event_proxy
            .send_event(RioEvent::HideOtherApplications, self.window_id);
    }

    #[inline]
    pub fn select_last_tab(&mut self) {
        if self.config.is_native {
            self.event_proxy
                .send_event(RioEvent::SelectNativeTabLast, self.window_id);
            return;
        }

        self.set_current(self.contexts.len() - 1);
    }

    #[inline]
    pub fn switch_to_settings(&mut self) {
        self.event_proxy
            .send_event(RioEvent::CreateConfigEditor, self.window_id);
    }

    #[inline]
    pub fn select_route_from_current_grid(&mut self) {
        self.sync_current_route();
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.contexts.len()
    }

    #[cfg(test)]
    pub fn title(&self, index: usize) -> Option<&ContextTitle> {
        self.contexts.get(index).map(|grid| &grid.current().title)
    }

    #[inline]
    pub fn custom_title(&self, index: usize) -> Option<&str> {
        self.contexts
            .get(index)
            .and_then(|grid| grid.custom_title.as_deref())
    }

    #[inline]
    /// A rename changes what `displayed_title_for_tab` returns, so the
    /// native titlebar gets the same convergence poke every other
    /// displayed-title mutation sends.
    pub fn set_custom_title(&mut self, index: usize, title: Option<String>) {
        if let Some(grid) = self.contexts.get_mut(index) {
            grid.custom_title = title;
        }
        self.sync_window_title();
    }

    /// Drop every tab's bell mark, used when the indicator is disabled
    /// by a live config reload: the ring-time gate stops new marks but
    /// cannot retract ones already set.
    pub fn clear_all_bells(&mut self) {
        for grid in self.contexts.iter_mut() {
            grid.bell = false;
        }
    }

    #[inline]
    pub fn custom_color(&self, index: usize) -> Option<[f32; 4]> {
        self.contexts.get(index).and_then(|grid| grid.custom_color)
    }

    #[inline]
    pub fn set_custom_color(&mut self, index: usize, color: Option<[f32; 4]>) {
        if let Some(grid) = self.contexts.get_mut(index) {
            grid.custom_color = color;
        }
    }

    /// Index of the tab containing `route_id`'s pane. Per-route state
    /// can live ANYWHERE (a background split of a background tab
    /// included), so the search covers every panel of every tab.
    #[inline]
    pub fn tab_index_for_route(&mut self, route_id: usize) -> Option<usize> {
        self.contexts
            .iter_mut()
            .position(|grid| grid.get_by_route_id(route_id).is_some())
    }

    /// A pane rang the bell. Flags its tab so the strip can surface it.
    /// The current tab is skipped only while the window has focus: the
    /// user is already looking at it then, but a ring in the visible
    /// tab of an UNFOCUSED window would otherwise leave no trace at
    /// all. Edge-triggered: a BEL flood marks once and repaints once.
    #[inline]
    pub fn ring_bell(&mut self, route_id: usize) -> bool {
        let Some(tab_index) = self.tab_index_for_route(route_id) else {
            return false;
        };

        if self.window_focused && tab_index == self.current_index {
            return false;
        }

        !std::mem::replace(&mut self.contexts[tab_index].bell, true)
    }

    #[inline]
    pub fn bell(&self, index: usize) -> bool {
        self.contexts.get(index).is_some_and(|grid| grid.bell)
    }

    /// Clears the focused tab's bell flag. Called every FOCUSED frame so
    /// the mark drops the moment the tab is shown to the user, whatever
    /// brought it to the front; an unfocused window still renders on PTY
    /// damage, and clearing there would wipe a mark nobody has seen yet.
    #[inline]
    pub fn clear_current_bell(&mut self) -> bool {
        let grid = &mut self.contexts[self.current_index];
        std::mem::replace(&mut grid.bell, false)
    }

    fn tab_index_for_grid(&self, id: GridId) -> Option<usize> {
        self.contexts.iter().position(|grid| grid.id() == id)
    }

    /// Start a divider drag if a border of the current grid is under
    /// the pointer. Returns the divider's direction, for the cursor.
    pub fn begin_border_drag(&mut self, x: f32, y: f32) -> Option<BorderDirection> {
        let grid = &self.contexts[self.current_index];
        let border = grid.find_border_at_position(x, y)?;
        let start_pos = match border.direction {
            BorderDirection::Vertical => x,
            BorderDirection::Horizontal => y,
        };
        let original_sizes = (
            grid.get_panel_size(border.left_or_top, border.direction),
            grid.get_panel_size(border.right_or_bottom, border.direction),
        );
        self.border_drag = Some(BorderDrag {
            grid: grid.id(),
            shape: grid.shape(),
            state: ResizeState {
                border,
                start_pos,
                original_sizes,
            },
        });
        Some(border.direction)
    }

    /// Move the dragged divider to the pointer. The grid it started on
    /// is the one resized, whichever tab is showing now and however the
    /// tabs were renumbered since. A grid whose shape changed mid-drag
    /// is left alone, its tree is not the one the drag knows, but the
    /// drag stays on so the motion is still swallowed until the release.
    /// `None` only when no drag is in progress.
    pub fn drag_border_to(&mut self, x: f32, y: f32) -> Option<DragStep> {
        let drag = *self.border_drag.as_ref()?;
        let state = drag.state;
        let direction = state.border.direction;
        let current_pos = match direction {
            BorderDirection::Vertical => x,
            BorderDirection::Horizontal => y,
        };
        let Some(tab) = self.tab_index_for_grid(drag.grid) else {
            return Some(DragStep {
                direction,
                visible: false,
            });
        };
        let visible = tab == self.current_index;
        let grid = &mut self.contexts[tab];
        if grid.shape() == drag.shape {
            grid.resize_border(
                &state.border,
                state.original_sizes,
                current_pos - state.start_pos,
            );
        }
        Some(DragStep { direction, visible })
    }

    /// End the divider drag, however it ends: the button released, the
    /// window lost focus. The dragged grid sends the winsizes it held
    /// back; every grid does when that grid cannot be found any more.
    /// Returns whether a drag was in progress.
    pub fn end_border_drag(&mut self) -> bool {
        let Some(drag) = self.border_drag.take() else {
            return false;
        };
        match self.tab_index_for_grid(drag.grid) {
            Some(tab) => self.contexts[tab].end_border_resize(),
            None => {
                for context_grid in self.contexts.iter_mut() {
                    context_grid.end_border_resize();
                }
            }
        }
        true
    }

    #[inline]
    pub fn resize_all_grids(&mut self, width: f32, height: f32) {
        for context_grid in self.contexts.iter_mut() {
            context_grid.resize(width, height);
        }
    }

    /// Re-render one pane's title. Returns whether the displayed text
    /// changed (the empty-content fallback is the pane's static
    /// spawned program, so displayed text changes exactly when the
    /// content does).
    fn refresh_item_title(
        template: &str,
        context: &mut Context<T>,
        prefetched_title: Option<&str>,
    ) -> bool {
        let content = update_title(template, context, prefetched_title);
        if content == context.title.content {
            return false;
        }
        context.title = ContextTitle { content };
        true
    }

    /// A pane's title data changed: an OSC 0/2 title (carried in
    /// `raw_title`, so the common `{{ title }}` render never locks the
    /// terminal) or an OSC 7 working directory (`raw_title` None: the
    /// render re-reads the stored directory). A DISPLAYED pane (its
    /// tab's current) re-renders immediately; a hidden pane is only
    /// marked dirty (one flag write, no locks, no render) and renders
    /// when it surfaces, so a background split streaming titles costs
    /// nothing visible. One route scan serves every decision. Returns
    /// whether the strip must repaint.
    pub fn on_title_change(&mut self, route_id: usize, raw_title: Option<&str>) -> bool {
        let Some(tab_index) = self.tab_index_for_route(route_id) else {
            return false;
        };
        if self.contexts[tab_index].current().route_id != route_id {
            if let Some(item) = self.contexts[tab_index].get_by_route_id(route_id) {
                item.context_mut().title_dirty = true;
            }
            return false;
        }
        let template = self.config.title.content.clone();
        let context = self.contexts[tab_index].current_mut();
        context.title_dirty = false;
        Self::refresh_item_title(&template, context, raw_title)
    }

    /// Mark every pane's title stale. For changes that affect panes no
    /// walk re-renders (a resize changing `{{columns}}`, a config
    /// reload changing the template), hidden splits and unwalked tabs
    /// re-render lazily when they surface via `sync_current_route`.
    pub fn mark_all_titles_dirty(&mut self) {
        for grid in self.contexts.iter_mut() {
            for item in grid.contexts_mut().values_mut() {
                item.context_mut().title_dirty = true;
            }
        }
    }

    /// Re-render tab titles from local state: a config reload can
    /// change the template, and a resize changes `{{columns}}`/
    /// `{{lines}}`. OSC title and OSC 7 changes arrive via
    /// `on_title_change` instead; nothing calls this on a timer. The
    /// chrome repaint rides the returned flag, and one unconditional
    /// titlebar poke per run makes the native title CONVERGE on the
    /// displayed text (the poke is payload-less and deduped at the
    /// sink, so a run that changed nothing costs one no-op event).
    /// `only_current` restricts the walk to the displayed tab: with
    /// the tab strip absent (navigation disabled) background tabs'
    /// titles render nowhere, so refreshing them buys nothing.
    pub fn update_titles(&mut self, only_current: bool) -> bool {
        let template = self.config.title.content.clone();
        let mut repaint = false;
        let range = if only_current {
            self.current_index..self.current_index + 1
        } else {
            0..self.contexts.len()
        };
        for index in range {
            let context = self.contexts[index].current_mut();
            context.title_dirty = false;
            repaint |= Self::refresh_item_title(&template, context, None);
        }
        self.sync_window_title();
        repaint
    }

    /// The title the strip displays for `index`'s tab: the user rename,
    /// else the rendered content, else the foreground program, else
    /// "~". The native titlebar reads the same chain, so the two can
    /// never disagree.
    pub fn displayed_title_for_tab(&self, index: usize) -> String {
        if let Some(custom) = self.custom_title(index) {
            return custom.to_string();
        }
        if let Some(grid) = self.contexts.get(index) {
            let context = grid.current();
            if !context.title.content.is_empty() {
                return context.title.content.clone();
            }
            if !context.spawned_program.is_empty() {
                return context.spawned_program.clone();
            }
        }
        String::from("~")
    }

    #[inline]
    pub fn displayed_title_for_current_tab(&self) -> String {
        self.displayed_title_for_tab(self.current_index)
    }

    /// Ask the event loop to refresh the native titlebar. Payload-less
    /// on purpose: the handler re-reads the displayed title at handling
    /// time, so a queued poke can never overwrite a newer title with a
    /// stale snapshot, and redundant pokes dedupe at the sink.
    fn sync_window_title(&mut self) {
        self.event_proxy
            .send_event(RioEvent::SyncWindowTitle, self.window_id);
    }

    /// Point `current_route` at the pane the user now sees and poke the
    /// titlebar. Every displayed-pane change (tab switch, split
    /// selection, split death, new splits) funnels here so the titlebar
    /// can never be left showing a pane that is not on screen.
    fn sync_current_route(&mut self) {
        self.current_route = self.current().route_id;
        if self.current().title_dirty {
            let template = self.config.title.content.clone();
            let context = self.contexts[self.current_index].current_mut();
            context.title_dirty = false;
            Self::refresh_item_title(&template, context, None);
        }
        self.sync_window_title();
        self.sync_focus_reports();
    }

    /// Window focus from the platform. Together with the active pane it
    /// decides which program is told it has focus.
    pub fn set_window_focus(&mut self, focused: bool) {
        self.window_focused = focused;
        self.sync_focus_reports();
    }

    /// To a program, focus means "my pane is the active one in a focused
    /// window". Tell the pane that stops matching (CSI O) and the one that
    /// starts (CSI I); each terminal speaks only when its program asked for
    /// focus events. Idempotent, so every active-pane change calls it; a
    /// pane that died owes nothing.
    fn sync_focus_reports(&mut self) {
        let wanted = self.window_focused.then(|| self.current().route_id);
        if wanted == self.focused_route {
            return;
        }
        if let Some(previous) = self.focused_route.take() {
            if let Some(item) = self.get_by_route_id(previous) {
                Self::write_focus(item.context(), false);
            }
        }
        if wanted.is_some() {
            Self::write_focus(self.current(), true);
        }
        self.focused_route = wanted;
    }

    /// The report goes down the same channel as keys, so a key typed
    /// right after a focus change cannot overtake it.
    fn write_focus(context: &Context<T>, focused: bool) {
        // Under the lock, like the reply to a program enabling focus
        // events, so the two cannot cross.
        let mut terminal = context.terminal.lock();
        if let Some(report) = terminal.set_focus(focused) {
            context.messenger.send_write(report.as_bytes());
        }
    }

    #[inline]
    pub fn get_by_route_id(
        &mut self,
        route_id: usize,
    ) -> Option<&mut ContextGridItem<T>> {
        // Search every tab, current first: per-route events (damage marks,
        // titles, color/size requests) must reach panes in background tabs,
        // otherwise their state is silently dropped until the pane's own
        // PTY speaks again.
        let current = self.current_index;
        if self.contexts[current].get_by_route_id(route_id).is_some() {
            return self.contexts[current].get_by_route_id(route_id);
        }
        self.contexts
            .iter_mut()
            .enumerate()
            .filter(|(i, _)| *i != current)
            .find_map(|(_, grid)| grid.get_by_route_id(route_id))
    }

    #[inline]
    pub fn contexts_mut(
        &mut self,
    ) -> &mut SmallVec<[ContextGrid<T>; DEFAULT_CONTEXT_CAPACITY]> {
        &mut self.contexts
    }

    #[inline]
    pub fn current_grid_len(&self) -> usize {
        self.contexts[self.current_index].len()
    }

    #[inline]
    pub fn remove_current_grid(&mut self, sugarloaf: &mut impl PanelImages) {
        if let Some(removed) = self.contexts[self.current_index].remove_current() {
            removed.release(sugarloaf);
        }
        self.sync_current_route();
    }

    #[inline]
    pub fn current_grid_mut(&mut self) -> &mut ContextGrid<T> {
        &mut self.contexts[self.current_index]
    }

    #[inline]
    pub fn current_grid(&self) -> &ContextGrid<T> {
        &self.contexts[self.current_index]
    }

    #[inline]
    pub fn get_panel_borders(&self) -> Vec<Rect> {
        self.contexts[self.current_index].get_panel_borders()
    }

    #[inline]
    pub fn get_current_grid_scaled_margin(&self) -> rio_backend::config::layout::Margin {
        self.contexts[self.current_index].get_scaled_margin()
    }

    #[cfg(test)]
    pub fn increase_capacity(&mut self, inc_val: usize) {
        self.capacity += inc_val;
    }

    #[inline]
    pub fn set_current(&mut self, context_id: usize) {
        if context_id < self.contexts.len() {
            self.current_index = context_id;
            self.sync_current_route();
        }
    }

    #[inline]
    pub fn close_current_context(&mut self, sugarloaf: &mut impl PanelImages) {
        if self.contexts.len() == 1 {
            // MacOS: Close last tab will work, leading to hide and
            // keep Rio running in background.
            #[cfg(target_os = "macos")]
            {
                self.event_proxy
                    .send_event(RioEvent::CloseWindow, self.window_id);
            }
            return;
        }

        let index_to_remove = self.current_index;

        // Remove all rich text from the grid before removing the context
        self.contexts[index_to_remove].remove_from_sugarloaf(sugarloaf);
        self.contexts.remove(index_to_remove);

        // The neighbour is selected only now, so no focus report goes to
        // the pane that just went away.
        self.set_current(index_to_remove.saturating_sub(1));

        self.keep_only_active_context_visible(sugarloaf);
    }

    #[inline]
    pub fn current_index(&self) -> usize {
        self.current_index
    }

    #[inline]
    pub fn current_route(&self) -> usize {
        self.current_route
    }

    #[inline]
    pub fn current(&self) -> &Context<T> {
        self.contexts[self.current_index].current()
    }

    #[inline]
    pub fn current_mut(&mut self) -> &mut Context<T> {
        self.contexts[self.current_index].current_mut()
    }

    /// Move to the next or previous tab without syncing the route, so a
    /// caller that goes on to pick a split in that tab syncs once, for the
    /// pane that will actually show; a sync in between would tell the
    /// tab's remembered split it gained and lost focus. With native tabs
    /// the platform is asked instead and nothing moves here; returns
    /// whether the index moved.
    fn step_tab(&mut self, forward: bool) -> bool {
        if self.config.is_native {
            let event = if forward {
                RioEvent::SelectNativeTabNext
            } else {
                RioEvent::SelectNativeTabPrev
            };
            self.event_proxy.send_event(event, self.window_id);
            return false;
        }

        let last = self.contexts.len() - 1;
        self.current_index = match (forward, self.current_index) {
            (true, index) if index == last => 0,
            (true, index) => index + 1,
            (false, 0) => last,
            (false, index) => index - 1,
        };
        true
    }

    #[inline]
    pub fn switch_to_next(&mut self) {
        if self.step_tab(true) {
            self.sync_current_route();
        }
    }

    #[inline]
    pub fn switch_to_prev(&mut self) {
        if self.step_tab(false) {
            self.sync_current_route();
        }
    }

    #[inline]
    pub fn move_current_to_prev(&mut self) {
        let len = self.contexts.len();
        if len <= 1 {
            return;
        }

        let current = self.current_index;
        let target_index = if current == 0 { len - 1 } else { current - 1 };
        self.contexts.swap(current, target_index);
        self.select_tab(target_index);
    }

    #[inline]
    pub fn move_current_to_next(&mut self) {
        let len = self.contexts.len();
        if len <= 1 {
            return;
        }

        let current = self.current_index;
        let target_index = if current == len - 1 { 0 } else { current + 1 };
        self.contexts.swap(current, target_index);
        self.select_tab(target_index);
    }

    #[inline]
    pub fn move_current_tab_to(&mut self, target: usize) {
        if self.config.is_native {
            return;
        }

        let current = self.current_index;
        if target == current || target >= self.contexts.len() {
            return;
        }

        let grid = self.contexts.remove(current);
        self.contexts.insert(target, grid);
        self.set_current(target);
    }

    pub fn split(&mut self, rich_text_id: usize, split_down: bool) {
        let mut working_dir = self.config.working_dir.clone();
        if self.config.cwd {
            #[cfg(not(target_os = "windows"))]
            {
                let current_context = self.current();
                if let Ok(path) = teletypewriter::foreground_process_path(
                    *current_context.main_fd,
                    current_context.shell_pid,
                ) {
                    working_dir = Some(path.to_string_lossy().to_string());
                }
            }

            #[cfg(target_os = "windows")]
            {
                // if let Ok(path) = teletypewriter::foreground_process_path() {
                //     working_dir =
                //         Some(path.to_string_lossy().to_string());
                // }
                working_dir = None;
            }
        }

        let mut cloned_config = self.config.clone();
        if working_dir.is_some() {
            cloned_config.working_dir = working_dir;
        }

        let current = self.current();
        let cursor = current.cursor_from_ref();

        match ContextManager::create_context(
            (&cursor, current.renderable_content.has_blinking_enabled),
            self.event_proxy.clone(),
            self.window_id,
            rich_text_id,
            self.current().dimension,
            &cloned_config,
        ) {
            Ok(new_context) => {
                if split_down {
                    self.contexts[self.current_index].split_down(new_context);
                } else {
                    self.contexts[self.current_index].split_right(new_context);
                }

                // The grid's current pane is the new split when the
                // split took, and the old one when it did not.
                self.sync_current_route();
            }
            Err(..) => {
                tracing::error!("not able to create a new context");
            }
        }
    }

    pub fn split_from_config(
        &mut self,
        rich_text_id: usize,
        split_down: bool,
        config: rio_backend::config::Config,
    ) {
        let (shell, working_dir) = process_open_url(
            config.shell.to_owned(),
            config.working_dir.to_owned(),
            config.editor.to_owned(),
            None,
        );

        let context_manager_config = ContextManagerConfig {
            #[cfg(test)]
            dead_pty: false,
            cwd: config.navigation.current_working_directory,
            shell,
            working_dir,
            spawn_performer: true,
            #[cfg(not(target_os = "windows"))]
            use_fork: config.use_fork,
            shell_integration: config.shell_integration,
            is_native: config.navigation.is_native(),
            // When navigation is collapsed and does not contain any color rule
            // does not make sense fetch for foreground process names
            split_color: config.colors.split,
            split_active_color: config.colors.split_active,
            panel: config.panel,
            title: config.title,
            keyboard: config.keyboard,
            scrollback_history_limit: config.scrollback_history_limit,
            grapheme_clustering: config.grapheme_clustering,
            colors: rio_backend::config::colors::term::List::from(&config.colors),
        };

        let current = self.current();
        let cursor = current.cursor_from_ref();

        match ContextManager::create_context(
            (&cursor, current.renderable_content.has_blinking_enabled),
            self.event_proxy.clone(),
            self.window_id,
            rich_text_id,
            self.current().dimension,
            &context_manager_config,
        ) {
            Ok(new_context) => {
                if split_down {
                    self.contexts[self.current_index].split_down(new_context);
                } else {
                    self.contexts[self.current_index].split_right(new_context);
                }

                // The grid's current pane is the new split when the
                // split took, and the old one when it did not.
                self.sync_current_route();
            }
            Err(..) => {
                tracing::error!("not able to create a new context");
            }
        }
    }

    #[inline]
    pub fn add_context(&mut self, redirect: bool, rich_text_id: usize) {
        let mut working_dir = self.config.working_dir.clone();
        if self.config.cwd {
            #[cfg(not(target_os = "windows"))]
            {
                let current_context = self.current();
                if let Ok(path) = teletypewriter::foreground_process_path(
                    *current_context.main_fd,
                    current_context.shell_pid,
                ) {
                    working_dir = Some(path.to_string_lossy().to_string());
                }
            }

            #[cfg(target_os = "windows")]
            {
                // if let Ok(path) = teletypewriter::foreground_process_path() {
                //     working_dir =
                //         Some(path.to_string_lossy().to_string());
                // }
                working_dir = None;
            }
        }

        if self.config.is_native {
            self.event_proxy
                .send_event(RioEvent::CreateNativeTab(working_dir), self.window_id);
            return;
        }

        let size = self.contexts.len();
        if size < self.capacity {
            let last_index = self.contexts.len();

            let mut cloned_config = self.config.clone();
            if working_dir.is_some() {
                cloned_config.working_dir = working_dir;
            }

            let current = self.current();
            let cursor = current.cursor_from_ref();
            let mut dimension = current.dimension;

            // If current has splits then shouldn't use that dimension
            if self.current_grid().len() > 1 {
                dimension = self.current_grid().grid_dimension();
            }

            match ContextManager::create_context(
                (&cursor, current.renderable_content.has_blinking_enabled),
                self.event_proxy.clone(),
                self.window_id,
                rich_text_id,
                dimension,
                &cloned_config,
            ) {
                Ok(new_context) => {
                    let previous_scaled_margin =
                        self.contexts[self.current_index].scaled_margin;
                    self.contexts.push(ContextGrid::new(
                        new_context,
                        previous_scaled_margin,
                        self.config.split_color,
                        self.config.split_active_color,
                        self.config.panel,
                    ));
                    if redirect {
                        self.current_index = last_index;
                        self.sync_current_route();
                    }
                }
                Err(..) => {
                    tracing::error!("not able to create a new context");
                }
            }
        }
    }

    /// Hide all rich text components except for the current tab
    #[inline]
    pub fn keep_only_active_context_visible(&self, sugarloaf: &mut impl PanelImages) {
        for (idx, context) in self.contexts.iter().enumerate() {
            // Skip the current tab
            if idx == self.current_index {
                context.set_all_rich_text_visibility(sugarloaf, true);
                continue;
            }

            context.set_all_rich_text_visibility(sugarloaf, false);
        }
    }

    /// Switch visibility between two contexts (hide old, show new)
    #[inline]
    pub fn switch_context_visibility(
        &self,
        sugarloaf: &mut impl PanelImages,
        old_index: usize,
        new_index: usize,
    ) {
        if let Some(old_context) = self.contexts.get(old_index) {
            old_context.set_all_rich_text_visibility(sugarloaf, false);
        }
        if let Some(new_context) = self.contexts.get(new_index) {
            new_context.set_all_rich_text_visibility(sugarloaf, true);
        }
    }
}

pub fn process_open_url(
    mut shell: Shell,
    mut working_dir: Option<String>,
    editor: Shell,
    open_url: Option<&str>,
) -> (Shell, Option<String>) {
    if open_url.is_none() {
        return (shell, working_dir);
    }

    if let Ok(url) = url::Url::parse(open_url.unwrap_or_default()) {
        if let Ok(path_buf) = url.to_file_path() {
            if path_buf.exists() {
                if path_buf.is_file() {
                    let mut args = editor.args;
                    args.push(path_buf.display().to_string());
                    shell = Shell {
                        program: editor.program,
                        args,
                    }
                } else if path_buf.is_dir() {
                    working_dir = Some(path_buf.display().to_string());
                }
            }
        }
    }

    (shell, working_dir)
}

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::event::VoidListener;

    #[derive(Clone, Default)]
    struct FocusRecorder(std::sync::Arc<std::sync::Mutex<Vec<(usize, String)>>>);

    impl EventListener for FocusRecorder {
        fn send_event(&self, event: RioEvent, _: WindowId) {
            match event {
                RioEvent::PtyWrite(route, text) => {
                    self.0.lock().unwrap().push((route, text));
                }
                RioEvent::FocusReport(route, focused) => {
                    self.0
                        .lock()
                        .unwrap()
                        .push((route, format!("reply:{focused}")));
                }
                _ => {}
            }
        }
    }

    // Focus follows the pane the user is looking at, not just the window:
    // a tab switch tells the old pane it lost focus and the new one it
    // gained it, when they asked for focus events.
    #[test]
    fn focus_reports_follow_tab_switches() {
        use rio_backend::performer::handler::Processor;
        let recorder = FocusRecorder::default();
        let mut cm =
            ContextManager::start_with_capacity(5, recorder.clone(), WindowId::from(0))
                .unwrap();
        cm.add_context(true, 0);
        assert_eq!(cm.len(), 2);
        // Each pane's PTY input lands on a channel the test reads.
        let mut routes = Vec::new();
        let mut inputs = Vec::new();
        for i in 0..2 {
            let (tx, rx) = corcovado::channel::channel();
            let context = cm.contexts[i].current_mut();
            context.messenger = Messenger::new(tx);
            routes.push(context.route_id);
            inputs.push(rx);
            let mut parser = Processor::default();
            parser.advance(&mut *context.terminal.lock(), b"\x1b[?1004h");
        }
        let drain = |rx: &corcovado::channel::Receiver<Msg>| {
            let mut out = Vec::new();
            while let Ok(Msg::Input(bytes)) = rx.try_recv() {
                out.push(bytes.into_owned());
            }
            out
        };
        let gained = vec![b"\x1b[I".to_vec()];
        let lost = vec![b"\x1b[O".to_vec()];
        let none: Vec<Vec<u8>> = Vec::new();

        // The window was never focused: enabling the mode asked for a
        // reply telling each program it is unfocused, and the switch
        // add_context made reported nothing.
        assert_eq!(
            *recorder.0.lock().unwrap(),
            vec![
                (routes[0], "reply:false".to_string()),
                (routes[1], "reply:false".to_string())
            ]
        );
        assert_eq!(drain(&inputs[0]), none);
        assert_eq!(drain(&inputs[1]), none);

        cm.set_window_focus(true); // tab 1 gains
        assert_eq!(drain(&inputs[0]), none);
        assert_eq!(drain(&inputs[1]), gained);

        cm.switch_to_prev(); // tab 1 loses, tab 0 gains
        assert_eq!(drain(&inputs[0]), gained);
        assert_eq!(drain(&inputs[1]), lost);

        cm.set_window_focus(false); // tab 0 loses
        cm.set_window_focus(false); // a repeat: nothing
        cm.switch_to_next(); // unfocused window: nothing
        assert_eq!(drain(&inputs[0]), lost);
        assert_eq!(drain(&inputs[1]), none);

        cm.set_window_focus(true); // tab 1 gains
        assert_eq!(drain(&inputs[0]), none);
        assert_eq!(drain(&inputs[1]), gained);

        // Nothing but the two enable replies rode the event path.
        assert_eq!(recorder.0.lock().unwrap().len(), 2);
    }

    #[test]
    fn test_capacity() {
        let window_id: WindowId = WindowId::from(0);

        let context_manager =
            ContextManager::start_with_capacity(5, VoidListener {}, window_id).unwrap();
        assert_eq!(context_manager.capacity, 5);

        let mut context_manager =
            ContextManager::start_with_capacity(5, VoidListener {}, window_id).unwrap();
        context_manager.increase_capacity(3);
        assert_eq!(context_manager.capacity, 8);
    }

    #[test]
    fn test_add_context() {
        let window_id: WindowId = WindowId::from(0);

        let mut context_manager =
            ContextManager::start_with_capacity(5, VoidListener {}, window_id).unwrap();
        assert_eq!(context_manager.capacity, 5);
        assert_eq!(context_manager.current_index, 0);

        let should_redirect = false;
        context_manager.add_context(should_redirect, 0);
        assert_eq!(context_manager.capacity, 5);
        assert_eq!(context_manager.current_index, 0);

        let should_redirect = true;
        context_manager.add_context(should_redirect, 0);
        assert_eq!(context_manager.capacity, 5);
        assert_eq!(context_manager.current_index, 2);
    }

    #[test]
    fn test_add_context_start_with_capacity_limit() {
        let window_id: WindowId = WindowId::from(0);

        let mut context_manager =
            ContextManager::start_with_capacity(3, VoidListener {}, window_id).unwrap();
        assert_eq!(context_manager.capacity, 3);
        assert_eq!(context_manager.current_index, 0);
        let should_redirect = false;
        context_manager.add_context(should_redirect, 0);
        assert_eq!(context_manager.len(), 2);
        context_manager.add_context(should_redirect, 0);
        assert_eq!(context_manager.len(), 3);

        for _ in 0..20 {
            context_manager.add_context(should_redirect, 0);
        }

        assert_eq!(context_manager.len(), 3);
        assert_eq!(context_manager.capacity, 3);
    }

    /// The tmux `new -ADXs` crash (#1848): SIGHUP kills a BACKGROUND
    /// tab's shell, the tab at a lower index is removed, and the
    /// focused index must shift left with it or the very next
    /// `contexts[current_index]` is out of bounds (len 1, index 1).
    #[test]
    fn current_index_adjusts_after_tab_removal() {
        // Background tab before the focused one dies: shift left.
        assert_eq!(current_index_after_tab_removal(1, 0), 0);
        assert_eq!(current_index_after_tab_removal(3, 1), 2);
        // Background tab after the focused one: nothing shifts.
        assert_eq!(current_index_after_tab_removal(0, 1), 0);
        assert_eq!(current_index_after_tab_removal(2, 3), 2);
        // The focused tab itself: fall back to the left neighbor.
        assert_eq!(current_index_after_tab_removal(0, 0), 0);
        assert_eq!(current_index_after_tab_removal(2, 2), 1);
    }

    #[test]
    fn test_set_current() {
        let window_id: WindowId = WindowId::from(0);

        let mut context_manager =
            ContextManager::start_with_capacity(8, VoidListener {}, window_id).unwrap();
        let should_redirect = true;

        context_manager.add_context(should_redirect, 0);
        assert_eq!(context_manager.current_index, 1);
        context_manager.set_current(0);
        assert_eq!(context_manager.current_index, 0);
        assert_eq!(context_manager.len(), 2);
        assert_eq!(context_manager.capacity, 8);

        let should_redirect = false;
        context_manager.add_context(should_redirect, 0);
        context_manager.add_context(should_redirect, 0);
        context_manager.set_current(3);
        assert_eq!(context_manager.current_index, 3);

        context_manager.set_current(8);
        assert_eq!(context_manager.current_index, 3);
    }

    #[test]
    fn bell_marks_background_tabs_only_and_clears_on_focus() {
        let mut cm =
            ContextManager::start_with_capacity(5, VoidListener {}, WindowId::from(0))
                .unwrap();
        cm.add_context(true, 0);
        cm.add_context(true, 0);
        cm.set_current(0);

        let background_route = cm.contexts[2].current().route_id;
        let focused_route = cm.contexts[0].current().route_id;

        cm.set_window_focus(true);
        assert!(cm.ring_bell(background_route));
        assert!(cm.bell(2));

        assert!(!cm.ring_bell(background_route));
        assert!(cm.bell(2));

        assert!(!cm.ring_bell(focused_route));
        assert!(!cm.bell(0));

        cm.set_window_focus(false);
        assert!(cm.ring_bell(focused_route));
        assert!(cm.bell(0));
        cm.contexts[0].bell = false;
        cm.set_window_focus(true);

        // Unknown routes (already-closed panes) are a no-op.
        assert!(!cm.ring_bell(usize::MAX));

        // Focusing the tab drops the mark on the next rendered frame.
        cm.set_current(2);
        assert!(cm.bell(2));
        assert!(cm.clear_current_bell());
        assert!(!cm.bell(2));
    }

    #[test]
    fn hold_keeps_exited_pane_until_closed() {
        use rio_backend::crosswords::pos::{Column, Line, Pos};
        use rio_backend::crosswords::{CrosswordsSize, Mode};

        let mut cm =
            ContextManager::start_with_capacity(5, VoidListener {}, WindowId::from(0))
                .unwrap();
        let route = cm.current().route_id;
        {
            let mut terminal = cm.current().terminal.lock();
            terminal.resize(CrosswordsSize::new(80, 4));
            Handler::set_keyboard_mode(
                &mut *terminal,
                KeyboardModes::DISAMBIGUATE_ESC_CODES,
                KeyboardModesApplyBehavior::Replace,
            );
            terminal.set_private_mode(PrivateMode::Named(
                NamedPrivateMode::ReportAllMouseMotion,
            ));
            terminal.toggle_vi_mode();
        }
        assert!(!cm.close_if_exited());

        assert!(cm.hold_exited(route));
        assert!(cm.current().exited);
        let mode = cm.current().terminal.lock().mode();
        assert!(!mode.contains(Mode::SHOW_CURSOR));
        assert!(!mode.intersects(Mode::KITTY_KEYBOARD_PROTOCOL));
        assert!(!mode.intersects(Mode::MOUSE_MODE));
        assert!(!mode.contains(Mode::VI));
        let text = {
            let terminal = cm.current().terminal.lock();
            let end = Pos::new(
                Line(terminal.screen_lines() as i32 - 1),
                Column(terminal.columns() - 1),
            );
            terminal.bounds_to_string(Pos::new(Line(0), Column(0)), end)
        };
        assert!(
            text.contains("Process exited. Press any key to close the terminal."),
            "{text:?}"
        );

        assert!(cm.close_if_exited());
        assert!(!cm.hold_exited(route));
        assert!(!cm.hold_exited(usize::MAX));
    }

    #[test]
    fn update_titles_only_current_leaves_other_tabs_dirty() {
        let mut cm =
            ContextManager::start_with_capacity(5, VoidListener {}, WindowId::from(0))
                .unwrap();
        cm.add_context(false, 0);
        cm.config.title.content = "{{ columns }}".to_string();

        cm.mark_all_titles_dirty();
        assert!(cm.contexts[0].current().title_dirty);
        assert!(cm.contexts[1].current().title_dirty);

        // A current-only walk (tab strip absent) must not silently
        // clear panes it never re-rendered.
        assert!(cm.update_titles(true));
        assert!(!cm.contexts[0].current().title_dirty);
        assert!(cm.contexts[1].current().title_dirty);
        let columns = cm.contexts[0].current().dimension.columns.to_string();
        assert_eq!(cm.contexts[0].current().title.content, columns);

        // Surfacing the stale tab re-renders it via sync_current_route.
        cm.set_current(1);
        assert!(!cm.contexts[1].current().title_dirty);
        let columns = cm.contexts[1].current().dimension.columns.to_string();
        assert_eq!(cm.contexts[1].current().title.content, columns);
    }

    #[test]
    fn on_title_change_renders_displayed_pane_from_event_string() {
        let mut cm =
            ContextManager::start_with_capacity(5, VoidListener {}, WindowId::from(0))
                .unwrap();
        cm.add_context(false, 0);
        cm.config.title.content = "{{ title }}".to_string();

        let background_route = cm.contexts[1].current().route_id;
        cm.contexts[1].current_mut().title_dirty = true;
        assert!(cm.on_title_change(background_route, Some("hello")));
        assert_eq!(cm.contexts[1].current().title.content, "hello");
        assert!(!cm.contexts[1].current().title_dirty);

        // The same title again changes nothing: no repaint requested.
        assert!(!cm.on_title_change(background_route, Some("hello")));

        // Unknown routes (already-closed panes) are a no-op.
        assert!(!cm.on_title_change(usize::MAX, Some("x")));
    }

    fn set_tab_title(cm: &mut ContextManager<VoidListener>, index: usize, content: &str) {
        cm.contexts[index].current_mut().title.content = content.to_string();
    }

    fn tab_titles(cm: &ContextManager<VoidListener>) -> Vec<String> {
        (0..cm.len())
            .map(|i| cm.title(i).unwrap().content.clone())
            .collect()
    }

    #[test]
    fn test_title_follows_tab_move() {
        let window_id = WindowId::from(0);
        let mut cm =
            ContextManager::start_with_capacity(5, VoidListener {}, window_id).unwrap();
        for _ in 0..3 {
            cm.add_context(false, 0);
        }
        assert_eq!(cm.len(), 4);
        for (i, label) in ["a", "b", "c", "d"].iter().enumerate() {
            set_tab_title(&mut cm, i, label);
        }

        // Drag tab 1 to slot 3 (rotate). The title must track the moved
        // tab immediately, without waiting on the next update_titles tick.
        cm.set_current(1);
        cm.move_current_tab_to(3);

        assert_eq!(tab_titles(&cm), ["a", "c", "d", "b"]);
        assert_eq!(cm.current().title.content, "b");
    }

    #[test]
    fn test_title_follows_tab_swap() {
        let window_id = WindowId::from(0);
        let mut cm =
            ContextManager::start_with_capacity(5, VoidListener {}, window_id).unwrap();
        for _ in 0..3 {
            cm.add_context(false, 0);
        }
        for (i, label) in ["a", "b", "c", "d"].iter().enumerate() {
            set_tab_title(&mut cm, i, label);
        }

        // Swap current (0) with its neighbor (1).
        cm.set_current(0);
        cm.move_current_to_next();

        assert_eq!(tab_titles(&cm), ["b", "a", "c", "d"]);
    }

    #[test]
    fn test_custom_title_follows_tab_move() {
        let window_id = WindowId::from(0);
        let mut cm =
            ContextManager::start_with_capacity(5, VoidListener {}, window_id).unwrap();
        for _ in 0..3 {
            cm.add_context(false, 0);
        }
        cm.set_custom_title(2, Some("work".to_string()));

        // Move tab 1 → 3 (rotate): the override on tab 2 shifts to slot 1,
        // with no remap bookkeeping.
        cm.set_current(1);
        cm.move_current_tab_to(3);

        assert_eq!(cm.custom_title(1), Some("work"));
        assert_eq!(cm.custom_title(2), None);

        // Clearing with None removes the override.
        cm.set_custom_title(1, None);
        assert_eq!(cm.custom_title(1), None);
    }

    #[test]
    fn test_custom_color_follows_tab_move() {
        let window_id = WindowId::from(0);
        let mut cm =
            ContextManager::start_with_capacity(5, VoidListener {}, window_id).unwrap();
        for _ in 0..3 {
            cm.add_context(false, 0);
        }
        let red = [1.0, 0.0, 0.0, 1.0];
        cm.set_custom_color(2, Some(red));

        // Move tab 1 → 3 (rotate): the color on tab 2 shifts to slot 1.
        cm.set_current(1);
        cm.move_current_tab_to(3);

        assert_eq!(cm.custom_color(1), Some(red));
        assert_eq!(cm.custom_color(2), None);

        cm.set_custom_color(1, None);
        assert_eq!(cm.custom_color(1), None);
    }

    #[test]
    fn test_switch_to_next() {
        let window_id: WindowId = WindowId::from(0);

        let mut context_manager =
            ContextManager::start_with_capacity(5, VoidListener {}, window_id).unwrap();
        let should_redirect = false;

        context_manager.add_context(should_redirect, 0);
        context_manager.add_context(should_redirect, 0);
        context_manager.add_context(should_redirect, 0);
        context_manager.add_context(should_redirect, 0);
        context_manager.add_context(should_redirect, 0);
        assert_eq!(context_manager.len(), 5);
        assert_eq!(context_manager.current_index, 0);

        context_manager.switch_to_next();
        assert_eq!(context_manager.current_index, 1);
        context_manager.switch_to_next();
        assert_eq!(context_manager.current_index, 2);
        context_manager.switch_to_next();
        assert_eq!(context_manager.current_index, 3);
        context_manager.switch_to_next();
        assert_eq!(context_manager.current_index, 4);
        context_manager.switch_to_next();
        assert_eq!(context_manager.current_index, 0);
        context_manager.switch_to_next();
        assert_eq!(context_manager.current_index, 1);
    }

    #[test]
    fn test_move_current_to_next() {
        let window_id = WindowId::from(0);

        let mut context_manager =
            ContextManager::start_with_capacity(5, VoidListener {}, window_id).unwrap();
        let should_redirect = false;

        context_manager.current_mut().rich_text_id = 1;
        context_manager.add_context(should_redirect, 0);
        context_manager.add_context(should_redirect, 0);
        context_manager.add_context(should_redirect, 0);
        context_manager.add_context(should_redirect, 0);

        assert_eq!(context_manager.len(), 5);
        assert_eq!(context_manager.current_index, 0);
        assert_eq!(context_manager.current().rich_text_id, 1);

        context_manager.move_current_to_next();
        assert_eq!(context_manager.current_index, 1);
        assert_eq!(context_manager.current().rich_text_id, 1);

        context_manager.move_current_to_next();
        assert_eq!(context_manager.current_index, 2);
        assert_eq!(context_manager.current().rich_text_id, 1);

        context_manager.move_current_to_next();
        assert_eq!(context_manager.current_index, 3);
        assert_eq!(context_manager.current().rich_text_id, 1);

        context_manager.move_current_to_next();
        assert_eq!(context_manager.current_index, 4);
        assert_eq!(context_manager.current().rich_text_id, 1);

        context_manager.move_current_to_next();
        assert_eq!(context_manager.current_index, 0);
        assert_eq!(context_manager.current().rich_text_id, 1);

        context_manager.move_current_to_next();
        assert_eq!(context_manager.current_index, 1);
        assert_eq!(context_manager.current().rich_text_id, 1);
    }

    #[test]
    fn test_move_current_to_prev() {
        let window_id = WindowId::from(0);

        let mut context_manager =
            ContextManager::start_with_capacity(5, VoidListener {}, window_id).unwrap();
        let should_redirect = false;

        context_manager.current_mut().rich_text_id = 1;
        context_manager.add_context(should_redirect, 0);
        context_manager.add_context(should_redirect, 0);
        context_manager.add_context(should_redirect, 0);
        context_manager.add_context(should_redirect, 0);

        assert_eq!(context_manager.len(), 5);
        assert_eq!(context_manager.current_index, 0);
        assert_eq!(context_manager.current().rich_text_id, 1);

        context_manager.move_current_to_prev();
        assert_eq!(context_manager.current_index, 4);
        assert_eq!(context_manager.current().rich_text_id, 1);

        context_manager.move_current_to_prev();
        assert_eq!(context_manager.current_index, 3);
        assert_eq!(context_manager.current().rich_text_id, 1);

        context_manager.move_current_to_prev();
        assert_eq!(context_manager.current_index, 2);
        assert_eq!(context_manager.current().rich_text_id, 1);

        context_manager.move_current_to_prev();
        assert_eq!(context_manager.current_index, 1);
        assert_eq!(context_manager.current().rich_text_id, 1);

        context_manager.move_current_to_prev();
        assert_eq!(context_manager.current_index, 0);
        assert_eq!(context_manager.current().rich_text_id, 1);

        context_manager.move_current_to_prev();
        assert_eq!(context_manager.current_index, 4);
        assert_eq!(context_manager.current().rich_text_id, 1);
    }

    #[test]
    fn test_move_current_tab_to() {
        let window_id = WindowId::from(0);

        let mut context_manager =
            ContextManager::start_with_capacity(5, VoidListener {}, window_id).unwrap();
        let should_redirect = false;

        context_manager.add_context(should_redirect, 0);
        context_manager.add_context(should_redirect, 0);
        context_manager.add_context(should_redirect, 0);
        context_manager.add_context(should_redirect, 0);

        // Tag every tab with its starting position.
        for i in 0..5 {
            context_manager.set_current(i);
            context_manager.current_mut().rich_text_id = i;
        }

        let order = |cm: &mut ContextManager<VoidListener>| -> Vec<usize> {
            (0..5)
                .map(|i| {
                    cm.set_current(i);
                    cm.current().rich_text_id
                })
                .collect()
        };

        // Multi-slot jump forward: tabs in between shift left by one.
        context_manager.set_current(1);
        context_manager.move_current_tab_to(3);
        assert_eq!(context_manager.current_index, 3);
        assert_eq!(context_manager.current().rich_text_id, 1);
        assert_eq!(order(&mut context_manager), vec![0, 2, 3, 1, 4]);

        // Multi-slot jump backward: tabs in between shift right by one.
        context_manager.set_current(3);
        context_manager.move_current_tab_to(0);
        assert_eq!(context_manager.current_index, 0);
        assert_eq!(context_manager.current().rich_text_id, 1);
        assert_eq!(order(&mut context_manager), vec![1, 0, 2, 3, 4]);

        // No-op cases: same index and out-of-bounds target.
        context_manager.set_current(2);
        context_manager.move_current_tab_to(2);
        assert_eq!(context_manager.current_index, 2);
        context_manager.move_current_tab_to(5);
        assert_eq!(context_manager.current_index, 2);
        assert_eq!(order(&mut context_manager), vec![1, 0, 2, 3, 4]);
    }
}

#[cfg(test)]
mod sugarloaf_free {
    use super::*;
    use crate::layout::{BorderDirection, PanelImages};
    use rio_backend::event::{VoidListener, WindowId};

    /// Stands in for sugarloaf: records what the manager asks it to drop.
    #[derive(Default)]
    struct Images {
        cleared: Vec<usize>,
        removed_routes: Vec<usize>,
    }

    impl PanelImages for Images {
        fn clear_image_overlays_for(&mut self, rich_text_id: usize) {
            self.cleared.push(rich_text_id);
        }

        fn remove_route_images(&mut self, route_id: usize) {
            self.removed_routes.push(route_id);
        }
    }

    fn manager() -> ContextManager<VoidListener> {
        let mut cm =
            ContextManager::start_with_capacity(5, VoidListener {}, WindowId::from(0))
                .unwrap();
        cm.resize_all_grids(1200.0, 800.0);
        cm
    }

    fn sorted(mut v: Vec<usize>) -> Vec<usize> {
        v.sort_unstable();
        v
    }

    #[test]
    fn closing_a_split_releases_only_that_pane() {
        let mut cm = manager();
        cm.split(7, false);
        assert_eq!(cm.current_grid_len(), 2);
        let route = cm.current().route_id;
        let kept = cm
            .current_grid()
            .contexts()
            .values()
            .find(|item| item.val.rich_text_id != 7)
            .map(|item| item.val.route_id)
            .unwrap();

        let mut images = Images::default();
        cm.remove_current_grid(&mut images);
        assert_eq!(images.cleared, vec![7]);
        assert_eq!(images.removed_routes, vec![route]);
        assert_eq!(cm.current_grid_len(), 1);
        assert_eq!(cm.current().route_id, kept);
    }

    #[test]
    fn closing_a_tab_releases_every_pane_in_it() {
        let mut cm = manager();
        cm.add_context(true, 1);
        cm.split(2, true);
        assert_eq!(cm.len(), 2);
        let routes: Vec<usize> = cm
            .current_grid()
            .contexts()
            .values()
            .map(|i| i.val.route_id)
            .collect();

        let mut images = Images::default();
        cm.close_current_context(&mut images);
        assert_eq!(sorted(images.cleared), vec![1, 2]);
        assert_eq!(sorted(images.removed_routes), sorted(routes));
        assert_eq!(cm.len(), 1);
        assert_eq!(cm.current().rich_text_id, 0);
    }

    #[test]
    fn switching_tabs_clears_overlays_of_the_tabs_not_shown() {
        let mut cm = manager();
        cm.add_context(true, 1);
        cm.add_context(true, 2);
        cm.select_tab(1);
        let mut images = Images::default();
        cm.keep_only_active_context_visible(&mut images);
        assert_eq!(sorted(images.cleared), vec![0, 2]);
        assert!(
            images.removed_routes.is_empty(),
            "a tab switch frees no images"
        );
    }

    #[test]
    fn closing_unfocused_tabs_releases_their_panes_and_keeps_the_current() {
        let mut cm = manager();
        cm.add_context(true, 1);
        cm.add_context(true, 2);
        cm.select_tab(1);
        let routes: Vec<usize> = [0, 2]
            .iter()
            .map(|&tab| cm.contexts[tab].current().route_id)
            .collect();
        let mut images = Images::default();
        cm.close_unfocused_tabs(&mut images);
        assert_eq!(sorted(images.cleared), vec![0, 2]);
        assert_eq!(sorted(images.removed_routes), sorted(routes));
        assert_eq!(cm.len(), 1);
        assert_eq!(cm.current().rich_text_id, 1);
    }

    /// Window position of the separator between the two leftmost panes
    /// of a grid, a y inside both, and their widths.
    fn separator(cm: &ContextManager<VoidListener>, tab: usize) -> (f32, f32, f32, f32) {
        let grid = &cm.contexts[tab];
        let mut rects: Vec<[f32; 4]> =
            grid.contexts().values().map(|i| i.layout_rect).collect();
        rects.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let (left, right) = (rects[0], rects[1]);
        let m = grid.scaled_margin;
        (
            m.left + ((left[0] + left[2]) + right[0]) / 2.0,
            m.top + left[1] + 1.0,
            left[2],
            right[2],
        )
    }

    fn widths(cm: &ContextManager<VoidListener>, tab: usize) -> (f32, f32) {
        let (_, _, l, r) = separator(cm, tab);
        (l, r)
    }

    const VERTICAL: Option<DragStep> = Some(DragStep {
        direction: BorderDirection::Vertical,
        visible: true,
    });

    #[test]
    fn a_divider_drag_follows_the_pointer_and_survives_a_tab_switch() {
        let mut cm = manager();
        cm.split(1, false);
        let (x, y, lw, rw) = separator(&cm, 0);

        assert_eq!(cm.drag_border_to(x, y), None, "no drag before a press");
        assert_eq!(cm.begin_border_drag(x + 300.0, y), None, "not a border");
        assert_eq!(cm.begin_border_drag(x, y), Some(BorderDirection::Vertical));

        assert_eq!(cm.drag_border_to(x + 100.0, y), VERTICAL);
        let (l, r) = widths(&cm, 0);
        assert!((l - (lw + 100.0)).abs() <= 1.0, "left {l} from {lw}");
        assert!((r - (rw - 100.0)).abs() <= 1.0, "right {r} from {rw}");

        // A tab switch mid-drag: the drag stays on the grid it started
        // on, and reports that grid is not the one on screen.
        cm.add_context(true, 9);
        assert_eq!(cm.current_index, 1);
        assert_eq!(
            cm.drag_border_to(x + 150.0, y),
            Some(DragStep {
                direction: BorderDirection::Vertical,
                visible: false,
            })
        );
        let (l, _) = widths(&cm, 0);
        assert!((l - (lw + 150.0)).abs() <= 1.0, "left {l} from {lw}");
        assert_eq!(
            cm.contexts[1].panel_count(),
            1,
            "the new tab was not touched"
        );

        assert!(cm.end_border_drag(), "a drag was in progress");
        assert!(!cm.end_border_drag(), "and now it is not");
        assert_eq!(cm.drag_border_to(x, y), None);
        let (l, _) = widths(&cm, 0);
        assert!(
            (l - (lw + 150.0)).abs() <= 1.0,
            "ending the drag keeps the size"
        );
    }

    // Tabs are renumbered when one closes. A drag pinned to tab 1
    // keeps resizing that grid after tab 0 is closed and it becomes
    // tab 0.
    #[test]
    fn a_divider_drag_survives_a_tab_closing_before_it() {
        let mut cm = manager();
        cm.add_context(true, 1);
        assert_eq!(cm.current_index, 1);
        cm.split(2, false);
        let (x, y, lw, _) = separator(&cm, 1);
        assert_eq!(cm.begin_border_drag(x, y), Some(BorderDirection::Vertical));
        assert_eq!(cm.drag_border_to(x + 50.0, y), VERTICAL);

        cm.select_tab(0);
        let mut images = Images::default();
        cm.close_current_context(&mut images);
        assert_eq!(cm.len(), 1);
        assert_eq!(
            cm.contexts[0].panel_count(),
            2,
            "the split tab is now tab 0"
        );

        assert_eq!(cm.drag_border_to(x + 120.0, y), VERTICAL);
        let (l, _) = widths(&cm, 0);
        assert!((l - (lw + 120.0)).abs() <= 1.0, "left {l} from {lw}");
        assert!(cm.end_border_drag());
    }

    // A pane closing in the dragged grid changes its tree: the drag
    // stops moving anything but stays on, swallowing the motion, until
    // the release ends it. A split that brings the pane count back does
    // not revive it: the tree is still not the one the drag knows.
    #[test]
    fn a_divider_drag_freezes_when_its_grid_changes_shape() {
        let mut cm = manager();
        cm.split(1, false);
        cm.split(2, true);
        assert_eq!(cm.current_grid_len(), 3);
        let (x, y, _, _) = separator(&cm, 0);
        let rects = |cm: &ContextManager<VoidListener>| -> Vec<[f32; 4]> {
            let mut rects: Vec<[f32; 4]> = cm
                .current_grid()
                .contexts()
                .values()
                .map(|i| i.layout_rect)
                .collect();
            rects.sort_by(|a, b| a.partial_cmp(b).unwrap());
            rects
        };
        assert_eq!(cm.begin_border_drag(x, y), Some(BorderDirection::Vertical));

        let mut images = Images::default();
        cm.remove_current_grid(&mut images);
        assert_eq!(cm.current_grid_len(), 2);
        let before = rects(&cm);
        assert_eq!(cm.drag_border_to(x + 80.0, y), VERTICAL, "still a drag");
        assert_eq!(rects(&cm), before, "nothing moved");

        cm.split(3, false);
        assert_eq!(cm.current_grid_len(), 3, "the pane count is back");
        let before = rects(&cm);
        assert_eq!(cm.drag_border_to(x + 160.0, y), VERTICAL);
        assert_eq!(
            rects(&cm),
            before,
            "the drag does not resize a rebuilt tree"
        );
        assert!(cm.end_border_drag(), "the release still ends the drag");
    }

    // Route ids are minted once, never zero, never reused: that is what
    // lets a grid be named by its first pane's route.
    #[test]
    fn route_ids_are_minted_once_and_never_zero() {
        let first = next_route_id();
        let second = next_route_id();
        assert!(first > 0);
        assert!(second > first);
        let cm = manager();
        assert!(cm.current().route_id > second);
        assert_eq!(cm.current_grid().id(), GridId(cm.current().route_id));
    }
}
