use crate::async_executor::{AsyncExecutor, UiUpdate};
use rio_backend::crosswords::Crosswords;
use rio_backend::event::{sync::FairMutex, EventListener};
use std::sync::Arc;

/// Async-safe terminal operations
/// Provides non-blocking alternatives to common terminal operations
pub struct AsyncTerminalOps;

impl AsyncTerminalOps {
    /// Check if terminal has damage and emit events without blocking
    pub fn try_check_and_emit_damage<T: EventListener>(
        terminal: &Arc<FairMutex<Crosswords<T>>>,
        display_offset: usize,
    ) -> Option<bool> {
        terminal.try_lock_unfair().map(|t| {
            let has_damage = t.is_fully_damaged();
            if has_damage {
                t.emit_damage_event(display_offset);
            }
            has_damage
        })
    }

    /// Spawn async damage check and event emission
    pub fn spawn_damage_check_and_emit<T: EventListener + Send + 'static>(
        executor: &AsyncExecutor,
        terminal: Arc<FairMutex<Crosswords<T>>>,
        route_id: usize,
        display_offset: usize,
    ) {
        let ui_sender = executor.ui_sender();
        executor.spawn_background(async move {
            let result = tokio::task::spawn_blocking(move || {
                if let Some(terminal) = terminal.try_lock_unfair() {
                    let has_damage = terminal.is_fully_damaged();
                    if has_damage {
                        terminal.emit_damage_event(display_offset);
                    }
                    has_damage
                } else {
                    false
                }
            })
            .await;

            if result.unwrap_or(false) {
                let _ = ui_sender.unbounded_send(UiUpdate::RequestRedraw(route_id));
            }
        });
    }
}
