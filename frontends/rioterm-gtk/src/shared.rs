//! What every terminal shares: the config, and the channel their
//! sessions' events reach the GTK main loop through. Sessions raise
//! events on their PTY threads, where nothing may touch GTK.

use crate::session::{Event, Sink};
use crate::terminal::TerminalView;
use gtk::glib;
use gtk::prelude::*;
use rio_backend::config::Config;
use rio_vt::clipboard::ClipboardType;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

struct ChannelSink {
    events: async_channel::Sender<(usize, Event)>,
    awake: Mutex<FxHashSet<usize>>,
}

impl Sink for ChannelSink {
    fn send(&self, id: usize, event: Event) {
        let _ = self.events.try_send((id, event));
    }

    fn wake(&self, id: usize) -> bool {
        self.awake
            .lock()
            .map(|mut awake| awake.insert(id))
            .unwrap_or(true)
    }
}

pub struct Shared {
    /// The config as the file has it.
    pub base: RefCell<Config>,
    /// What terminals use: `base` with the colors for the current style.
    pub config: RefCell<Config>,
    monitor: RefCell<Option<gtk::gio::FileMonitor>>,
    pub bindings: RefCell<Vec<crate::bindings::Binding>>,
    sink: Arc<ChannelSink>,
    next_id: Cell<usize>,
    views: RefCell<FxHashMap<usize, glib::WeakRef<TerminalView>>>,
}

impl Shared {
    pub fn start(config: Config) -> Rc<Self> {
        let (sender, receiver) = async_channel::unbounded();
        let dark = adw::StyleManager::default().is_dark();
        let bindings = crate::bindings::parse(&config.bindings.keys);
        let shared = Rc::new(Shared {
            bindings: RefCell::new(bindings),
            config: RefCell::new(crate::config::effective(&config, dark)),
            base: RefCell::new(config),
            monitor: RefCell::default(),
            sink: Arc::new(ChannelSink {
                events: sender,
                awake: Mutex::default(),
            }),
            next_id: Cell::new(1),
            views: RefCell::default(),
        });
        let weak = Rc::downgrade(&shared);
        glib::spawn_future_local(async move {
            while let Ok((id, event)) = receiver.recv().await {
                let Some(shared) = weak.upgrade() else {
                    break;
                };
                shared.dispatch(id, event);
            }
        });
        shared.apply_shortcuts();
        let monitor = crate::config::watch(&shared);
        shared.monitor.replace(monitor);
        shared
    }

    /// A new config from the file.
    pub fn reload(&self, base: Config) {
        if *self.base.borrow() == base {
            return;
        }
        adw::StyleManager::default().set_color_scheme(crate::config::color_scheme(&base));
        let dark = adw::StyleManager::default().is_dark();
        *self.config.borrow_mut() = crate::config::effective(&base, dark);
        *self.bindings.borrow_mut() = crate::bindings::parse(&base.bindings.keys);
        *self.base.borrow_mut() = base;
        self.apply_shortcuts();
        self.apply();
    }

    fn apply_shortcuts(&self) {
        let app = gtk::gio::Application::default().and_downcast::<gtk::Application>();
        if let Some(app) = app {
            crate::window::apply_shortcuts(&app, &self.bindings.borrow());
        }
    }

    /// Switch the window style between light and dark, and the colors
    /// with it when the config has adaptive ones.
    pub fn toggle_theme(&self) {
        let style = adw::StyleManager::default();
        style.set_color_scheme(if style.is_dark() {
            adw::ColorScheme::ForceLight
        } else {
            adw::ColorScheme::ForceDark
        });
    }

    /// The desktop switched between light and dark.
    pub fn restyle(&self, dark: bool) {
        let config = crate::config::effective(&self.base.borrow(), dark);
        if config.colors == self.config.borrow().colors {
            return;
        }
        *self.config.borrow_mut() = config;
        self.apply();
    }

    fn apply(&self) {
        let views: Vec<TerminalView> = self
            .views
            .borrow()
            .values()
            .filter_map(|view| view.upgrade())
            .collect();
        for view in views {
            view.apply_config();
        }
    }

    pub fn sink(&self) -> Arc<dyn Sink> {
        self.sink.clone()
    }

    pub fn register(&self, view: &TerminalView) -> usize {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        self.views.borrow_mut().insert(id, view.downgrade());
        id
    }

    pub fn unregister(&self, id: usize) {
        self.views.borrow_mut().remove(&id);
    }

    /// Ready for the next wakeup of `id`: called right before a frame
    /// takes the terminal's state, so output after it isn't lost.
    pub fn awake(&self, id: usize) {
        if let Ok(mut awake) = self.sink.awake.lock() {
            awake.remove(&id);
        }
    }

    fn dispatch(&self, id: usize, event: Event) {
        let view = self.views.borrow().get(&id).and_then(|v| v.upgrade());
        let Some(view) = view else {
            return;
        };
        match event {
            Event::Wakeup => view.queue_draw(),
            Event::Graphics(queues) => view.apply_graphics(queues),
            Event::Title(title) => view.set_program_title(title),
            Event::Directory => view.refresh_title(),
            Event::Bell => view.ring_bell(),
            Event::Clipboard(ClipboardType::Clipboard, text) => {
                WidgetExt::clipboard(&view).set_text(&text)
            }
            Event::Clipboard(ClipboardType::Selection, text) => {
                WidgetExt::primary_clipboard(&view).set_text(&text)
            }
            Event::ClipboardLoad(kind, format) => view.clipboard_load(kind, format),
            Event::CursorBlinking => view.restart_blink(),
            Event::Progress(value, state) => view.set_progress(state, value),
            Event::Notification(title, body) => view.notify(&title, &body),
            Event::CommandFinished(status, duration) => {
                view.command_finished(status, duration)
            }
            Event::Exited => view.emit_by_name::<()>("exited", &[]),
        }
    }
}
