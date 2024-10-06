pub mod grid;
pub mod renderable;

use crate::ansi::CursorShape;
use crate::context::grid::ContextDimension;
use crate::context::grid::ContextGrid;
use crate::context::grid::Delta;
use crate::crosswords::pos::CursorState;
use crate::event::sync::FairMutex;
use crate::event::RioEvent;
use crate::messenger::Messenger;
use crate::performer::Machine;
use renderable::RenderableContent;
use rio_backend::config::Shell;
use rio_backend::crosswords::{Crosswords, MIN_COLUMNS, MIN_LINES};
use rio_backend::error::{RioError, RioErrorLevel, RioErrorType};
use rio_backend::event::EventListener;
use rio_backend::event::WindowId;
use rio_backend::sugarloaf::{font::SugarloafFont, Object, SugarloafErrors};
use std::borrow::Cow;
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[cfg(target_os = "windows")]
use teletypewriter::create_pty;
#[cfg(not(target_os = "windows"))]
use teletypewriter::{create_pty_with_fork, create_pty_with_spawn};

const DEFAULT_CONTEXT_CAPACITY: usize = 28;

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
}

impl<T: rio_backend::event::EventListener> Drop for Context<T> {
    fn drop(&mut self) {
        #[cfg(not(target_os = "windows"))]
        teletypewriter::kill_pid(self.shell_pid as i32);
    }
}

#[derive(Clone, Default)]
pub struct ContextManagerConfig {
    pub shell: Shell,
    #[cfg(not(target_os = "windows"))]
    pub use_fork: bool,
    pub working_dir: Option<String>,
    pub spawn_performer: bool,
    pub use_current_path: bool,
    pub is_native: bool,
    pub should_update_titles: bool,
}

pub struct ContextManagerTitles {
    last_title_update: Instant,
    pub titles: HashMap<usize, [String; 3]>,
    pub key: String,
}

impl ContextManagerTitles {
    pub fn new(
        idx: usize,
        program: String,
        terminal_title: String,
        path: String,
    ) -> ContextManagerTitles {
        let last_title_update = Instant::now();
        ContextManagerTitles {
            key: format!("{}{}{};", idx, program, terminal_title),
            titles: HashMap::from([(idx, [program, terminal_title, path])]),
            last_title_update,
        }
    }

    #[inline]
    pub fn set_key_val(
        &mut self,
        idx: usize,
        program: String,
        terminal_title: String,
        path: String,
    ) {
        self.titles.insert(idx, [program, terminal_title, path]);
    }

    #[inline]
    pub fn set_key(&mut self, key: String) {
        self.key = key;
    }
}

pub struct ContextManager<T: EventListener> {
    contexts: Vec<ContextGrid<T>>,
    current_index: usize,
    current_route: usize,
    acc_current_route: usize,
    #[allow(unused)]
    capacity: usize,
    event_proxy: T,
    window_id: WindowId,
    pub config: ContextManagerConfig,
    pub titles: ContextManagerTitles,
}

pub fn create_mock_context<T: rio_backend::event::EventListener>(
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
        renderable_content: RenderableContent::default(),
        terminal,
        rich_text_id,
        dimension,
    }
}

impl<T: EventListener + Clone + std::marker::Send + 'static> ContextManager<T> {
    #[inline]
    fn create_context(
        cursor_state: (&CursorState, bool),
        event_proxy: T,
        window_id: WindowId,
        route_id: usize,
        rich_text_id: usize,
        dimension: ContextDimension,
        config: &ContextManagerConfig,
    ) -> Result<Context<T>, Box<dyn Error>> {
        let cols: u16 = dimension.columns.try_into().unwrap_or(MIN_COLUMNS as u16);
        let rows: u16 = dimension.lines.try_into().unwrap_or(MIN_LINES as u16);

        let mut terminal = Crosswords::new(
            dimension,
            cursor_state.0.content,
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
        if config.spawn_performer {
            machine.spawn();
        }

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
            renderable_content: RenderableContent::default(),
            dimension,
        })
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        cursor_state: (&CursorState, bool),
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
            route_id,
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

                create_mock_context(
                    event_proxy.clone(),
                    window_id,
                    route_id,
                    0,
                    ContextDimension::default(),
                )
            }
        };

        let titles = ContextManagerTitles::new(
            0,
            String::from("tab"),
            String::new(),
            ctx_config.working_dir.clone().unwrap_or_default(),
        );

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
            acc_current_route: 0,
            contexts: vec![ContextGrid::new(initial_context, margin)],
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
            should_update_titles: false,
            use_current_path: false,
        };
        let initial_context = ContextManager::create_context(
            (&CursorState::new('_'), false),
            event_proxy.clone(),
            window_id,
            0,
            0,
            ContextDimension::default(),
            &config,
        )?;

        let titles =
            ContextManagerTitles::new(0, String::new(), String::new(), String::new());

        Ok(ContextManager {
            current_index: 0,
            current_route: 0,
            acc_current_route: 0,
            contexts: vec![ContextGrid::new(initial_context, Delta::<f32>::default())],
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
    pub fn schedule_render(&mut self, scheduled_time: u64) {
        self.event_proxy
            .send_event(RioEvent::PrepareRender(scheduled_time), self.window_id);
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
    pub fn grid_objects(&self) -> Vec<Object> {
        self.contexts[self.current_index].objects()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.contexts.len()
    }

    pub fn update_titles(&mut self) {
        if !self.config.should_update_titles {
            return;
        }

        #[cfg(unix)]
        {
            let interval_time = Duration::from_secs(2);
            if self.titles.last_title_update.elapsed() > interval_time {
                self.titles.last_title_update = Instant::now();
                let mut id = String::default();
                for (i, context) in self.contexts.iter_mut().enumerate() {
                    let program = teletypewriter::foreground_process_name(
                        *context.current().main_fd,
                        context.current().shell_pid,
                    );

                    let path = teletypewriter::foreground_process_path(
                        *context.current().main_fd,
                        context.current().shell_pid,
                    )
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();

                    let terminal_title = {
                        let terminal = context.current().terminal.lock();
                        terminal.title.to_string()
                    };

                    if self.config.is_native {
                        let window_title = if terminal_title.is_empty() {
                            program.to_owned()
                        } else {
                            format!("{} ({})", terminal_title, program)
                        };

                        if cfg!(target_os = "macos") {
                            self.event_proxy.send_event(
                                RioEvent::TitleWithSubtitle(window_title, path.clone()),
                                self.window_id,
                            );
                        } else {
                            self.event_proxy.send_event(
                                RioEvent::Title(window_title),
                                self.window_id,
                            );
                        }
                    }

                    id =
                        id.to_owned() + &(format!("{}{}{};", i, program, terminal_title));
                    self.titles.set_key_val(i, program, terminal_title, path);
                }
                self.titles.set_key(id);
            }
        }

        #[cfg(not(unix))]
        {
            if self.titles.last_title_update.elapsed() > Duration::from_secs(2) {
                self.titles.last_title_update = Instant::now();
                let mut id = String::from("");
                for (i, _context) in self.contexts.iter().enumerate() {
                    let program = self.config.shell.program.to_owned();
                    id = id.to_owned()
                        + &(format!("{}{}{};", i, program, String::default()));
                    self.titles.set_key_val(
                        i,
                        program,
                        String::default(),
                        String::default(),
                    );
                }
                self.titles.set_key(id);
            }
        }
    }

    #[inline]
    pub fn contexts(&self) -> &Vec<ContextGrid<T>> {
        &self.contexts
    }

    #[inline]
    pub fn current_grid_mut(&mut self) -> &mut ContextGrid<T> {
        &mut self.contexts[self.current_index]
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
    pub fn renderable_content(&mut self) -> &RenderableContent {
        let current = self.current_mut();
        let terminal = current.terminal.lock();
        current.renderable_content.update(
            terminal.visible_rows(),
            terminal.display_offset(),
            terminal.cursor(),
            terminal.blinking_cursor,
        );
        drop(terminal);

        &current.renderable_content
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
        &self.contexts[self.current_index].current()
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
    pub fn add_context(&mut self, redirect: bool, cursor_state: (&CursorState, bool)) {
        let mut working_dir = None;
        if self.config.use_current_path && self.config.working_dir.is_none() {
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

            self.acc_current_route += 1;
            match ContextManager::create_context(
                cursor_state,
                self.event_proxy.clone(),
                self.window_id,
                0,
                self.acc_current_route,
                self.current().dimension,
                &cloned_config,
            ) {
                Ok(new_context) => {
                    let previous_margin = self.contexts[self.current_index].margin;
                    self.contexts
                        .push(ContextGrid::new(new_context, previous_margin));
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
        context_manager.add_context(should_redirect, (&CursorState::new('_'), false));
        assert_eq!(context_manager.capacity, 5);
        assert_eq!(context_manager.current_index, 0);

        let should_redirect = true;
        context_manager.add_context(should_redirect, (&CursorState::new('_'), false));
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
        context_manager.add_context(should_redirect, (&CursorState::new('_'), false));
        assert_eq!(context_manager.len(), 2);
        context_manager.add_context(should_redirect, (&CursorState::new('_'), false));
        assert_eq!(context_manager.len(), 3);

        for _ in 0..20 {
            context_manager.add_context(should_redirect, (&CursorState::new('_'), false));
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

        context_manager.add_context(should_redirect, (&CursorState::new('_'), false));
        assert_eq!(context_manager.current_index, 1);
        context_manager.set_current(0);
        assert_eq!(context_manager.current_index, 0);
        assert_eq!(context_manager.len(), 2);
        assert_eq!(context_manager.capacity, 8);

        let should_redirect = false;
        context_manager.add_context(should_redirect, (&CursorState::new('_'), false));
        context_manager.add_context(should_redirect, (&CursorState::new('_'), false));
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

        context_manager.add_context(should_redirect, (&CursorState::new('_'), false));
        context_manager.add_context(should_redirect, (&CursorState::new('_'), false));
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

        context_manager.add_context(should_redirect, (&CursorState::new('_'), false));
        context_manager.add_context(should_redirect, (&CursorState::new('_'), false));
        context_manager.add_context(should_redirect, (&CursorState::new('_'), false));
        context_manager.add_context(should_redirect, (&CursorState::new('_'), false));

        context_manager.close_current_context();
        context_manager.close_current_context();
        context_manager.close_current_context();
        context_manager.close_current_context();

        assert_eq!(context_manager.len(), 1);
        assert_eq!(context_manager.current_index, 0);

        context_manager.add_context(should_redirect, (&CursorState::new('_'), false));

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

        context_manager.add_context(should_redirect, (&CursorState::new('_'), false));
        context_manager.add_context(should_redirect, (&CursorState::new('_'), false));
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

        context_manager.add_context(should_redirect, (&CursorState::new('_'), false));
        context_manager.add_context(should_redirect, (&CursorState::new('_'), false));
        context_manager.add_context(should_redirect, (&CursorState::new('_'), false));
        context_manager.add_context(should_redirect, (&CursorState::new('_'), false));
        context_manager.add_context(should_redirect, (&CursorState::new('_'), false));
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
}
