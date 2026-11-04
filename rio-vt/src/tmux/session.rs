//! The reader-thread half of control mode: consumes the stream, tracks the
//! attached session and routes pane output.

use super::layout::LayoutNode;
use super::protocol::{id_of, parse_line, Notification};
use super::{PaneSink, TmuxSnapshot, TmuxWindow};
use crate::crosswords::grid::Dimensions;
use crate::crosswords::Crosswords;
use crate::event::sync::FairMutex;
use crate::event::{EventListener, RioEvent, WindowId};
use crate::performer::handler::Processor;
use crate::time::Instant;
use rustc_hash::FxHashMap;
use std::collections::VecDeque;
use std::fmt::Write;
use std::sync::Arc;

const LIST_WINDOWS: &str = "list-windows -F '#{window_id} #{window_layout} \
                            #{window_visible_layout} #{window_active} #{pane_id} \
                            #{window_name}'";

/// Comma separated so a variable an old tmux does not know expands to an
/// empty field rather than shifting the others.
const PANE_STATE_FORMAT: &str = "#{alternate_on},#{cursor_x},#{cursor_y},#{cursor_flag},\
                                 #{mouse_standard_flag},#{mouse_button_flag},\
                                 #{mouse_all_flag},#{mouse_sgr_flag}";

/// Scrollback lines to request when a pane appears.
const CAPTURE_HISTORY_LINES: usize = 5000;

/// A line this long is not tmux: it escapes control bytes, so `%output`
/// always reaches a newline. Bounds the buffer if something else took over
/// the PTY.
const MAX_LINE_LENGTH: usize = 64 * 1024 * 1024;

/// Why the session stopped reading the stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EndReason {
    /// tmux sent `%exit`: detached, the session ended, or the server died.
    Exit(Option<String>),
    /// Something other than tmux is writing: the client was killed or the
    /// connection dropped and the shell is back.
    Foreign,
}

/// Result of [`ControlSession::feed`].
#[derive(Debug, PartialEq, Eq)]
pub struct Fed {
    /// Bytes used. Anything left over after the session ended belongs to
    /// the terminal that hosted it.
    pub consumed: usize,
    pub ended: Option<EndReason>,
}

/// What a command's reply is for. Replies arrive in the order commands were
/// written, so this queue is the only correlation needed.
#[derive(Debug, PartialEq, Eq)]
enum Expect {
    Nothing,
    WindowList,
    PaneState(u32),
    PaneCapture(u32),
}

struct Block {
    number: u64,
    /// Started by a command of ours, rather than the `new-session` or
    /// `attach-session` on tmux's own command line.
    ours: bool,
    lines: Vec<Vec<u8>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PaneState {
    alternate_screen: bool,
    cursor_x: u32,
    cursor_y: u32,
    cursor_visible: bool,
    mouse_standard: bool,
    mouse_button: bool,
    mouse_all: bool,
    mouse_sgr: bool,
}

impl PaneState {
    fn parse(line: &[u8]) -> PaneState {
        let line = String::from_utf8_lossy(line);
        let mut fields = line.split(',');
        let mut number = || -> u32 {
            fields
                .next()
                .and_then(|field| field.parse().ok())
                .unwrap_or(0)
        };
        PaneState {
            alternate_screen: number() != 0,
            cursor_x: number(),
            cursor_y: number(),
            cursor_visible: number() != 0,
            mouse_standard: number() != 0,
            mouse_button: number() != 0,
            mouse_all: number() != 0,
            mouse_sgr: number() != 0,
        }
    }

    /// Rebuild a pane's content from its `capture-pane` lines.
    fn replay(&self, lines: &[Vec<u8>]) -> Vec<u8> {
        let mut replay = Vec::new();
        if self.alternate_screen {
            replay.extend_from_slice(b"\x1b[?1049h");
        }
        for (index, line) in lines.iter().enumerate() {
            if index != 0 {
                replay.extend_from_slice(b"\r\n");
            }
            replay.extend_from_slice(line);
        }

        let mut tail = String::from("\x1b[0m");
        let _ = write!(tail, "\x1b[{};{}H", self.cursor_y + 1, self.cursor_x + 1);
        if !self.cursor_visible {
            tail.push_str("\x1b[?25l");
        }
        for (enabled, mode) in [
            (self.mouse_standard, 1000),
            (self.mouse_button, 1002),
            (self.mouse_all, 1003),
            (self.mouse_sgr, 1006),
        ] {
            if enabled {
                let _ = write!(tail, "\x1b[?{mode}h");
            }
        }
        replay.extend_from_slice(tail.as_bytes());
        replay
    }
}

#[derive(Debug, PartialEq, Eq)]
enum PanePhase {
    /// Output is dropped until the capture lands: whatever was written
    /// before tmux ran `capture-pane` is part of the capture.
    AwaitingState,
    AwaitingCapture(PaneState),
    Live,
}

struct Pane {
    sink: Box<dyn PaneSink>,
    phase: PanePhase,
}

/// State of one attached control client.
#[derive(Default)]
pub struct ControlSession {
    line: Vec<u8>,
    block: Option<Block>,
    expected: VecDeque<Expect>,
    commands: Vec<Vec<u8>>,
    /// Set by the first protocol line.
    confirmed: bool,
    session_id: Option<u32>,
    windows: Vec<TmuxWindow>,
    active_window: Option<u32>,
    panes: FxHashMap<u32, Pane>,
    /// A `list-windows` is in flight; it will see any window added before
    /// its reply, since notifications and replies share one ordered stream.
    refreshing: bool,
    /// The first `list-windows` reply has landed.
    synced: bool,
    dirty: bool,
}

impl ControlSession {
    pub fn new() -> Self {
        Self::default()
    }

    /// Consume stream bytes, stopping at the end of the session.
    pub fn feed(&mut self, bytes: &[u8]) -> Fed {
        let mut consumed = 0;
        while consumed < bytes.len() {
            let rest = &bytes[consumed..];

            // Checked per line start, not per complete line: a shell prompt
            // has no newline and would otherwise sit unseen in the buffer.
            if self.line.is_empty() && self.block.is_none() && rest[0] != b'%' {
                return Fed {
                    consumed,
                    ended: Some(EndReason::Foreign),
                };
            }

            let Some(newline) = memchr::memchr(b'\n', rest) else {
                self.line.extend_from_slice(rest);
                consumed = bytes.len();
                if self.line.len() > MAX_LINE_LENGTH {
                    return Fed {
                        consumed,
                        ended: Some(EndReason::Foreign),
                    };
                }
                break;
            };

            self.line.extend_from_slice(&rest[..newline]);
            consumed += newline + 1;

            let mut line = std::mem::take(&mut self.line);
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            let ended = self.handle_line(&mut line);
            line.clear();
            self.line = line;
            if ended.is_some() {
                return Fed { consumed, ended };
            }
        }
        Fed {
            consumed,
            ended: None,
        }
    }

    /// Queue a command. Must only be called with text built from ids and
    /// literals: a line is executed by whatever reads the PTY.
    pub fn command(&mut self, command: &str) {
        self.send(command, Expect::Nothing);
    }

    /// Start routing a pane's output to `sink`, replaying its current
    /// content first.
    pub fn attach_pane(&mut self, id: u32, mut sink: Box<dyn PaneSink>) {
        let Some((_, columns, lines)) = self
            .windows
            .iter()
            .flat_map(|window| window.layout.panes())
            .find(|(pane, ..)| *pane == id)
        else {
            return;
        };

        sink.resize(columns, lines);
        self.panes.insert(
            id,
            Pane {
                sink,
                phase: PanePhase::AwaitingState,
            },
        );
        self.send(
            &format!("display-message -p -t %{id} '{PANE_STATE_FORMAT}'"),
            Expect::PaneState(id),
        );
        self.send(
            &format!("capture-pane -p -e -q -J -S -{CAPTURE_HISTORY_LINES} -t %{id}"),
            Expect::PaneCapture(id),
        );
    }

    /// Commands waiting to be written to the PTY.
    pub fn take_commands(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.commands)
    }

    /// The session's shape, if it changed since the last call.
    ///
    /// Held back while a `list-windows` is in flight, which is what a new
    /// window looks like: tmux announces it as the active window before
    /// adding it, and a snapshot naming a window it does not list would
    /// read as a different window being selected.
    pub fn take_snapshot(&mut self) -> Option<TmuxSnapshot> {
        if !self.dirty || !self.synced || self.refreshing {
            return None;
        }
        self.dirty = false;
        Some(TmuxSnapshot {
            windows: self.windows.clone(),
            active_window: self.active_window,
        })
    }

    /// The earliest synchronized update deadline among the panes.
    pub fn sync_deadline(&self) -> Option<Instant> {
        self.panes
            .values()
            .filter_map(|pane| pane.sink.sync_deadline())
            .min()
    }

    pub fn expire_sync(&mut self, now: Instant) {
        for pane in self.panes.values_mut() {
            pane.sink.expire_sync(now);
        }
    }

    fn send(&mut self, command: &str, expect: Expect) {
        let mut line = Vec::with_capacity(command.len() + 1);
        line.extend_from_slice(command.as_bytes());
        line.push(b'\n');
        self.commands.push(line);
        self.expected.push_back(expect);
    }

    fn refresh_windows(&mut self) {
        if !self.refreshing {
            self.refreshing = true;
            self.send(LIST_WINDOWS, Expect::WindowList);
        }
    }

    fn handle_line(&mut self, line: &mut Vec<u8>) -> Option<EndReason> {
        if let Some(block) = &mut self.block {
            // Reply text is free-form, so only the guard closing this very
            // block ends it.
            let closes = match parse_line(line) {
                Some(Notification::End { number }) if number == block.number => {
                    Some(true)
                }
                Some(Notification::Error { number }) if number == block.number => {
                    Some(false)
                }
                _ => None,
            };
            match closes {
                Some(success) => self.finish_block(success),
                None if block.ours => block.lines.push(std::mem::take(line)),
                None => (),
            }
            return None;
        }

        let Some(notification) = parse_line(line) else {
            return Some(EndReason::Foreign);
        };

        // Nothing is written to the PTY until the stream proves to be tmux:
        // `DCS 1000 p` alone can come from `cat`ing a file, and a shell
        // would run whatever we send.
        if !self.confirmed {
            self.confirmed = true;
            self.refresh_windows();
        }

        match notification {
            Notification::Begin { number, flags } => {
                self.block = Some(Block {
                    number,
                    ours: flags & 1 != 0,
                    lines: Vec::new(),
                });
            }
            Notification::End { .. } | Notification::Error { .. } => (),
            Notification::Output { pane, data } => {
                if let Some(pane) = self.panes.get_mut(&pane) {
                    if pane.phase == PanePhase::Live {
                        pane.sink.write(&data);
                    }
                }
            }
            Notification::LayoutChange {
                window,
                visible_layout,
            } => self.update_layout(window, &visible_layout),
            Notification::WindowAdd(_) | Notification::SessionChanged(_) => {
                if let Notification::SessionChanged(id) = notification {
                    self.session_id = Some(id);
                }
                self.refresh_windows();
            }
            Notification::WindowClose(id) => {
                self.windows.retain(|window| window.id != id);
                if self.active_window == Some(id) {
                    self.active_window = None;
                }
                self.prune_panes();
                self.dirty = true;
            }
            Notification::WindowRenamed { window, name } => {
                if let Some(window) = self.window_mut(window) {
                    window.name = name;
                    self.dirty = true;
                }
            }
            Notification::WindowPaneChanged { window, pane } => {
                if let Some(window) = self.window_mut(window) {
                    window.active_pane = Some(pane);
                    self.dirty = true;
                }
            }
            Notification::SessionWindowChanged { session, window } => {
                if self.session_id.is_none_or(|id| id == session) {
                    self.active_window = Some(window);
                    self.dirty = true;
                }
            }
            Notification::Exit(reason) => return Some(EndReason::Exit(reason)),
            Notification::Other => (),
        }
        None
    }

    fn window_mut(&mut self, id: u32) -> Option<&mut TmuxWindow> {
        self.windows.iter_mut().find(|window| window.id == id)
    }

    fn finish_block(&mut self, success: bool) {
        let Some(block) = self.block.take() else {
            return;
        };
        if !block.ours {
            return;
        }

        match self.expected.pop_front() {
            Some(Expect::WindowList) => {
                self.refreshing = false;
                if success {
                    self.apply_window_list(&block.lines);
                }
            }
            Some(Expect::PaneState(id)) => match block.lines.first() {
                Some(line) if success => {
                    if let Some(pane) = self.panes.get_mut(&id) {
                        pane.phase = PanePhase::AwaitingCapture(PaneState::parse(line));
                    }
                }
                _ => {
                    self.panes.remove(&id);
                }
            },
            Some(Expect::PaneCapture(id)) => {
                let Some(pane) = self.panes.get_mut(&id) else {
                    return;
                };
                match pane.phase {
                    PanePhase::AwaitingCapture(state) if success => {
                        pane.sink.write(&state.replay(&block.lines));
                        pane.phase = PanePhase::Live;
                    }
                    _ => {
                        self.panes.remove(&id);
                    }
                }
            }
            Some(Expect::Nothing) | None => (),
        }
    }

    fn apply_window_list(&mut self, lines: &[Vec<u8>]) {
        let mut windows = Vec::with_capacity(lines.len());
        let mut active_window = self.active_window;
        for line in lines {
            let mut fields = line.splitn(6, |byte| *byte == b' ');
            let (
                Some(id),
                Some(_layout),
                Some(visible_layout),
                Some(active),
                Some(pane),
                Some(name),
            ) = (
                fields.next(),
                fields.next(),
                fields.next(),
                fields.next(),
                fields.next(),
                fields.next(),
            )
            else {
                continue;
            };
            let (Some(id), Some(layout)) = (
                id_of(id, b'@'),
                std::str::from_utf8(visible_layout)
                    .ok()
                    .and_then(LayoutNode::parse),
            ) else {
                continue;
            };
            if active == b"1" {
                active_window = Some(id);
            }
            windows.push(TmuxWindow {
                id,
                name: String::from_utf8_lossy(name).into_owned(),
                layout,
                active_pane: id_of(pane, b'%'),
            });
        }

        self.windows = windows;
        self.active_window = active_window;
        self.synced = true;
        self.dirty = true;
        self.resize_panes();
        self.prune_panes();
    }

    fn update_layout(&mut self, id: u32, visible_layout: &str) {
        let Some(layout) = LayoutNode::parse(visible_layout) else {
            return;
        };
        let Some(window) = self.window_mut(id) else {
            // Reported ahead of the window itself.
            self.refresh_windows();
            return;
        };
        window.layout = layout;
        self.dirty = true;
        self.resize_panes();
        self.prune_panes();
    }

    /// Resizing here rather than when the frontend gets to the snapshot
    /// keeps the size in step with the stream: the redraw an application
    /// does after SIGWINCH is the very next `%output`.
    fn resize_panes(&mut self) {
        for window in &self.windows {
            for (id, columns, lines) in window.layout.panes() {
                if let Some(pane) = self.panes.get_mut(&id) {
                    pane.sink.resize(columns, lines);
                }
            }
        }
    }

    /// Stop routing to panes no longer visible, including those hidden by a
    /// zoom. The frontend attaches them afresh if they come back.
    fn prune_panes(&mut self) {
        let visible: Vec<u32> = self
            .windows
            .iter()
            .flat_map(|window| window.layout.panes())
            .map(|(id, ..)| id)
            .collect();
        self.panes.retain(|id, _| visible.contains(id));
    }
}

/// Parses one pane's output into its terminal.
pub struct TerminalSink<U: EventListener> {
    terminal: Arc<FairMutex<Crosswords<U>>>,
    parser: Processor,
    event_proxy: U,
    window_id: WindowId,
    route_id: usize,
    deadline: Option<Instant>,
}

impl<U: EventListener> TerminalSink<U> {
    pub fn new(
        terminal: Arc<FairMutex<Crosswords<U>>>,
        event_proxy: U,
        window_id: WindowId,
        route_id: usize,
    ) -> Self {
        Self {
            terminal,
            parser: Processor::default(),
            event_proxy,
            window_id,
            route_id,
            deadline: None,
        }
    }

    fn notify_damage(&self, terminal: &mut Crosswords<U>) {
        if !terminal.damage_event_in_flight {
            terminal.damage_event_in_flight = true;
            self.event_proxy
                .send_event(RioEvent::TerminalDamaged(self.route_id), self.window_id);
        }
    }
}

impl<U: EventListener> std::fmt::Debug for TerminalSink<U> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TerminalSink(route={})", self.route_id)
    }
}

/// A size in cells, keeping the terminal's cell metrics: tmux only decides
/// the former.
struct PaneSize {
    columns: usize,
    lines: usize,
    square_width: f32,
    square_height: f32,
}

impl Dimensions for PaneSize {
    fn total_lines(&self) -> usize {
        self.lines
    }

    fn screen_lines(&self) -> usize {
        self.lines
    }

    fn columns(&self) -> usize {
        self.columns
    }

    fn square_width(&self) -> f32 {
        self.square_width
    }

    fn square_height(&self) -> f32 {
        self.square_height
    }
}

impl<U: EventListener + Send + 'static> PaneSink for TerminalSink<U> {
    fn write(&mut self, bytes: &[u8]) {
        let mut guard = self.terminal.lock();
        let terminal = &mut *guard;
        let held_before = terminal.synchronized_output();
        self.parser.advance(terminal, bytes);

        self.deadline = terminal.sync_update_deadline();
        let held = terminal.synchronized_output();
        if (held_before && !held) || (!held && terminal.peek_damage_event().is_some()) {
            self.notify_damage(terminal);
        }
    }

    fn resize(&mut self, columns: usize, lines: usize) {
        let mut guard = self.terminal.lock();
        let terminal = &mut *guard;
        if terminal.columns() == columns && terminal.screen_lines() == lines {
            return;
        }
        let size = PaneSize {
            columns,
            lines,
            square_width: terminal.square_width(),
            square_height: terminal.square_height(),
        };
        terminal.resize(size);
        self.notify_damage(terminal);
    }

    fn sync_deadline(&self) -> Option<Instant> {
        self.deadline
    }

    fn expire_sync(&mut self, now: Instant) {
        if self.deadline.is_none_or(|deadline| deadline > now) {
            return;
        }
        let mut guard = self.terminal.lock();
        let terminal = &mut *guard;
        if terminal.sync_update_expired(now) {
            terminal.end_sync_update();
            self.notify_damage(terminal);
        }
        self.deadline = terminal.sync_update_deadline();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// The opening of a tmux 3.2a session: attach, `split-window -h`, then
    /// `new-window`. Adapted from a capture, with the output shortened and
    /// the `list-windows` reply in the format this session asks for.
    const TRANSCRIPT: &[u8] = b"%begin 1793790354 257 0\r\n\
        %end 1793790354 257 0\r\n\
        %window-add @0\r\n\
        %sessions-changed\r\n\
        %session-changed $0 cap\r\n\
        %output %0 Welcome\\015\\012\r\n\
        %begin 1793790355 262 1\r\n\
        @0 a87d,100x30,0,0,0 a87d,100x30,0,0,0 1 %0 bash\r\n\
        %end 1793790355 262 1\r\n\
        %layout-change @0 a87d,100x30,0,0,0 a87d,100x30,0,0,0 *\r\n\
        %window-pane-changed @0 %1\r\n\
        %layout-change @0 6b8b,100x30,0,0{50x30,0,0,0,49x30,51,0,1} \
        6b8b,100x30,0,0{50x30,0,0,0,49x30,51,0,1} *\r\n\
        %session-window-changed $0 @1\r\n\
        %window-add @1\r\n\
        %begin 1793790357 271 1\r\n\
        @0 6b8b,100x30,0,0{50x30,0,0,0,49x30,51,0,1} \
        6b8b,100x30,0,0{50x30,0,0,0,49x30,51,0,1} 0 %1 bash\r\n\
        @1 a87e,100x30,0,0,2 a87e,100x30,0,0,2 1 %2 two\r\n\
        %end 1793790357 271 1\r\n";

    #[derive(Clone, Default)]
    struct Recorder(Arc<Mutex<Recorded>>);

    #[derive(Default)]
    struct Recorded {
        written: Vec<u8>,
        size: Option<(usize, usize)>,
    }

    impl std::fmt::Debug for Recorder {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "Recorder")
        }
    }

    impl PaneSink for Recorder {
        fn write(&mut self, bytes: &[u8]) {
            self.0.lock().unwrap().written.extend_from_slice(bytes);
        }
        fn resize(&mut self, columns: usize, lines: usize) {
            self.0.lock().unwrap().size = Some((columns, lines));
        }
        fn sync_deadline(&self) -> Option<Instant> {
            None
        }
        fn expire_sync(&mut self, _: Instant) {}
    }

    impl Recorder {
        fn written(&self) -> Vec<u8> {
            self.0.lock().unwrap().written.clone()
        }
    }

    fn commands(session: &mut ControlSession) -> Vec<String> {
        session
            .take_commands()
            .into_iter()
            .map(|line| String::from_utf8(line).unwrap())
            .collect()
    }

    fn feed(session: &mut ControlSession, bytes: &[u8]) {
        assert_eq!(
            session.feed(bytes),
            Fed {
                consumed: bytes.len(),
                ended: None
            }
        );
    }

    /// A session past its first `list-windows`, with `%0` in window `@0`.
    fn synced_session() -> ControlSession {
        let mut session = ControlSession::new();
        feed(&mut session, b"%begin 1 1 0\r\n%end 1 1 0\r\n");
        session.take_commands();
        feed(
            &mut session,
            b"%begin 2 2 1\r\n\
              @0 a87d,80x24,0,0,0 a87d,80x24,0,0,0 1 %0 bash\r\n\
              %end 2 2 1\r\n",
        );
        session
    }

    fn attach(
        session: &mut ControlSession,
        id: u32,
        state: &[u8],
        capture: &[u8],
    ) -> Recorder {
        let recorder = Recorder::default();
        session.attach_pane(id, Box::new(recorder.clone()));
        session.take_commands();
        let mut reply = b"%begin 3 3 1\r\n".to_vec();
        reply.extend_from_slice(state);
        reply.extend_from_slice(b"\r\n%end 3 3 1\r\n%begin 4 4 1\r\n");
        reply.extend_from_slice(capture);
        reply.extend_from_slice(b"%end 4 4 1\r\n");
        feed(session, &reply);
        recorder
    }

    #[test]
    fn real_transcript_builds_the_session() {
        let mut session = ControlSession::new();
        feed(&mut session, TRANSCRIPT);

        let snapshot = session.take_snapshot().unwrap();
        assert_eq!(snapshot.active_window, Some(1));
        assert_eq!(snapshot.windows.len(), 2);
        assert_eq!(snapshot.windows[1].name, "two");
        let window = &snapshot.windows[0];
        assert_eq!(window.id, 0);
        assert_eq!(window.name, "bash");
        assert_eq!(window.active_pane, Some(1));
        assert_eq!(window.layout.panes(), vec![(0, 50, 30), (1, 49, 30)]);
    }

    #[test]
    fn nothing_is_sent_before_the_stream_looks_like_tmux() {
        let mut session = ControlSession::new();
        let fed = session.feed(b"just a file someone printed\r\n$ ");
        assert_eq!(fed.consumed, 0);
        assert_eq!(fed.ended, Some(EndReason::Foreign));
        assert!(session.take_commands().is_empty());
    }

    #[test]
    fn first_protocol_line_requests_the_windows() {
        let mut session = ControlSession::new();
        feed(&mut session, b"%begin 1 1 0\r\n");
        assert_eq!(commands(&mut session), vec![format!("{LIST_WINDOWS}\n")]);
    }

    #[test]
    fn startup_block_does_not_consume_a_reply_slot() {
        // The attach command's own block carries flags 0; matching it
        // against our queue would shift every later reply by one.
        let mut session = ControlSession::new();
        feed(&mut session, b"%begin 1 1 0\r\n%end 1 1 0\r\n");
        assert_eq!(session.expected.front(), Some(&Expect::WindowList));
    }

    #[test]
    fn window_add_coalesces_while_a_refresh_is_in_flight() {
        let mut session = ControlSession::new();
        feed(&mut session, b"%window-add @0\r\n%window-add @1\r\n");
        assert_eq!(commands(&mut session).len(), 1);
    }

    #[test]
    fn lines_may_arrive_in_pieces() {
        let mut session = ControlSession::new();
        for byte in TRANSCRIPT {
            feed(&mut session, std::slice::from_ref(byte));
        }
        assert_eq!(session.take_snapshot().unwrap().windows.len(), 2);
    }

    #[test]
    fn a_window_is_never_active_before_it_is_listed() {
        let mut session = synced_session();
        session.take_snapshot();
        feed(
            &mut session,
            b"%session-window-changed $0 @1\r\n%window-add @1\r\n",
        );
        assert_eq!(session.take_snapshot(), None);

        feed(
            &mut session,
            b"%begin 5 5 1\r\n\
              @0 a87d,80x24,0,0,0 a87d,80x24,0,0,0 0 %0 bash\r\n\
              @1 a87e,80x24,0,0,1 a87e,80x24,0,0,1 1 %1 two\r\n\
              %end 5 5 1\r\n",
        );
        let snapshot = session.take_snapshot().unwrap();
        assert_eq!(snapshot.active_window, Some(1));
        assert_eq!(snapshot.windows.len(), 2);
    }

    #[test]
    fn exit_leaves_the_trailing_bytes_to_the_terminal() {
        let mut session = synced_session();
        let bytes = b"%exit\r\n\x1b\\$ ";
        assert_eq!(
            session.feed(bytes),
            Fed {
                consumed: 7,
                ended: Some(EndReason::Exit(None))
            }
        );
    }

    #[test]
    fn dropped_connection_returns_the_output_to_the_terminal() {
        let mut session = synced_session();
        let bytes = b"%output %0 x\r\nConnection to host closed.\r\n$ ";
        assert_eq!(
            session.feed(bytes),
            Fed {
                consumed: 14,
                ended: Some(EndReason::Foreign)
            }
        );
    }

    #[test]
    fn shell_prompt_without_a_newline_is_noticed() {
        let mut session = synced_session();
        assert_eq!(session.feed(b"$ ").ended, Some(EndReason::Foreign));
    }

    #[test]
    fn reply_text_is_not_parsed_as_protocol() {
        let mut session = synced_session();
        session.command("show-options");
        feed(
            &mut session,
            b"%begin 5 5 1\r\n%exit\r\nplain text\r\n%end 99 99 1\r\n%end 5 5 1\r\n",
        );
        assert!(session.block.is_none());
    }

    #[test]
    fn replies_are_matched_in_order_even_after_errors() {
        let mut session = synced_session();
        session.take_snapshot();
        session.command("nosuchcmd");
        session.refresh_windows();
        feed(
            &mut session,
            b"%begin 5 5 1\r\nparse error: unknown command: nosuchcmd\r\n%error 5 5 1\r\n\
              %begin 6 6 1\r\n\
              @0 a87d,80x24,0,0,0 a87d,80x24,0,0,0 1 %0 renamed\r\n\
              %end 6 6 1\r\n",
        );
        assert_eq!(session.take_snapshot().unwrap().windows[0].name, "renamed");
    }

    #[test]
    fn capture_replaces_output_from_before_it() {
        let mut session = synced_session();
        let recorder = Recorder::default();
        session.attach_pane(0, Box::new(recorder.clone()));
        let sent = commands(&mut session);
        assert!(sent[0].starts_with("display-message -p -t %0 "));
        assert!(sent[1].starts_with("capture-pane -p -e -q -J "));
        assert!(sent[1].ends_with("-t %0\n"));

        feed(
            &mut session,
            b"%output %0 stale\r\n\
              %begin 3 3 1\r\n0,2,1,1,0,0,0,0\r\n%end 3 3 1\r\n\
              %output %0 stale\r\n\
              %begin 4 4 1\r\none\r\n\x1b[1mtwo\r\n%end 4 4 1\r\n\
              %output %0 fresh\r\n",
        );
        assert_eq!(
            recorder.written(),
            b"one\r\n\x1b[1mtwo\x1b[0m\x1b[2;3Hfresh".to_vec()
        );
    }

    #[test]
    fn replay_restores_alternate_screen_and_modes() {
        let mut session = synced_session();
        let recorder = attach(&mut session, 0, b"1,0,0,0,1,0,0,1", b"vim\r\n");
        assert_eq!(
            recorder.written(),
            b"\x1b[?1049hvim\x1b[0m\x1b[1;1H\x1b[?25l\x1b[?1000h\x1b[?1006h".to_vec()
        );
    }

    #[test]
    fn state_with_missing_fields_defaults_to_off() {
        let state = PaneState::parse(b"0,4,5,1,,,,");
        assert_eq!(state.cursor_x, 4);
        assert!(state.cursor_visible);
        assert!(!state.mouse_sgr);
    }

    #[test]
    fn output_for_a_failed_capture_is_not_routed() {
        let mut session = synced_session();
        let recorder = Recorder::default();
        session.attach_pane(0, Box::new(recorder.clone()));
        feed(
            &mut session,
            b"%begin 3 3 1\r\ncan't find pane\r\n%error 3 3 1\r\n\
              %begin 4 4 1\r\n%error 4 4 1\r\n\
              %output %0 lost\r\n",
        );
        assert!(recorder.written().is_empty());
        assert!(session.panes.is_empty());
    }

    #[test]
    fn layout_change_resizes_attached_panes_immediately() {
        let mut session = synced_session();
        let recorder = attach(&mut session, 0, b"0,0,0,1,0,0,0,0", b"");
        feed(
            &mut session,
            b"%layout-change @0 a87d,120x40,0,0,0 a87d,120x40,0,0,0 *\r\n",
        );
        assert_eq!(recorder.0.lock().unwrap().size, Some((120, 40)));
    }

    #[test]
    fn zooming_stops_routing_to_hidden_panes() {
        let mut session = synced_session();
        attach(&mut session, 0, b"0,0,0,1,0,0,0,0", b"");
        feed(
            &mut session,
            b"%layout-change @0 aaaa,80x24,0,0,1 aaaa,80x24,0,0,1 *Z\r\n",
        );
        assert!(!session.panes.contains_key(&0));
    }

    #[test]
    fn attaching_a_pane_tmux_no_longer_has_is_ignored() {
        let mut session = synced_session();
        session.attach_pane(9, Box::new(Recorder::default()));
        assert!(session.take_commands().is_empty());
    }

    #[test]
    fn closing_a_window_drops_its_panes() {
        let mut session = synced_session();
        attach(&mut session, 0, b"0,0,0,1,0,0,0,0", b"");
        session.take_snapshot();
        feed(&mut session, b"%unlinked-window-close @0\r\n");
        assert!(session.take_snapshot().unwrap().windows.is_empty());
        assert!(session.panes.is_empty());
    }

    #[test]
    fn other_sessions_do_not_move_the_active_window() {
        let mut session = ControlSession::new();
        feed(
            &mut session,
            b"%session-changed $3 mine\r\n%session-window-changed $9 @7\r\n",
        );
        assert_eq!(session.active_window, None);
    }

    #[test]
    fn snapshots_only_follow_changes() {
        let mut session = synced_session();
        assert!(session.take_snapshot().is_some());
        assert!(session.take_snapshot().is_none());
    }
}
