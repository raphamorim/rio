use futures::channel::mpsc::{self, UnboundedReceiver, UnboundedSender};
use std::future::Future;
use std::sync::Arc;
use tokio::runtime::{Builder, Runtime};
use tokio::sync::Mutex as TokioMutex;

/// Async task manager for Rio terminal operations
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
}

impl AsyncExecutor {
    pub fn new() -> Self {
        // Create a multi-threaded tokio runtime with 2 worker threads
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

    /// Get a sender for UI updates (for use in background tasks)
    pub fn ui_sender(&self) -> UnboundedSender<UiUpdate> {
        self.ui_sender.clone()
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
