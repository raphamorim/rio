use crate::ansi::CursorShape;
use crate::crosswords::pos::CursorState;
use crate::event::sync::FairMutex;
use crate::event::RioEvent;
use crate::messenger::Messenger;
use crate::performer::Machine;
use rio_backend::config::Shell;
use rio_backend::crosswords::CrosswordsSize;
use rio_backend::crosswords::{Crosswords, MIN_COLUMNS, MIN_LINES};
use rio_backend::error::{RioError, RioErrorLevel, RioErrorType};
use rio_backend::event::EventListener;
use rio_backend::event::WindowId;
use rio_backend::sugarloaf::layout::SugarloafLayout;
use rio_backend::sugarloaf::{font::SugarloafFont, SugarloafErrors};
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
    contexts: Vec<Context<T>>,
    current_index: usize,
    #[allow(unused)]
    capacity: usize,
    event_proxy: T,
    window_id: WindowId,
    pub config: ContextManagerConfig,
    pub titles: ContextManagerTitles,
}

impl<T: EventListener + Clone + std::marker::Send + 'static> ContextManager<T> {
    #[inline]
    pub fn create_dead_context(event_proxy: T, window_id: WindowId) -> Context<T> {
        let size = CrosswordsSize::new(MIN_COLUMNS, MIN_LINES);
        let terminal = Crosswords::new(size, CursorShape::Block, event_proxy, window_id);
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
        cursor_state: (&CursorState, bool),
        event_proxy: T,
        window_id: WindowId,
        size: SugarloafLayout,
        config: &ContextManagerConfig,
    ) -> Result<Context<T>, Box<dyn Error>> {
        #[cfg(target_os = "windows")]
        let width = size.width;

        #[cfg(target_os = "windows")]
        let height = size.height;

        let cols: u16 = size.columns.try_into().unwrap_or(MIN_COLUMNS as u16);
        let rows: u16 = size.lines.try_into().unwrap_or(MIN_LINES as u16);

        let mut terminal =
            Crosswords::new(size, cursor_state.0.content, event_proxy.clone(), window_id);
        terminal.blinking_cursor = cursor_state.1;
        let terminal: Arc<FairMutex<Crosswords<T>>> = Arc::new(FairMutex::new(terminal));

        let pty;
        #[cfg(not(target_os = "windows"))]
        {
            if config.use_fork {
                log::info!("rio -> teletypewriter: create_pty_with_fork");
                pty = match create_pty_with_fork(
                    &Cow::Borrowed(&config.shell.program),
                    cols,
                    rows,
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
                    cols,
                    rows,
                ) {
                    Ok(created_pty) => created_pty,
                    Err(err) => {
                        log::error!("{err:?}");
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
            pty = create_pty(
                &Cow::Borrowed(&config.shell.program),
                config.shell.args.clone(),
                &config.working_dir,
                2,
                1,
            );
        }

        let machine =
            Machine::new(Arc::clone(&terminal), pty, event_proxy.clone(), window_id)?;
        let channel = machine.channel();
        if config.spawn_performer {
            machine.spawn();
        }

        let messenger = Messenger::new(channel);

        #[cfg(target_os = "windows")]
        {
            if let Err(resize_error) =
                messenger.send_resize(width as u16, height as u16, cols, rows)
            {
                log::error!("{resize_error:?}");
            }
        };

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
        cursor_state: (&CursorState, bool),
        event_proxy: T,
        window_id: WindowId,
        ctx_config: ContextManagerConfig,
        size: SugarloafLayout,
        sugarloaf_errors: Option<SugarloafErrors>,
    ) -> Result<Self, Box<dyn Error>> {
        let initial_context = match ContextManager::create_context(
            cursor_state,
            event_proxy.clone(),
            window_id,
            size,
            &ctx_config,
        ) {
            Ok(context) => context,
            Err(err_message) => {
                log::error!("{:?}", err_message);

                event_proxy.send_event(
                    RioEvent::ReportToAssistant(RioError {
                        report: RioErrorType::InitializationError(
                            err_message.to_string(),
                        ),
                        level: RioErrorLevel::Error,
                    }),
                    window_id,
                );

                ContextManager::create_dead_context(event_proxy.clone(), window_id)
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
            (&CursorState::new('_'), false),
            event_proxy.clone(),
            window_id,
            SugarloafLayout::default(),
            &config,
        )?;

        let titles =
            ContextManagerTitles::new(0, String::new(), String::new(), String::new());

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
    #[allow(unused)]
    pub fn schedule_render(&mut self, scheduled_time: u64) {
        self.event_proxy
            .send_event(RioEvent::PrepareRender(scheduled_time), self.window_id);
    }

    #[inline]
    #[allow(unused)]
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
    pub fn close_current_window(&mut self, is_last_tab: bool) {
        if self.config.is_native {
            self.event_proxy
                .send_event(RioEvent::CloseWindow, self.window_id);
        } else if !is_last_tab {
            self.close_context();
        }
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
    #[allow(unused)]
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
            let interval_time = Duration::from_secs(3);
            if self.titles.last_title_update.elapsed() > interval_time {
                self.titles.last_title_update = Instant::now();
                let mut id = String::from("");
                for (i, context) in self.contexts.iter_mut().enumerate() {
                    let program = teletypewriter::foreground_process_name(
                        *context.main_fd,
                        context.shell_pid,
                    );

                    #[cfg(any(use_wa, target_os = "macos"))]
                    let path = teletypewriter::foreground_process_path(
                        *context.main_fd,
                        context.shell_pid,
                    )
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();

                    #[cfg(not(any(use_wa, target_os = "macos")))]
                    let path = String::default();

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
                            format!("{} ({})", terminal_title, program)
                        };

                        #[cfg(any(use_wa, target_os = "macos"))]
                        self.event_proxy.send_event(
                            RioEvent::TitleWithSubtitle(window_title, path.clone()),
                            self.window_id,
                        );

                        #[cfg(not(any(use_wa, target_os = "macos")))]
                        self.event_proxy
                            .send_event(RioEvent::Title(window_title), self.window_id);
                    }

                    id =
                        id.to_owned() + &(format!("{}{}{};", i, program, terminal_title));
                    self.titles.set_key_val(i, program, terminal_title, path);
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
                    let empty_string = String::from("");

                    id = id.to_owned() + &(format!("{}{}{};", i, program, empty_string));
                    self.titles.set_key_val(
                        i,
                        program,
                        empty_string.clone(),
                        empty_string,
                    );
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
            // In MacOS: Close last tab will work, leading to hide and
            // keep Rio running in background if allow_close_last_tab is true
            #[cfg(target_os = "macos")]
            {
                self.close_current_window(true);
            }

            return;
        }

        let index_to_remove = self.current_index;

        #[cfg(not(target_os = "windows"))]
        {
            // The reason why we don't use close context here is because it is unix is handled by
            // by Rio event lifecycle as well, so calling close_context on unix could trigger
            // two close tabs events since we listen for SIGHUP in teletypewriter to close a tab as well
            let pid = self.contexts[index_to_remove].shell_pid;
            if pid > 0 {
                self.titles.titles.remove(&index_to_remove);
                teletypewriter::kill_pid(pid as i32);
            }
        }

        #[cfg(target_os = "windows")]
        self.close_context();

        #[cfg(use_wa)]
        self.close_current_window(false);
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

        if self.contexts.len() - 1 == self.current_index {
            self.current_index = 0;
        } else {
            self.current_index += 1;
        }
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
    }

    #[inline]
    pub fn add_context(
        &mut self,
        redirect: bool,
        layout: SugarloafLayout,
        cursor_state: (&CursorState, bool),
    ) {
        let mut working_dir = None;
        if self.config.use_current_path && self.config.working_dir.is_none() {
            let current_context = self.current();

            #[cfg(not(target_os = "windows"))]
            {
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

            match ContextManager::create_context(
                cursor_state,
                self.event_proxy.clone(),
                self.window_id,
                layout,
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

pub fn process_open_url(
    mut shell: Shell,
    mut working_dir: Option<String>,
    editor: String,
    open_url: Option<&str>,
) -> (Shell, Option<String>) {
    if open_url.is_none() {
        return (shell, working_dir);
    }

    if let Ok(url) = url::Url::parse(open_url.unwrap_or_default()) {
        if let Ok(path_buf) = url.to_file_path() {
            if path_buf.exists() {
                if path_buf.is_file() {
                    shell = Shell {
                        program: editor,
                        args: vec![path_buf.display().to_string()],
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
        #[cfg(use_wa)]
        let window_id: WindowId = 0;

        #[cfg(not(use_wa))]
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
        #[cfg(use_wa)]
        let window_id: WindowId = 0;

        #[cfg(not(use_wa))]
        let window_id: WindowId = WindowId::from(0);

        let mut context_manager =
            ContextManager::start_with_capacity(5, VoidListener {}, window_id).unwrap();
        assert_eq!(context_manager.capacity, 5);
        assert_eq!(context_manager.current_index, 0);

        let should_redirect = false;
        context_manager.add_context(
            should_redirect,
            SugarloafLayout::default(),
            (&CursorState::new('_'), false),
        );
        assert_eq!(context_manager.capacity, 5);
        assert_eq!(context_manager.current_index, 0);

        let should_redirect = true;
        context_manager.add_context(
            should_redirect,
            SugarloafLayout::default(),
            (&CursorState::new('_'), false),
        );
        assert_eq!(context_manager.capacity, 5);
        assert_eq!(context_manager.current_index, 2);
    }

    #[test]
    fn test_add_context_start_with_capacity_limit() {
        #[cfg(use_wa)]
        let window_id: WindowId = 0;

        #[cfg(not(use_wa))]
        let window_id: WindowId = WindowId::from(0);

        let mut context_manager =
            ContextManager::start_with_capacity(3, VoidListener {}, window_id).unwrap();
        assert_eq!(context_manager.capacity, 3);
        assert_eq!(context_manager.current_index, 0);
        let should_redirect = false;
        context_manager.add_context(
            should_redirect,
            SugarloafLayout::default(),
            (&CursorState::new('_'), false),
        );
        assert_eq!(context_manager.len(), 2);
        context_manager.add_context(
            should_redirect,
            SugarloafLayout::default(),
            (&CursorState::new('_'), false),
        );
        assert_eq!(context_manager.len(), 3);

        for _ in 0..20 {
            context_manager.add_context(
                should_redirect,
                SugarloafLayout::default(),
                (&CursorState::new('_'), false),
            );
        }

        assert_eq!(context_manager.len(), 3);
        assert_eq!(context_manager.capacity, 3);
    }

    #[test]
    fn test_set_current() {
        #[cfg(use_wa)]
        let window_id: WindowId = 0;

        #[cfg(not(use_wa))]
        let window_id: WindowId = WindowId::from(0);

        let mut context_manager =
            ContextManager::start_with_capacity(8, VoidListener {}, window_id).unwrap();
        let should_redirect = true;

        context_manager.add_context(
            should_redirect,
            SugarloafLayout::default(),
            (&CursorState::new('_'), false),
        );
        assert_eq!(context_manager.current_index, 1);
        context_manager.set_current(0);
        assert_eq!(context_manager.current_index, 0);
        assert_eq!(context_manager.len(), 2);
        assert_eq!(context_manager.capacity, 8);

        let should_redirect = false;
        context_manager.add_context(
            should_redirect,
            SugarloafLayout::default(),
            (&CursorState::new('_'), false),
        );
        context_manager.add_context(
            should_redirect,
            SugarloafLayout::default(),
            (&CursorState::new('_'), false),
        );
        context_manager.set_current(3);
        assert_eq!(context_manager.current_index, 3);

        context_manager.set_current(8);
        assert_eq!(context_manager.current_index, 3);
    }

    #[test]
    fn test_close_context() {
        #[cfg(use_wa)]
        let window_id: WindowId = 0;

        #[cfg(not(use_wa))]
        let window_id: WindowId = WindowId::from(0);

        let mut context_manager =
            ContextManager::start_with_capacity(3, VoidListener {}, window_id).unwrap();
        let should_redirect = false;

        context_manager.add_context(
            should_redirect,
            SugarloafLayout::default(),
            (&CursorState::new('_'), false),
        );
        context_manager.add_context(
            should_redirect,
            SugarloafLayout::default(),
            (&CursorState::new('_'), false),
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
        #[cfg(use_wa)]
        let window_id: WindowId = 0;

        #[cfg(not(use_wa))]
        let window_id: WindowId = WindowId::from(0);

        let mut context_manager =
            ContextManager::start_with_capacity(5, VoidListener {}, window_id).unwrap();
        let should_redirect = false;

        context_manager.add_context(
            should_redirect,
            SugarloafLayout::default(),
            (&CursorState::new('_'), false),
        );
        context_manager.add_context(
            should_redirect,
            SugarloafLayout::default(),
            (&CursorState::new('_'), false),
        );
        context_manager.add_context(
            should_redirect,
            SugarloafLayout::default(),
            (&CursorState::new('_'), false),
        );
        context_manager.add_context(
            should_redirect,
            SugarloafLayout::default(),
            (&CursorState::new('_'), false),
        );

        context_manager.close_context();
        context_manager.close_context();
        context_manager.close_context();
        context_manager.close_context();

        assert_eq!(context_manager.len(), 1);
        assert_eq!(context_manager.current_index, 0);

        context_manager.add_context(
            should_redirect,
            SugarloafLayout::default(),
            (&CursorState::new('_'), false),
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
        #[cfg(use_wa)]
        let window_id: WindowId = 0;

        #[cfg(not(use_wa))]
        let window_id: WindowId = WindowId::from(0);

        let mut context_manager =
            ContextManager::start_with_capacity(2, VoidListener {}, window_id).unwrap();
        let should_redirect = false;

        context_manager.add_context(
            should_redirect,
            SugarloafLayout::default(),
            (&CursorState::new('_'), false),
        );
        context_manager.add_context(
            should_redirect,
            SugarloafLayout::default(),
            (&CursorState::new('_'), false),
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
        #[cfg(use_wa)]
        let window_id: WindowId = 0;

        #[cfg(not(use_wa))]
        let window_id: WindowId = WindowId::from(0);

        let mut context_manager =
            ContextManager::start_with_capacity(5, VoidListener {}, window_id).unwrap();
        let should_redirect = false;

        context_manager.add_context(
            should_redirect,
            SugarloafLayout::default(),
            (&CursorState::new('_'), false),
        );
        context_manager.add_context(
            should_redirect,
            SugarloafLayout::default(),
            (&CursorState::new('_'), false),
        );
        context_manager.add_context(
            should_redirect,
            SugarloafLayout::default(),
            (&CursorState::new('_'), false),
        );
        context_manager.add_context(
            should_redirect,
            SugarloafLayout::default(),
            (&CursorState::new('_'), false),
        );
        context_manager.add_context(
            should_redirect,
            SugarloafLayout::default(),
            (&CursorState::new('_'), false),
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
