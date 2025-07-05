//! GCD (Grand Central Dispatch) bindings and utilities
//!
//! This module provides safe Rust bindings for macOS Grand Central Dispatch,
//! for VSync synchronization in Rio's rendering pipeline.

use std::ptr::addr_of;

/// Generated GCD dispatch bindings
pub(crate) mod dispatch_sys {
    #![allow(non_upper_case_globals, non_camel_case_types, non_snake_case)]
    include!(concat!(env!("OUT_DIR"), "/dispatch_sys.rs"));
}

pub use dispatch_sys::*;

/// Get the main dispatch queue (equivalent to dispatch_get_main_queue())
pub(crate) fn dispatch_get_main_queue() -> dispatch_queue_t {
    addr_of!(_dispatch_main_q) as *const _ as dispatch_queue_t
}
