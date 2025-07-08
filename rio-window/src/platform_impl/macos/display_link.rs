//! CVDisplayLink Integration for VSync Synchronization
//!
//! This module implements CVDisplayLink integration to provide precise VSync timing
//! for Rio's rendering system, using Grand Central Dispatch for thread-safe communication.
//!
//! ## Why CVDisplayLink vs NSTimer/Event-Driven Rendering?
//!
//! ### Traditional Approach (Rio's previous method):
//! - Event-driven rendering - fires whenever something changes
//! - request_redraw() calls have irregular timing, not VSync aligned
//!
//! - Or NSTimer-based rendering with approximate 16.67ms timing
//! - Timer callbacks are close but not precise to display refresh
//!
//! ### CVDisplayLink Approach (Rio's new method):
//! - VSync-synchronized rendering - fires exactly when display is ready
//! - CVDisplayLink -> GCD dispatch -> main thread callback -> request_frame()
//!
//! ## Key Benefits:
//!
//! 1. **Hardware VSync Synchronization**: Perfect timing with display refresh
//! 2. **Adaptive Refresh Rate**: 60Hz, 120Hz, ProMotion support
//! 3. **Multi-Display Support**: Automatic adaptation when moving windows
//! 4. **Thread Safety**: GCD handles cross-thread communication safely
//! 5. **Power Efficiency**: Only fires when display refreshes

use std::ffi::c_void;
use std::ptr;

use core_graphics::display::CGDirectDisplayID;

use super::dispatcher::{dispatch_get_main_queue, dispatch_sys::*};
use super::ffi::CVDisplayLinkRelease;
use super::window::WindowId;

/// CVDisplayLink callback function type
pub type CVDisplayLinkOutputCallback = unsafe extern "C" fn(
    display_link: CVDisplayLinkRef,
    current_time: *const CVTimeStamp,
    output_time: *const CVTimeStamp,
    flags_in: i64,
    flags_out: *mut i64,
    user_info: *mut c_void,
) -> i32;

/// CVTimeStamp structure
#[repr(C)]
pub struct CVTimeStamp {
    pub version: u32,
    pub video_time_scale: i32,
    pub video_time: i64,
    pub host_time: u64,
    pub rate_scalar: f64,
    pub video_refresh_period: i64,
    pub smpte_time: CVSMPTETime,
    pub flags: u64,
    pub reserved: u64,
}

/// CVSMPTETime structure
#[repr(C)]
pub struct CVSMPTETime {
    pub subframes: i16,
    pub subframe_divisor: i16,
    pub counter: u32,
    pub type_: u32,
    pub flags: u32,
    pub hours: i16,
    pub minutes: i16,
    pub seconds: i16,
    pub frames: i16,
}

/// Use existing CVDisplayLinkRef from ffi.rs
use super::ffi::CVDisplayLinkRef;

#[link(name = "CoreVideo", kind = "framework")]
extern "C" {
    fn CVDisplayLinkCreateWithActiveCGDisplays(
        display_link_out: *mut CVDisplayLinkRef,
    ) -> i32;
    fn CVDisplayLinkSetCurrentCGDisplay(
        display_link: CVDisplayLinkRef,
        display_id: CGDirectDisplayID,
    ) -> i32;
    fn CVDisplayLinkSetOutputCallback(
        display_link: CVDisplayLinkRef,
        callback: CVDisplayLinkOutputCallback,
        user_info: *mut c_void,
    ) -> i32;
    fn CVDisplayLinkStart(display_link: CVDisplayLinkRef) -> i32;
    fn CVDisplayLinkStop(display_link: CVDisplayLinkRef) -> i32;
}

/// User data for the display link callback
#[repr(C)]
#[derive(Debug)]
pub struct DisplayLinkUserData {
    pub window_id: WindowId,
    pub dispatch_source: dispatch_source_t,
    pub view_ptr: *mut std::ffi::c_void, // Pointer to the view for direct access
}

/// DisplayLink wrapper using GCD-based approach
#[derive(Debug)]
pub struct DisplayLink {
    display_link: CVDisplayLinkRef,
    dispatch_source: dispatch_source_t,
    user_data: Box<DisplayLinkUserData>,
    is_running: std::cell::Cell<bool>,
}

unsafe impl Send for DisplayLink {}
unsafe impl Sync for DisplayLink {}

impl DisplayLink {
    /// Create a new DisplayLink using GCD-based approach
    ///
    /// This provides VSync synchronization with thread-safe communication
    /// via Grand Central Dispatch instead of CFRunLoopSource.
    pub fn new(
        display_id: CGDirectDisplayID,
        window_id: WindowId,
        view_ptr: *mut c_void,
        callback: unsafe extern "C" fn(*mut c_void),
    ) -> Result<Self, &'static str> {
        unsafe {
            // Create GCD dispatch source for main queue communication
            let dispatch_source = dispatch_source_create(
                &_dispatch_source_type_data_add,
                0,
                0,
                dispatch_get_main_queue(),
            );

            if dispatch_source.is_null() {
                return Err("Failed to create GCD dispatch source");
            }

            // Create user data
            let user_data = Box::new(DisplayLinkUserData {
                window_id,
                dispatch_source,
                view_ptr,
            });

            // Set up GCD event handler
            dispatch_set_context(
                super::dispatcher::dispatch_sys::dispatch_object_t {
                    _ds: dispatch_source,
                },
                &*user_data as *const _ as *mut c_void,
            );
            dispatch_source_set_event_handler_f(dispatch_source, Some(callback));

            // Create CVDisplayLink
            let mut display_link: CVDisplayLinkRef = ptr::null_mut();
            let result = CVDisplayLinkCreateWithActiveCGDisplays(&mut display_link);
            if result != 0 {
                dispatch_source_cancel(dispatch_source);
                return Err("Failed to create CVDisplayLink");
            }

            // Set the display
            let result = CVDisplayLinkSetCurrentCGDisplay(display_link, display_id);
            if result != 0 {
                CVDisplayLinkRelease(display_link);
                dispatch_source_cancel(dispatch_source);
                return Err("Failed to set display for CVDisplayLink");
            }

            // Set the VSync callback
            let user_data_ptr = &*user_data as *const DisplayLinkUserData as *mut c_void;
            let result = CVDisplayLinkSetOutputCallback(
                display_link,
                display_link_callback,
                user_data_ptr,
            );

            if result != 0 {
                CVDisplayLinkRelease(display_link);
                dispatch_source_cancel(dispatch_source);
                return Err("Failed to set callback for CVDisplayLink");
            }

            tracing::info!(
                "CVDisplayLink created with GCD for window {:?} on display {}",
                window_id,
                display_id
            );

            Ok(DisplayLink {
                display_link,
                dispatch_source,
                user_data,
                is_running: std::cell::Cell::new(false),
            })
        }
    }

    /// Start VSync-synchronized rendering
    pub fn start(&self) -> Result<(), &'static str> {
        if self.is_running.get() {
            tracing::debug!(
                "Display link already running for window {:?}",
                self.user_data.window_id
            );
            return Ok(());
        }

        unsafe {
            // Resume GCD dispatch source
            dispatch_resume(super::dispatcher::dispatch_sys::dispatch_object_t {
                _ds: self.dispatch_source,
            });

            // Start CVDisplayLink
            let result = CVDisplayLinkStart(self.display_link);
            if result != 0 {
                dispatch_suspend(super::dispatcher::dispatch_sys::dispatch_object_t {
                    _ds: self.dispatch_source,
                });
                Err("Failed to start CVDisplayLink")
            } else {
                self.is_running.set(true);
                tracing::info!(
                    "CVDisplayLink started - VSync callbacks active for window {:?}",
                    self.user_data.window_id
                );
                Ok(())
            }
        }
    }

    /// Stop VSync-synchronized rendering
    pub fn stop(&self) -> Result<(), &'static str> {
        if !self.is_running.get() {
            tracing::debug!(
                "Display link already stopped for window {:?}",
                self.user_data.window_id
            );
            return Ok(());
        }

        unsafe {
            // Stop CVDisplayLink
            let result = CVDisplayLinkStop(self.display_link);

            // Suspend GCD dispatch source
            dispatch_suspend(super::dispatcher::dispatch_sys::dispatch_object_t {
                _ds: self.dispatch_source,
            });

            if result != 0 {
                Err("Failed to stop CVDisplayLink")
            } else {
                self.is_running.set(false);
                tracing::info!(
                    "CVDisplayLink stopped for window {:?}",
                    self.user_data.window_id
                );
                Ok(())
            }
        }
    }
}

impl Drop for DisplayLink {
    fn drop(&mut self) {
        unsafe {
            // Stop first
            let _ = self.stop();

            // Cancel GCD dispatch source
            dispatch_source_cancel(self.dispatch_source);

            // Release CVDisplayLink
            CVDisplayLinkRelease(self.display_link);
        }
    }
}

/// CVDisplayLink callback - runs on dedicated CVDisplayLink thread
///
/// This callback fires at exactly the right time for VSync, then uses
/// GCD to safely communicate with the main thread.
unsafe extern "C" fn display_link_callback(
    _display_link: CVDisplayLinkRef,
    current_time: *const CVTimeStamp,
    output_time: *const CVTimeStamp,
    _flags_in: i64,
    _flags_out: *mut i64,
    user_info: *mut c_void,
) -> i32 {
    if user_info.is_null() {
        return 0;
    }

    unsafe {
        let user_data = &*(user_info as *const DisplayLinkUserData);

        // Extract timing information for debugging
        if tracing::enabled!(tracing::Level::TRACE) {
            let _current_host_time = (*current_time).host_time;
            let _output_host_time = (*output_time).host_time;
            let refresh_period = (*output_time).video_refresh_period;
            let refresh_rate = if refresh_period > 0 {
                (*output_time).video_time_scale as f64 / refresh_period as f64
            } else {
                60.0
            };

            tracing::trace!(
                "VSync callback on CVDisplayLink thread: window={:?}, refresh_rate={:.1}Hz",
                user_data.window_id,
                refresh_rate
            );
        }

        // Signal main thread via GCD - this is the key to the approach
        dispatch_source_merge_data(user_data.dispatch_source, 1);
    }

    0 // Success
}

/// Extension trait for WindowDelegate to support display link integration
pub trait DisplayLinkSupport {
    /// Set up the display link for this window
    fn setup_display_link(&self) -> Result<(), &'static str>;

    /// Start VSync-synchronized rendering
    fn start_display_link(&self) -> Result<(), &'static str>;

    /// Stop VSync-synchronized rendering
    fn stop_display_link(&self) -> Result<(), &'static str>;
}
