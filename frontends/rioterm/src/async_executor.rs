use futures::channel::mpsc::{self, UnboundedReceiver, UnboundedSender};
use std::future::Future;
use std::sync::Arc;
use tokio::runtime::{Builder, Runtime};
use tokio::sync::Mutex as TokioMutex;

/// Async task manager for Rio terminal operations
/// Inspired by Zed's architecture but simplified for Phase 1
pub struct AsyncExecutor {
    /// Tokio runtime for heavy I/O operations
    runtime: Arc<Runtime>,
    /// Channel for sending UI update requests back to main thread
    ui_sender: UnboundedSender<UiUpdate>,
    /// Channel for receiving UI updates in main thread
    pub ui_receiver: Arc<TokioMutex<UnboundedReceiver<UiUpdate>>>,
}

/// UI update messages sent from background tasks to main thread
#[derive(Debug)]
pub enum UiUpdate {
    /// Request redraw for specific window
    RequestRedraw(usize),
    /// Terminal parsing completed
    TerminalParseComplete { route_id: usize, needs_redraw: bool },
    /// Damage check completed
    DamageCheckComplete { route_id: usize, has_damage: bool },
}

impl AsyncExecutor {
    pub fn new() -> Self {
        // Create a multi-threaded tokio runtime with 2 worker threads
        // (following Zed's pattern)
        let runtime = Arc::new(
            Builder::new_multi_thread()
                .worker_threads(2)
                .thread_name("rio-async")
                .enable_all()
                .build()
                .expect("Failed to create tokio runtime"),
        );

        let (ui_sender, ui_receiver) = mpsc::unbounded();

        Self {
            runtime,
            ui_sender,
            ui_receiver: Arc::new(TokioMutex::new(ui_receiver)),
        }
    }

    /// Spawn a background task that doesn't need to update UI
    pub fn spawn_background<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.runtime.spawn(future);
    }

    /// Spawn a background task that may need to send UI updates
    pub fn spawn_with_ui_updates<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.runtime.spawn(async move {
            future.await;
            // Task completed - could send completion notification if needed
        });
    }

    /// Get a sender for UI updates (for use in background tasks)
    pub fn ui_sender(&self) -> UnboundedSender<UiUpdate> {
        self.ui_sender.clone()
    }

    /// Process pending UI updates (called from main thread)
    pub async fn process_ui_updates<F>(&self, mut handler: F)
    where
        F: FnMut(UiUpdate),
    {
        let mut receiver = self.ui_receiver.lock().await;
        while let Ok(Some(update)) = receiver.try_next() {
            handler(update);
        }
    }

    /// Spawn a terminal parsing task
    pub fn spawn_terminal_parse<F>(&self, route_id: usize, parse_fn: F)
    where
        F: FnOnce() -> bool + Send + 'static,
    {
        let sender = self.ui_sender.clone();
        self.runtime.spawn(async move {
            // Run the parsing function in a blocking task to avoid blocking the async runtime
            let needs_redraw =
                tokio::task::spawn_blocking(parse_fn).await.unwrap_or(false);

            // Send result back to main thread
            let _ = sender.unbounded_send(UiUpdate::TerminalParseComplete {
                route_id,
                needs_redraw,
            });
        });
    }

    /// Spawn a damage check task
    pub fn spawn_damage_check<F>(&self, route_id: usize, check_fn: F)
    where
        F: FnOnce() -> bool + Send + 'static,
    {
        let sender = self.ui_sender.clone();
        self.runtime.spawn(async move {
            // Run the damage check in a blocking task
            let has_damage = tokio::task::spawn_blocking(check_fn).await.unwrap_or(false);

            // Send result back to main thread
            let _ = sender.unbounded_send(UiUpdate::DamageCheckComplete {
                route_id,
                has_damage,
            });
        });
    }

    /// Shutdown the executor
    pub fn shutdown(self) {
        // Extract the runtime from the Arc
        if let Ok(runtime) = Arc::try_unwrap(self.runtime) {
            runtime.shutdown_background();
        }
        // If Arc::try_unwrap fails, there are other references to the runtime
        // In that case, we'll let it shut down naturally when all references are dropped
    }
}

impl Default for AsyncExecutor {
    fn default() -> Self {
        Self::new()
    }
}
