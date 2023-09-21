use crate::crosswords::pos::CursorState;
use crate::event::sync::FairMutex;
use crate::event::{EventListener, RioEvent};
use crate::performer::Machine;
use crate::router::assistant::AssistantReport::{FontsNotFound, InitializationError};
use crate::router::assistant::{AssistantReportLevel, ErrorReport};
use crate::screen::Crosswords;
use crate::screen::Messenger;
use rio_config::Shell;
use std::borrow::Cow;
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};
use sugarloaf::{font::SugarloafFont, SugarloafErrors};
use winit::window::WindowId;

#[cfg(target_os = "windows")]
use teletypewriter::create_pty;
#[cfg(not(target_os = "windows"))]
use teletypewriter::{create_pty_with_fork, create_pty_with_spawn};

const DEFAULT_CONTEXT_CAPACITY: usize = 20;

pub struct Context<T: EventListener> {
    pub terminal: Arc<FairMutex<Crosswords<T>>>,
    pub messenger: Messenger,
    #[cfg(not(target_os = "windows"))]
    pub main_fd: Arc<i32>,
    #[cfg(not(target_os = "windows"))]
    pub shell_pid: u32,
}

#[derive(Clone, Default)]
pub struct ContextManagerConfig {
    pub shell: Shell,
    pub use_fork: bool,
    pub working_dir: Option<String>,
    pub spawn_performer: bool,
    pub use_current_path: bool,
    pub is_collapsed: bool,
    pub is_native: bool,
    pub should_update_titles: bool,
}

pub struct ContextManagerTitles {
    last_title_update: Instant,
    pub titles: HashMap<usize, [String; 2]>,
    pub key: String,
}

impl ContextManagerTitles {
    pub fn new(
        idx: usize,
        program: String,
        terminal_title: String,
    ) -> ContextManagerTitles {
        let last_title_update = Instant::now();
        ContextManagerTitles {
            titles: HashMap::from([(
                idx,
                [program.to_owned(), terminal_title.to_owned()],
            )]),
            key: format!("{}{}{};", idx, program, terminal_title),
            last_title_update,
        }
    }

    pub fn set_key_val(&mut self, idx: usize, program: String, terminal_title: String) {
        self.titles.insert(idx, [program, terminal_title]);
    }

    pub fn set_key(&mut self, key: String) {
        self.key = key;
    }
}

pub struct ContextManager<T: EventListener> {
    contexts: Vec<Context<T>>,
    current_index: usize,
    capacity: usize,
    event_proxy: T,
    window_id: WindowId,
    pub config: ContextManagerConfig,
    pub titles: ContextManagerTitles,
}

impl<T: EventListener + Clone + std::marker::Send + 'static> ContextManager<T> {
    #[inline]
    pub fn create_dead_context(event_proxy: T, window_id: WindowId) -> Context<T> {
        let terminal = Crosswords::new(1, 1, event_proxy, window_id);
        let terminal: Arc<FairMutex<Crosswords<T>>> = Arc::new(FairMutex::new(terminal));
        let (sender, _receiver) = corcovado::channel::channel();

        Context {
            #[cfg(not(target_os = "windows"))]
            main_fd: Arc::new(-1),
            #[cfg(not(target_os = "windows"))]
            shell_pid: 1,
            messenger: Messenger::new(sender),
            terminal,
        }
    }

    #[inline]
    pub fn create_context(
        dimensions: (u32, u32),
        cols_rows: (usize, usize),
        cursor_state: (&CursorState, bool),
        event_proxy: T,
        window_id: WindowId,
        config: &ContextManagerConfig,
    ) -> Result<Context<T>, Box<dyn Error>> {
        let event_proxy_clone = event_proxy.clone();
        let mut terminal =
            Crosswords::new(cols_rows.0, cols_rows.1, event_proxy, window_id);
        terminal.cursor_shape = cursor_state.0.content;
        terminal.blinking_cursor = cursor_state.1;
        let terminal: Arc<FairMutex<Crosswords<T>>> = Arc::new(FairMutex::new(terminal));

        let pty;
        #[cfg(not(target_os = "windows"))]
        {
            if config.use_fork {
                log::info!("rio -> teletypewriter: create_pty_with_fork");
                pty = match create_pty_with_fork(
                    &Cow::Borrowed(&config.shell.program),
                    cols_rows.0 as u16,
                    cols_rows.1 as u16,
                ) {
                    Ok(created_pty) => created_pty,
                    Err(err) => {
                        log::error!("{err:?}");
                        return Err(Box::new(err));
                    }
                }
            } else {
                log::info!("rio -> teletypewriter: create_pty_with_spawn");
                pty = match create_pty_with_spawn(
                    &Cow::Borrowed(&config.shell.program),
                    config.shell.args.clone(),
                    &config.working_dir,
                    cols_rows.0 as u16,
                    cols_rows.1 as u16,
                ) {
                    Ok(created_pty) => created_pty,
                    Err(err) => {
                        log::error!("{err:?}");
                        return Err(Box::new(err));
                    }
                }
            };
        }

        #[cfg(target_os = "windows")]
        {
            pty = create_pty(
                &Cow::Borrowed(&config.shell.program),
                config.shell.args.clone(),
                &config.working_dir,
                cols_rows.0 as u16,
                cols_rows.1 as u16,
            );
        }

        #[cfg(not(target_os = "windows"))]
        let main_fd = pty.child.id.clone();
        #[cfg(not(target_os = "windows"))]
        let shell_pid = *pty.child.pid.clone() as u32;

        let machine =
            Machine::new(Arc::clone(&terminal), pty, event_proxy_clone, window_id)?;
        let channel = machine.channel();
        if config.spawn_performer {
            machine.spawn();
        }
        let messenger = Messenger::new(channel);

        let width = dimensions.0 as u16;
        let height = dimensions.1 as u16;
        let _ =
            messenger.send_resize(width, height, cols_rows.0 as u16, cols_rows.1 as u16);

        Ok(Context {
            #[cfg(not(target_os = "windows"))]
            main_fd,
            #[cfg(not(target_os = "windows"))]
            shell_pid,
            messenger,
            terminal,
        })
    }

    #[inline]
    pub fn start(
        dimensions: (u32, u32),
        col_rows: (usize, usize),
        cursor_state: (&CursorState, bool),
        event_proxy: T,
        window_id: WindowId,
        ctx_config: ContextManagerConfig,
        sugarloaf_errors: Option<SugarloafErrors>,
    ) -> Result<Self, Box<dyn Error>> {
        let initial_context = match ContextManager::create_context(
            (dimensions.0, dimensions.1),
            (col_rows.0, col_rows.1),
            cursor_state,
            event_proxy.clone(),
            window_id,
            &ctx_config,
        ) {
            Ok(context) => context,
            Err(err_message) => {
                log::error!("{:?}", err_message);

                event_proxy.send_event(
                    RioEvent::ReportToAssistant(ErrorReport {
                        report: InitializationError(err_message.to_string()),
                        level: AssistantReportLevel::Error,
                    }),
                    window_id,
                );

                ContextManager::create_dead_context(event_proxy.clone(), window_id)
            }
        };

        let titles =
            ContextManagerTitles::new(0, String::from("new tab"), String::from(""));

        // Sugarloaf has found errors and context need to notify it for the user
        if let Some(errors) = sugarloaf_errors {
            if !errors.fonts_not_found.is_empty() {
                event_proxy.send_event(
                    RioEvent::ReportToAssistant({
                        ErrorReport {
                            report: FontsNotFound(errors.fonts_not_found),
                            level: AssistantReportLevel::Warning,
                        }
                    }),
                    window_id,
                );
            }
        }

        Ok(ContextManager {
            current_index: 0,
            contexts: vec![initial_context],
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
            use_fork: true,
            working_dir: None,
            shell: Shell {
                program: std::env::var("SHELL").unwrap_or("bash".to_string()),
                args: vec![],
            },
            spawn_performer: false,
            is_collapsed: true,
            is_native: false,
            should_update_titles: false,
            use_current_path: false,
        };
        let initial_context = ContextManager::create_context(
            (100, 100),
            (1, 1),
            (&CursorState::default(), false),
            event_proxy.clone(),
            window_id,
            &config,
        )?;

        let titles = ContextManagerTitles::new(0, String::from(""), String::from(""));

        Ok(ContextManager {
            current_index: 0,
            contexts: vec![initial_context],
            capacity,
            event_proxy,
            window_id,
            config,
            titles,
        })
    }

    #[inline]
    pub fn schedule_cursor_blinking_render(&self) {
        self.event_proxy
            .send_event(RioEvent::PrepareRender(800), self.window_id);
    }

    #[inline]
    pub fn report_error_fonts_not_found(&self, fonts_not_found: Vec<SugarloafFont>) {
        if !fonts_not_found.is_empty() {
            self.event_proxy.send_event(
                RioEvent::ReportToAssistant({
                    ErrorReport {
                        report: FontsNotFound(fonts_not_found),
                        level: AssistantReportLevel::Warning,
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
    fn create_new_native_tab(&self) {
        self.event_proxy
            .send_event(RioEvent::CreateNativeTab, self.window_id);
    }

    #[inline]
    pub fn close_current_window(&self) {
        self.event_proxy
            .send_event(RioEvent::CloseWindow, self.window_id);
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
    pub fn minimize(&mut self) {
        self.event_proxy
            .send_event(RioEvent::Minimize(true), self.window_id);
    }

    #[inline]
    pub fn hide(&mut self) {
        self.event_proxy.send_event(RioEvent::Hide, self.window_id);
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
    pub fn switch_to_settings(&self) {
        self.event_proxy
            .send_event(RioEvent::CreateConfigEditor, self.window_id);
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.contexts.len()
    }

    #[inline]
    pub fn update_titles(&mut self) {
        if !self.config.should_update_titles {
            return;
        }

        #[cfg(not(target_os = "windows"))]
        {
            let interval_time = if self.config.is_native {
                Duration::from_secs(3)
            } else {
                Duration::from_secs(5)
            };

            if self.titles.last_title_update.elapsed() > interval_time {
                self.titles.last_title_update = Instant::now();
                let mut id = String::from("");
                for (i, context) in self.contexts.iter_mut().enumerate() {
                    let program = teletypewriter::foreground_process_name(
                        *context.main_fd,
                        context.shell_pid,
                    );

                    #[cfg(not(target_os = "macos"))]
                    let terminal_title = String::from("");

                    #[cfg(target_os = "macos")]
                    #[allow(unused)]
                    let mut terminal_title = String::from("");

                    #[cfg(target_os = "macos")]
                    {
                        let terminal = context.terminal.lock();
                        terminal_title = terminal.title.to_string();
                        drop(terminal);
                    }

                    if self.config.is_native {
                        let window_title = if terminal_title.is_empty() {
                            program.to_owned()
                        } else {
                            terminal_title.to_owned()
                        };

                        self.event_proxy
                            .send_event(RioEvent::Title(window_title), self.window_id);
                    }

                    id =
                        id.to_owned() + &(format!("{}{}{};", i, program, terminal_title));
                    self.titles.set_key_val(i, program, terminal_title);
                }
                self.titles.set_key(id);
            }
        }

        #[cfg(target_os = "windows")]
        {
            if self.titles.last_title_update.elapsed() > Duration::from_secs(5) {
                self.titles.last_title_update = Instant::now();
                let mut id = String::from("");
                for (i, context) in self.contexts.iter_mut().enumerate() {
                    let program = self.config.shell.program.to_owned();
                    let terminal_title = String::from("");

                    id =
                        id.to_owned() + &(format!("{}{}{};", i, program, terminal_title));
                    self.titles.set_key_val(i, program, terminal_title);
                }
                self.titles.set_key(id);
            }
        }
    }

    #[inline]
    pub fn contexts(&self) -> &Vec<Context<T>> {
        &self.contexts
    }

    #[cfg(test)]
    pub fn increase_capacity(&mut self, inc_val: usize) {
        self.capacity += inc_val;
    }

    #[inline]
    pub fn set_current(&mut self, context_id: usize) {
        if context_id < self.contexts.len() {
            self.current_index = context_id;
        }
    }

    #[inline]
    pub fn close_context(&mut self) {
        if self.contexts.len() <= 1 {
            self.current_index = 0;
            return;
        }

        let index_to_remove = self.current_index;
        if index_to_remove > 1 {
            self.set_current(self.current_index - 1);
        } else {
            self.set_current(0);
        }

        self.titles.titles.remove(&index_to_remove);
        self.contexts.remove(index_to_remove);
    }

    #[inline]
    pub fn kill_current_context(&mut self) {
        if self.contexts.len() <= 1 {
            self.current_index = 0;
            return;
        }

        let index_to_remove = self.current_index;

        #[cfg(not(target_os = "windows"))]
        {
            let pid = self.contexts[index_to_remove].shell_pid;
            if pid > 0 {
                teletypewriter::kill_pid(pid as i32);
            }
        }

        #[cfg(target_os = "windows")]
        {
            self.close_context();
        }
    }

    #[inline]
    pub fn current_index(&self) -> usize {
        self.current_index
    }

    #[inline]
    pub fn current(&self) -> &Context<T> {
        &self.contexts[self.current_index]
    }

    #[inline]
    pub fn current_mut(&mut self) -> &mut Context<T> {
        &mut self.contexts[self.current_index]
    }

    #[inline]
    pub fn switch_to_next(&mut self) {
        if self.config.is_native {
            self.event_proxy
                .send_event(RioEvent::SelectNativeTabNext, self.window_id);
            return;
        }

        if !self.config.is_collapsed {
            self.move_next();
        } else {
            // Collapsed tabs are rendered in backwards
            self.move_back();
        }
    }

    #[inline]
    fn move_back(&mut self) {
        if self.contexts.len() - 1 == self.current_index {
            self.current_index = 0;
        } else {
            self.current_index += 1;
        }
    }

    #[inline]
    fn move_next(&mut self) {
        if self.current_index == 0 {
            self.current_index = self.contexts.len() - 1;
        } else {
            self.current_index -= 1;
        }
    }

    #[inline]
    pub fn switch_to_prev(&mut self) {
        if self.config.is_native {
            self.event_proxy
                .send_event(RioEvent::SelectNativeTabPrev, self.window_id);
            return;
        }

        if !self.config.is_collapsed {
            self.move_back();
        } else {
            // Collapsed tabs are rendered in backwards
            self.move_next();
        }
    }

    #[inline]
    pub fn add_context(
        &mut self,
        redirect: bool,
        dimensions: (u32, u32),
        col_rows: (usize, usize),
        cursor_state: (&CursorState, bool),
    ) {
        // Native tabs do not use Context tabbing API, instead it will
        // ask winit to create a window with a tab id
        if self.config.is_native {
            self.create_new_native_tab();
            return;
        }

        let size = self.contexts.len();
        if size < self.capacity {
            let last_index = self.contexts.len();

            #[cfg(target_os = "windows")]
            let cloned_config = &self.config;
            #[cfg(not(target_os = "windows"))]
            let mut cloned_config = self.config.clone();

            #[cfg(not(target_os = "windows"))]
            {
                if cloned_config.use_current_path && cloned_config.working_dir.is_none() {
                    let current_context = self.current();
                    if let Ok(path) = teletypewriter::foreground_process_path(
                        *current_context.main_fd,
                        current_context.shell_pid,
                    ) {
                        cloned_config.working_dir =
                            Some(path.to_string_lossy().to_string());
                    }
                }
            }

            match ContextManager::create_context(
                dimensions,
                col_rows,
                cursor_state,
                self.event_proxy.clone(),
                self.window_id,
                &cloned_config,
            ) {
                Ok(new_context) => {
                    self.contexts.push(new_context);
                    if redirect {
                        self.current_index = last_index;
                    }
                }
                Err(..) => {
                    log::error!("not able to create a new context");
                }
            }
        }
    }
}

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::event::VoidListener;

    #[test]
    fn test_capacity() {
        let context_manager =
            ContextManager::start_with_capacity(5, VoidListener {}, WindowId::from(0))
                .unwrap();
        assert_eq!(context_manager.capacity, 5);

        let mut context_manager =
            ContextManager::start_with_capacity(5, VoidListener {}, WindowId::from(0))
                .unwrap();
        context_manager.increase_capacity(3);
        assert_eq!(context_manager.capacity, 8);
    }

    #[test]
    fn test_add_context() {
        let mut context_manager =
            ContextManager::start_with_capacity(5, VoidListener {}, WindowId::from(0))
                .unwrap();
        assert_eq!(context_manager.capacity, 5);
        assert_eq!(context_manager.current_index, 0);

        let should_redirect = false;
        context_manager.add_context(
            should_redirect,
            (100, 100),
            (1, 1),
            (&CursorState::default(), false),
        );
        assert_eq!(context_manager.capacity, 5);
        assert_eq!(context_manager.current_index, 0);

        let should_redirect = true;
        context_manager.add_context(
            should_redirect,
            (100, 100),
            (1, 1),
            (&CursorState::default(), false),
        );
        assert_eq!(context_manager.capacity, 5);
        assert_eq!(context_manager.current_index, 2);
    }

    #[test]
    fn test_add_context_start_with_capacity_limit() {
        let mut context_manager =
            ContextManager::start_with_capacity(3, VoidListener {}, WindowId::from(0))
                .unwrap();
        assert_eq!(context_manager.capacity, 3);
        assert_eq!(context_manager.current_index, 0);
        let should_redirect = false;
        context_manager.add_context(
            should_redirect,
            (100, 100),
            (1, 1),
            (&CursorState::default(), false),
        );
        assert_eq!(context_manager.len(), 2);
        context_manager.add_context(
            should_redirect,
            (100, 100),
            (1, 1),
            (&CursorState::default(), false),
        );
        assert_eq!(context_manager.len(), 3);

        for _ in 0..20 {
            context_manager.add_context(
                should_redirect,
                (100, 100),
                (1, 1),
                (&CursorState::default(), false),
            );
        }

        assert_eq!(context_manager.len(), 3);
        assert_eq!(context_manager.capacity, 3);
    }

    #[test]
    fn test_set_current() {
        let mut context_manager =
            ContextManager::start_with_capacity(8, VoidListener {}, WindowId::from(0))
                .unwrap();
        let should_redirect = true;

        context_manager.add_context(
            should_redirect,
            (100, 100),
            (1, 1),
            (&CursorState::default(), false),
        );
        assert_eq!(context_manager.current_index, 1);
        context_manager.set_current(0);
        assert_eq!(context_manager.current_index, 0);
        assert_eq!(context_manager.len(), 2);
        assert_eq!(context_manager.capacity, 8);

        let should_redirect = false;
        context_manager.add_context(
            should_redirect,
            (100, 100),
            (1, 1),
            (&CursorState::default(), false),
        );
        context_manager.add_context(
            should_redirect,
            (100, 100),
            (1, 1),
            (&CursorState::default(), false),
        );
        context_manager.set_current(3);
        assert_eq!(context_manager.current_index, 3);

        context_manager.set_current(8);
        assert_eq!(context_manager.current_index, 3);
    }

    #[test]
    fn test_close_context() {
        let mut context_manager =
            ContextManager::start_with_capacity(3, VoidListener {}, WindowId::from(0))
                .unwrap();
        let should_redirect = false;

        context_manager.add_context(
            should_redirect,
            (100, 100),
            (1, 1),
            (&CursorState::default(), false),
        );
        context_manager.add_context(
            should_redirect,
            (100, 100),
            (1, 1),
            (&CursorState::default(), false),
        );
        assert_eq!(context_manager.len(), 3);

        assert_eq!(context_manager.current_index, 0);
        context_manager.set_current(2);
        assert_eq!(context_manager.current_index, 2);
        context_manager.set_current(0);

        context_manager.close_context();
        context_manager.set_current(2);
        assert_eq!(context_manager.current_index, 0);
        assert_eq!(context_manager.len(), 2);
    }

    #[test]
    fn test_close_context_upcoming_ids() {
        let mut context_manager =
            ContextManager::start_with_capacity(5, VoidListener {}, WindowId::from(0))
                .unwrap();
        let should_redirect = false;

        context_manager.add_context(
            should_redirect,
            (100, 100),
            (1, 1),
            (&CursorState::default(), false),
        );
        context_manager.add_context(
            should_redirect,
            (100, 100),
            (1, 1),
            (&CursorState::default(), false),
        );
        context_manager.add_context(
            should_redirect,
            (100, 100),
            (1, 1),
            (&CursorState::default(), false),
        );
        context_manager.add_context(
            should_redirect,
            (100, 100),
            (1, 1),
            (&CursorState::default(), false),
        );

        context_manager.close_context();
        context_manager.close_context();
        context_manager.close_context();
        context_manager.close_context();

        assert_eq!(context_manager.len(), 1);
        assert_eq!(context_manager.current_index, 0);

        context_manager.add_context(
            should_redirect,
            (100, 100),
            (1, 1),
            (&CursorState::default(), false),
        );

        assert_eq!(context_manager.len(), 2);
        context_manager.set_current(1);
        assert_eq!(context_manager.current_index, 1);
        context_manager.close_context();
        assert_eq!(context_manager.len(), 1);
        assert_eq!(context_manager.current_index, 0);
    }

    #[test]
    fn test_close_last_context() {
        let mut context_manager =
            ContextManager::start_with_capacity(2, VoidListener {}, WindowId::from(0))
                .unwrap();
        let should_redirect = false;

        context_manager.add_context(
            should_redirect,
            (100, 100),
            (1, 1),
            (&CursorState::default(), false),
        );
        context_manager.add_context(
            should_redirect,
            (100, 100),
            (1, 1),
            (&CursorState::default(), false),
        );
        assert_eq!(context_manager.len(), 2);
        assert_eq!(context_manager.current_index, 0);

        context_manager.close_context();
        assert_eq!(context_manager.len(), 1);

        // Last context should not be closed
        context_manager.close_context();
        assert_eq!(context_manager.len(), 1);
    }

    #[test]
    fn test_switch_to_next() {
        let mut context_manager =
            ContextManager::start_with_capacity(5, VoidListener {}, WindowId::from(0))
                .unwrap();
        let should_redirect = false;

        context_manager.add_context(
            should_redirect,
            (100, 100),
            (1, 1),
            (&CursorState::default(), false),
        );
        context_manager.add_context(
            should_redirect,
            (100, 100),
            (1, 1),
            (&CursorState::default(), false),
        );
        context_manager.add_context(
            should_redirect,
            (100, 100),
            (1, 1),
            (&CursorState::default(), false),
        );
        context_manager.add_context(
            should_redirect,
            (100, 100),
            (1, 1),
            (&CursorState::default(), false),
        );
        context_manager.add_context(
            should_redirect,
            (100, 100),
            (1, 1),
            (&CursorState::default(), false),
        );
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
