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

/// Capabilities this frontend advertises (spec §4).
pub fn caps() -> RmxCaps {
    RmxCaps {
        core: true,
        raw: true,
        layout: false,
        nest: false,
        scrollback: false,
        max_buffers: MAX_BUFFERS,
        initial_credit: INITIAL_CREDIT,
    }
}

/// Buffers one owning pane may hold open at once (spec §4 `max`).
pub const MAX_BUFFERS: u16 = 16;

/// Per-buffer starting credit in bytes (spec §9).
pub const INITIAL_CREDIT: u32 = 262_144;

/// A virtual buffer: a real pane with a grid, no PTY of its own.
pub struct RmxBuffer {
    /// Route of the Context rendering this buffer.
    pub route_id: usize,
    /// Parser feeding this buffer's grid; per buffer so sync-update
    /// and partial-sequence state stay isolated (spec §12).
    pub parser: Processor,
    /// Remaining write credit in bytes.
    pub credit: u32,
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

    pub fn contains(&self, key: &BufferKey) -> bool {
        self.buffers.contains_key(key.as_str())
    }

    pub fn len(&self) -> usize {
        self.buffers.len()
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

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }
}

/// Link stored on a virtual buffer's Context: where its input goes.
#[derive(Debug, Clone)]
pub struct RmxLink {
    /// Route of the pane whose PTY carries this buffer's frames.
    pub owner_route: usize,
    /// This buffer's key, for tagging input events.
    pub key: BufferKey,
}
