pub mod grid;
pub mod renderable;
pub mod title;

use crate::ansi::CursorShape;
use crate::context::grid::{ContextDimension, ContextGrid, ContextGridItem, Delta};
use crate::context::title::{
    create_title_extra_from_context, update_title, ContextManagerTitles,
};
use crate::event::sync::FairMutex;
use crate::event::{Msg, RioEvent};
use crate::ime::Ime;
use crate::messenger::Messenger;
use crate::performer::{self, Machine};
use renderable::Cursor;
use renderable::RenderableContent;
use rio_backend::config::Shell;
use smallvec::{smallvec, SmallVec};

use rio_backend::crosswords::{Crosswords, MIN_COLUMNS, MIN_LINES};
use rio_backend::error::{RioError, RioErrorLevel, RioErrorType};
use rio_backend::event::EventListener;
use rio_backend::event::WindowId;
use rio_backend::selection::SelectionRange;
use rio_backend::sugarloaf::{font::SugarloafFont, Object, SugarloafErrors};
use std::borrow::Cow;
use std::error::Error;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

// Global atomic counter for generating unique route IDs
static ROUTE_ID_COUNTER: AtomicUsize = AtomicUsize::new(1);

#[cfg(target_os = "windows")]
use teletypewriter::create_pty;
#[cfg(not(target_os = "windows"))]
use teletypewriter::{create_pty_with_fork, create_pty_with_spawn};

pub struct Context<T: EventListener> {
    pub route_id: usize,
    pub terminal: Arc<FairMutex<Crosswords<T>>>,
    pub renderable_content: RenderableContent,
    pub messenger: Messenger,
    #[cfg(not(target_os = "windows"))]
    pub main_fd: Arc<i32>,
    #[cfg(not(target_os = "windows"))]
    pub shell_pid: u32,
    pub rich_text_id: usize,
    pub dimension: ContextDimension,
    pub ime: Ime,
    _io_thread: Option<JoinHandle<(Machine<teletypewriter::Pty, T>, performer::State)>>,
}

impl<T: rio_backend::event::EventListener> Drop for Context<T> {
    fn drop(&mut self) {
        // Shutdown the terminal's PTY.
        let _ = self.messenger.channel.send(Msg::Shutdown);

        #[cfg(not(target_os = "windows"))]
        teletypewriter::kill_pid(self.shell_pid as i32);
    }
}

impl<T: EventListener> Context<T> {
    #[inline]
    pub fn set_selection(&mut self, selection_range: Option<SelectionRange>) {
        let old_selection = self.renderable_content.selection_range;
        let has_updated = old_selection != selection_range;

        if has_updated {
            // Calculate partial damage for selection changes
            let damage = calculate_selection_damage(old_selection, selection_range);
            self.renderable_content.pending_update.set_ui_damage(damage);
        }

        self.renderable_content.selection_range = selection_range;
    }

    #[inline]
    pub fn set_hyperlink_range(&mut self, hyperlink_range: Option<SelectionRange>) {
        let old_hyperlink = self.renderable_content.hyperlink_range;

        if old_hyperlink != hyperlink_range {
            // For hyperlinks, use full damage as they're less frequent
            self.renderable_content
                .pending_update
                .set_ui_damage(rio_backend::event::TerminalDamage::Full);
        }

        self.renderable_content.hyperlink_range = hyperlink_range;
    }

    #[inline]
    pub fn has_hyperlink_range(&self) -> bool {
        self.renderable_content.hyperlink_range.is_some()
    }

    #[inline]
    pub fn cursor_from_ref(&self) -> Cursor {
        Cursor {
            state: self.renderable_content.cursor.state.new_from_self(),
            content: self.renderable_content.cursor.content_ref,
            content_ref: self.renderable_content.cursor.content_ref,
            is_ime_enabled: false,
        }
    }
}

#[derive(Clone, Default)]
pub struct ContextManagerConfig {
    pub shell: Shell,
    #[cfg(not(target_os = "windows"))]
    pub use_fork: bool,
    pub working_dir: Option<String>,
    pub spawn_performer: bool,
    pub cwd: bool,
    pub is_native: bool,
    pub should_update_title_extra: bool,
    pub split_color: [f32; 4],
    pub title: rio_backend::config::title::Title,
    pub keyboard: rio_backend::config::keyboard::Keyboard,
}

const DEFAULT_CONTEXT_CAPACITY: usize = 28;

pub struct ContextManager<T: EventListener> {
    contexts: SmallVec<[ContextGrid<T>; DEFAULT_CONTEXT_CAPACITY]>,
    current_index: usize,
    current_route: usize,
    #[allow(unused)]
    capacity: usize,
    event_proxy: T,
    window_id: WindowId,
    pub config: ContextManagerConfig,
    pub titles: ContextManagerTitles,
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
    );
    let terminal: Arc<FairMutex<Crosswords<T>>> = Arc::new(FairMutex::new(terminal));
    let (sender, _receiver) = corcovado::channel::channel();

    Context {
        route_id,
        #[cfg(not(target_os = "windows"))]
        main_fd: Arc::new(-1),
        #[cfg(not(target_os = "windows"))]
        shell_pid: 1,
        messenger: Messenger::new(sender),
        renderable_content: RenderableContent::new(Cursor::default()),
        terminal,
        rich_text_id,
        dimension,
        ime: Ime::new(),
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
        #[cfg(not(target_os = "windows"))]
        use_fork: true,
        working_dir: None,
        shell: Shell {
            program: std::env::var("SHELL").unwrap_or("bash".to_string()),
            args: vec![],
        },
        spawn_performer: false,
        is_native: false,
        should_update_title_extra: false,
        cwd: false,
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
        let route_id = ROUTE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        let cols: u16 = dimension.columns.try_into().unwrap_or(MIN_COLUMNS as u16);
        let rows: u16 = dimension.lines.try_into().unwrap_or(MIN_LINES as u16);

        let mut terminal = Crosswords::new(
            dimension,
            CursorShape::from_char(cursor_state.0.content),
            event_proxy.clone(),
            window_id,
            route_id,
        );
        terminal.blinking_cursor = cursor_state.1;
        let terminal: Arc<FairMutex<Crosswords<T>>> = Arc::new(FairMutex::new(terminal));

        let pty;
        #[cfg(not(target_os = "windows"))]
        {
            if config.use_fork {
                tracing::info!("rio -> teletypewriter: create_pty_with_fork");
                pty = match create_pty_with_fork(
                    &Cow::Borrowed(&config.shell.program),
                    cols,
                    rows,
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
                    &Cow::Borrowed(&config.shell.program),
                    config.shell.args.clone(),
                    &config.working_dir,
                    cols,
                    rows,
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

        #[cfg(target_os = "windows")]
        {
            pty = match create_pty(
                &Cow::Borrowed(&config.shell.program),
                config.shell.args.clone(),
                &config.working_dir,
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

        Ok(Context {
            route_id,
            #[cfg(not(target_os = "windows"))]
            main_fd,
            #[cfg(not(target_os = "windows"))]
            shell_pid,
            messenger,
            terminal,
            rich_text_id,
            renderable_content: RenderableContent::new(cursor_state.0.clone()),
            dimension,
            ime: Ime::new(),
            _io_thread: io_thread,
        })
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        cursor_state: (&Cursor, bool),
        event_proxy: T,
        window_id: WindowId,
        route_id: usize,
        rich_text_id: usize,
        ctx_config: ContextManagerConfig,
        size: ContextDimension,
        margin: Delta<f32>,
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

                create_dead_context(
                    event_proxy.clone(),
                    window_id,
                    route_id,
                    0,
                    ContextDimension::default(),
                )
            }
        };

        let titles = ContextManagerTitles::new(0, String::from("tab"), None);

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

        Ok(ContextManager {
            current_index: 0,
            current_route: 0,
            contexts: smallvec![ContextGrid::new(
                initial_context,
                margin,
                ctx_config.split_color,
            )],
            capacity: DEFAULT_CONTEXT_CAPACITY,
            event_proxy,
            window_id,
            config: ctx_config,
            titles,
        })
    }

    #[cfg(test)]
    pub fn start_with_capacity(
        capacity: usize,
        event_proxy: T,
        window_id: WindowId,
    ) -> Result<Self, Box<dyn Error>> {
        let config = ContextManagerConfig {
            #[cfg(not(target_os = "windows"))]
            use_fork: true,
            working_dir: None,
            shell: Shell {
                program: std::env::var("SHELL").unwrap_or("bash".to_string()),
                args: vec![],
            },
            spawn_performer: false,
            is_native: false,
            should_update_title_extra: false,
            cwd: false,
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

        let titles = ContextManagerTitles::new(0, String::new(), None);

        Ok(ContextManager {
            current_index: 0,
            current_route: 0,
            contexts: smallvec![ContextGrid::new(
                initial_context,
                Delta::<f32>::default(),
                config.split_color,
            )],
            capacity,
            event_proxy,
            window_id,
            config,
            titles,
        })
    }

    #[inline]
    pub fn should_close_context_manager(&mut self, route_id: usize) -> bool {
        let requires_change_route = self.current_route == route_id;

        // should_close_context_manager is only called when terminal.exit()
        // is triggered. The terminal.exit() happens for any drop on context
        // by tab removal or if the Pty is exited (e.g: exit/control+d)
        //
        // In the tab case we already have removed the context with the
        // specified route_id so isn't gonna find anything. Then will be false.
        //
        // However if the tab is killed by Pty and not a tab action then
        // it means we need to clean the context with the specified route_id.
        // If there's no context then should return true and kill the window.
        if !self.contexts.is_empty() {
            // In case Grid has more than one item
            if self.current_grid().len() > 1 {
                if self.current().route_id == route_id {
                    self.remove_current_grid();
                }

                return false;
            }

            // In case Grid has only one item
            if let Some(index_to_remove) = self
                .contexts
                .iter()
                .position(|ctx| ctx.current().route_id == route_id)
            {
                let mut should_set_current = false;
                if requires_change_route {
                    if index_to_remove > 1 {
                        self.set_current(index_to_remove - 1);
                    } else {
                        should_set_current = true;
                    }
                }
                self.contexts.remove(index_to_remove);
                self.titles.titles.remove(&index_to_remove);

                if should_set_current {
                    self.set_current(0);
                }
            };
        }

        self.contexts.is_empty()
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
    pub fn close_unfocused_tabs(&mut self) {
        let current_route_id = self.current().route_id;
        self.titles.titles.retain(|&i, _| i == self.current_index);
        self.contexts
            .retain(|ctx| ctx.current().route_id == current_route_id);
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
        self.current_route = self.current().route_id;
    }

    #[inline]
    pub fn select_prev_split(&mut self) {
        self.contexts[self.current_index].select_prev_split();
        self.current_route = self.current().route_id;
    }

    #[inline]
    pub fn switch_to_next_split_or_tab(&mut self) {
        if self.contexts[self.current_index].select_next_split_no_loop() {
            self.current_route = self.current().route_id;
            return;
        }
        self.switch_to_next();
        // Make sure first split is selected - get the root key
        let current_tab = &mut self.contexts[self.current_index];
        if let Some(root) = current_tab.root {
            current_tab.current = root;
        }
        self.current_route = self.current().route_id;
    }

    #[inline]
    pub fn switch_to_prev_split_or_tab(&mut self) {
        if self.contexts[self.current_index].select_prev_split_no_loop() {
            self.current_route = self.current().route_id;
            return;
        }
        self.switch_to_prev();
        // Make sure last split is selected - get the last key in order
        let current_tab = &mut self.contexts[self.current_index];
        let ordered_keys = current_tab.get_ordered_keys();
        if let Some(&last_key) = ordered_keys.last() {
            current_tab.current = last_key;
        }
        self.current_route = self.current().route_id;
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
        self.current_route = self.current().route_id;
    }

    #[inline]
    pub fn extend_with_grid_objects(&self, target: &mut Vec<Object>) {
        self.contexts[self.current_index].extend_with_objects(target);
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.contexts.len()
    }

    pub fn update_titles(&mut self) {
        let interval_time = Duration::from_secs(2);
        if self
            .titles
            .last_title_update
            .map(|i| i.elapsed() > interval_time)
            .unwrap_or(true)
        {
            self.titles.last_title_update = Some(Instant::now());
            let mut id = String::default();
            for (i, context) in self.contexts.iter_mut().enumerate() {
                let content = update_title(&self.config.title.content, context.current());

                self.event_proxy
                    .send_event(RioEvent::Title(content.to_owned()), self.window_id);

                id.push_str(&format!("{i}{content};"));

                if self.config.should_update_title_extra {
                    self.titles.set_key_val(
                        i,
                        content,
                        create_title_extra_from_context(context.current()),
                    );
                } else {
                    self.titles.set_key_val(i, content, None);
                }
            }

            self.titles.set_key(id);
        }
    }

    #[inline]
    pub fn get_mut(&mut self, route_id: usize) -> Option<&mut ContextGridItem<T>> {
        self.contexts[self.current_index].get_mut(route_id)
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
    pub fn remove_current_grid(&mut self) {
        self.contexts[self.current_index].remove_current();
        self.current_route = self.contexts[self.current_index].current().route_id;
    }

    #[inline]
    pub fn current_grid_mut(&mut self) -> &mut ContextGrid<T> {
        &mut self.contexts[self.current_index]
    }

    #[inline]
    pub fn current_grid(&self) -> &ContextGrid<T> {
        &self.contexts[self.current_index]
    }

    #[cfg(test)]
    pub fn increase_capacity(&mut self, inc_val: usize) {
        self.capacity += inc_val;
    }

    #[inline]
    pub fn set_current(&mut self, context_id: usize) {
        if context_id < self.contexts.len() {
            self.current_index = context_id;
            self.current_route = self.current().route_id;
        }
    }

    #[inline]
    pub fn close_current_context(&mut self) {
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
        let mut should_set_current = false;
        if index_to_remove > 1 {
            self.set_current(self.current_index - 1);
        } else {
            should_set_current = true;
        }

        self.titles.titles.remove(&index_to_remove);
        self.contexts.remove(index_to_remove);

        if should_set_current {
            self.set_current(0);
        }
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

    #[inline]
    pub fn switch_to_next(&mut self) {
        if self.config.is_native {
            self.event_proxy
                .send_event(RioEvent::SelectNativeTabNext, self.window_id);
            return;
        }

        if self.contexts.len() - 1 == self.current_index {
            self.current_index = 0;
        } else {
            self.current_index += 1;
        }

        self.current_route = self.current().route_id;
    }

    #[inline]
    pub fn switch_to_prev(&mut self) {
        if self.config.is_native {
            self.event_proxy
                .send_event(RioEvent::SelectNativeTabPrev, self.window_id);
            return;
        }

        if self.current_index == 0 {
            self.current_index = self.contexts.len() - 1;
        } else {
            self.current_index -= 1;
        }

        self.current_route = self.current().route_id;
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
                let new_route_id = new_context.route_id;
                if split_down {
                    self.contexts[self.current_index].split_down(new_context);
                } else {
                    self.contexts[self.current_index].split_right(new_context);
                }

                self.current_route = new_route_id;
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
            cwd: config.navigation.current_working_directory,
            shell,
            working_dir,
            spawn_performer: true,
            #[cfg(not(target_os = "windows"))]
            use_fork: config.use_fork,
            is_native: config.navigation.is_native(),
            // When navigation is collapsed and does not contain any color rule
            // does not make sense fetch for foreground process names
            should_update_title_extra: !config.navigation.color_automation.is_empty(),
            split_color: config.colors.split,
            title: config.title,
            keyboard: config.keyboard,
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
                let new_route_id = new_context.route_id;
                if split_down {
                    self.contexts[self.current_index].split_down(new_context);
                } else {
                    self.contexts[self.current_index].split_right(new_context);
                }

                self.current_route = new_route_id;
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
                    let previous_margin = self.contexts[self.current_index].margin;
                    self.contexts.push(ContextGrid::new(
                        new_context,
                        previous_margin,
                        self.config.split_color,
                    ));
                    if redirect {
                        self.current_index = last_index;
                        self.current_route = self.current().route_id;
                    }
                }
                Err(..) => {
                    tracing::error!("not able to create a new context");
                }
            }
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

// Standalone function for calculating selection damage
#[inline]
fn calculate_selection_damage(
    old: Option<SelectionRange>,
    new: Option<SelectionRange>,
) -> rio_backend::event::TerminalDamage {
    use rio_backend::crosswords::LineDamage;
    use std::collections::BTreeSet;

    match (old, new) {
        // No selection -> No selection: no damage
        (None, None) => rio_backend::event::TerminalDamage::CursorOnly,

        // Selection cleared or created: need full damage
        (None, Some(_)) | (Some(_), None) => rio_backend::event::TerminalDamage::Full,

        // Selection changed: calculate partial damage
        (Some(old_range), Some(new_range)) => {
            let mut damaged_lines = BTreeSet::new();

            // If block selection mode changed, use full damage
            if old_range.is_block != new_range.is_block {
                return rio_backend::event::TerminalDamage::Full;
            }

            // Calculate the range of lines that need updating
            let min_start_row = old_range.start.row.min(new_range.start.row);
            let max_start_row = old_range.start.row.max(new_range.start.row);
            let min_end_row = old_range.end.row.min(new_range.end.row);
            let max_end_row = old_range.end.row.max(new_range.end.row);

            // Start row changes
            if old_range.start.row != new_range.start.row {
                for row in min_start_row.0..=max_start_row.0 {
                    damaged_lines.insert(LineDamage::new(row as usize, true));
                }
            }

            // End row changes
            if old_range.end.row != new_range.end.row {
                for row in min_end_row.0..=max_end_row.0 {
                    damaged_lines.insert(LineDamage::new(row as usize, true));
                }
            }

            // If only columns changed on the same rows
            if old_range.start.row == new_range.start.row
                && old_range.start.col != new_range.start.col
            {
                damaged_lines
                    .insert(LineDamage::new(old_range.start.row.0 as usize, true));
            }

            if old_range.end.row == new_range.end.row
                && old_range.end.col != new_range.end.col
            {
                damaged_lines.insert(LineDamage::new(old_range.end.row.0 as usize, true));
            }

            rio_backend::event::TerminalDamage::Partial(damaged_lines)
        }
    }
}

#[cfg(test)]
mod selection_damage_tests {
    use super::*;
    use rio_backend::crosswords::pos::{Column, Line, Pos};
    use rio_backend::selection::SelectionRange;

    #[test]
    fn test_no_selection_to_no_selection() {
        let damage = calculate_selection_damage(None, None);
        assert!(matches!(
            damage,
            rio_backend::event::TerminalDamage::CursorOnly
        ));
    }

    #[test]
    fn test_new_selection_created() {
        let new_selection = Some(SelectionRange::new(
            Pos::new(Line(0), Column(0)),
            Pos::new(Line(2), Column(10)),
            false,
        ));
        let damage = calculate_selection_damage(None, new_selection);
        assert!(matches!(damage, rio_backend::event::TerminalDamage::Full));
    }

    #[test]
    fn test_selection_cleared() {
        let old_selection = Some(SelectionRange::new(
            Pos::new(Line(0), Column(0)),
            Pos::new(Line(2), Column(10)),
            false,
        ));
        let damage = calculate_selection_damage(old_selection, None);
        assert!(matches!(damage, rio_backend::event::TerminalDamage::Full));
    }

    #[test]
    fn test_block_mode_change() {
        let old_selection = Some(SelectionRange::new(
            Pos::new(Line(0), Column(0)),
            Pos::new(Line(2), Column(10)),
            false, // regular selection
        ));
        let new_selection = Some(SelectionRange::new(
            Pos::new(Line(0), Column(0)),
            Pos::new(Line(2), Column(10)),
            true, // block selection
        ));
        let damage = calculate_selection_damage(old_selection, new_selection);
        assert!(matches!(damage, rio_backend::event::TerminalDamage::Full));
    }

    #[test]
    fn test_selection_extend_down() {
        let old_selection = Some(SelectionRange::new(
            Pos::new(Line(0), Column(0)),
            Pos::new(Line(2), Column(10)),
            false,
        ));
        let new_selection = Some(SelectionRange::new(
            Pos::new(Line(0), Column(0)),
            Pos::new(Line(3), Column(10)), // Extended down by one line
            false,
        ));
        let damage = calculate_selection_damage(old_selection, new_selection);

        if let rio_backend::event::TerminalDamage::Partial(lines) = damage {
            // Should only damage lines 2 and 3 (the changed end rows)
            assert_eq!(lines.len(), 2);
            let damaged: Vec<_> = lines.iter().map(|l| l.line).collect();
            assert!(damaged.contains(&2));
            assert!(damaged.contains(&3));
        } else {
            panic!("Expected partial damage");
        }
    }

    #[test]
    fn test_selection_shrink_up() {
        let old_selection = Some(SelectionRange::new(
            Pos::new(Line(0), Column(0)),
            Pos::new(Line(3), Column(10)),
            false,
        ));
        let new_selection = Some(SelectionRange::new(
            Pos::new(Line(1), Column(0)), // Start moved down
            Pos::new(Line(3), Column(10)),
            false,
        ));
        let damage = calculate_selection_damage(old_selection, new_selection);

        if let rio_backend::event::TerminalDamage::Partial(lines) = damage {
            // Should damage lines 0 and 1 (the changed start rows)
            assert_eq!(lines.len(), 2);
            let damaged: Vec<_> = lines.iter().map(|l| l.line).collect();
            assert!(damaged.contains(&0));
            assert!(damaged.contains(&1));
        } else {
            panic!("Expected partial damage");
        }
    }

    #[test]
    fn test_column_change_same_line() {
        let old_selection = Some(SelectionRange::new(
            Pos::new(Line(1), Column(5)),
            Pos::new(Line(1), Column(10)),
            false,
        ));
        let new_selection = Some(SelectionRange::new(
            Pos::new(Line(1), Column(5)),
            Pos::new(Line(1), Column(15)), // Extended right on same line
            false,
        ));
        let damage = calculate_selection_damage(old_selection, new_selection);

        if let rio_backend::event::TerminalDamage::Partial(lines) = damage {
            // Should only damage line 1
            assert_eq!(lines.len(), 1);
            assert_eq!(lines.iter().next().unwrap().line, 1);
        } else {
            panic!("Expected partial damage");
        }
    }

    #[test]
    fn test_both_ends_change() {
        let old_selection = Some(SelectionRange::new(
            Pos::new(Line(2), Column(5)),
            Pos::new(Line(4), Column(10)),
            false,
        ));
        let new_selection = Some(SelectionRange::new(
            Pos::new(Line(1), Column(0)),  // Start moved up
            Pos::new(Line(5), Column(15)), // End moved down
            false,
        ));
        let damage = calculate_selection_damage(old_selection, new_selection);

        if let rio_backend::event::TerminalDamage::Partial(lines) = damage {
            // Should damage lines 1-2 (start change) and 4-5 (end change)
            assert_eq!(lines.len(), 4);
            let damaged: Vec<_> = lines.iter().map(|l| l.line).collect();
            assert!(damaged.contains(&1));
            assert!(damaged.contains(&2));
            assert!(damaged.contains(&4));
            assert!(damaged.contains(&5));
        } else {
            panic!("Expected partial damage");
        }
    }
}

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::event::VoidListener;

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
    fn test_close_context() {
        let window_id: WindowId = WindowId::from(0);

        let mut context_manager =
            ContextManager::start_with_capacity(3, VoidListener {}, window_id).unwrap();
        let should_redirect = false;

        context_manager.add_context(should_redirect, 0);
        context_manager.add_context(should_redirect, 0);
        assert_eq!(context_manager.len(), 3);

        assert_eq!(context_manager.current_index, 0);
        context_manager.set_current(2);
        assert_eq!(context_manager.current_index, 2);
        context_manager.set_current(0);

        context_manager.close_current_context();
        context_manager.set_current(2);
        assert_eq!(context_manager.current_index, 0);
        assert_eq!(context_manager.len(), 2);
    }

    #[test]
    fn test_close_context_upcoming_ids() {
        let window_id: WindowId = WindowId::from(0);

        let mut context_manager =
            ContextManager::start_with_capacity(5, VoidListener {}, window_id).unwrap();
        let should_redirect = false;

        context_manager.add_context(should_redirect, 0);
        context_manager.add_context(should_redirect, 0);
        context_manager.add_context(should_redirect, 0);
        context_manager.add_context(should_redirect, 0);

        context_manager.close_current_context();
        context_manager.close_current_context();
        context_manager.close_current_context();
        context_manager.close_current_context();

        assert_eq!(context_manager.len(), 1);
        assert_eq!(context_manager.current_index, 0);

        context_manager.add_context(should_redirect, 0);

        assert_eq!(context_manager.len(), 2);
        context_manager.set_current(1);
        assert_eq!(context_manager.current_index, 1);
        context_manager.close_current_context();
        assert_eq!(context_manager.len(), 1);
        assert_eq!(context_manager.current_index, 0);
    }

    #[test]
    fn test_close_last_context() {
        let window_id: WindowId = WindowId::from(0);

        let mut context_manager =
            ContextManager::start_with_capacity(2, VoidListener {}, window_id).unwrap();
        let should_redirect = false;

        context_manager.add_context(should_redirect, 0);
        context_manager.add_context(should_redirect, 0);
        assert_eq!(context_manager.len(), 2);
        assert_eq!(context_manager.current_index, 0);

        context_manager.close_current_context();
        assert_eq!(context_manager.len(), 1);

        // Last context should not be closed
        context_manager.close_current_context();
        assert_eq!(context_manager.len(), 1);
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
    fn test_switch_to_next_split_or_tab() {
        let window_id: WindowId = WindowId::from(0);

        let mut context_manager =
            ContextManager::start_with_capacity(5, VoidListener {}, window_id).unwrap();
        let should_redirect = true;
        let split_down = false;

        context_manager.add_context(should_redirect, 0);
        context_manager.split(0, split_down);
        context_manager.split(0, split_down);
        context_manager.add_context(should_redirect, 0);
        context_manager.add_context(should_redirect, 0);
        context_manager.split(0, split_down);
        context_manager.add_context(should_redirect, 0);
        context_manager.set_current(0);
        assert_eq!(context_manager.len(), 5);
        assert_eq!(context_manager.current_index, 0);

        let mut current_index;

        context_manager.switch_to_next_split_or_tab();
        current_index = context_manager.current_index;
        assert_eq!(current_index, 1);
        assert_eq!(context_manager.contexts[current_index].current_index(), 0);

        context_manager.switch_to_next_split_or_tab();
        current_index = context_manager.current_index;
        assert_eq!(current_index, 1);
        assert_eq!(context_manager.contexts[current_index].current_index(), 1);

        context_manager.switch_to_next_split_or_tab();
        current_index = context_manager.current_index;
        assert_eq!(current_index, 1);
        assert_eq!(context_manager.contexts[current_index].current_index(), 2);

        context_manager.switch_to_next_split_or_tab();
        current_index = context_manager.current_index;
        assert_eq!(current_index, 2);
        assert_eq!(context_manager.contexts[current_index].current_index(), 0);

        context_manager.switch_to_next_split_or_tab();
        current_index = context_manager.current_index;
        assert_eq!(current_index, 3);
        assert_eq!(context_manager.contexts[current_index].current_index(), 0);

        context_manager.switch_to_next_split_or_tab();
        current_index = context_manager.current_index;
        assert_eq!(current_index, 3);
        assert_eq!(context_manager.contexts[current_index].current_index(), 1);

        context_manager.switch_to_next_split_or_tab();
        current_index = context_manager.current_index;
        assert_eq!(current_index, 4);
        assert_eq!(context_manager.contexts[current_index].current_index(), 0);

        context_manager.switch_to_next_split_or_tab();
        current_index = context_manager.current_index;
        assert_eq!(current_index, 0);
        assert_eq!(context_manager.contexts[current_index].current_index(), 0);
    }

    #[test]
    fn test_switch_to_prev_split_or_tab() {
        let window_id: WindowId = WindowId::from(0);

        let mut context_manager =
            ContextManager::start_with_capacity(5, VoidListener {}, window_id).unwrap();
        let should_redirect = true;
        let split_down = false;

        context_manager.add_context(should_redirect, 0);
        context_manager.split(0, split_down);
        context_manager.split(0, split_down);
        context_manager.add_context(should_redirect, 0);
        context_manager.add_context(should_redirect, 0);
        context_manager.split(0, split_down);
        context_manager.add_context(should_redirect, 0);
        context_manager.set_current(0);
        assert_eq!(context_manager.len(), 5);
        assert_eq!(context_manager.current_index, 0);

        let mut current_index;

        context_manager.switch_to_prev_split_or_tab();
        current_index = context_manager.current_index;
        assert_eq!(current_index, 4);
        assert_eq!(context_manager.contexts[current_index].current_index(), 0);

        context_manager.switch_to_prev_split_or_tab();
        current_index = context_manager.current_index;
        assert_eq!(current_index, 3);
        assert_eq!(context_manager.contexts[current_index].current_index(), 1);

        context_manager.switch_to_prev_split_or_tab();
        current_index = context_manager.current_index;
        assert_eq!(current_index, 3);
        assert_eq!(context_manager.contexts[current_index].current_index(), 0);

        context_manager.switch_to_prev_split_or_tab();
        current_index = context_manager.current_index;
        assert_eq!(current_index, 2);
        assert_eq!(context_manager.contexts[current_index].current_index(), 0);

        context_manager.switch_to_prev_split_or_tab();
        current_index = context_manager.current_index;
        assert_eq!(current_index, 1);
        assert_eq!(context_manager.contexts[current_index].current_index(), 2);

        context_manager.switch_to_prev_split_or_tab();
        current_index = context_manager.current_index;
        assert_eq!(current_index, 1);
        assert_eq!(context_manager.contexts[current_index].current_index(), 1);

        context_manager.switch_to_prev_split_or_tab();
        current_index = context_manager.current_index;
        assert_eq!(current_index, 1);
        assert_eq!(context_manager.contexts[current_index].current_index(), 0);

        context_manager.switch_to_prev_split_or_tab();
        current_index = context_manager.current_index;
        assert_eq!(current_index, 0);
        assert_eq!(context_manager.contexts[current_index].current_index(), 0);
    }

    #[test]
    fn test_switch_to_next_and_prev_split_or_tab() {
        let window_id: WindowId = WindowId::from(0);

        let mut context_manager =
            ContextManager::start_with_capacity(5, VoidListener {}, window_id).unwrap();
        let should_redirect = true;
        let split_down = false;

        context_manager.add_context(should_redirect, 0);
        context_manager.split(0, split_down);
        context_manager.split(0, split_down);
        context_manager.add_context(should_redirect, 0);
        context_manager.set_current(0);
        assert_eq!(context_manager.len(), 3);
        assert_eq!(context_manager.current_index, 0);

        let mut current_index;

        // Next
        context_manager.switch_to_next_split_or_tab();
        current_index = context_manager.current_index;
        assert_eq!(current_index, 1);
        assert_eq!(context_manager.contexts[current_index].current_index(), 0);

        context_manager.switch_to_next_split_or_tab();
        current_index = context_manager.current_index;
        assert_eq!(current_index, 1);
        assert_eq!(context_manager.contexts[current_index].current_index(), 1);

        context_manager.switch_to_next_split_or_tab();
        current_index = context_manager.current_index;
        assert_eq!(current_index, 1);
        assert_eq!(context_manager.contexts[current_index].current_index(), 2);

        context_manager.switch_to_next_split_or_tab();
        current_index = context_manager.current_index;
        assert_eq!(current_index, 2);
        assert_eq!(context_manager.contexts[current_index].current_index(), 0);

        // Prev
        context_manager.switch_to_prev_split_or_tab();
        current_index = context_manager.current_index;
        assert_eq!(current_index, 1);
        assert_eq!(context_manager.contexts[current_index].current_index(), 2);

        context_manager.switch_to_prev_split_or_tab();
        current_index = context_manager.current_index;
        assert_eq!(current_index, 1);
        assert_eq!(context_manager.contexts[current_index].current_index(), 1);

        context_manager.switch_to_prev_split_or_tab();
        current_index = context_manager.current_index;
        assert_eq!(current_index, 1);
        assert_eq!(context_manager.contexts[current_index].current_index(), 0);

        context_manager.switch_to_prev_split_or_tab();
        current_index = context_manager.current_index;
        assert_eq!(current_index, 0);
        assert_eq!(context_manager.contexts[current_index].current_index(), 0);
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
}
