#[cfg(feature = "capi")]
pub mod capi;
pub mod key;
mod render_state;

pub use key::{encode as encode_key, Key, KeyEvent, Modifiers};
pub use render_state::RenderState;
pub use rio_backend::clipboard::ClipboardType;
pub use rio_backend::config::colors::{AnsiColor, ColorRgb, NamedColor};
pub use rio_backend::crosswords::pos::Column;
pub use rio_backend::crosswords::square::Square;
pub use rio_backend::crosswords::style::{Style, StyleFlags};

use rio_backend::ansi::CursorShape;
use rio_backend::crosswords::{Crosswords, Mode};
use rio_backend::event::sync::FairMutex;
use rio_backend::event::{EventListener, Msg, RioEvent, WindowId};
use rio_backend::performer::Machine;
use std::borrow::Cow;
use std::error::Error;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use teletypewriter::{create_pty_with_spawn, WinsizeBuilder};

pub type SurfaceId = usize;

struct GridSize {
    rows: usize,
    cols: usize,
}

impl rio_backend::crosswords::grid::Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.rows
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.cols
    }

    fn square_width(&self) -> f32 {
        0.
    }

    fn square_height(&self) -> f32 {
        0.
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    SetTitle {
        title: String,
        subtitle: Option<String>,
    },
    RingBell,
    CursorBlinkingChange,
}

pub trait SurfaceDelegate: Send + Sync + 'static {
    fn wakeup(&self, surface: SurfaceId);
    fn action(&self, _surface: SurfaceId, _action: Action) {}
    fn clipboard_write(&self, _surface: SurfaceId, _kind: ClipboardType, _text: String) {}
    fn close_surface(&self, _surface: SurfaceId) {}
}

#[derive(Clone)]
pub(crate) struct Listener {
    surface_id: SurfaceId,
    delegate: Arc<dyn SurfaceDelegate>,
    pty_writer: Arc<Mutex<Option<corcovado::channel::Sender<Msg>>>>,
}

impl Listener {
    fn dispatch(&self, event: RioEvent) {
        match event {
            RioEvent::TerminalDamaged(_)
            | RioEvent::Render
            | RioEvent::RenderRoute(_) => {
                self.delegate.wakeup(self.surface_id);
            }
            RioEvent::Title(title) => {
                self.delegate.action(
                    self.surface_id,
                    Action::SetTitle {
                        title,
                        subtitle: None,
                    },
                );
            }
            RioEvent::TitleWithSubtitle(title, subtitle) => {
                self.delegate.action(
                    self.surface_id,
                    Action::SetTitle {
                        title,
                        subtitle: Some(subtitle),
                    },
                );
            }
            RioEvent::Bell => {
                self.delegate.action(self.surface_id, Action::RingBell);
            }
            RioEvent::CursorBlinkingChange | RioEvent::CursorBlinkingChangeOnRoute(_) => {
                self.delegate
                    .action(self.surface_id, Action::CursorBlinkingChange);
            }
            RioEvent::ClipboardStore(kind, text) => {
                self.delegate.clipboard_write(self.surface_id, kind, text);
            }
            RioEvent::PtyWrite(_, text) => {
                if let Some(channel) = self.pty_writer.lock().unwrap().as_ref() {
                    let _ = channel.send(Msg::Input(Cow::Owned(text.into_bytes())));
                }
            }
            RioEvent::CloseTerminal(_) | RioEvent::Exit => {
                self.delegate.close_surface(self.surface_id);
            }
            _ => {}
        }
    }
}

impl EventListener for Listener {
    fn event(&self) -> (Option<RioEvent>, bool) {
        (None, false)
    }

    fn send_event(&self, event: RioEvent, _id: WindowId) {
        self.dispatch(event);
    }

    fn send_event_with_high_priority(&self, event: RioEvent, _id: WindowId) {
        self.dispatch(event);
    }
}

#[derive(Debug, Clone)]
pub struct SurfaceDesc {
    pub shell: Option<String>,
    pub args: Vec<String>,
    pub working_dir: Option<String>,
    pub cols: u16,
    pub rows: u16,
    pub pixel_width: u16,
    pub pixel_height: u16,
    pub scrollback: usize,
}

impl Default for SurfaceDesc {
    fn default() -> Self {
        Self {
            shell: None,
            args: Vec::new(),
            working_dir: None,
            cols: 80,
            rows: 24,
            pixel_width: 720,
            pixel_height: 432,
            scrollback: 10_000,
        }
    }
}

pub struct Engine {
    delegate: Arc<dyn SurfaceDelegate>,
    next_surface_id: AtomicUsize,
}

impl Engine {
    pub fn new(delegate: Arc<dyn SurfaceDelegate>) -> Self {
        Self {
            delegate,
            next_surface_id: AtomicUsize::new(1),
        }
    }

    pub fn create_surface(
        &self,
        desc: &SurfaceDesc,
    ) -> Result<Surface, Box<dyn Error + Send + Sync>> {
        Surface::new(self, desc)
    }
}

pub struct Surface {
    id: SurfaceId,
    terminal: Arc<FairMutex<Crosswords<Listener>>>,
    channel: corcovado::channel::Sender<Msg>,
    shell_pid: u32,
    _io_thread: std::thread::JoinHandle<(
        Machine<teletypewriter::Pty, Listener>,
        rio_backend::performer::State,
    )>,
}

impl Surface {
    fn new(
        engine: &Engine,
        desc: &SurfaceDesc,
    ) -> Result<Surface, Box<dyn Error + Send + Sync>> {
        let id = engine.next_surface_id.fetch_add(1, Ordering::SeqCst);
        let pty_writer = Arc::new(Mutex::new(None));
        let listener = Listener {
            surface_id: id,
            delegate: engine.delegate.clone(),
            pty_writer: pty_writer.clone(),
        };

        let terminal = Crosswords::new(
            GridSize {
                rows: desc.rows as usize,
                cols: desc.cols as usize,
            },
            CursorShape::Block,
            listener.clone(),
            WindowId::from(id as u64),
            id,
            desc.scrollback,
        );
        let terminal = Arc::new(FairMutex::new(terminal));

        let shell = desc
            .shell
            .clone()
            .or_else(|| std::env::var("SHELL").ok())
            .unwrap_or_else(|| String::from("/bin/sh"));

        let pty = create_pty_with_spawn(
            &Cow::Borrowed(shell.as_str()),
            desc.args.clone(),
            &desc.working_dir,
            desc.cols,
            desc.rows,
            desc.pixel_width,
            desc.pixel_height,
        )
        .map_err(|err| Box::new(err) as Box<dyn Error + Send + Sync>)?;

        let shell_pid = *pty.child.pid.clone() as u32;

        let machine = Machine::new(
            Arc::clone(&terminal),
            pty,
            listener,
            WindowId::from(id as u64),
            id,
        )
        .map_err(|err| std::io::Error::other(err.to_string()))?;
        let channel = machine.channel();
        *pty_writer.lock().unwrap() = Some(channel.clone());
        let io_thread = machine.spawn();

        Ok(Surface {
            id,
            terminal,
            channel,
            shell_pid,
            _io_thread: io_thread,
        })
    }

    pub fn id(&self) -> SurfaceId {
        self.id
    }

    pub fn write<B: Into<Cow<'static, [u8]>>>(&self, bytes: B) {
        let _ = self.channel.send(Msg::Input(bytes.into()));
    }

    pub fn text(&self, text: &str) {
        self.write(text.as_bytes().to_vec());
    }

    pub fn key(&self, event: KeyEvent) -> bool {
        let app_cursor = self.terminal.lock().mode().contains(Mode::APP_CURSOR);
        match key::encode(event, app_cursor) {
            Some(bytes) => {
                self.write(bytes);
                true
            }
            None => false,
        }
    }

    pub fn resize(&self, cols: u16, rows: u16, pixel_width: u16, pixel_height: u16) {
        self.terminal.lock().resize(GridSize {
            rows: rows as usize,
            cols: cols as usize,
        });
        let _ = self.channel.send(Msg::Resize(WinsizeBuilder {
            rows,
            cols,
            width: pixel_width,
            height: pixel_height,
        }));
    }

    pub fn scroll(&self, delta_lines: i32) {
        use rio_backend::crosswords::grid::Scroll;
        self.terminal
            .lock()
            .scroll_display(Scroll::Delta(delta_lines));
    }

    pub(crate) fn terminal(&self) -> Arc<FairMutex<Crosswords<Listener>>> {
        self.terminal.clone()
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        let _ = self.channel.send(Msg::Shutdown);
        teletypewriter::kill_pid(self.shell_pid as i32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::time::{Duration, Instant};

    struct CountingDelegate {
        wakeups: AtomicUsize,
    }

    impl SurfaceDelegate for CountingDelegate {
        fn wakeup(&self, _surface: SurfaceId) {
            self.wakeups.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn drives_a_real_shell_and_reads_cells() {
        let delegate = Arc::new(CountingDelegate {
            wakeups: AtomicUsize::new(0),
        });
        let engine = Engine::new(delegate.clone());
        let desc = SurfaceDesc::default();
        let surface = engine.create_surface(&desc).expect("spawn shell");
        let mut state = RenderState::new(&surface);

        std::thread::sleep(Duration::from_millis(400));
        surface.text("printf '%s%s\\n' li brio-gate\r");

        let deadline = Instant::now() + Duration::from_secs(8);
        let mut found = false;
        while Instant::now() < deadline {
            state.update();
            let lines = state.lines();
            if (0..lines).any(|i| state.text_row(i).contains("librio-gate")) {
                found = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }

        if !found {
            let rows: Vec<String> = (0..6).map(|i| state.text_row(i)).collect();
            panic!(
                "expected shell output in grid; wakeups={} rows={:?}",
                delegate.wakeups.load(Ordering::SeqCst),
                rows
            );
        }
        assert!(delegate.wakeups.load(Ordering::SeqCst) > 0);
    }
}
