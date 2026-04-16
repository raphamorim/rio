//! DwmFlush-driven vsync worker that posts a tick message to the
//! event loop's `thread_msg_target` per composition cycle.
//!
//! Modelled on zed's `crates/gpui_windows/src/vsync.rs` +
//! `begin_vsync_thread` in `crates/gpui_windows/src/platform.rs`.
//! The handler in `event_loop::thread_event_target_callback` decides
//! per tick whether to fan out a `RedrawRequested` to visible
//! windows, gated by `EventLoopRunner::should_present_after_input`
//! (1 s window, matches macOS).
//!
//! When DWM is disabled, when the monitor is unplugged, or under
//! some RDP modes, `DwmFlush` returns immediately. We detect that
//! via a 1 ms threshold and fall back to `thread::sleep` at the
//! queried refresh interval.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{HWND, S_OK};
use windows_sys::Win32::Graphics::Dwm::{
    DwmFlush, DwmGetCompositionTimingInfo, DWM_TIMING_INFO,
};
use windows_sys::Win32::System::Performance::QueryPerformanceFrequency;
use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, RegisterWindowMessageA};

const VSYNC_INTERVAL_THRESHOLD: Duration = Duration::from_millis(1);
const DEFAULT_VSYNC_INTERVAL: Duration = Duration::from_micros(16_666); // ~60Hz

/// Custom window message posted from the worker thread to
/// `thread_msg_target` once per vsync. The event-loop side
/// dispatches it in `thread_event_target_callback`.
pub(super) static VSYNC_TICK_MSG_ID: LazyVsyncMsgId =
    LazyVsyncMsgId::new("Winit::VsyncTick\0");

/// Lazy `RegisterWindowMessageA` wrapper. Mirrors the
/// `LazyMessageId` pattern in `event_loop.rs` but kept here to
/// avoid widening the visibility of that type.
pub(super) struct LazyVsyncMsgId {
    id: AtomicU32,
    name: &'static str,
}

impl LazyVsyncMsgId {
    pub(super) const fn new(name: &'static str) -> Self {
        Self {
            id: AtomicU32::new(0),
            name,
        }
    }

    pub(super) fn get(&self) -> u32 {
        let id = self.id.load(Ordering::Relaxed);
        if id != 0 {
            return id;
        }
        assert!(self.name.ends_with('\0'));
        let new_id = unsafe { RegisterWindowMessageA(self.name.as_ptr()) };
        assert_ne!(
            new_id, 0,
            "RegisterWindowMessageA failed for '{}'",
            self.name
        );
        self.id.store(new_id, Ordering::Relaxed);
        new_id
    }
}

/// Send-able HWND wrapper. HWND is `*mut c_void` (`!Send`), and
/// edition-2021 disjoint-capture would otherwise capture the inner
/// pointer field directly into the worker closure. Stashing the
/// raw bits as `usize` sidesteps that and keeps the cast local.
#[derive(Clone, Copy)]
struct SendHwnd(usize);

impl SendHwnd {
    fn new(hwnd: HWND) -> Self {
        Self(hwnd as usize)
    }

    fn raw(self) -> HWND {
        self.0 as HWND
    }
}

// SAFETY: HWND is treated as opaque by the worker — it is only
// used as the destination of `PostMessageW`, which is documented
// to be thread-safe.
unsafe impl Send for SendHwnd {}

/// Owns the worker thread. Drop signals stop and joins.
pub(super) struct VSyncThread {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl VSyncThread {
    /// A no-op `VSyncThread` used to swap out the real worker
    /// during `EventLoop::drop` so that joining can happen before
    /// the target window is destroyed.
    pub(super) fn stub() -> Self {
        Self {
            stop: Arc::new(AtomicBool::new(true)),
            handle: None,
        }
    }

    pub(super) fn spawn(thread_msg_target: HWND) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_worker = stop.clone();
        let target = SendHwnd::new(thread_msg_target);
        let tick_msg = VSYNC_TICK_MSG_ID.get();

        let handle = std::thread::Builder::new()
            .name("rio-window::vsync".to_owned())
            .spawn(move || {
                let provider = VSyncProvider::new();
                while !stop_worker.load(Ordering::Acquire) {
                    provider.wait_for_vsync();
                    if stop_worker.load(Ordering::Acquire) {
                        break;
                    }
                    // SAFETY: PostMessageW with a valid registered
                    // message id is sound; failure means the target
                    // window has been destroyed, so we exit.
                    let posted = unsafe { PostMessageW(target.raw(), tick_msg, 0, 0) };
                    if posted == 0 {
                        break;
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
            // The worker exits at the start of the next iteration
            // after the current DwmFlush returns (typically <16 ms).
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
        // DwmFlush returns immediately when DWM is disabled, when
        // the monitor is asleep / unplugged, or under some RDP
        // modes. The 1 ms threshold catches that and we sleep the
        // queried refresh interval as a fallback. Same heuristic
        // zed uses in vsync.rs:51.
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
    // qpcRefreshPeriod is sometimes spuriously low (a value of 60
    // ticks → 29 microseconds was observed in zed). Fall back to
    // the rateRefresh ratio when that happens.
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
