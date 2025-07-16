//! # macOS / AppKit
//!
//! Winit has an OS requirement of macOS 10.11 or higher (same as Rust
//! itself), and is regularly tested on macOS 10.14.
//!
//! A lot of functionality expects the application to be ready before you
//! start doing anything; this includes creating windows, fetching monitors,
//! drawing, and so on, see issues [#2238], [#2051] and [#2087].
//!
//! If you encounter problems, you should try doing your initialization inside
//! `Event::Resumed`.
//!
//! [#2238]: https://github.com/rust-windowing/winit/issues/2238
//! [#2051]: https://github.com/rust-windowing/winit/issues/2051
//! [#2087]: https://github.com/rust-windowing/winit/issues/2087

use std::os::raw::c_void;

use crate::event_loop::{ActiveEventLoop, EventLoopBuilder};
use crate::monitor::MonitorHandle;
use crate::window::{Window, WindowAttributes};

/// Colorspace options for macOS windows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Colorspace {
    /// Standard sRGB colorspace
    Srgb,
    /// Display P3 wide color gamut
    DisplayP3,
    /// Rec. 2020 ultra-wide color gamut
    Rec2020,
}

/// Additional methods on [`Window`] that are specific to MacOS.
pub trait WindowExtMacOS {
    /// Returns whether or not the window is in simple fullscreen mode.
    fn simple_fullscreen(&self) -> bool;

    /// Toggles a fullscreen mode that doesn't require a new macOS space.
    /// Returns a boolean indicating whether the transition was successful (this
    /// won't work if the window was already in the native fullscreen).
    ///
    /// This is how fullscreen used to work on macOS in versions before Lion.
    /// And allows the user to have a fullscreen window without using another
    /// space or taking control over the entire monitor.
    fn set_simple_fullscreen(&self, fullscreen: bool) -> bool;

    /// Returns whether or not the window has shadow.
    fn has_shadow(&self) -> bool;

    /// Sets whether or not the window has shadow.
    fn set_has_shadow(&self, has_shadow: bool);

    /// Sets background color.
    fn set_background_color(&self, _r: f64, _g: f64, _b: f64, _a: f64);

    /// Group windows together by using the same tabbing identifier.
    ///
    /// <https://developer.apple.com/documentation/appkit/nswindow/1644704-tabbingidentifier>
    fn set_tabbing_identifier(&self, identifier: &str);

    /// Returns the window's tabbing identifier.
    fn tabbing_identifier(&self) -> String;

    /// Select next tab.
    fn select_next_tab(&self);

    /// Select previous tab.
    fn select_previous_tab(&self);

    /// Select the tab with the given index.
    ///
    /// Will no-op when the index is out of bounds.
    fn select_tab_at_index(&self, index: usize);

    /// Get the number of tabs in the window tab group.
    fn num_tabs(&self) -> usize;

    /// Get the window's edit state.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// WindowEvent::CloseRequested => {
    ///     if window.is_document_edited() {
    ///         // Show the user a save pop-up or similar
    ///     } else {
    ///         // Close the window
    ///         drop(window);
    ///     }
    /// }
    /// ```
    fn is_document_edited(&self) -> bool;

    /// Put the window in a state which indicates a file save is required.
    fn set_document_edited(&self, edited: bool);

    /// Set option as alt behavior as described in [`OptionAsAlt`].
    ///
    /// This will ignore diacritical marks and accent characters from
    /// being processed as received characters. Instead, the input
    /// device's raw character will be placed in event queues with the
    /// Alt modifier set.
    fn set_option_as_alt(&self, option_as_alt: OptionAsAlt);

    /// Getter for the [`WindowExtMacOS::set_option_as_alt`].
    fn option_as_alt(&self) -> OptionAsAlt;

    /// Makes the titlebar bigger, effectively adding more space around the
    /// window controls if the titlebar is invisible.
    fn set_unified_titlebar(&self, unified_titlebar: bool);
    /// Getter for the [`WindowExtMacOS::set_unified_titlebar`].
    fn unified_titlebar(&self) -> bool;

    /// Sets the window's colorspace for wide color gamut support.
    fn set_colorspace(&self, colorspace: Colorspace);
}

impl WindowExtMacOS for Window {
    #[inline]
    fn simple_fullscreen(&self) -> bool {
        self.window.maybe_wait_on_main(|w| w.simple_fullscreen())
    }

    #[inline]
    fn set_simple_fullscreen(&self, fullscreen: bool) -> bool {
        self.window
            .maybe_wait_on_main(move |w| w.set_simple_fullscreen(fullscreen))
    }

    #[inline]
    fn has_shadow(&self) -> bool {
        self.window.maybe_wait_on_main(|w| w.has_shadow())
    }

    #[inline]
    fn set_has_shadow(&self, has_shadow: bool) {
        self.window
            .maybe_queue_on_main(move |w| w.set_has_shadow(has_shadow))
    }

    #[inline]
    fn set_background_color(&self, r: f64, g: f64, b: f64, a: f64) {
        self.window
            .maybe_queue_on_main(move |w| w.set_background_color(r, g, b, a))
    }

    #[inline]
    fn set_tabbing_identifier(&self, identifier: &str) {
        self.window
            .maybe_wait_on_main(|w| w.set_tabbing_identifier(identifier))
    }

    #[inline]
    fn tabbing_identifier(&self) -> String {
        self.window.maybe_wait_on_main(|w| w.tabbing_identifier())
    }

    #[inline]
    fn select_next_tab(&self) {
        self.window.maybe_queue_on_main(|w| w.select_next_tab())
    }

    #[inline]
    fn select_previous_tab(&self) {
        self.window.maybe_queue_on_main(|w| w.select_previous_tab())
    }

    #[inline]
    fn select_tab_at_index(&self, index: usize) {
        self.window
            .maybe_queue_on_main(move |w| w.select_tab_at_index(index))
    }

    #[inline]
    fn num_tabs(&self) -> usize {
        self.window.maybe_wait_on_main(|w| w.num_tabs())
    }

    #[inline]
    fn is_document_edited(&self) -> bool {
        self.window.maybe_wait_on_main(|w| w.is_document_edited())
    }

    #[inline]
    fn set_document_edited(&self, edited: bool) {
        self.window
            .maybe_queue_on_main(move |w| w.set_document_edited(edited))
    }

    #[inline]
    fn set_option_as_alt(&self, option_as_alt: OptionAsAlt) {
        self.window
            .maybe_queue_on_main(move |w| w.set_option_as_alt(option_as_alt))
    }

    #[inline]
    fn option_as_alt(&self) -> OptionAsAlt {
        self.window.maybe_wait_on_main(|w| w.option_as_alt())
    }

    #[inline]
    fn set_unified_titlebar(&self, unified_titlebar: bool) {
        self.window
            .maybe_wait_on_main(|w| w.set_unified_titlebar(unified_titlebar))
    }

    #[inline]
    fn unified_titlebar(&self) -> bool {
        self.window.maybe_wait_on_main(|w| w.unified_titlebar())
    }

    #[inline]
    fn set_colorspace(&self, colorspace: Colorspace) {
        self.window
            .maybe_queue_on_main(move |w| w.set_colorspace(colorspace))
    }
}

/// Corresponds to `NSApplicationActivationPolicy`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActivationPolicy {
    /// Corresponds to `NSApplicationActivationPolicyRegular`.
    #[default]
    Regular,

    /// Corresponds to `NSApplicationActivationPolicyAccessory`.
    Accessory,

    /// Corresponds to `NSApplicationActivationPolicyProhibited`.
    Prohibited,
}

/// Additional methods on [`WindowAttributes`] that are specific to MacOS.
///
/// **Note:** Properties dealing with the titlebar will be overwritten by the
/// [`WindowAttributes::with_decorations`] method:
/// - `with_titlebar_transparent`
/// - `with_title_hidden`
/// - `with_titlebar_hidden`
/// - `with_titlebar_buttons_hidden`
/// - `with_fullsize_content_view`
pub trait WindowAttributesExtMacOS {
    /// Enables click-and-drag behavior for the entire window, not just the titlebar.
    fn with_movable_by_window_background(
        self,
        movable_by_window_background: bool,
    ) -> Self;
    /// Makes the titlebar transparent and allows the content to appear behind it.
    fn with_titlebar_transparent(self, titlebar_transparent: bool) -> Self;
    /// Hides the window title.
    fn with_title_hidden(self, title_hidden: bool) -> Self;
    /// Hides the window titlebar.
    fn with_titlebar_hidden(self, titlebar_hidden: bool) -> Self;
    /// Hides the window titlebar buttons.
    fn with_titlebar_buttons_hidden(self, titlebar_buttons_hidden: bool) -> Self;
    /// Makes the window content appear behind the titlebar.
    fn with_fullsize_content_view(self, fullsize_content_view: bool) -> Self;
    fn with_disallow_hidpi(self, disallow_hidpi: bool) -> Self;
    fn with_has_shadow(self, has_shadow: bool) -> Self;
    /// Window accepts click-through mouse events.
    fn with_accepts_first_mouse(self, accepts_first_mouse: bool) -> Self;
    /// Defines the window tabbing identifier.
    ///
    /// <https://developer.apple.com/documentation/appkit/nswindow/1644704-tabbingidentifier>
    fn with_tabbing_identifier(self, identifier: &str) -> Self;
    /// Set how the <kbd>Option</kbd> keys are interpreted.
    ///
    /// See [`WindowExtMacOS::set_option_as_alt`] for details on what this means if set.
    fn with_option_as_alt(self, option_as_alt: OptionAsAlt) -> Self;
    /// See [`WindowExtMacOS::set_unified_titlebar`] for details on what this means if set.
    fn with_unified_titlebar(self, unified_titlebar: bool) -> Self;
    /// Sets the window's colorspace for wide color gamut support.
    fn with_colorspace(self, colorspace: Colorspace) -> Self;
}

impl WindowAttributesExtMacOS for WindowAttributes {
    #[inline]
    fn with_movable_by_window_background(
        mut self,
        movable_by_window_background: bool,
    ) -> Self {
        self.platform_specific.movable_by_window_background =
            movable_by_window_background;
        self
    }

    #[inline]
    fn with_titlebar_transparent(mut self, titlebar_transparent: bool) -> Self {
        self.platform_specific.titlebar_transparent = titlebar_transparent;
        self
    }

    #[inline]
    fn with_titlebar_hidden(mut self, titlebar_hidden: bool) -> Self {
        self.platform_specific.titlebar_hidden = titlebar_hidden;
        self
    }

    #[inline]
    fn with_titlebar_buttons_hidden(mut self, titlebar_buttons_hidden: bool) -> Self {
        self.platform_specific.titlebar_buttons_hidden = titlebar_buttons_hidden;
        self
    }

    #[inline]
    fn with_title_hidden(mut self, title_hidden: bool) -> Self {
        self.platform_specific.title_hidden = title_hidden;
        self
    }

    #[inline]
    fn with_fullsize_content_view(mut self, fullsize_content_view: bool) -> Self {
        self.platform_specific.fullsize_content_view = fullsize_content_view;
        self
    }

    #[inline]
    fn with_disallow_hidpi(mut self, disallow_hidpi: bool) -> Self {
        self.platform_specific.disallow_hidpi = disallow_hidpi;
        self
    }

    #[inline]
    fn with_has_shadow(mut self, has_shadow: bool) -> Self {
        self.platform_specific.has_shadow = has_shadow;
        self
    }

    #[inline]
    fn with_accepts_first_mouse(mut self, accepts_first_mouse: bool) -> Self {
        self.platform_specific.accepts_first_mouse = accepts_first_mouse;
        self
    }

    #[inline]
    fn with_tabbing_identifier(mut self, tabbing_identifier: &str) -> Self {
        self.platform_specific
            .tabbing_identifier
            .replace(tabbing_identifier.to_string());
        self
    }

    #[inline]
    fn with_option_as_alt(mut self, option_as_alt: OptionAsAlt) -> Self {
        self.platform_specific.option_as_alt = option_as_alt;
        self
    }

    #[inline]
    fn with_unified_titlebar(mut self, unified_titlebar: bool) -> Self {
        self.platform_specific.unified_titlebar = unified_titlebar;
        self
    }

    #[inline]
    fn with_colorspace(mut self, colorspace: Colorspace) -> Self {
        self.platform_specific.colorspace = Some(colorspace);
        self
    }
}

pub trait EventLoopBuilderExtMacOS {
    /// Sets the activation policy for the application.
    ///
    /// It is set to [`ActivationPolicy::Regular`] by default.
    ///
    /// # Example
    ///
    /// Set the activation policy to "accessory".
    ///
    /// ```
    /// use rio_window::event_loop::EventLoopBuilder;
    /// #[cfg(target_os = "macos")]
    /// use rio_window::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
    ///
    /// let mut builder = EventLoopBuilder::new();
    /// #[cfg(target_os = "macos")]
    /// builder.with_activation_policy(ActivationPolicy::Accessory);
    /// # if false { // We can't test this part
    /// let event_loop = builder.build();
    /// # }
    /// ```
    fn with_activation_policy(
        &mut self,
        activation_policy: ActivationPolicy,
    ) -> &mut Self;

    /// Used to control whether a default menubar menu is created.
    ///
    /// Menu creation is enabled by default.
    ///
    /// # Example
    ///
    /// Disable creating a default menubar.
    ///
    /// ```
    /// use rio_window::event_loop::EventLoopBuilder;
    /// #[cfg(target_os = "macos")]
    /// use rio_window::platform::macos::EventLoopBuilderExtMacOS;
    ///
    /// let mut builder = EventLoopBuilder::new();
    /// #[cfg(target_os = "macos")]
    /// builder.with_default_menu(false);
    /// # if false { // We can't test this part
    /// let event_loop = builder.build();
    /// # }
    /// ```
    fn with_default_menu(&mut self, enable: bool) -> &mut Self;

    /// Used to prevent the application from automatically activating when launched if
    /// another application is already active.
    ///
    /// The default behavior is to ignore other applications and activate when launched.
    fn with_activate_ignoring_other_apps(&mut self, ignore: bool) -> &mut Self;
}

impl<T> EventLoopBuilderExtMacOS for EventLoopBuilder<T> {
    #[inline]
    fn with_activation_policy(
        &mut self,
        activation_policy: ActivationPolicy,
    ) -> &mut Self {
        self.platform_specific.activation_policy = activation_policy;
        self
    }

    #[inline]
    fn with_default_menu(&mut self, enable: bool) -> &mut Self {
        self.platform_specific.default_menu = enable;
        self
    }

    #[inline]
    fn with_activate_ignoring_other_apps(&mut self, ignore: bool) -> &mut Self {
        self.platform_specific.activate_ignoring_other_apps = ignore;
        self
    }
}

/// Additional methods on [`MonitorHandle`] that are specific to MacOS.
pub trait MonitorHandleExtMacOS {
    /// Returns the identifier of the monitor for Cocoa.
    fn native_id(&self) -> u32;
    /// Returns a pointer to the NSScreen representing this monitor.
    fn ns_screen(&self) -> Option<*mut c_void>;
}

impl MonitorHandleExtMacOS for MonitorHandle {
    #[inline]
    fn native_id(&self) -> u32 {
        self.inner.native_identifier()
    }

    fn ns_screen(&self) -> Option<*mut c_void> {
        // SAFETY: We only use the marker to get a pointer
        let mtm = unsafe { objc2_foundation::MainThreadMarker::new_unchecked() };
        self.inner
            .ns_screen(mtm)
            .map(|s| objc2::rc::Retained::as_ptr(&s) as _)
    }
}

/// Additional methods on [`ActiveEventLoop`] that are specific to macOS.
pub trait ActiveEventLoopExtMacOS {
    /// Hide the entire application. In most applications this is typically triggered with
    /// Command-H.
    fn hide_application(&self);
    /// Hide the other applications. In most applications this is typically triggered with
    /// Command+Option-H.
    fn hide_other_applications(&self);
    /// Set whether the system can automatically organize windows into tabs.
    ///
    /// <https://developer.apple.com/documentation/appkit/nswindow/1646657-allowsautomaticwindowtabbing>
    fn set_allows_automatic_window_tabbing(&self, enabled: bool);
    /// Returns whether the system can automatically organize windows into tabs.
    fn allows_automatic_window_tabbing(&self) -> bool;
}

impl ActiveEventLoopExtMacOS for ActiveEventLoop {
    fn hide_application(&self) {
        self.p.hide_application()
    }

    fn hide_other_applications(&self) {
        self.p.hide_other_applications()
    }

    fn set_allows_automatic_window_tabbing(&self, enabled: bool) {
        self.p.set_allows_automatic_window_tabbing(enabled);
    }

    fn allows_automatic_window_tabbing(&self) -> bool {
        self.p.allows_automatic_window_tabbing()
    }
}

/// Option as alt behavior.
///
/// The default is `None`.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionAsAlt {
    /// The left `Option` key is treated as `Alt`.
    OnlyLeft,

    /// The right `Option` key is treated as `Alt`.
    OnlyRight,

    /// Both `Option` keys are treated as `Alt`.
    Both,

    /// No special handling is applied for `Option` key.
    #[default]
    None,
}
