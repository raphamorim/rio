mod menu;
mod route;

use crate::event::RioEvent;
use crate::ime::Preedit;
use crate::routes::RoutePath;
use crate::scheduler::{Scheduler, TimerId, Topic};
use rio_backend::error::RioError;
use rio_backend::event::{EventPayload, EventProxy, RioEventType};
use rio_backend::sugarloaf::font::FontLibrary;
use route::Route;
use std::collections::HashMap;
use std::error::Error;
use std::rc::Rc;
use std::time::Duration;
use wa::event_loop::{EventLoop, EventLoopProxy};
use wa::*;

pub struct Router {
    route: Route,
    id: u16,
    #[cfg(target_os = "macos")]
    tab_group: Option<u64>,
}

pub fn create_window(
    event_loop_proxy: EventLoopProxy<EventPayload>,
    config: &Rc<rio_backend::config::Config>,
    config_error: &Option<rio_backend::config::ConfigError>,
    font_library: FontLibrary,
    tab_group: Option<u64>,
    open_file_url: Option<&str>,
) -> Result<Router, Box<dyn std::error::Error>> {
    let hide_toolbar_buttons = config.window.decorations
        == rio_backend::config::window::Decorations::Buttonless
        || config.window.decorations
            == rio_backend::config::window::Decorations::Disabled;

    #[cfg(target_os = "macos")]
    let tab_identifier = if tab_group.is_some() {
        Some(format!("tab-group-{}", tab_group.unwrap()))
    } else {
        None
    };

    let wa_conf = conf::Conf {
        window_title: String::from("~"),
        window_width: config.window.width,
        window_height: config.window.height,
        fullscreen: config.window.is_fullscreen(),
        transparency: config.window.background_opacity < 1.,
        blur: config.window.blur,
        hide_toolbar: !config.navigation.is_native(),
        hide_toolbar_buttons,
        #[cfg(target_os = "macos")]
        tab_identifier,
        ..Default::default()
    };

    let event_proxy = EventProxy::new(event_loop_proxy);
    let (created_window, create_window_dimensions) =
        futures::executor::block_on(Window::new(wa_conf))?;

    if config_error.is_some() {
        event_proxy.send_event(
            RioEventType::Rio(RioEvent::ReportToAssistant(
                RioError::configuration_not_found(),
            )),
            created_window.id,
        );
    }

    let route = Route::new(
        created_window.id,
        event_proxy,
        created_window.raw_window_handle,
        created_window.raw_display_handle,
        config.clone(),
        font_library.clone(),
        (
            create_window_dimensions.0,
            create_window_dimensions.1,
            create_window_dimensions.2,
        ),
        open_file_url,
    )?;

    Ok(Router {
        route,
        tab_group,
        id: created_window.id,
    })
}

struct EventHandlerInstance {
    config: Rc<rio_backend::config::Config>,
    font_library: FontLibrary,
    scheduler: Scheduler,
    routes: HashMap<u16, Router>,
    event_loop: EventLoop<EventPayload>,
    event_loop_proxy: EventLoopProxy<EventPayload>,
    modifiers: ModifiersState,
    #[cfg(target_os = "macos")]
    last_tab_group: Option<u64>,
}

impl EventHandlerInstance {
    fn new(config: rio_backend::config::Config) -> Self {
        let font_library = FontLibrary::new(config.fonts.to_owned());
        // let mut sugarloaf_errors = None;

        // let (font_library, fonts_not_found) = loader;

        // if !fonts_not_found.is_empty() {
        //     sugarloaf_errors = Some(SugarloafErrors { fonts_not_found });
        // }

        let config = Rc::new(config);
        let event_loop = EventLoop::<rio_backend::event::EventPayload>::build()
            .expect("expected event loop to be created");
        let event_loop_proxy = event_loop.create_proxy();
        let scheduler = Scheduler::new(event_loop_proxy.clone());
        Self {
            routes: HashMap::default(),
            event_loop,
            event_loop_proxy,
            scheduler,
            font_library,
            config,
            modifiers: ModifiersState::empty(),
            #[cfg(target_os = "macos")]
            last_tab_group: None,
        }
    }
}

impl EventHandler for EventHandlerInstance {
    fn create_window(&mut self) {
        if let Ok(router) = create_window(
            self.event_loop_proxy.clone(),
            &self.config,
            &None,
            self.font_library.clone(),
            None,
            None,
        ) {
            self.modifiers = ModifiersState::empty();
            self.routes.insert(router.id, router);
        }
    }

    fn create_tab(&mut self, open_file_url: Option<&str>) {
        if let Ok(router) = create_window(
            self.event_loop_proxy.clone(),
            &self.config,
            &None,
            self.font_library.clone(),
            self.last_tab_group,
            open_file_url,
        ) {
            let id = router.id;
            self.modifiers = ModifiersState::empty();
            self.routes.insert(id, router);
            // if let Some(file_url) = open_file_url {
            //     wa::window::open_url(id, file_url);
            // }
        }
    }

    fn process(&mut self) -> EventHandlerControl {
        for event in self.event_loop.receiver.try_iter() {
            let window_id = event.window_id;
            match event.payload {
                RioEventType::Rio(RioEvent::CloseWindow) => {
                    // TODO
                }
                RioEventType::Rio(RioEvent::Wakeup) => {
                    if let Some(current) = self.routes.get_mut(&window_id) {
                        current.route.sugarloaf.mark_dirty();
                        current.route.render();
                    }
                }
                RioEventType::Rio(RioEvent::Render) => {
                    if let Some(current) = self.routes.get_mut(&window_id) {
                        current.route.render();
                    }
                }
                RioEventType::Rio(RioEvent::CreateWindow) => {
                    if let Some(current) = self.routes.get_mut(&window_id) {
                        let new_tab_group = if self.config.navigation.is_native() {
                            current
                                .tab_group
                                .map(|current_tab_group| current_tab_group + 1)
                        } else {
                            None
                        };

                        if let Ok(router) = create_window(
                            self.event_loop_proxy.clone(),
                            &self.config,
                            &None,
                            self.font_library.clone(),
                            new_tab_group,
                            None,
                        ) {
                            self.modifiers = ModifiersState::empty();
                            self.routes.insert(router.id, router);
                        }
                    }
                }
                #[cfg(target_os = "macos")]
                RioEventType::Rio(RioEvent::CreateNativeTab(_)) => {
                    if let Some(current) = self.routes.get_mut(&window_id) {
                        if let Ok(router) = create_window(
                            self.event_loop_proxy.clone(),
                            &self.config,
                            &None,
                            self.font_library.clone(),
                            current.tab_group,
                            None,
                        ) {
                            self.routes.insert(router.id, router);
                        }
                    }
                }
                RioEventType::Rio(RioEvent::UpdateConfig) => {
                    let (config, config_error) =
                        match rio_backend::config::Config::try_load() {
                            Ok(config) => (config, None),
                            Err(error) => {
                                (rio_backend::config::Config::default(), Some(error))
                            }
                        };

                    self.config = config.into();
                    let appearance = wa::window::get_appearance();

                    if let Some(current) = self.routes.get_mut(&window_id) {
                        if let Some(error) = &config_error {
                            current.route.report_error(&error.to_owned().into());
                        } else {
                            current.route.clear_assistant_errors();
                        }

                        current.route.update_config(&self.config, appearance);
                    }
                }
                RioEventType::Rio(RioEvent::TitleWithSubtitle(title, subtitle)) => {
                    if let Some(current) = self.routes.get_mut(&window_id) {
                        window::set_window_title(current.id, title, subtitle);
                    }
                }
                RioEventType::Rio(RioEvent::MouseCursorDirty) => {
                    if let Some(current) = self.routes.get_mut(&window_id) {
                        current.route.mouse.accumulated_scroll =
                            crate::mouse::AccumulatedScroll::default();
                    }
                }
                RioEventType::Rio(RioEvent::Scroll(scroll)) => {
                    if let Some(current) = self.routes.get_mut(&window_id) {
                        let mut terminal = current.route.ctx.current().terminal.lock();
                        terminal.scroll_display(scroll);
                        drop(terminal);
                    }
                }
                RioEventType::Rio(RioEvent::Quit) => {
                    window::request_quit();
                }
                RioEventType::Rio(RioEvent::ClipboardLoad(clipboard_type, format)) => {
                    if let Some(current) = self.routes.get_mut(&window_id) {
                        if current.route.is_focused {
                            let text = format(
                                current.route.clipboard_get(clipboard_type).as_str(),
                            );
                            current
                                .route
                                .ctx
                                .current_mut()
                                .messenger
                                .send_bytes(text.into_bytes());
                        }
                    }
                }
                RioEventType::Rio(RioEvent::ClipboardStore(clipboard_type, content)) => {
                    if let Some(current) = self.routes.get_mut(&window_id) {
                        if current.route.is_focused {
                            current.route.clipboard_store(clipboard_type, content);
                        }
                    }
                }
                RioEventType::Rio(RioEvent::PtyWrite(text)) => {
                    if let Some(current) = self.routes.get_mut(&window_id) {
                        current
                            .route
                            .ctx
                            .current_mut()
                            .messenger
                            .send_bytes(text.into_bytes());
                    }
                }
                RioEventType::Rio(RioEvent::ReportToAssistant(error)) => {
                    if let Some(current) = self.routes.get_mut(&window_id) {
                        current.route.report_error(&error);
                    }
                }
                RioEventType::Rio(RioEvent::UpdateGraphicLibrary) => {
                    if let Some(current) = self.routes.get_mut(&window_id) {
                        let mut terminal = current.route.ctx.current().terminal.lock();
                        let graphics = terminal.graphics_take_queues();
                        if let Some(graphic_queues) = graphics {
                            let renderer = &mut current.route.sugarloaf;
                            for graphic_data in graphic_queues.pending {
                                renderer.add_graphic(graphic_data);
                            }

                            for graphic_data in graphic_queues.remove_queue {
                                renderer.remove_graphic(&graphic_data);
                            }
                        }
                    }
                }
                RioEventType::Rio(RioEvent::PrepareRender(millis)) => {
                    let timer_id = TimerId::new(Topic::Render, 0);
                    let event =
                        EventPayload::new(RioEventType::Rio(RioEvent::Render), window_id);

                    if !self.scheduler.scheduled(timer_id) {
                        self.scheduler.schedule(
                            event,
                            Duration::from_millis(millis),
                            false,
                            timer_id,
                        );
                    }
                }
                _ => {}
            };
        }

        // Update the scheduler after event processing to ensure
        // the event loop deadline is as accurate as possible.
        match self.scheduler.update() {
            Some(instant) => EventHandlerControl::WaitUntil(instant),
            None => EventHandlerControl::Wait,
        }
    }

    fn focus_event(&mut self, window_id: u16, focused: bool) {
        if let Some(current) = self.routes.get_mut(&window_id) {
            current.route.is_focused = focused;
            current.route.on_focus_change(focused);
        }
    }

    fn ime_event(&mut self, window_id: u16, ime_state: ImeState) {
        if let Some(current) = self.routes.get_mut(&window_id) {
            if current.route.path != RoutePath::Terminal {
                return;
            }

            match ime_state {
                ImeState::Commit(text) => {
                    // Don't use bracketed paste for single char input.
                    current.route.paste(&text, text.chars().count() > 1);
                }
                ImeState::Preedit(text, cursor_offset) => {
                    let preedit = if text.is_empty() {
                        None
                    } else {
                        Some(Preedit::new(text, cursor_offset.map(|offset| offset.0)))
                    };

                    if current.route.ime.preedit() != preedit.as_ref() {
                        current.route.ime.set_preedit(preedit);
                        current.route.render();
                    }
                }
                ImeState::Enabled => {
                    current.route.ime.set_enabled(true);
                }
                ImeState::Disabled => {
                    current.route.ime.set_enabled(false);
                }
            }
        }
    }

    fn modifiers_event(
        &mut self,
        window_id: u16,
        keycode: Option<KeyCode>,
        mods: ModifiersState,
    ) {
        self.modifiers = mods;

        if keycode == Some(KeyCode::LeftSuper) || keycode == Some(KeyCode::RightSuper) {
            if let Some(current) = self.routes.get_mut(&window_id) {
                if current
                    .route
                    .search_nearest_hyperlink_from_pos(&self.modifiers)
                {
                    window::set_mouse_cursor(current.id, wa::CursorIcon::Pointer);
                    current.route.render();
                }
            }
        }
    }

    fn key_down_event(
        &mut self,
        window_id: u16,
        keycode: KeyCode,
        repeat: bool,
        character: Option<smol_str::SmolStr>,
    ) {
        // FIX: Tab isn't being captured whenever other key is holding
        if let Some(current) = self.routes.get_mut(&window_id) {
            if current.route.has_key_wait(keycode) {
                return;
            }

            current.route.process_key_event(
                keycode,
                true,
                repeat,
                character,
                self.modifiers,
            );
        }
    }
    fn key_up_event(&mut self, window_id: u16, keycode: KeyCode) {
        if let Some(current) = self.routes.get_mut(&window_id) {
            if current.route.has_key_wait(keycode) {
                if current.route.path != RoutePath::Terminal {
                    // Scheduler must be cleaned after leave the terminal route
                    self.scheduler
                        .unschedule(TimerId::new(Topic::Render, window_id));
                }

                return;
            }

            current
                .route
                .process_key_event(keycode, false, false, None, self.modifiers);
        }
    }
    fn mouse_motion_event(&mut self, window_id: u16, x: f32, y: f32) {
        if let Some(current) = self.routes.get_mut(&window_id) {
            if current.route.path != RoutePath::Terminal {
                return;
            }

            if self.config.hide_cursor_when_typing {
                window::show_mouse(current.id, true);
            }

            if let Some(cursor) =
                current.route.process_motion_event(x, y, &self.modifiers)
            {
                window::set_mouse_cursor(current.id, cursor);
            }
        }
    }
    fn appearance_change_event(&mut self, window_id: u16, appearance: Appearance) {
        if let Some(current) = self.routes.get_mut(&window_id) {
            current.route.update_config(&self.config, appearance);
        }
    }
    fn touch_event(
        &mut self,
        window_id: u16,
        phase: TouchPhase,
        _id: u64,
        _x: f32,
        _y: f32,
    ) {
        if phase == TouchPhase::Started {
            if let Some(current) = self.routes.get_mut(&window_id) {
                current.route.mouse.accumulated_scroll = Default::default();
            }
        }
    }
    fn open_file_event(&mut self, window_id: u16, filepath: String) {
        if let Some(current) = self.routes.get_mut(&window_id) {
            current.route.paste(&filepath, true);
        }
    }
    // fn open_url_event(&mut self, window_id: u16, url: &str) {
    //     if let Some(current) = self.routes.get_mut(&window_id) {
    //         current.route.paste(&url, true);
    //         current.route.render();
    //     }
    // }
    fn mouse_wheel_event(&mut self, window_id: u16, mut x: f32, mut y: f32) {
        if let Some(current) = self.routes.get_mut(&window_id) {
            if current.route.path != RoutePath::Terminal {
                return;
            }

            if self.config.hide_cursor_when_typing {
                window::show_mouse(current.id, true);
            }

            // match delta {
            //     MouseScrollDelta::LineDelta(columns, lines) => {
            //         let new_scroll_px_x = columns
            //             * route.window.screen.sugarloaf.layout.font_size;
            //         let new_scroll_px_y = lines
            //             * route.window.screen.sugarloaf.layout.font_size;
            //         route.window.screen.scroll(
            //             new_scroll_px_x as f64,
            //             new_scroll_px_y as f64,
            //         );
            //     }

            // When the angle between (x, 0) and (x, y) is lower than ~25 degrees
            // (cosine is larger that 0.9) we consider this scrolling as horizontal.
            if x.abs() / x.hypot(y) > 0.9 {
                y = 0.;
            } else {
                x = 0.;
            }

            current.route.scroll(x.into(), y.into(), &self.modifiers);
            // current.render();
        }
    }
    fn mouse_button_down_event(
        &mut self,
        window_id: u16,
        button: MouseButton,
        x: f32,
        y: f32,
    ) {
        if let Some(current) = self.routes.get_mut(&window_id) {
            if current.route.path != RoutePath::Terminal {
                return;
            }

            if self.config.hide_cursor_when_typing {
                window::show_mouse(current.id, true);
            }

            current
                .route
                .process_mouse(button, x, y, true, self.modifiers);
        }
    }
    fn mouse_button_up_event(
        &mut self,
        window_id: u16,
        button: MouseButton,
        x: f32,
        y: f32,
    ) {
        if let Some(current) = self.routes.get_mut(&window_id) {
            if current.route.path != RoutePath::Terminal {
                return;
            }

            if self.config.hide_cursor_when_typing {
                window::show_mouse(current.id, true);
            }

            current
                .route
                .process_mouse(button, x, y, false, self.modifiers);
        }
    }
    fn resize_event(
        &mut self,
        window_id: u16,
        w: i32,
        h: i32,
        scale_factor: f32,
        rescale: bool,
    ) {
        if let Some(current) = self.routes.get_mut(&window_id) {
            if rescale {
                current.route.sugarloaf.rescale(scale_factor);
                current
                    .route
                    .sugarloaf
                    .resize(w.try_into().unwrap(), h.try_into().unwrap());
            } else {
                current
                    .route
                    .sugarloaf
                    .resize(w.try_into().unwrap(), h.try_into().unwrap());
            }
            current.route.resize_all_contexts();
        }
    }
    fn quit_requested_event(&mut self) {
        // window::cancel_quit(self.id);
    }
    fn files_dragged_event(
        &mut self,
        window_id: u16,
        _filepaths: Vec<std::path::PathBuf>,
        drag_state: DragState,
    ) {
        if let Some(current) = self.routes.get_mut(&window_id) {
            match drag_state {
                DragState::Entered => {
                    current.route.state.decrease_foreground_opacity(0.3);
                    current.route.render();
                }
                DragState::Exited => {
                    current.route.state.increase_foreground_opacity(0.3);
                    current.route.render();
                }
            }
        }
    }
    fn files_dropped_event(
        &mut self,
        window_id: u16,
        filepaths: Vec<std::path::PathBuf>,
    ) {
        if filepaths.is_empty() {
            return;
        }

        if let Some(current) = self.routes.get_mut(&window_id) {
            if current.route.path != RoutePath::Terminal {
                return;
            }

            let mut dropped_files = String::from("");
            for filepath in filepaths {
                dropped_files.push_str(&(filepath.to_string_lossy().to_string() + " "));
            }

            if !dropped_files.is_empty() {
                current.route.paste(&dropped_files, true);
            }
        }
    }

    // This is executed only in the initialization of App
    fn start(&mut self) {
        let proxy = self.event_loop.create_proxy();

        self.last_tab_group = if self.config.navigation.is_native() {
            Some(0)
        } else {
            None
        };

        let _ = crate::watcher::configuration_file_updates(
            rio_backend::config::config_dir_path(),
            EventProxy::new(proxy.clone()),
        );

        if let Ok(router) = create_window(
            proxy,
            &self.config,
            &None,
            self.font_library.clone(),
            self.last_tab_group,
            None,
        ) {
            self.routes.insert(router.id, router);
        }
    }
}

#[inline]
pub async fn run(
    config: rio_backend::config::Config,
    _config_error: Option<rio_backend::config::ConfigError>,
) -> Result<(), Box<dyn Error>> {
    let app = App::new(
        wa::Target::Application,
        Box::new(EventHandlerInstance::new(config)),
    );
    menu::create_menu();
    app.run();
    Ok(())
}
