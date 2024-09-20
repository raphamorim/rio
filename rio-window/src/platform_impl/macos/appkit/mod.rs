//! Safe bindings for the AppKit framework.
//!
//! These are split out from the rest of `winit` to make safety easier to review.
//! In the future, these should probably live in another crate like `cacao`.
//!
//! TODO: Main thread safety.
// Objective-C methods have different conventions, and it's much easier to
// understand if we just use the same names
#![allow(non_snake_case)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::enum_variant_names)]
#![allow(non_upper_case_globals)]

mod appearance;
mod application;
mod button;
mod color;
mod control;
mod cursor;
mod event;
mod image;
mod menu;
mod menu_item;
mod pasteboard;
mod responder;
mod screen;
mod tab_group;
mod text_input_client;
mod text_input_context;
mod version;
mod view;
mod window;

pub(crate) use self::appearance::NSAppearance;
pub(crate) use self::application::{
    NSApp, NSApplication, NSApplicationActivationPolicy, NSApplicationPresentationOptions,
    NSRequestUserAttentionType,
};
pub(crate) use self::button::NSButton;
pub(crate) use self::color::NSColor;
pub(crate) use self::control::NSControl;
pub(crate) use self::cursor::NSCursor;
#[allow(unused_imports)]
pub(crate) use self::event::{
    NSEvent, NSEventModifierFlags, NSEventPhase, NSEventSubtype, NSEventType,
};
pub(crate) use self::image::NSImage;
pub(crate) use self::menu::NSMenu;
pub(crate) use self::menu_item::NSMenuItem;
pub(crate) use self::pasteboard::{NSFilenamesPboardType, NSPasteboard, NSPasteboardType};
pub(crate) use self::responder::NSResponder;
#[allow(unused_imports)]
pub(crate) use self::screen::{NSDeviceDescriptionKey, NSScreen};
pub(crate) use self::tab_group::NSWindowTabGroup;
pub(crate) use self::text_input_client::NSTextInputClient;
pub(crate) use self::text_input_context::NSTextInputContext;
pub(crate) use self::version::NSAppKitVersion;
pub(crate) use self::view::{NSTrackingRectTag, NSView};
pub(crate) use self::window::{
    NSBackingStoreType, NSWindow, NSWindowButton, NSWindowLevel, NSWindowOcclusionState,
    NSWindowOrderingMode, NSWindowSharingType, NSWindowStyleMask, NSWindowTabbingMode,
    NSWindowTitleVisibility,
};

#[link(name = "AppKit", kind = "framework")]
extern "C" {}
