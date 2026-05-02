//! `NSGlassEffectView` wrapper for macOS 26 (Tahoe) liquid-glass blur.
//!
//! `NSGlassEffectView` is a brand-new AppKit class — `objc2-app-kit`
//! at the version this crate is pinned to doesn't expose it, so we
//! reach for the runtime directly. The class is looked up by name and
//! returns `None` on macOS < 26 (or when running against an older
//! AppKit), letting `set_blur` fall back to the CGS system-blur path.
//!
//! View-hierarchy contract:
//!
//! - In non-glass modes the `NSWindow.contentView` is `WinitView`
//!   directly (status quo).
//! - In glass mode `NSWindow.contentView` is an `NSGlassEffectView`
//!   and `glass.contentView` is `WinitView`. The glass paints the
//!   blurred backdrop; `WinitView`'s `CAMetalLayer` (which we already
//!   flip non-opaque via `Sugarloaf::set_window_opaque`) composites
//!   on top.
//!
//! ## Lifetime hygiene
//!
//! Installing glass relocates `WinitView` from the window's
//! `contentView` slot into the glass's `contentView` slot. AppKit
//! retains the view in its new home before releasing the old slot, so
//! a Retained<WinitView> stashed elsewhere stays valid throughout.
//! The reverse path (uninstall) re-parents `WinitView` back to the
//! window before dropping the glass, in the same retain-then-release
//! order.

use objc2::rc::{Allocated, Retained};
use objc2::runtime::AnyClass;
use objc2::{msg_send, msg_send_id};
use objc2_app_kit::{NSColor, NSView};

/// Maps to `NSGlassEffectViewStyle` raw values from AppKit. Apple
/// docs:
///
/// - `Regular = 0` — standard glass with some opacity.
/// - `Clear = 1` — highly transparent glass, content shows through
///   more.
#[derive(Clone, Copy, Debug)]
pub(crate) enum GlassStyle {
    Regular,
    Clear,
}

impl GlassStyle {
    fn raw(self) -> isize {
        match self {
            GlassStyle::Regular => 0,
            GlassStyle::Clear => 1,
        }
    }
}

/// Strongly-typed handle to an `NSGlassEffectView` instance. Owns the
/// Retained reference; dropping it releases the underlying NSView.
#[derive(Debug)]
pub(crate) struct GlassEffect {
    /// Stored as `Retained<NSView>` because we don't have a generated
    /// `NSGlassEffectView` Rust type — `NSView` is the closest
    /// supertype `objc2-app-kit` ships, and the runtime calls below
    /// don't need the leaf type.
    view: Retained<NSView>,
}

impl GlassEffect {
    /// `true` iff `NSGlassEffectView` is registered with the Objective-C
    /// runtime. Returns `false` on macOS < 26 / older AppKit and on
    /// platforms that don't link AppKit. Stable across the process —
    /// callers can cache the result.
    #[inline]
    pub(crate) fn class_available() -> bool {
        AnyClass::get("NSGlassEffectView").is_some()
    }

    /// Allocate and `-init` a fresh `NSGlassEffectView`. Returns
    /// `None` if the class isn't available at runtime.
    pub(crate) fn new() -> Option<Self> {
        let cls = AnyClass::get("NSGlassEffectView")?;
        // SAFETY: NSGlassEffectView's `+alloc` / `-init` are the
        // standard NSObject lifecycle methods inherited from NSView;
        // they return a +1 retained instance with no in-band errors.
        // objc2's `msg_send_id!` requires the
        // `+alloc → Allocated<T>` / `-init → Retained<T>` rituals;
        // both are stable across all NSView subclasses.
        let view: Retained<NSView> = unsafe {
            let alloc: Allocated<NSView> = msg_send_id![cls, alloc];
            msg_send_id![alloc, init]
        };
        Some(GlassEffect { view })
    }

    /// Set `NSGlassEffectViewStyle` (regular vs clear).
    pub(crate) fn set_style(&self, style: GlassStyle) {
        let raw = style.raw();
        // SAFETY: `setStyle:` accepts an `NSGlassEffectViewStyle`
        // (NSInteger). Passing 0 / 1 matches the documented raw
        // values; out-of-range writes are clamped by AppKit.
        unsafe {
            let _: () = msg_send![&*self.view, setStyle: raw];
        }
    }

    /// Tint the glass with `bg × opacity` — sets `tintColor` to the
    /// host bg colour with its alpha channel multiplied by the
    /// configured `window.opacity`. Without the opacity multiply, an
    /// opaque-bg tint masks the blur entirely on `macos-glass-clear`.
    pub(crate) fn set_tint_color_with_opacity(&self, color: &NSColor, opacity: f64) {
        let opacity = opacity.clamp(0.0, 1.0);
        // SAFETY: `colorWithAlphaComponent:` returns an autoreleased
        // NSColor copy; `msg_send_id!` retains it for our use.
        unsafe {
            let tinted: Retained<NSColor> =
                msg_send_id![color, colorWithAlphaComponent: opacity];
            let _: () = msg_send![&*self.view, setTintColor: &*tinted];
        }
    }

    /// Set the glass's corner radius in points. Pass the host
    /// window's `_cornerRadius` so the glass matches the rounded
    /// window chrome — without this, the glass paints to its square
    /// bounds and a rim of un-blurred pixels appears at the rounded
    /// corners.
    pub(crate) fn set_corner_radius(&self, radius: f64) {
        // SAFETY: `setCornerRadius:` is a CGFloat (f64 on x86_64
        // / aarch64 macOS) setter; out-of-range values are clamped
        // by AppKit.
        unsafe {
            let _: () = msg_send![&*self.view, setCornerRadius: radius];
        }
    }

    /// Install the given view as the glass's contained content. The
    /// glass renders behind / under it; the content view's own
    /// translucent regions show the glass through.
    pub(crate) fn set_content_view(&self, content: &NSView) {
        // SAFETY: `setContentView:` adopts the view as a subview of
        // the glass and retains it.
        unsafe {
            let _: () = msg_send![&*self.view, setContentView: content];
        }
    }

    /// Borrow a Retained reference to the glass's current contentView
    /// — typically `WinitView`. Used at uninstall time to recover the
    /// inner view before reparenting it back onto the NSWindow, and
    /// by `WindowDelegate::view()` to keep the existing
    /// `Retained<WinitView>` accessor working in glass mode.
    pub(crate) fn content_view(&self) -> Option<Retained<NSView>> {
        // SAFETY: `-contentView` is a standard `+0` getter; the
        // `msg_send_id!` flavour with `Option<Retained<_>>` handles
        // the nil / non-nil + retain semantics correctly.
        unsafe { msg_send_id![&*self.view, contentView] }
    }

    /// Borrow the glass as an `NSView` for use as the host window's
    /// contentView slot.
    pub(crate) fn as_ns_view(&self) -> &NSView {
        &self.view
    }
}
