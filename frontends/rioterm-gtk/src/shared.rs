//! The librio engine all terminals share, and the way its events reach
//! GTK: librio calls its delegate on its reader threads (and inside
//! surface calls), where nothing may touch GTK, so the delegate only sends
//! each event down a channel the main loop drains.

use crate::terminal::TerminalView;
use gtk::glib;
use gtk::prelude::*;
use librio::{Action, ClipboardType, SurfaceDelegate, SurfaceId};
use rio_backend::config::Config;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

pub enum Event {
    /// The surface has something new to show.
    Wakeup,
    Title(String),
    Bell,
    /// A program set the clipboard (OSC 52).
    Clipboard(ClipboardType, String),
    /// The shell exited.
    Exited,
}

struct Delegate {
    events: async_channel::Sender<(SurfaceId, Event)>,
    /// Surfaces with a wakeup on its way to the main loop. librio can wake
    /// a surface for every chunk of output; one queued redraw covers them
    /// all, so later ones aren't sent until the main loop takes it.
    awake: Mutex<FxHashSet<SurfaceId>>,
}

impl Delegate {
    fn send(&self, surface: SurfaceId, event: Event) {
        // Unbounded, so this never blocks; it fails only once the main
        // loop is gone, when there is nobody left to tell.
        let _ = self.events.try_send((surface, event));
    }
}

impl SurfaceDelegate for Delegate {
    fn wakeup(&self, surface: SurfaceId) {
        let newly = self
            .awake
            .lock()
            .map(|mut awake| awake.insert(surface))
            .unwrap_or(true);
        if newly {
            self.send(surface, Event::Wakeup);
        }
    }

    fn action(&self, surface: SurfaceId, action: Action) {
        match action {
            Action::SetTitle { title, .. } => self.send(surface, Event::Title(title)),
            Action::RingBell => self.send(surface, Event::Bell),
            _ => {}
        }
    }

    fn clipboard_write(&self, surface: SurfaceId, kind: ClipboardType, text: String) {
        self.send(surface, Event::Clipboard(kind, text));
    }

    fn close_surface(&self, surface: SurfaceId) {
        self.send(surface, Event::Exited);
    }
}

pub struct Shared {
    pub engine: librio::Engine,
    pub config: Config,
    delegate: Arc<Delegate>,
    views: RefCell<FxHashMap<SurfaceId, glib::WeakRef<TerminalView>>>,
}

impl Shared {
    /// The engine, and the main-loop task that hands its events to the
    /// terminal they belong to.
    pub fn start(config: Config) -> Rc<Self> {
        let (sender, receiver) = async_channel::unbounded();
        let delegate = Arc::new(Delegate {
            events: sender,
            awake: Mutex::default(),
        });
        let shared = Rc::new(Shared {
            engine: librio::Engine::new(delegate.clone()),
            config,
            delegate,
            views: RefCell::default(),
        });
        let weak = Rc::downgrade(&shared);
        glib::spawn_future_local(async move {
            while let Ok((surface, event)) = receiver.recv().await {
                let Some(shared) = weak.upgrade() else {
                    break;
                };
                shared.dispatch(surface, event);
            }
        });
        shared
    }

    pub fn register(&self, surface: SurfaceId, view: &TerminalView) {
        self.views.borrow_mut().insert(surface, view.downgrade());
    }

    pub fn unregister(&self, surface: SurfaceId) {
        self.views.borrow_mut().remove(&surface);
    }

    fn dispatch(&self, surface: SurfaceId, event: Event) {
        if matches!(event, Event::Wakeup) {
            if let Ok(mut awake) = self.delegate.awake.lock() {
                awake.remove(&surface);
            }
        }
        let view = self.views.borrow().get(&surface).and_then(|v| v.upgrade());
        let Some(view) = view else {
            return;
        };
        match event {
            Event::Wakeup => view.queue_draw(),
            Event::Title(title) => view.set_title(title),
            Event::Bell => {
                view.error_bell();
                view.emit_by_name::<()>("bell", &[]);
            }
            Event::Clipboard(ClipboardType::Clipboard, text) => {
                WidgetExt::clipboard(&view).set_text(&text)
            }
            Event::Clipboard(ClipboardType::Selection, text) => {
                WidgetExt::primary_clipboard(&view).set_text(&text)
            }
            Event::Exited => view.emit_by_name::<()>("exited", &[]),
        }
    }
}
