//! DwmFlush-driven vsync worker that drives all window repaints.
//!
//! Mirrors the macOS CVDisplayLink model: `Window::request_redraw`
//! sets a per-window `Arc<AtomicBool>` dirty flag, and the worker
//! is the single source of frame timing. Per composition cycle it
//! iterates the window registry and, for each dirty window, fires
//! `RedrawWindow(.., RDW_INVALIDATE)`. The app's `WM_PAINT` /
//! `RedrawRequested` path is unchanged.
//!
//! When DWM is disabled, the monitor is unplugged, or under some
//! RDP modes, `DwmFlush` returns immediately. The 1 ms threshold
//! catches that and we fall back to `thread::sleep` at the queried
//! refresh interval. Same heuristic as zed's
//! `gpui_windows/src/vsync.rs`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{HWND, S_OK};
use windows_sys::Win32::Graphics::Dwm::{
    DwmFlush, DwmGetCompositionTimingInfo, DWM_TIMING_INFO,
};
use windows_sys::Win32::Graphics::Gdi::{RedrawWindow, RDW_INVALIDATE};
use windows_sys::Win32::System::Performance::QueryPerformanceFrequency;
use windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible;

const VSYNC_INTERVAL_THRESHOLD: Duration = Duration::from_millis(1);
const DEFAULT_VSYNC_INTERVAL: Duration = Duration::from_micros(16_666); // ~60Hz

/// State shared between the event loop, window-callback thread,
/// and the DwmFlush worker thread. Holds the per-window dirty-flag
/// registry.
pub(crate) struct VSyncSharedState {
    /// HWND (as `usize` for `Hash`/`Eq`) → per-window dirty flag.
    /// `Window::request_redraw` sets the flag; the worker reads
    /// and clears it on each tick.
    windows: RwLock<HashMap<usize, Arc<AtomicBool>>>,
}

impl VSyncSharedState {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            windows: RwLock::new(HashMap::new()),
        })
    }

    /// Insert a window and return its dirty-flag handle. The
    /// caller (the `Window` constructor) keeps the returned `Arc`
    /// so `request_redraw` can flip it without taking the registry
    /// lock.
    pub(crate) fn register_window(&self, hwnd: HWND) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        self.windows
            .write()
            .unwrap()
            .insert(hwnd as usize, flag.clone());
        flag
    }

    pub(crate) fn unregister_window(&self, hwnd: HWND) {
        self.windows.write().unwrap().remove(&(hwnd as usize));
    }

    /// Clear a window's dirty flag without invalidating it. Called by
    /// the `REDRAW_REQUESTED_MSG` handler so the DwmFlush worker's
    /// next tick skips the dirty-path `RedrawWindow` — the paint is
    /// already happening via the custom message, a second `WM_PAINT`
    /// would just dispatch a no-op `RedrawRequested`.
    pub(crate) fn clear_dirty(&self, hwnd: HWND) {
        if let Some(flag) = self.windows.read().unwrap().get(&(hwnd as usize)) {
            flag.store(false, Ordering::Release);
        }
    }

    pub(crate) fn window_count(&self) -> usize {
        self.windows.read().unwrap().len()
    }
}

/// Owns the worker thread. Drop signals stop and joins.
pub(super) struct VSyncThread {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl VSyncThread {
    pub(super) fn spawn(state: Arc<VSyncSharedState>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_worker = stop.clone();

        let handle = std::thread::Builder::new()
            .name("rio-window::vsync".to_owned())
            .spawn(move || {
                let provider = VSyncProvider::new();
                while !stop_worker.load(Ordering::Acquire) {
                    provider.wait_for_vsync();
                    if stop_worker.load(Ordering::Acquire) {
                        break;
                    }

                    // Snapshot HWND + flag pairs so we don't hold
                    // the registry lock across `RedrawWindow`.
                    let snapshot: Vec<(usize, Arc<AtomicBool>)> = state
                        .windows
                        .read()
                        .unwrap()
                        .iter()
                        .map(|(&hwnd, flag)| (hwnd, flag.clone()))
                        .collect();

                    for (hwnd_bits, flag) in snapshot {
                        // Only invalidate when something explicitly asked
                        // for a redraw. The previous
                        // `present_after_input` fallback kept painting at
                        // vblank for a second after every input, which
                        // doubles every paint when the app (rio) already
                        // drives redraws via `REDRAW_REQUESTED_MSG` —
                        // each IME key-repeat tick becomes a paint from
                        // the custom message *and* a paint from the
                        // worker, halving throughput.
                        let was_dirty = flag.swap(false, Ordering::AcqRel);
                        if !was_dirty {
                            continue;
                        }
                        let hwnd = hwnd_bits as HWND;
                        // SAFETY: `IsWindowVisible` and
                        // `RedrawWindow` are documented thread-safe.
                        unsafe {
                            if IsWindowVisible(hwnd) != 0 {
                                RedrawWindow(
                                    hwnd,
                                    std::ptr::null(),
                                    std::ptr::null_mut(),
                                    RDW_INVALIDATE,
                                );
                            }
                        }
                    }
                }
            })
            .expect("failed to spawn rio-window vsync thread");

        Self {
            stop,
            handle: Some(handle),
        }
    }
}

impl Drop for VSyncThread {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            // Worker exits at the start of the next iteration after
            // the current DwmFlush returns (typically <16 ms).
            let _ = handle.join();
        }
    }
}

struct VSyncProvider {
    interval: Duration,
}

impl VSyncProvider {
    fn new() -> Self {
        let interval = query_dwm_interval().unwrap_or(DEFAULT_VSYNC_INTERVAL);
        Self { interval }
    }

    fn wait_for_vsync(&self) {
        let start = Instant::now();
        let hr = unsafe { DwmFlush() };
        let elapsed = start.elapsed();
        if hr != S_OK || elapsed < VSYNC_INTERVAL_THRESHOLD {
            std::thread::sleep(self.interval);
        }
    }
}

fn query_dwm_interval() -> Option<Duration> {
    let mut frequency: i64 = 0;
    if unsafe { QueryPerformanceFrequency(&mut frequency) } == 0 || frequency <= 0 {
        return None;
    }
    let qpc_per_second = frequency as u64;

    let mut info: DWM_TIMING_INFO = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<DWM_TIMING_INFO>() as u32;
    if unsafe { DwmGetCompositionTimingInfo(std::ptr::null_mut(), &mut info) } != S_OK {
        return None;
    }

    let interval = ticks_to_duration(info.qpcRefreshPeriod, qpc_per_second);
    if interval >= VSYNC_INTERVAL_THRESHOLD {
        return Some(interval);
    }
    if info.rateRefresh.uiNumerator == 0 {
        return None;
    }
    Some(ticks_to_duration(
        info.rateRefresh.uiDenominator as u64,
        info.rateRefresh.uiNumerator as u64,
    ))
}

#[inline]
fn ticks_to_duration(counts: u64, ticks_per_second: u64) -> Duration {
    let ticks_per_microsecond = (ticks_per_second / 1_000_000).max(1);
    Duration::from_micros(counts / ticks_per_microsecond)
}
