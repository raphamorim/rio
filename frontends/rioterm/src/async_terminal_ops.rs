use crate::async_executor::{AsyncExecutor, UiUpdate};
use rio_backend::crosswords::Crosswords;
use rio_backend::event::{sync::FairMutex, EventListener};
use std::sync::Arc;

/// Async-safe terminal operations
/// Provides non-blocking alternatives to common terminal operations
pub struct AsyncTerminalOps;

impl AsyncTerminalOps {
    /// Check if terminal is fully damaged without blocking
    /// Returns None if lock cannot be acquired immediately
    pub fn try_check_damage<T: EventListener>(
        terminal: &Arc<FairMutex<Crosswords<T>>>,
    ) -> Option<bool> {
        terminal.try_lock_unfair().map(|t| t.is_fully_damaged())
    }

    /// Spawn async damage check
    pub fn spawn_damage_check<T: EventListener + Send + 'static>(
        executor: &AsyncExecutor,
        terminal: Arc<FairMutex<Crosswords<T>>>,
        route_id: usize,
    ) {
        executor.spawn_damage_check(route_id, move || {
            if let Some(terminal) = terminal.try_lock_unfair() {
                terminal.is_fully_damaged()
            } else {
                // If we can't get the lock immediately, assume no damage
                // to avoid blocking. The next check will catch it.
                false
            }
        });
    }

    /// Spawn async terminal parsing operation
    pub fn spawn_terminal_parse<T: EventListener + Send + 'static, F>(
        executor: &AsyncExecutor,
        terminal: Arc<FairMutex<Crosswords<T>>>,
        route_id: usize,
        parse_operation: F,
    ) where
        F: FnOnce(&mut Crosswords<T>) -> bool + Send + 'static,
    {
        executor.spawn_terminal_parse(route_id, move || {
            // Try to get the lock with a timeout approach
            match terminal.try_lock_unfair() {
                Some(mut terminal) => parse_operation(&mut terminal),
                None => {
                    // If we can't get the lock, defer the operation
                    // This prevents blocking the async executor
                    false
                }
            }
        });
    }

    /// Perform a quick non-blocking terminal operation
    /// Returns None if the operation cannot be performed immediately
    pub fn try_terminal_operation<T: EventListener, R, F>(
        terminal: &Arc<FairMutex<Crosswords<T>>>,
        operation: F,
    ) -> Option<R>
    where
        F: FnOnce(&mut Crosswords<T>) -> R,
    {
        terminal.try_lock_unfair().map(|mut t| operation(&mut t))
    }

    /// Spawn async selection operation
    pub fn spawn_selection_operation<T: EventListener + Send + 'static, F>(
        executor: &AsyncExecutor,
        terminal: Arc<FairMutex<Crosswords<T>>>,
        route_id: usize,
        selection_op: F,
    ) where
        F: FnOnce(&mut Crosswords<T>) + Send + 'static,
    {
        let ui_sender = executor.ui_sender();
        executor.spawn_background(async move {
            // Use spawn_blocking for potentially blocking operations
            let result = tokio::task::spawn_blocking(move || {
                // Try to get the lock, but don't block indefinitely
                if let Some(mut terminal) = terminal.try_lock_unfair() {
                    selection_op(&mut terminal);
                    true
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

    /// Spawn async scroll operation
    pub fn spawn_scroll_operation<T: EventListener + Send + 'static, F>(
        executor: &AsyncExecutor,
        terminal: Arc<FairMutex<Crosswords<T>>>,
        route_id: usize,
        scroll_op: F,
    ) where
        F: FnOnce(&mut Crosswords<T>) + Send + 'static,
    {
        let ui_sender = executor.ui_sender();
        executor.spawn_background(async move {
            let result = tokio::task::spawn_blocking(move || {
                if let Some(mut terminal) = terminal.try_lock_unfair() {
                    scroll_op(&mut terminal);
                    true
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

/// Extension trait for Arc<FairMutex<Crosswords<T>>> to add async operations
pub trait AsyncTerminalExt<T: EventListener> {
    /// Try to check damage without blocking
    fn try_check_damage(&self) -> Option<bool>;

    /// Spawn async damage check
    fn spawn_damage_check(&self, executor: &AsyncExecutor, route_id: usize)
    where
        T: Send + 'static;
}

impl<T: EventListener> AsyncTerminalExt<T> for Arc<FairMutex<Crosswords<T>>> {
    fn try_check_damage(&self) -> Option<bool> {
        AsyncTerminalOps::try_check_damage(self)
    }

    fn spawn_damage_check(&self, executor: &AsyncExecutor, route_id: usize)
    where
        T: Send + 'static,
    {
        AsyncTerminalOps::spawn_damage_check(executor, self.clone(), route_id);
    }
}
