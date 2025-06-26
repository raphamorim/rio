use std::sync::{mpsc::{self, Receiver, Sender}, LazyLock};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tracing::{debug, error, warn};

/// Background operations for font management to prevent main thread blocking
pub struct FontBackgroundOps {
    sender: Sender<FontOperation>,
    _handle: JoinHandle<()>,
}

/// Operations that can be performed in the background
#[derive(Debug)]
pub enum FontOperation {
    /// Release font resources (expensive cleanup)
    ReleaseFontData(Vec<Arc<Vec<u8>>>),
    /// Preload fonts for better cache hit rates
    PreloadFonts(Vec<String>),
    /// Cleanup expired cache entries
    CleanupCache,
    /// Shutdown the background thread
    Shutdown,
}

impl Default for FontBackgroundOps {
    fn default() -> Self {
        Self::new()
    }
}

impl FontBackgroundOps {
    /// Create a new background operations handler
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();

        let handle = thread::Builder::new()
            .name("font-background-ops".to_string())
            .spawn(move || {
                Self::background_worker(receiver);
            })
            .expect("Failed to spawn font background operations thread");

        Self {
            sender,
            _handle: handle,
        }
    }

    /// Send a font operation to be processed in the background
    pub fn send_operation(&self, operation: FontOperation) {
        if let Err(e) = self.sender.send(operation) {
            error!("Failed to send font operation to background thread: {}", e);
        }
    }

    /// Release font data in the background to prevent main thread blocking
    pub fn release_font_data(&self, font_data: Vec<Arc<Vec<u8>>>) {
        if !font_data.is_empty() {
            debug!(
                "Scheduling {} font data releases for background processing",
                font_data.len()
            );
            self.send_operation(FontOperation::ReleaseFontData(font_data));
        }
    }

    /// Preload fonts in the background
    pub fn preload_fonts(&self, font_paths: Vec<String>) {
        if !font_paths.is_empty() {
            debug!(
                "Scheduling {} fonts for background preloading",
                font_paths.len()
            );
            self.send_operation(FontOperation::PreloadFonts(font_paths));
        }
    }

    /// Schedule cache cleanup in the background
    pub fn cleanup_cache(&self) {
        self.send_operation(FontOperation::CleanupCache);
    }

    /// Background worker thread that processes font operations
    fn background_worker(receiver: Receiver<FontOperation>) {
        debug!("Font background operations thread started");

        while let Ok(operation) = receiver.recv() {
            match operation {
                FontOperation::ReleaseFontData(font_data) => {
                    Self::process_font_release(font_data);
                }
                FontOperation::PreloadFonts(font_paths) => {
                    Self::process_font_preload(font_paths);
                }
                FontOperation::CleanupCache => {
                    Self::process_cache_cleanup();
                }
                FontOperation::Shutdown => {
                    debug!("Font background operations thread shutting down");
                    break;
                }
            }
        }

        debug!("Font background operations thread terminated");
    }

    /// Process font data release in background
    fn process_font_release(font_data: Vec<Arc<Vec<u8>>>) {
        let start = std::time::Instant::now();
        let count = font_data.len();

        // Simply dropping the Arc<Vec<u8>> will deallocate the memory
        // This happens in the background thread, not blocking the main thread
        drop(font_data);

        let duration = start.elapsed();
        if duration > Duration::from_millis(10) {
            debug!("Released {} font data entries in {:?}", count, duration);
        }
    }

    /// Process font preloading in background
    fn process_font_preload(font_paths: Vec<String>) {
        let start = std::time::Instant::now();
        let count = font_paths.len();

        for path in &font_paths {
            // This would integrate with the existing font loader
            // For now, just simulate the work
            if let Err(e) = std::fs::metadata(path) {
                warn!("Failed to preload font {}: {}", path, e);
            }
        }

        let duration = start.elapsed();
        debug!("Preloaded {} fonts in {:?}", count, duration);
    }

    /// Process cache cleanup in background
    fn process_cache_cleanup() {
        let start = std::time::Instant::now();

        // This would integrate with the font cache to clean up expired entries
        // For now, just log the operation
        debug!("Performing background cache cleanup");

        let duration = start.elapsed();
        if duration > Duration::from_millis(5) {
            debug!("Cache cleanup completed in {:?}", duration);
        }
    }
}

impl Drop for FontBackgroundOps {
    fn drop(&mut self) {
        // Signal the background thread to shutdown
        let _ = self.sender.send(FontOperation::Shutdown);
    }
}

/// Global background operations instance using LazyLock for thread safety
static BACKGROUND_OPS: LazyLock<FontBackgroundOps> = LazyLock::new(FontBackgroundOps::new);

/// Get the global background operations instance
pub fn get_background_ops() -> &'static FontBackgroundOps {
    &BACKGROUND_OPS
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_background_font_release() {
        let ops = FontBackgroundOps::new();

        // Create some dummy font data
        let font_data = vec![Arc::new(vec![0u8; 1024]), Arc::new(vec![1u8; 2048])];

        ops.release_font_data(font_data);

        // Give the background thread time to process
        thread::sleep(Duration::from_millis(100));
    }

    #[test]
    fn test_background_preload() {
        let ops = FontBackgroundOps::new();

        let font_paths = vec![
            "/System/Library/Fonts/Arial.ttf".to_string(),
            "/System/Library/Fonts/Helvetica.ttc".to_string(),
        ];

        ops.preload_fonts(font_paths);

        // Give the background thread time to process
        thread::sleep(Duration::from_millis(100));
    }
}
