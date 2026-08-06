//! rmx (Rio Multiplex Protocol) session state, frontend side.
//!
//! The wire grammar lives in `rio_backend::ansi::rmx`; this module owns
//! what the spec calls buffer semantics: which virtual buffers exist
//! per owning pane, their credit accounts, and the parser that turns
//! `w` payloads into grid updates.
//!
//! Spec: `specs/rio-multiplex-protocol.md`.

use rio_backend::ansi::rmx::{BufferKey, RmxCaps};
use rio_backend::performer::handler::Processor;
use rustc_hash::FxHashMap;

/// Capabilities this frontend advertises to a stream running at
/// `depth` (spec §4): `0` is a pane's own PTY, `1` a virtual buffer
/// opened from it, and so on.
///
/// A stream at the depth cap can host nothing, so it advertises the
/// empty capability set the spec reserves for exactly that (§4): the
/// terminal knows rmx, currently offers nothing, every `o` is
/// rejected. `nest` says whether buffers opened from this stream may
/// in turn host buffers of their own, which needs two more levels.
pub fn caps_at_depth(depth: u8) -> RmxCaps {
    if depth >= MAX_NEST_DEPTH {
        return RmxCaps {
            max_buffers: MAX_BUFFERS,
            initial_credit: INITIAL_CREDIT,
            ..Default::default()
        };
    }
    RmxCaps {
        core: true,
        raw: true,
        layout: true,
        nest: depth + 2 <= MAX_NEST_DEPTH,
        scrollback: true,
        max_buffers: MAX_BUFFERS,
        initial_credit: INITIAL_CREDIT,
    }
}

/// Buffers one owning pane may hold open at once (spec §4 `max`).
pub const MAX_BUFFERS: u16 = 16;

/// How deep buffers may nest (spec §11: RECOMMENDED at least 2). A
/// buffer opened from a pane's own PTY has depth 1, one opened from
/// inside that buffer's content has depth 2, and that is the end.
pub const MAX_NEST_DEPTH: u8 = 2;

/// Per-buffer starting credit in bytes (spec §9).
pub const INITIAL_CREDIT: u32 = 262_144;

/// Grants are batched until this many bytes are owed, a quarter of the
/// window, so an app always has room in flight while it waits.
pub const GRANT_THRESHOLD: u32 = INITIAL_CREDIT / 4;

/// A virtual buffer: a real pane with a grid, no PTY of its own.
pub struct RmxBuffer {
    /// Route of the Context rendering this buffer.
    pub route_id: usize,
    /// Route of the real pane at the bottom of the owner chain. Nested
    /// buffers count against that pane's quota (spec §11).
    pub root_owner: usize,
    /// Parser feeding this buffer's grid; per buffer so sync-update
    /// and partial-sequence state stay isolated (spec §12).
    pub parser: Processor,
    /// Remaining write credit in bytes.
    pub credit: u32,
    /// Rendered bytes not yet acknowledged with an `a` frame. macOS pty
    /// reads cap near 200 bytes, so granting per read costs one reverse
    /// frame per read; batching keeps that overhead off the wire.
    pub pending_grant: u32,
}

/// Every buffer owned by one pane (the pane whose PTY carries the
/// rmx frames).
#[derive(Default)]
pub struct RmxSession {
    buffers: FxHashMap<String, RmxBuffer>,
}

impl RmxSession {
    pub fn get_mut(&mut self, key: &BufferKey) -> Option<&mut RmxBuffer> {
        self.buffers.get_mut(key.as_str())
    }

    pub fn insert(&mut self, key: &BufferKey, buffer: RmxBuffer) {
        self.buffers.insert(key.as_str().to_owned(), buffer);
    }

    pub fn remove(&mut self, key: &BufferKey) -> Option<RmxBuffer> {
        self.buffers.remove(key.as_str())
    }

    /// Route ids of every buffer, for teardown (spec §11).
    pub fn route_ids(&self) -> Vec<usize> {
        self.buffers.values().map(|b| b.route_id).collect()
    }

    pub fn keys_and_routes(&self) -> Vec<(String, usize)> {
        self.buffers
            .iter()
            .map(|(k, b)| (k.clone(), b.route_id))
            .collect()
    }
}

impl RmxState {
    /// Buffers charged to one real pane, nested ones included: the
    /// quota a further `o` is measured against (spec §11).
    pub fn count_for_root(&self, root_owner: usize) -> usize {
        self.sessions
            .values()
            .flat_map(|s| s.buffers.values())
            .filter(|b| b.root_owner == root_owner)
            .count()
    }

    /// Route of the buffer named `key` anywhere under one real pane, so
    /// an `at=` hint can point at a sibling, an uncle, or a buffer of a
    /// buffer (spec §5.1).
    pub fn route_in_tree(&self, root_owner: usize, key: &BufferKey) -> Option<usize> {
        self.sessions.values().find_map(|s| {
            s.buffers
                .get(key.as_str())
                .filter(|b| b.root_owner == root_owner)
                .map(|b| b.route_id)
        })
    }
}

/// Frames for a focus crossing between two panes, in emit order: the
/// buffer being left reports `state=out`, the one entered `state=in`
/// (spec §8). Plain panes are `None` and produce nothing; staying
/// inside one buffer produces nothing either.
pub fn focus_transition(
    prev: Option<&RmxLink>,
    now: Option<&RmxLink>,
) -> Vec<(usize, String)> {
    use rio_backend::ansi::rmx::format_event_focus;
    let same = match (prev, now) {
        (Some(a), Some(b)) => a.owner_route == b.owner_route && a.key == b.key,
        (None, None) => true,
        _ => false,
    };
    if same {
        return Vec::new();
    }
    let mut out = Vec::new();
    if let Some(prev) = prev {
        out.push((prev.owner_route, format_event_focus(&prev.key, false)));
    }
    if let Some(now) = now {
        out.push((now.owner_route, format_event_focus(&now.key, true)));
    }
    out
}

/// The corpse of a buffer closed with `disp=scrollback`: its final
/// screen as plain annotated lines for the owner's scrollback
/// (spec §10.1, §16.4 leaves the annotation to the implementation).
///
/// Every control character is dropped and trailing blank lines are
/// trimmed: a dying buffer must not be able to paint escapes, move the
/// cursor, or open frames in the pane that outlives it.
pub fn corpse_lines(key: &BufferKey, screen: &str) -> Vec<String> {
    let mut lines: Vec<String> = screen
        .lines()
        .map(|line| {
            line.chars()
                .filter(|c| !c.is_control())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect();
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    let mut out = Vec::with_capacity(lines.len() + 2);
    out.push(format!("--- rmx buffer {} ---", key.as_str()));
    out.append(&mut lines);
    out.push(format!("--- end of {} ---", key.as_str()));
    out
}

/// All rmx sessions in one window, keyed by the owning pane's route.
#[derive(Default)]
pub struct RmxState {
    sessions: FxHashMap<usize, RmxSession>,
}

impl RmxState {
    pub fn session_mut(&mut self, owner_route: usize) -> &mut RmxSession {
        self.sessions.entry(owner_route).or_default()
    }

    pub fn session(&self, owner_route: usize) -> Option<&RmxSession> {
        self.sessions.get(&owner_route)
    }

    /// Drop an owner's session, returning its buffers' routes so the
    /// caller can close the panes (spec §11 teardown).
    pub fn take_session(&mut self, owner_route: usize) -> Vec<usize> {
        self.sessions
            .remove(&owner_route)
            .map(|s| s.route_ids())
            .unwrap_or_default()
    }

    /// Drop an owner's buffers and everything nested inside them,
    /// returning every route so the caller can close the panes. A
    /// buffer's sub-buffers cannot outlive the stream that carried
    /// them (spec §11).
    pub fn take_tree(&mut self, owner_route: usize) -> Vec<usize> {
        let mut routes = Vec::new();
        let mut pending = vec![owner_route];
        while let Some(owner) = pending.pop() {
            for route in self.take_session(owner) {
                pending.push(route);
                routes.push(route);
            }
        }
        routes
    }

    /// Owning routes with live sessions, for the user veto.
    pub fn owner_routes(&self) -> Vec<usize> {
        self.sessions.keys().copied().collect()
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }
}

/// Link stored on a virtual buffer's Context: where its input goes.
#[derive(Debug, Clone)]
pub struct RmxLink {
    /// Route of the pane whose stream carries this buffer's frames.
    /// It is a real pane at depth 1 and another buffer when nested.
    pub owner_route: usize,
    /// This buffer's key, for tagging input events.
    pub key: BufferKey,
    /// 1 for a buffer of a real pane, +1 per nesting level (spec §11).
    pub depth: u8,
    /// Route of the real pane at the bottom of the owner chain.
    pub root_owner: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(s: &str) -> BufferKey {
        BufferKey::parse(s.as_bytes()).unwrap()
    }

    fn link(owner: usize, k: &str, depth: u8, root: usize) -> RmxLink {
        RmxLink {
            owner_route: owner,
            key: key(k),
            depth,
            root_owner: root,
        }
    }

    fn buffer(route_id: usize, root_owner: usize) -> RmxBuffer {
        RmxBuffer {
            route_id,
            root_owner,
            parser: Default::default(),
            credit: INITIAL_CREDIT,
            pending_grant: 0,
        }
    }

    #[test]
    fn primary_stream_advertises_every_capability() {
        let caps = caps_at_depth(0);
        assert!(caps.core && caps.raw && caps.layout && caps.scrollback);
        assert!(caps.nest, "a pane's buffers may host buffers of their own");
        assert_eq!(caps.max_buffers, MAX_BUFFERS);
    }

    #[test]
    fn capabilities_shrink_with_nesting_depth() {
        // A buffer can still host children, but they are the last level,
        // so it does not promise `nest` to whatever runs inside it.
        let one = caps_at_depth(1);
        assert!(one.core && one.layout && one.scrollback);
        assert!(!one.nest);
        // At the cap the empty set says "rmx known, nothing offered".
        let two = caps_at_depth(MAX_NEST_DEPTH);
        assert!(!two.core && !two.raw && !two.layout && !two.nest && !two.scrollback);
        assert_eq!(
            rio_backend::ansi::rmx::format_support_reply(&two),
            format!(
                "\x1b_rmx;s;v=1;cap=;max={MAX_BUFFERS};credit={INITIAL_CREDIT}\x1b\\"
            )
        );
    }

    #[test]
    fn nested_buffers_count_against_the_owning_pane() {
        let mut state = RmxState::default();
        // Two buffers on pane 1, one buffer nested inside the first.
        state.session_mut(1).insert(&key("a"), buffer(10, 1));
        state.session_mut(1).insert(&key("b"), buffer(11, 1));
        state.session_mut(10).insert(&key("c"), buffer(12, 1));
        // A different pane keeps its own account.
        state.session_mut(2).insert(&key("d"), buffer(13, 2));

        assert_eq!(state.count_for_root(1), 3);
        assert_eq!(state.count_for_root(2), 1);
        assert_eq!(state.count_for_root(99), 0);
    }

    #[test]
    fn teardown_takes_nested_buffers_with_it() {
        let mut state = RmxState::default();
        // Pane 1 owns `a`; `a` owns `c`; `c` owns `d` (depth 2 is the
        // cap, so this is the deepest tree that can exist).
        state.session_mut(1).insert(&key("a"), buffer(10, 1));
        state.session_mut(10).insert(&key("c"), buffer(12, 1));
        state.session_mut(12).insert(&key("d"), buffer(13, 1));
        state.session_mut(2).insert(&key("other"), buffer(20, 2));

        let mut routes = state.take_tree(1);
        routes.sort();
        assert_eq!(routes, vec![10, 12, 13]);
        assert_eq!(state.count_for_root(1), 0);
        // A different pane's session is untouched.
        assert_eq!(state.count_for_root(2), 1);
        // Taking twice is harmless.
        assert!(state.take_tree(1).is_empty());
    }

    #[test]
    fn at_hint_resolves_anywhere_under_the_same_pane() {
        let mut state = RmxState::default();
        state.session_mut(1).insert(&key("a"), buffer(10, 1));
        state.session_mut(10).insert(&key("c"), buffer(12, 1));
        state.session_mut(2).insert(&key("d"), buffer(13, 2));

        assert_eq!(state.route_in_tree(1, &key("a")), Some(10));
        assert_eq!(state.route_in_tree(1, &key("c")), Some(12));
        // Never across panes: buffer state must not leak (spec §11).
        assert_eq!(state.route_in_tree(1, &key("d")), None);
        assert_eq!(state.route_in_tree(1, &key("nope")), None);
    }

    #[test]
    fn focus_transition_reports_both_edges() {
        let a = link(1, "a", 1, 1);
        let b = link(1, "b", 1, 1);
        assert_eq!(
            focus_transition(Some(&a), Some(&b)),
            vec![
                (1, "\x1b_rmx;i;k=a;ev=focus;state=out\x1b\\".to_owned()),
                (1, "\x1b_rmx;i;k=b;ev=focus;state=in\x1b\\".to_owned()),
            ]
        );
    }

    #[test]
    fn focus_transition_at_the_plain_pane_boundary() {
        let a = link(1, "a", 1, 1);
        assert_eq!(
            focus_transition(None, Some(&a)),
            vec![(1, "\x1b_rmx;i;k=a;ev=focus;state=in\x1b\\".to_owned())]
        );
        assert_eq!(
            focus_transition(Some(&a), None),
            vec![(1, "\x1b_rmx;i;k=a;ev=focus;state=out\x1b\\".to_owned())]
        );
    }

    #[test]
    fn focus_transition_is_silent_without_a_crossing() {
        let a = link(1, "a", 1, 1);
        let same = link(1, "a", 1, 1);
        assert!(focus_transition(Some(&a), Some(&same)).is_empty());
        assert!(focus_transition(None, None).is_empty());
        // Same key under a different owner is a different buffer.
        let other = link(2, "a", 1, 2);
        assert_eq!(focus_transition(Some(&a), Some(&other)).len(), 2);
    }

    #[test]
    fn corpse_is_annotated_and_trimmed() {
        let lines = corpse_lines(&key("build"), "make: ok   \n\n\n");
        assert_eq!(
            lines,
            vec![
                "--- rmx buffer build ---".to_owned(),
                "make: ok".to_owned(),
                "--- end of build ---".to_owned(),
            ]
        );
    }

    #[test]
    fn corpse_cannot_carry_escapes_into_the_owner() {
        let lines = corpse_lines(&key("x"), "a\x1b[31mred\x07\x1b_rmx;q\x1b\\b");
        assert_eq!(lines.len(), 3);
        assert!(
            !lines[1].contains('\x1b'),
            "escapes stripped: {:?}",
            lines[1]
        );
        // What is left is inert text: the APC introducer and the BEL are
        // gone, so the corpse cannot open a frame in the owner's grid.
        assert_eq!(lines[1], "a[31mred_rmx;q\\b");
    }
}
