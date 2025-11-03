#![allow(clippy::unnecessary_cast)]
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;

use core_graphics::display::{CGDisplay, CGPoint};
use monitor::VideoModeHandle;
use objc2::rc::{autoreleasepool, Retained};
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{declare_class, msg_send_id, mutability, sel, ClassType, DeclaredClass};
use objc2_app_kit::{
    NSAppKitVersionNumber, NSAppKitVersionNumber10_12, NSAppearance, NSApplication,
    NSApplicationPresentationOptions, NSBackingStoreType, NSColor, NSDraggingDestination,
    NSFilenamesPboardType, NSPasteboard, NSRequestUserAttentionType, NSScreen, NSToolbar,
    NSView, NSWindowButton, NSWindowDelegate, NSWindowFullScreenButton, NSWindowLevel,
    NSWindowOcclusionState, NSWindowOrderingMode, NSWindowSharingType, NSWindowStyleMask,
    NSWindowTabbingMode, NSWindowTitleVisibility, NSWindowToolbarStyle,
};
use objc2_foundation::{
    ns_string, CGFloat, MainThreadMarker, NSArray, NSCopying,
    NSDistributedNotificationCenter, NSObject, NSObjectNSDelayedPerforming,
    NSObjectNSThreadPerformAdditions, NSObjectProtocol, NSPoint, NSRect, NSSize,
    NSString,
};

use super::app_delegate::ApplicationDelegate;
use super::cursor::cursor_from_icon;
use super::display_link::{DisplayLink, DisplayLinkSupport};
use super::monitor::{self, flip_window_screen_coordinates, get_display_id};
use super::view::WinitView;
use super::window::WinitWindow;
use super::{ffi, Fullscreen, MonitorHandle, OsError, WindowId};
use crate::dpi::{
    LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize, Position, Size,
};
use crate::error::{ExternalError, NotSupportedError, OsError as RootOsError};
use crate::event::WindowEvent;
use crate::platform::macos::{OptionAsAlt, WindowExtMacOS};
use crate::window::{
    Cursor, CursorGrabMode, Icon, ImePurpose, ResizeDirection, Theme, UserAttentionType,
    WindowAttributes, WindowButtons, WindowLevel,
};
use objc2_app_kit::NSAppearanceNameAqua;

#[derive(Clone, Debug)]
pub struct PlatformSpecificWindowAttributes {
    pub movable_by_window_background: bool,
    pub titlebar_transparent: bool,
    pub title_hidden: bool,
    pub titlebar_hidden: bool,
    pub titlebar_buttons_hidden: bool,
    pub fullsize_content_view: bool,
    pub disallow_hidpi: bool,
    pub has_shadow: bool,
    pub accepts_first_mouse: bool,
    pub tabbing_identifier: Option<String>,
    pub option_as_alt: OptionAsAlt,
    pub unified_titlebar: bool,
    pub colorspace: Option<crate::platform::macos::Colorspace>,
}

impl Default for PlatformSpecificWindowAttributes {
    #[inline]
    fn default() -> Self {
        Self {
            movable_by_window_background: false,
            titlebar_transparent: false,
            title_hidden: false,
            titlebar_hidden: false,
            titlebar_buttons_hidden: false,
            fullsize_content_view: false,
            disallow_hidpi: false,
            has_shadow: true,
            accepts_first_mouse: true,
            tabbing_identifier: None,
            option_as_alt: Default::default(),
            unified_titlebar: false,
            colorspace: None,
        }
    }
}

#[derive(Debug)]
pub(crate) struct State {
    /// Strong reference to the global application state.
    app_delegate: Retained<ApplicationDelegate>,

    window: Retained<WinitWindow>,

    current_theme: Cell<Option<Theme>>,

    // During `windowDidResize`, we use this to only send Moved if the position changed.
    //
    // This is expressed in native screen coordinates.
    previous_position: Cell<Option<NSPoint>>,

    // Used to prevent redundant events.
    previous_scale_factor: Cell<f64>,

    /// The current resize increments for the window content.
    resize_increments: Cell<NSSize>,
    /// Whether the window is showing decorations.
    decorations: Cell<bool>,
    resizable: Cell<bool>,
    maximized: Cell<bool>,

    /// Presentation options saved before entering `set_simple_fullscreen`, and
    /// restored upon exiting it. Also used when transitioning from Borderless to
    /// Exclusive fullscreen in `set_fullscreen` because we need to disable the menu
    /// bar in exclusive fullscreen but want to restore the original options when
    /// transitioning back to borderless fullscreen.
    save_presentation_opts: Cell<Option<NSApplicationPresentationOptions>>,
    // This is set when WindowAttributes::with_fullscreen was set,
    // see comments of `window_did_fail_to_enter_fullscreen`
    initial_fullscreen: Cell<bool>,
    /// This field tracks the current fullscreen state of the window
    /// (as seen by `WindowDelegate`).
    fullscreen: RefCell<Option<Fullscreen>>,
    // If it is attempted to toggle fullscreen when in_fullscreen_transition is true,
    // Set target_fullscreen and do after fullscreen transition is end.
    target_fullscreen: RefCell<Option<Option<Fullscreen>>>,
    // This is true between windowWillEnterFullScreen and windowDidEnterFullScreen
    // or windowWillExitFullScreen and windowDidExitFullScreen.
    // We must not toggle fullscreen when this is true.
    in_fullscreen_transition: Cell<bool>,
    standard_frame: Cell<Option<NSRect>>,
    is_simple_fullscreen: Cell<bool>,
    saved_style: Cell<Option<NSWindowStyleMask>>,
    background_color: RefCell<Retained<NSColor>>,

    // Display link for VSync timing
    display_link: RefCell<Option<super::display_link::DisplayLink>>,
    // Track when rendering is needed (dirty state)
    needs_redraw: Cell<bool>,
    // Track last input timestamp for 1-second presentation window
    last_input_timestamp: Cell<std::time::Instant>,
}

declare_class!(
    pub(crate) struct WindowDelegate;

    unsafe impl ClassType for WindowDelegate {
        type Super = NSObject;
        type Mutability = mutability::MainThreadOnly;
        const NAME: &'static str = "WinitWindowDelegate";
    }

    impl DeclaredClass for WindowDelegate {
        type Ivars = State;
    }

    unsafe impl NSObjectProtocol for WindowDelegate {}

    unsafe impl NSWindowDelegate for WindowDelegate {
        #[method(windowShouldClose:)]
        fn window_should_close(&self, _: Option<&AnyObject>) -> bool {
            trace_scope!("windowShouldClose:");
            self.queue_event(WindowEvent::CloseRequested);
            false
        }

        #[method(windowWillClose:)]
        fn window_will_close(&self, _: Option<&AnyObject>) {
            trace_scope!("windowWillClose:");

            // Stop the display link before closing
            if let Err(e) = self.stop_display_link() {
                tracing::warn!("Failed to stop display link: {}", e);
            }

            // `setDelegate:` retains the previous value and then autoreleases it
            autoreleasepool(|_| {
                // Since El Capitan, we need to be careful that delegate methods can't
                // be called after the window closes.
                self.window().setDelegate(None);
            });
            self.queue_event(WindowEvent::Destroyed);
        }

        #[method(windowDidResize:)]
        fn window_did_resize(&self, _: Option<&AnyObject>) {
            trace_scope!("windowDidResize:");
            // NOTE: WindowEvent::Resized is reported in frameDidChange.
            self.emit_move_event();
        }

        #[method(windowWillStartLiveResize:)]
        fn window_will_start_live_resize(&self, _: Option<&AnyObject>) {
            trace_scope!("windowWillStartLiveResize:");

            let increments = self.ivars().resize_increments.get();
            self.set_resize_increments_inner(increments);
        }

        #[method(windowDidEndLiveResize:)]
        fn window_did_end_live_resize(&self, _: Option<&AnyObject>) {
            trace_scope!("windowDidEndLiveResize:");
            self.set_resize_increments_inner(NSSize::new(1., 1.));
        }

        // This won't be triggered if the move was part of a resize.
        #[method(windowDidMove:)]
        fn window_did_move(&self, _: Option<&AnyObject>) {
            trace_scope!("windowDidMove:");
            self.emit_move_event();
        }

        #[method(windowDidChangeBackingProperties:)]
        fn window_did_change_backing_properties(&self, _: Option<&AnyObject>) {
            trace_scope!("windowDidChangeBackingProperties:");
            self.queue_static_scale_factor_changed_event();
        }

        #[method(windowDidBecomeKey:)]
        fn window_did_become_key(&self, _: Option<&AnyObject>) {
            trace_scope!("windowDidBecomeKey:");
            // TODO: center the cursor if the window had mouse grab when it
            // lost focus
            self.queue_event(WindowEvent::Focused(true));
        }

        #[method(windowDidResignKey:)]
        fn window_did_resign_key(&self, _: Option<&AnyObject>) {
            trace_scope!("windowDidResignKey:");
            // It happens rather often, e.g. when the user is Cmd+Tabbing, that the
            // NSWindowDelegate will receive a didResignKey event despite no event
            // being received when the modifiers are released.  This is because
            // flagsChanged events are received by the NSView instead of the
            // NSWindowDelegate, and as a result a tracked modifiers state can quite
            // easily fall out of synchrony with reality.  This requires us to emit
            // a synthetic ModifiersChanged event when we lose focus.
            self.view().reset_modifiers();

            self.queue_event(WindowEvent::Focused(false));
        }

        /// Invoked when before enter fullscreen
        #[method(windowWillEnterFullScreen:)]
        fn window_will_enter_fullscreen(&self, _: Option<&AnyObject>) {
            trace_scope!("windowWillEnterFullScreen:");

            self.ivars().maximized.set(self.is_zoomed());
            let mut fullscreen = self.ivars().fullscreen.borrow_mut();
            match &*fullscreen {
                // Exclusive mode sets the state in `set_fullscreen` as the user
                // can't enter exclusive mode by other means (like the
                // fullscreen button on the window decorations)
                Some(Fullscreen::Exclusive(_)) => (),
                // `window_will_enter_fullscreen` was triggered and we're already
                // in fullscreen, so we must've reached here by `set_fullscreen`
                // as it updates the state
                Some(Fullscreen::Borderless(_)) => (),
                // Otherwise, we must've reached fullscreen by the user clicking
                // on the green fullscreen button. Update state!
                None => {
                    let current_monitor = self.current_monitor_inner();
                    *fullscreen = Some(Fullscreen::Borderless(current_monitor));
                },
            }
            self.ivars().in_fullscreen_transition.set(true);
        }

        /// Invoked when before exit fullscreen
        #[method(windowWillExitFullScreen:)]
        fn window_will_exit_fullscreen(&self, _: Option<&AnyObject>) {
            trace_scope!("windowWillExitFullScreen:");

            self.ivars().in_fullscreen_transition.set(true);
        }

        #[method(window:willUseFullScreenPresentationOptions:)]
        fn window_will_use_fullscreen_presentation_options(
            &self,
            _: Option<&AnyObject>,
            proposed_options: NSApplicationPresentationOptions,
        ) -> NSApplicationPresentationOptions {
            trace_scope!("window:willUseFullScreenPresentationOptions:");
            // Generally, games will want to disable the menu bar and the dock. Ideally,
            // this would be configurable by the user. Unfortunately because of our
            // `CGShieldingWindowLevel() + 1` hack (see `set_fullscreen`), our window is
            // placed on top of the menu bar in exclusive fullscreen mode. This looks
            // broken so we always disable the menu bar in exclusive fullscreen. We may
            // still want to make this configurable for borderless fullscreen. Right now
            // we don't, for consistency. If we do, it should be documented that the
            // user-provided options are ignored in exclusive fullscreen.
            let mut options = proposed_options;
            let fullscreen = self.ivars().fullscreen.borrow();
            if let Some(Fullscreen::Exclusive(_)) = &*fullscreen {
                options = NSApplicationPresentationOptions::NSApplicationPresentationFullScreen
                    | NSApplicationPresentationOptions::NSApplicationPresentationHideDock
                    | NSApplicationPresentationOptions::NSApplicationPresentationHideMenuBar;
            }

            options
        }

        /// Invoked when entered fullscreen
        #[method(windowDidEnterFullScreen:)]
        fn window_did_enter_fullscreen(&self, _: Option<&AnyObject>) {
            trace_scope!("windowDidEnterFullScreen:");
            self.ivars().initial_fullscreen.set(false);
            self.ivars().in_fullscreen_transition.set(false);
            if let Some(target_fullscreen) = self.ivars().target_fullscreen.take() {
                self.set_fullscreen(target_fullscreen);
            }
        }

        /// Invoked when exited fullscreen
        #[method(windowDidExitFullScreen:)]
        fn window_did_exit_fullscreen(&self, _: Option<&AnyObject>) {
            trace_scope!("windowDidExitFullScreen:");

            self.restore_state_from_fullscreen();
            self.ivars().in_fullscreen_transition.set(false);
            if let Some(target_fullscreen) = self.ivars().target_fullscreen.take() {
                self.set_fullscreen(target_fullscreen);
            }
        }

        /// Invoked when fail to enter fullscreen
        ///
        /// When this window launch from a fullscreen app (e.g. launch from VS Code
        /// terminal), it creates a new virtual desktop and a transition animation.
        /// This animation takes one second and cannot be disable without
        /// elevated privileges. In this animation time, all toggleFullscreen events
        /// will be failed. In this implementation, we will try again by using
        /// performSelector:withObject:afterDelay: until window_did_enter_fullscreen.
        /// It should be fine as we only do this at initialization (i.e with_fullscreen
        /// was set).
        ///
        /// From Apple doc:
        /// In some cases, the transition to enter full-screen mode can fail,
        /// due to being in the midst of handling some other animation or user gesture.
        /// This method indicates that there was an error, and you should clean up any
        /// work you may have done to prepare to enter full-screen mode.
        #[method(windowDidFailToEnterFullScreen:)]
        fn window_did_fail_to_enter_fullscreen(&self, _: Option<&AnyObject>) {
            trace_scope!("windowDidFailToEnterFullScreen:");
            self.ivars().in_fullscreen_transition.set(false);
            self.ivars().target_fullscreen.replace(None);
            if self.ivars().initial_fullscreen.get() {
                unsafe {
                    self.window().performSelector_withObject_afterDelay(
                        sel!(toggleFullScreen:),
                        None,
                        0.5,
                    )
                };
            } else {
                self.restore_state_from_fullscreen();
            }
        }

        // Invoked when the occlusion state of the window changes
        #[method(windowDidChangeOcclusionState:)]
        fn window_did_change_occlusion_state(&self, _: Option<&AnyObject>) {
            trace_scope!("windowDidChangeOcclusionState:");
            let visible = self.window().occlusionState().contains(NSWindowOcclusionState::Visible);

            // Manage display link based on window visibility for power efficiency
            if visible {
                if let Err(e) = self.start_display_link() {
                    tracing::warn!("Failed to start display link when window became visible: {}", e);
                }
            } else if let Err(e) = self.stop_display_link() {
                tracing::warn!("Failed to stop display link when window became occluded: {}", e);
            }

            self.queue_event(WindowEvent::Occluded(!visible));
        }

        #[method(windowDidChangeScreen:)]
        fn window_did_change_screen(&self, _: Option<&AnyObject>) {
            trace_scope!("windowDidChangeScreen:");
            let is_simple_fullscreen = self.ivars().is_simple_fullscreen.get();
            if is_simple_fullscreen {
                if let Some(screen) = self.window().screen() {
                    self.window().setFrame_display(screen.frame(), true);
                }
            }

            // Reinitialize display link for the new screen to ensure proper VSync timing
            // This is crucial for multi-display setups with different refresh rates
            tracing::info!("Window moved to different screen, reinitializing display link");
            if let Err(e) = self.stop_display_link() {
                tracing::warn!("Failed to stop display link before screen change: {}", e);
            }

            // Reinitialize with new display
            if let Err(e) = self.setup_display_link() {
                tracing::warn!("Failed to setup display link for new screen: {}", e);
            } else if let Err(e) = self.start_display_link() {
                tracing::warn!("Failed to start display link for new screen: {}", e);
            } else {
                tracing::info!("Display link reinitialized for new screen");
            }
        }
    }

    unsafe impl NSDraggingDestination for WindowDelegate {
        /// Invoked when the dragged image enters destination bounds or frame
        #[method(draggingEntered:)]
        fn dragging_entered(&self, sender: &NSObject) -> bool {
            trace_scope!("draggingEntered:");

            use std::path::PathBuf;

            let pb: Retained<NSPasteboard> = unsafe { msg_send_id![sender, draggingPasteboard] };
            let filenames = pb.propertyListForType(unsafe { NSFilenamesPboardType }).unwrap();
            let filenames: Retained<NSArray<NSString>> = unsafe { Retained::cast(filenames) };

            filenames.into_iter().for_each(|file| {
                let path = PathBuf::from(file.to_string());
                self.queue_event(WindowEvent::HoveredFile(path));
            });

            true
        }

        /// Invoked when the image is released
        #[method(prepareForDragOperation:)]
        fn prepare_for_drag_operation(&self, _sender: &NSObject) -> bool {
            trace_scope!("prepareForDragOperation:");
            true
        }

        /// Invoked after the released image has been removed from the screen
        #[method(performDragOperation:)]
        fn perform_drag_operation(&self, sender: &NSObject) -> bool {
            trace_scope!("performDragOperation:");

            use std::path::PathBuf;

            let pb: Retained<NSPasteboard> = unsafe { msg_send_id![sender, draggingPasteboard] };
            let filenames = pb.propertyListForType(unsafe { NSFilenamesPboardType }).unwrap();
            let filenames: Retained<NSArray<NSString>> = unsafe { Retained::cast(filenames) };

            filenames.into_iter().for_each(|file| {
                let path = PathBuf::from(file.to_string());
                self.queue_event(WindowEvent::DroppedFile(path));
            });

            true
        }

        /// Invoked when the dragging operation is complete
        #[method(concludeDragOperation:)]
        fn conclude_drag_operation(&self, _sender: Option<&NSObject>) {
            trace_scope!("concludeDragOperation:");
        }

        /// Invoked when the dragging operation is cancelled
        #[method(draggingExited:)]
        fn dragging_exited(&self, _sender: Option<&NSObject>) {
            trace_scope!("draggingExited:");
            self.queue_event(WindowEvent::HoveredFileCancelled);
        }
    }

    unsafe impl WindowDelegate {
        // Observe theme change
        #[method(effectiveAppearanceDidChange:)]
        fn effective_appearance_did_change(&self, sender: Option<&AnyObject>) {
            trace_scope!("effectiveAppearanceDidChange:");
            unsafe {
                self.performSelectorOnMainThread_withObject_waitUntilDone(
                    sel!(effectiveAppearanceDidChangedOnMainThread:),
                    sender,
                    false,
                )
            };
        }

        #[method(effectiveAppearanceDidChangedOnMainThread:)]
        fn effective_appearance_did_changed_on_main_thread(&self, _: Option<&AnyObject>) {
            let mtm = MainThreadMarker::from(self);
            let theme = get_ns_theme(mtm);
            let old_theme = self.ivars().current_theme.replace(Some(theme));
            if old_theme != Some(theme) {
                self.queue_event(WindowEvent::ThemeChanged(theme));
            }
        }
    }
);

fn new_window(
    app_delegate: &ApplicationDelegate,
    attrs: &WindowAttributes,
    mtm: MainThreadMarker,
) -> Option<Retained<WinitWindow>> {
    autoreleasepool(|_| {
        let screen = match attrs.fullscreen.clone().map(Into::into) {
            Some(Fullscreen::Borderless(Some(monitor)))
            | Some(Fullscreen::Exclusive(VideoModeHandle { monitor, .. })) => {
                monitor.ns_screen(mtm).or_else(|| NSScreen::mainScreen(mtm))
            }
            Some(Fullscreen::Borderless(None)) => NSScreen::mainScreen(mtm),
            None => None,
        };
        let frame = match &screen {
            Some(screen) => screen.frame(),
            None => {
                let scale_factor = NSScreen::mainScreen(mtm)
                    .map(|screen| screen.backingScaleFactor() as f64)
                    .unwrap_or(1.0);
                let size = match attrs.inner_size {
                    Some(size) => {
                        let size = size.to_logical(scale_factor);
                        NSSize::new(size.width, size.height)
                    }
                    None => NSSize::new(800.0, 600.0),
                };
                let position = match attrs.position {
                    Some(position) => {
                        let position = position.to_logical(scale_factor);
                        flip_window_screen_coordinates(NSRect::new(
                            NSPoint::new(position.x, position.y),
                            size,
                        ))
                    }
                    // This value is ignored by calling win.center() below
                    None => NSPoint::new(0.0, 0.0),
                };
                NSRect::new(position, size)
            }
        };

        let mut masks = if (!attrs.decorations && screen.is_none())
            || attrs.platform_specific.titlebar_hidden
        {
            // Resizable without a titlebar or borders
            // if decorations is set to false, ignore pl_attrs
            //
            // if the titlebar is hidden, ignore other pl_attrs
            NSWindowStyleMask::Borderless
                | NSWindowStyleMask::Resizable
                | NSWindowStyleMask::Miniaturizable
        } else {
            // default case, resizable window with titlebar and titlebar buttons
            NSWindowStyleMask::Closable
                | NSWindowStyleMask::Miniaturizable
                | NSWindowStyleMask::Resizable
                | NSWindowStyleMask::Titled
        };

        if !attrs.resizable {
            masks &= !NSWindowStyleMask::Resizable;
        }

        if !attrs.enabled_buttons.contains(WindowButtons::MINIMIZE) {
            masks &= !NSWindowStyleMask::Miniaturizable;
        }

        if !attrs.enabled_buttons.contains(WindowButtons::CLOSE) {
            masks &= !NSWindowStyleMask::Closable;
        }

        if attrs.platform_specific.fullsize_content_view {
            masks |= NSWindowStyleMask::FullSizeContentView;
        }

        let window: Option<Retained<WinitWindow>> = unsafe {
            msg_send_id![
                super(mtm.alloc().set_ivars(())),
                initWithContentRect: frame,
                styleMask: masks,
                backing: NSBackingStoreType::NSBackingStoreBuffered,
                defer: false,
            ]
        };
        let window = window?;

        // It is very important for correct memory management that we
        // disable the extra release that would otherwise happen when
        // calling `close` on the window.
        unsafe { window.setReleasedWhenClosed(false) };

        window.setTitle(&NSString::from_str(&attrs.title));
        window.setAcceptsMouseMovedEvents(true);

        if let Some(identifier) = &attrs.platform_specific.tabbing_identifier {
            window.setTabbingIdentifier(&NSString::from_str(identifier));
            window.setTabbingMode(NSWindowTabbingMode::Preferred);
        }

        if attrs.content_protected {
            window.setSharingType(NSWindowSharingType::NSWindowSharingNone);
        }

        if attrs.platform_specific.titlebar_transparent {
            window.setTitlebarAppearsTransparent(true);
        }
        if attrs.platform_specific.title_hidden {
            window.setTitleVisibility(NSWindowTitleVisibility::NSWindowTitleHidden);
        }
        if attrs.platform_specific.titlebar_buttons_hidden {
            for titlebar_button in &[
                #[allow(deprecated)]
                NSWindowFullScreenButton,
                NSWindowButton::NSWindowMiniaturizeButton,
                NSWindowButton::NSWindowCloseButton,
                NSWindowButton::NSWindowZoomButton,
            ] {
                if let Some(button) = window.standardWindowButton(*titlebar_button) {
                    button.setHidden(true);
                }
            }
        }
        if attrs.platform_specific.movable_by_window_background {
            window.setMovableByWindowBackground(true);
        }

        if attrs.platform_specific.unified_titlebar {
            unsafe {
                // The toolbar style is ignored if there is no toolbar, so it is
                // necessary to add one.
                window.setToolbar(Some(&NSToolbar::new(mtm)));
                window.setToolbarStyle(NSWindowToolbarStyle::Unified);
            }
        }

        if !attrs.enabled_buttons.contains(WindowButtons::MAXIMIZE) {
            if let Some(button) =
                window.standardWindowButton(NSWindowButton::NSWindowZoomButton)
            {
                button.setEnabled(false);
            }
        }

        if !attrs.platform_specific.has_shadow {
            window.setHasShadow(false);
        }
        if attrs.position.is_none() {
            window.center();
        }

        let view = WinitView::new(
            app_delegate,
            &window,
            attrs.platform_specific.accepts_first_mouse,
            attrs.platform_specific.option_as_alt,
        );

        // The default value of `setWantsBestResolutionOpenGLSurface:` was `false` until
        // macos 10.14 and `true` after 10.15, we should set it to `YES` or `NO` to avoid
        // always the default system value in favour of the user's code
        #[allow(deprecated)]
        view.setWantsBestResolutionOpenGLSurface(!attrs.platform_specific.disallow_hidpi);

        // On Mojave, views automatically become layer-backed shortly after being added to
        // a window. Changing the layer-backedness of a view breaks the association between
        // the view and its associated OpenGL context. To work around this, on Mojave we
        // explicitly make the view layer-backed up front so that AppKit doesn't do it
        // itself and break the association with its context.
        if unsafe { NSAppKitVersionNumber }.floor() > NSAppKitVersionNumber10_12 {
            view.setWantsLayer(true);
        }

        // Configure the new view as the "key view" for the window
        window.setContentView(Some(&view));
        window.setInitialFirstResponder(Some(&view));

        if attrs.transparent {
            window.setOpaque(false);
        }

        // register for drag and drop operations.
        window.registerForDraggedTypes(&NSArray::from_id_slice(&[unsafe {
            NSFilenamesPboardType
        }
        .copy()]));

        // Configure colorspace if specified
        if let Some(colorspace) = attrs.platform_specific.colorspace {
            configure_window_colorspace(&window, colorspace);
        }

        Some(window)
    })
}

impl WindowDelegate {
    pub(super) fn new(
        app_delegate: &ApplicationDelegate,
        attrs: WindowAttributes,
        mtm: MainThreadMarker,
    ) -> Result<Retained<Self>, RootOsError> {
        let window = new_window(app_delegate, &attrs, mtm).ok_or_else(|| {
            os_error!(OsError::CreationError("couldn't create `NSWindow`"))
        })?;

        match attrs.parent_window.map(|handle| handle.0) {
            Some(raw_window_handle::RawWindowHandle::AppKit(handle)) => {
                // SAFETY: Caller ensures the pointer is valid or NULL
                // Unwrap is fine, since the pointer comes from `NonNull`.
                let parent_view: Retained<NSView> =
                    unsafe { Retained::retain(handle.ns_view.as_ptr().cast()) }.unwrap();
                let parent = parent_view.window().ok_or_else(|| {
                    os_error!(OsError::CreationError(
                        "parent view should be installed in a window"
                    ))
                })?;

                // SAFETY: We know that there are no parent -> child -> parent cycles since the only
                // place in `winit` where we allow making a window a child window is
                // right here, just after it's been created.
                unsafe {
                    parent.addChildWindow_ordered(
                        &window,
                        NSWindowOrderingMode::NSWindowAbove,
                    )
                };
            }
            Some(raw) => panic!("invalid raw window handle {raw:?} on macOS"),
            None => (),
        }

        let resize_increments = match attrs
            .resize_increments
            .map(|i| i.to_logical(window.backingScaleFactor() as _))
        {
            Some(LogicalSize { width, height }) if width >= 1. && height >= 1. => {
                NSSize::new(width, height)
            }
            _ => NSSize::new(1., 1.),
        };

        let scale_factor = window.backingScaleFactor() as _;

        let current_theme = match attrs.preferred_theme {
            Some(theme) => Some(theme),
            None => Some(get_ns_theme(mtm)),
        };

        let delegate = mtm.alloc().set_ivars(State {
            app_delegate: app_delegate.retain(),
            window: window.retain(),
            current_theme: Cell::new(current_theme),
            previous_position: Cell::new(None),
            previous_scale_factor: Cell::new(scale_factor),
            resize_increments: Cell::new(resize_increments),
            decorations: Cell::new(attrs.decorations),
            resizable: Cell::new(attrs.resizable),
            maximized: Cell::new(attrs.maximized),
            save_presentation_opts: Cell::new(None),
            initial_fullscreen: Cell::new(attrs.fullscreen.is_some()),
            fullscreen: RefCell::new(None),
            target_fullscreen: RefCell::new(None),
            in_fullscreen_transition: Cell::new(false),
            standard_frame: Cell::new(None),
            is_simple_fullscreen: Cell::new(false),
            saved_style: Cell::new(None),
            background_color: unsafe { NSColor::blackColor().into() },
            display_link: RefCell::new(None),
            needs_redraw: Cell::new(false),
            last_input_timestamp: Cell::new(std::time::Instant::now()),
        });
        let delegate: Retained<WindowDelegate> =
            unsafe { msg_send_id![super(delegate), init] };

        if scale_factor != 1.0 {
            delegate.queue_static_scale_factor_changed_event();
        }
        window.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));

        // Enable theme change event
        let notification_center =
            unsafe { NSDistributedNotificationCenter::defaultCenter() };
        unsafe {
            notification_center.addObserver_selector_name_object(
                &delegate,
                sel!(effectiveAppearanceDidChange:),
                Some(ns_string!("AppleInterfaceThemeChangedNotification")),
                None,
            )
        };

        if attrs.blur {
            delegate.set_blur(attrs.blur);
        }

        if let Some(dim) = attrs.min_inner_size {
            delegate.set_min_inner_size(Some(dim));
        }
        if let Some(dim) = attrs.max_inner_size {
            delegate.set_max_inner_size(Some(dim));
        }

        delegate.set_window_level(attrs.window_level);

        delegate.set_cursor(attrs.cursor);

        // XXX Send `Focused(false)` right after creating the window delegate, so we won't
        // obscure the real focused events on the startup.
        delegate.queue_event(WindowEvent::Focused(false));

        // Set fullscreen mode after we setup everything
        delegate.set_fullscreen(attrs.fullscreen.map(Into::into));

        // Setting the window as key has to happen *after* we set the fullscreen
        // state, since otherwise we'll briefly see the window at normal size
        // before it transitions.
        if attrs.visible {
            if attrs.active {
                // Tightly linked with `app_state::window_activation_hack`
                window.makeKeyAndOrderFront(None);
            } else {
                window.orderFront(None);
            }
        }

        if attrs.maximized {
            delegate.set_maximized(attrs.maximized);
        }

        // Initialize display link for VSync timing
        delegate.initialize_display_link();

        Ok(delegate)
    }

    #[track_caller]
    pub(super) fn view(&self) -> Retained<WinitView> {
        // SAFETY: The view inside WinitWindow is always `WinitView`
        unsafe { Retained::cast(self.window().contentView().unwrap()) }
    }

    #[track_caller]
    pub(super) fn window(&self) -> &WinitWindow {
        &self.ivars().window
    }

    #[track_caller]
    pub(crate) fn id(&self) -> WindowId {
        self.window().id()
    }

    pub(crate) fn queue_event(&self, event: WindowEvent) {
        self.ivars()
            .app_delegate
            .queue_window_event(self.window().id(), event);
    }

    fn queue_static_scale_factor_changed_event(&self) {
        let scale_factor = self.scale_factor();
        if scale_factor == self.ivars().previous_scale_factor.get() {
            return;
        };

        self.ivars().previous_scale_factor.set(scale_factor);
        let content_size = self
            .window()
            .contentRectForFrameRect(self.window().frame())
            .size;
        let content_size = LogicalSize::new(content_size.width, content_size.height);

        self.ivars()
            .app_delegate
            .queue_static_scale_factor_changed_event(
                self.window().retain(),
                content_size.to_physical(scale_factor),
                scale_factor,
            );
    }

    fn emit_move_event(&self) {
        let frame = self.window().frame();
        if self.ivars().previous_position.get() == Some(frame.origin) {
            return;
        }
        self.ivars().previous_position.set(Some(frame.origin));

        let position = flip_window_screen_coordinates(frame);
        let position =
            LogicalPosition::new(position.x, position.y).to_physical(self.scale_factor());
        self.queue_event(WindowEvent::Moved(position));
    }

    fn set_style_mask(&self, mask: NSWindowStyleMask) {
        self.window().setStyleMask(mask);
        // If we don't do this, key handling will break
        // (at least until the window is clicked again/etc.)
        let _ = self.window().makeFirstResponder(Some(&self.view()));
    }

    pub fn set_title(&self, title: &str) {
        self.window().setTitle(&NSString::from_str(title))
    }

    pub fn set_subtitle(&self, subtitle: &str) {
        unsafe {
            self.window().setSubtitle(&NSString::from_str(subtitle));
        }
    }

    pub fn set_transparent(&self, transparent: bool) {
        self.window().setOpaque(!transparent);

        if transparent {
            unsafe {
                self.window()
                    .setBackgroundColor(Some(&NSColor::clearColor()));
            }
        } else {
            self.window()
                .setBackgroundColor(Some(&self.ivars().background_color.borrow()));
            // .setBackgroundColor(Some(&NSColor::blackColor()));
        }
    }

    pub fn set_blur(&self, blur: bool) {
        // NOTE: in general we want to specify the blur radius, but the choice of 80
        // should be a reasonable default.
        let radius = if blur { 80 } else { 0 };
        let window_number = unsafe { self.window().windowNumber() };
        unsafe {
            ffi::CGSSetWindowBackgroundBlurRadius(
                ffi::CGSMainConnectionID(),
                window_number,
                radius,
            );
        }
    }

    pub fn set_visible(&self, visible: bool) {
        match visible {
            true => self.window().makeKeyAndOrderFront(None),
            false => self.window().orderOut(None),
        }
    }

    #[inline]
    pub fn is_visible(&self) -> Option<bool> {
        Some(self.window().isVisible())
    }

    pub fn request_redraw(&self) {
        // Mark window as needing redraw instead of immediately queuing
        // The display link will handle the actual redraw on next VSync
        self.ivars().needs_redraw.set(true);
        tracing::trace!("Window {:?} marked as needing redraw", self.id());
    }

    pub fn initialize_display_link(&self) {
        if let Err(e) = self.setup_display_link() {
            tracing::warn!("Failed to setup display link: {}", e);
        } else if let Err(e) = self.start_display_link() {
            tracing::warn!("Failed to start display link: {}", e);
        } else {
            tracing::info!(
                "Display link initialized successfully for window {:?}",
                self.id()
            );
        }
    }

    #[inline]
    pub fn pre_present_notify(&self) {}

    /// Mark that input was received.
    ///
    /// This updates the timestamp used for the 1-second presentation window
    /// to prevent display downclocking.
    #[inline]
    pub(crate) fn mark_input_received(&self) {
        self.ivars()
            .last_input_timestamp
            .set(std::time::Instant::now());
    }

    /// Check if we should keep presenting frames after input.
    ///
    /// After ANY input, keeps presenting frames for 1 second to prevent
    /// the display from downclocking the refresh rate.
    #[inline]
    pub fn should_present_after_input(&self) -> bool {
        self.ivars().last_input_timestamp.get().elapsed()
            < std::time::Duration::from_secs(1)
    }

    pub fn outer_position(&self) -> Result<PhysicalPosition<i32>, NotSupportedError> {
        let position = flip_window_screen_coordinates(self.window().frame());
        Ok(LogicalPosition::new(position.x, position.y).to_physical(self.scale_factor()))
    }

    pub fn inner_position(&self) -> Result<PhysicalPosition<i32>, NotSupportedError> {
        let content_rect = self.window().contentRectForFrameRect(self.window().frame());
        let position = flip_window_screen_coordinates(content_rect);
        Ok(LogicalPosition::new(position.x, position.y).to_physical(self.scale_factor()))
    }

    pub fn set_outer_position(&self, position: Position) {
        let position = position.to_logical(self.scale_factor());
        let point = flip_window_screen_coordinates(NSRect::new(
            NSPoint::new(position.x, position.y),
            self.window().frame().size,
        ));
        unsafe { self.window().setFrameOrigin(point) };
    }

    #[inline]
    pub fn inner_size(&self) -> PhysicalSize<u32> {
        let content_rect = self.window().contentRectForFrameRect(self.window().frame());
        let logical = LogicalSize::new(content_rect.size.width, content_rect.size.height);
        logical.to_physical(self.scale_factor())
    }

    #[inline]
    pub fn outer_size(&self) -> PhysicalSize<u32> {
        let frame = self.window().frame();
        let logical = LogicalSize::new(frame.size.width, frame.size.height);
        logical.to_physical(self.scale_factor())
    }

    #[inline]
    pub fn request_inner_size(&self, size: Size) -> Option<PhysicalSize<u32>> {
        let scale_factor = self.scale_factor();
        let size = size.to_logical(scale_factor);
        self.window()
            .setContentSize(NSSize::new(size.width, size.height));
        None
    }

    pub fn set_min_inner_size(&self, dimensions: Option<Size>) {
        let dimensions = dimensions.unwrap_or(Size::Logical(LogicalSize {
            width: 0.0,
            height: 0.0,
        }));
        let min_size = dimensions.to_logical::<CGFloat>(self.scale_factor());

        let min_size = NSSize::new(min_size.width, min_size.height);
        unsafe { self.window().setContentMinSize(min_size) };

        // If necessary, resize the window to match constraint
        let mut current_size = self
            .window()
            .contentRectForFrameRect(self.window().frame())
            .size;
        if current_size.width < min_size.width {
            current_size.width = min_size.width;
        }
        if current_size.height < min_size.height {
            current_size.height = min_size.height;
        }
        self.window().setContentSize(current_size);
    }

    pub fn set_max_inner_size(&self, dimensions: Option<Size>) {
        let dimensions = dimensions.unwrap_or(Size::Logical(LogicalSize {
            width: f32::MAX as f64,
            height: f32::MAX as f64,
        }));
        let scale_factor = self.scale_factor();
        let max_size = dimensions.to_logical::<CGFloat>(scale_factor);

        let max_size = NSSize::new(max_size.width, max_size.height);
        unsafe { self.window().setContentMaxSize(max_size) };

        // If necessary, resize the window to match constraint
        let mut current_size = self
            .window()
            .contentRectForFrameRect(self.window().frame())
            .size;
        if max_size.width < current_size.width {
            current_size.width = max_size.width;
        }
        if max_size.height < current_size.height {
            current_size.height = max_size.height;
        }
        self.window().setContentSize(current_size);
    }

    pub fn resize_increments(&self) -> Option<PhysicalSize<u32>> {
        let increments = self.ivars().resize_increments.get();
        let (w, h) = (increments.width, increments.height);
        if w > 1.0 || h > 1.0 {
            Some(LogicalSize::new(w, h).to_physical(self.scale_factor()))
        } else {
            None
        }
    }

    pub fn set_resize_increments(&self, increments: Option<Size>) {
        // XXX the resize increments are only used during live resizes.
        self.ivars().resize_increments.set(
            increments
                .map(|increments| {
                    let logical = increments.to_logical::<f64>(self.scale_factor());
                    NSSize::new(logical.width.max(1.0), logical.height.max(1.0))
                })
                .unwrap_or_else(|| NSSize::new(1.0, 1.0)),
        );
    }

    pub(crate) fn set_resize_increments_inner(&self, size: NSSize) {
        // It was concluded (#2411) that there is never a use-case for
        // "outer" resize increments, hence we set "inner" ones here.
        // ("outer" in macOS being just resizeIncrements, and "inner" - contentResizeIncrements)
        // This is consistent with X11 size hints behavior
        self.window().setContentResizeIncrements(size);
    }

    #[inline]
    pub fn set_resizable(&self, resizable: bool) {
        self.ivars().resizable.set(resizable);
        let fullscreen = self.ivars().fullscreen.borrow().is_some();
        if !fullscreen {
            let mut mask = self.window().styleMask();
            if resizable {
                mask |= NSWindowStyleMask::Resizable;
            } else {
                mask &= !NSWindowStyleMask::Resizable;
            }
            self.set_style_mask(mask);
        }
        // Otherwise, we don't change the mask until we exit fullscreen.
    }

    #[inline]
    pub fn is_resizable(&self) -> bool {
        self.window().isResizable()
    }

    #[inline]
    pub fn set_enabled_buttons(&self, buttons: WindowButtons) {
        let mut mask = self.window().styleMask();

        if buttons.contains(WindowButtons::CLOSE) {
            mask |= NSWindowStyleMask::Closable;
        } else {
            mask &= !NSWindowStyleMask::Closable;
        }

        if buttons.contains(WindowButtons::MINIMIZE) {
            mask |= NSWindowStyleMask::Miniaturizable;
        } else {
            mask &= !NSWindowStyleMask::Miniaturizable;
        }

        // This must happen before the button's "enabled" status has been set,
        // hence we do it synchronously.
        self.set_style_mask(mask);

        // We edit the button directly instead of using `NSResizableWindowMask`,
        // since that mask also affect the resizability of the window (which is
        // controllable by other means in `winit`).
        if let Some(button) = self
            .window()
            .standardWindowButton(NSWindowButton::NSWindowZoomButton)
        {
            button.setEnabled(buttons.contains(WindowButtons::MAXIMIZE));
        }
    }

    #[inline]
    pub fn enabled_buttons(&self) -> WindowButtons {
        let mut buttons = WindowButtons::empty();
        if self.window().isMiniaturizable() {
            buttons |= WindowButtons::MINIMIZE;
        }
        if self
            .window()
            .standardWindowButton(NSWindowButton::NSWindowZoomButton)
            .map(|b| b.isEnabled())
            .unwrap_or(true)
        {
            buttons |= WindowButtons::MAXIMIZE;
        }
        if self.window().hasCloseBox() {
            buttons |= WindowButtons::CLOSE;
        }
        buttons
    }

    pub fn set_cursor(&self, cursor: Cursor) {
        let view = self.view();

        let cursor = match cursor {
            Cursor::Icon(icon) => cursor_from_icon(icon),
            Cursor::Custom(cursor) => cursor.inner.0,
        };

        if view.cursor_icon() == cursor {
            return;
        }

        view.set_cursor_icon(cursor);
        self.window().invalidateCursorRectsForView(&view);
    }

    #[inline]
    pub fn set_cursor_grab(&self, mode: CursorGrabMode) -> Result<(), ExternalError> {
        let associate_mouse_cursor = match mode {
            CursorGrabMode::Locked => false,
            CursorGrabMode::None => true,
            CursorGrabMode::Confined => {
                return Err(ExternalError::NotSupported(NotSupportedError::new()))
            }
        };

        // TODO: Do this for real https://stackoverflow.com/a/40922095/5435443
        CGDisplay::associate_mouse_and_mouse_cursor_position(associate_mouse_cursor)
            .map_err(|status| ExternalError::Os(os_error!(OsError::CGError(status))))
    }

    #[inline]
    pub fn set_cursor_visible(&self, visible: bool) {
        let view = self.view();
        let state_changed = view.set_cursor_visible(visible);
        if state_changed {
            self.window().invalidateCursorRectsForView(&view);
        }
    }

    #[inline]
    pub fn scale_factor(&self) -> f64 {
        self.window().backingScaleFactor() as _
    }

    #[inline]
    pub fn set_cursor_position(
        &self,
        cursor_position: Position,
    ) -> Result<(), ExternalError> {
        let physical_window_position = self.inner_position().unwrap();
        let scale_factor = self.scale_factor();
        let window_position =
            physical_window_position.to_logical::<CGFloat>(scale_factor);
        let logical_cursor_position = cursor_position.to_logical::<CGFloat>(scale_factor);
        let point = CGPoint {
            x: logical_cursor_position.x + window_position.x,
            y: logical_cursor_position.y + window_position.y,
        };
        CGDisplay::warp_mouse_cursor_position(point)
            .map_err(|e| ExternalError::Os(os_error!(OsError::CGError(e))))?;
        CGDisplay::associate_mouse_and_mouse_cursor_position(true)
            .map_err(|e| ExternalError::Os(os_error!(OsError::CGError(e))))?;

        Ok(())
    }

    #[inline]
    pub fn drag_window(&self) -> Result<(), ExternalError> {
        let mtm = MainThreadMarker::from(self);
        let event = NSApplication::sharedApplication(mtm)
            .currentEvent()
            .ok_or(ExternalError::Ignored)?;
        self.window().performWindowDragWithEvent(&event);
        Ok(())
    }

    #[inline]
    pub fn drag_resize_window(
        &self,
        _direction: ResizeDirection,
    ) -> Result<(), ExternalError> {
        Err(ExternalError::NotSupported(NotSupportedError::new()))
    }

    #[inline]
    pub fn show_window_menu(&self, _position: Position) {}

    #[inline]
    pub fn set_cursor_hittest(&self, hittest: bool) -> Result<(), ExternalError> {
        self.window().setIgnoresMouseEvents(!hittest);
        Ok(())
    }

    pub(crate) fn is_zoomed(&self) -> bool {
        // because `isZoomed` doesn't work if the window's borderless,
        // we make it resizable temporarily.
        let curr_mask = self.window().styleMask();

        let required = NSWindowStyleMask::Titled | NSWindowStyleMask::Resizable;
        let needs_temp_mask = !curr_mask.contains(required);
        if needs_temp_mask {
            self.set_style_mask(required);
        }

        let is_zoomed = self.window().isZoomed();

        // Roll back temp styles
        if needs_temp_mask {
            self.set_style_mask(curr_mask);
        }

        is_zoomed
    }

    fn saved_style(&self) -> NSWindowStyleMask {
        let base_mask = self
            .ivars()
            .saved_style
            .take()
            .unwrap_or_else(|| self.window().styleMask());
        if self.ivars().resizable.get() {
            base_mask | NSWindowStyleMask::Resizable
        } else {
            base_mask & !NSWindowStyleMask::Resizable
        }
    }

    /// This is called when the window is exiting fullscreen, whether by the
    /// user clicking on the green fullscreen button or programmatically by
    /// `toggleFullScreen:`
    pub(crate) fn restore_state_from_fullscreen(&self) {
        self.ivars().fullscreen.replace(None);

        let maximized = self.ivars().maximized.get();
        let mask = self.saved_style();

        self.set_style_mask(mask);
        self.set_maximized(maximized);
    }

    #[inline]
    pub fn set_minimized(&self, minimized: bool) {
        let is_minimized = self.window().isMiniaturized();
        if is_minimized == minimized {
            return;
        }

        if minimized {
            self.window().miniaturize(Some(self));
        } else {
            unsafe { self.window().deminiaturize(Some(self)) };
        }
    }

    #[inline]
    pub fn is_minimized(&self) -> Option<bool> {
        Some(self.window().isMiniaturized())
    }

    #[inline]
    pub fn set_maximized(&self, maximized: bool) {
        let mtm = MainThreadMarker::from(self);
        let is_zoomed = self.is_zoomed();
        if is_zoomed == maximized {
            return;
        };

        // Save the standard frame sized if it is not zoomed
        if !is_zoomed {
            self.ivars().standard_frame.set(Some(self.window().frame()));
        }

        self.ivars().maximized.set(maximized);

        if self.ivars().fullscreen.borrow().is_some() {
            // Handle it in window_did_exit_fullscreen
            return;
        }

        if self
            .window()
            .styleMask()
            .contains(NSWindowStyleMask::Resizable)
        {
            // Just use the native zoom if resizable
            self.window().zoom(None);
        } else {
            // if it's not resizable, we set the frame directly
            let new_rect = if maximized {
                let screen = NSScreen::mainScreen(mtm).expect("no screen found");
                screen.visibleFrame()
            } else {
                self.ivars()
                    .standard_frame
                    .get()
                    .unwrap_or(DEFAULT_STANDARD_FRAME)
            };
            self.window().setFrame_display(new_rect, false);
        }
    }

    #[inline]
    pub(crate) fn fullscreen(&self) -> Option<Fullscreen> {
        self.ivars().fullscreen.borrow().clone()
    }

    #[inline]
    pub fn is_maximized(&self) -> bool {
        self.is_zoomed()
    }

    #[inline]
    pub(crate) fn set_fullscreen(&self, fullscreen: Option<Fullscreen>) {
        let mtm = MainThreadMarker::from(self);
        let app = NSApplication::sharedApplication(mtm);

        if self.ivars().is_simple_fullscreen.get() {
            return;
        }
        if self.ivars().in_fullscreen_transition.get() {
            // We can't set fullscreen here.
            // Set fullscreen after transition.
            self.ivars().target_fullscreen.replace(Some(fullscreen));
            return;
        }
        let old_fullscreen = self.ivars().fullscreen.borrow().clone();
        if fullscreen == old_fullscreen {
            return;
        }

        // If the fullscreen is on a different monitor, we must move the window
        // to that monitor before we toggle fullscreen (as `toggleFullScreen`
        // does not take a screen parameter, but uses the current screen)
        if let Some(ref fullscreen) = fullscreen {
            let new_screen = match fullscreen {
                Fullscreen::Borderless(Some(monitor)) => monitor.clone(),
                Fullscreen::Borderless(None) => {
                    if let Some(monitor) = self.current_monitor_inner() {
                        monitor
                    } else {
                        return;
                    }
                }
                Fullscreen::Exclusive(video_mode) => video_mode.monitor(),
            }
            .ns_screen(mtm)
            .unwrap();

            let old_screen = self.window().screen().unwrap();
            if old_screen != new_screen {
                unsafe { self.window().setFrameOrigin(new_screen.frame().origin) };
            }
        }

        if let Some(Fullscreen::Exclusive(ref video_mode)) = fullscreen {
            // Note: `enterFullScreenMode:withOptions:` seems to do the exact
            // same thing as we're doing here (captures the display, sets the
            // video mode, and hides the menu bar and dock), with the exception
            // of that I couldn't figure out how to set the display mode with
            // it. I think `enterFullScreenMode:withOptions:` is still using the
            // older display mode API where display modes were of the type
            // `CFDictionary`, but this has changed, so we can't obtain the
            // correct parameter for this any longer. Apple's code samples for
            // this function seem to just pass in "YES" for the display mode
            // parameter, which is not consistent with the docs saying that it
            // takes a `NSDictionary`..

            let display_id = video_mode.monitor().native_identifier();

            let mut fade_token = ffi::kCGDisplayFadeReservationInvalidToken;

            if matches!(old_fullscreen, Some(Fullscreen::Borderless(_))) {
                self.ivars()
                    .save_presentation_opts
                    .replace(Some(app.presentationOptions()));
            }

            unsafe {
                // Fade to black (and wait for the fade to complete) to hide the
                // flicker from capturing the display and switching display mode
                if ffi::CGAcquireDisplayFadeReservation(5.0, &mut fade_token)
                    == ffi::kCGErrorSuccess
                {
                    ffi::CGDisplayFade(
                        fade_token,
                        0.3,
                        ffi::kCGDisplayBlendNormal,
                        ffi::kCGDisplayBlendSolidColor,
                        0.0,
                        0.0,
                        0.0,
                        ffi::TRUE,
                    );
                }

                assert_eq!(ffi::CGDisplayCapture(display_id), ffi::kCGErrorSuccess);
            }

            unsafe {
                let result = ffi::CGDisplaySetDisplayMode(
                    display_id,
                    video_mode.native_mode.0,
                    std::ptr::null(),
                );
                assert!(result == ffi::kCGErrorSuccess, "failed to set video mode");

                // After the display has been configured, fade back in
                // asynchronously
                if fade_token != ffi::kCGDisplayFadeReservationInvalidToken {
                    ffi::CGDisplayFade(
                        fade_token,
                        0.6,
                        ffi::kCGDisplayBlendSolidColor,
                        ffi::kCGDisplayBlendNormal,
                        0.0,
                        0.0,
                        0.0,
                        ffi::FALSE,
                    );
                    ffi::CGReleaseDisplayFadeReservation(fade_token);
                }
            }
        }

        self.ivars().fullscreen.replace(fullscreen.clone());

        fn toggle_fullscreen(window: &WinitWindow) {
            // Window level must be restored from `CGShieldingWindowLevel()
            // + 1` back to normal in order for `toggleFullScreen` to do
            // anything
            window.setLevel(ffi::kCGNormalWindowLevel as NSWindowLevel);
            window.toggleFullScreen(None);
        }

        match (old_fullscreen, fullscreen) {
            (None, Some(_)) => {
                // `toggleFullScreen` doesn't work if the `StyleMask` is none, so we
                // set a normal style temporarily. The previous state will be
                // restored in `WindowDelegate::window_did_exit_fullscreen`.
                let curr_mask = self.window().styleMask();
                let required = NSWindowStyleMask::Titled | NSWindowStyleMask::Resizable;
                if !curr_mask.contains(required) {
                    self.set_style_mask(required);
                    self.ivars().saved_style.set(Some(curr_mask));
                }
                toggle_fullscreen(self.window());
            }
            (Some(Fullscreen::Borderless(_)), None) => {
                // State is restored by `window_did_exit_fullscreen`
                toggle_fullscreen(self.window());
            }
            (Some(Fullscreen::Exclusive(ref video_mode)), None) => {
                restore_and_release_display(&video_mode.monitor());
                toggle_fullscreen(self.window());
            }
            (Some(Fullscreen::Borderless(_)), Some(Fullscreen::Exclusive(_))) => {
                // If we're already in fullscreen mode, calling
                // `CGDisplayCapture` will place the shielding window on top of
                // our window, which results in a black display and is not what
                // we want. So, we must place our window on top of the shielding
                // window. Unfortunately, this also makes our window be on top
                // of the menu bar, and this looks broken, so we must make sure
                // that the menu bar is disabled. This is done in the window
                // delegate in `window:willUseFullScreenPresentationOptions:`.
                self.ivars()
                    .save_presentation_opts
                    .set(Some(app.presentationOptions()));

                let presentation_options =
                    NSApplicationPresentationOptions::NSApplicationPresentationFullScreen
                        | NSApplicationPresentationOptions::NSApplicationPresentationHideDock
                        | NSApplicationPresentationOptions::NSApplicationPresentationHideMenuBar;
                app.setPresentationOptions(presentation_options);

                let window_level =
                    unsafe { ffi::CGShieldingWindowLevel() } as NSWindowLevel + 1;
                self.window().setLevel(window_level);
            }
            (
                Some(Fullscreen::Exclusive(ref video_mode)),
                Some(Fullscreen::Borderless(_)),
            ) => {
                let presentation_options = self.ivars().save_presentation_opts.get().unwrap_or(
                    NSApplicationPresentationOptions::NSApplicationPresentationFullScreen
                        | NSApplicationPresentationOptions::NSApplicationPresentationAutoHideDock
                        | NSApplicationPresentationOptions::NSApplicationPresentationAutoHideMenuBar
                );
                app.setPresentationOptions(presentation_options);

                restore_and_release_display(&video_mode.monitor());

                // Restore the normal window level following the Borderless fullscreen
                // `CGShieldingWindowLevel() + 1` hack.
                self.window()
                    .setLevel(ffi::kCGNormalWindowLevel as NSWindowLevel);
            }
            _ => {}
        };
    }

    #[inline]
    pub fn set_decorations(&self, decorations: bool) {
        if decorations == self.ivars().decorations.get() {
            return;
        }

        self.ivars().decorations.set(decorations);

        let fullscreen = self.ivars().fullscreen.borrow().is_some();
        let resizable = self.ivars().resizable.get();

        // If we're in fullscreen mode, we wait to apply decoration changes
        // until we're in `window_did_exit_fullscreen`.
        if fullscreen {
            return;
        }

        let new_mask = {
            let mut new_mask = if decorations {
                NSWindowStyleMask::Closable
                    | NSWindowStyleMask::Miniaturizable
                    | NSWindowStyleMask::Resizable
                    | NSWindowStyleMask::Titled
            } else {
                NSWindowStyleMask::Borderless | NSWindowStyleMask::Resizable
            };
            if !resizable {
                new_mask &= !NSWindowStyleMask::Resizable;
            }
            new_mask
        };
        self.set_style_mask(new_mask);
    }

    #[inline]
    pub fn is_decorated(&self) -> bool {
        self.ivars().decorations.get()
    }

    #[inline]
    pub fn set_window_level(&self, level: WindowLevel) {
        let level = match level {
            WindowLevel::AlwaysOnTop => ffi::kCGFloatingWindowLevel as NSWindowLevel,
            WindowLevel::AlwaysOnBottom => {
                (ffi::kCGNormalWindowLevel - 1) as NSWindowLevel
            }
            WindowLevel::Normal => ffi::kCGNormalWindowLevel as NSWindowLevel,
        };
        self.window().setLevel(level);
    }

    #[inline]
    pub fn set_window_icon(&self, _icon: Option<Icon>) {
        // macOS doesn't have window icons. Though, there is
        // `setRepresentedFilename`, but that's semantically distinct and should
        // only be used when the window is in some way representing a specific
        // file/directory. For instance, Terminal.app uses this for the CWD.
        // Anyway, that should eventually be implemented as
        // `WindowAttributesExt::with_represented_file` or something, and doesn't
        // have anything to do with `set_window_icon`.
        // https://developer.apple.com/library/content/documentation/Cocoa/Conceptual/WinPanel/Tasks/SettingWindowTitle.html
    }

    #[inline]
    pub fn set_ime_cursor_area(&self, spot: Position, size: Size) {
        let scale_factor = self.scale_factor();
        let logical_spot = spot.to_logical(scale_factor);
        let logical_spot = NSPoint::new(logical_spot.x, logical_spot.y);

        let size = size.to_logical(scale_factor);
        let size = NSSize::new(size.width, size.height);

        self.view().set_ime_cursor_area(logical_spot, size);
    }

    #[inline]
    pub fn set_ime_allowed(&self, allowed: bool) {
        self.view().set_ime_allowed(allowed);
    }

    #[inline]
    pub fn set_ime_purpose(&self, _purpose: ImePurpose) {}

    #[inline]
    pub fn focus_window(&self) {
        let mtm = MainThreadMarker::from(self);
        let is_minimized = self.window().isMiniaturized();
        let is_visible = self.window().isVisible();

        if !is_minimized && is_visible {
            #[allow(deprecated)]
            NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
            self.window().makeKeyAndOrderFront(None);
        }
    }

    #[inline]
    pub fn request_user_attention(&self, request_type: Option<UserAttentionType>) {
        let mtm = MainThreadMarker::from(self);
        let ns_request_type = request_type.map(|ty| match ty {
            UserAttentionType::Critical => NSRequestUserAttentionType::NSCriticalRequest,
            UserAttentionType::Informational => {
                NSRequestUserAttentionType::NSInformationalRequest
            }
        });
        if let Some(ty) = ns_request_type {
            NSApplication::sharedApplication(mtm).requestUserAttention(ty);
        }
    }

    #[inline]
    // Allow directly accessing the current monitor internally without unwrapping.
    pub(crate) fn current_monitor_inner(&self) -> Option<MonitorHandle> {
        let display_id = get_display_id(&*self.window().screen()?);
        Some(MonitorHandle::new(display_id))
    }

    #[inline]
    pub fn current_monitor(&self) -> Option<MonitorHandle> {
        self.current_monitor_inner()
    }

    #[inline]
    pub fn available_monitors(&self) -> VecDeque<MonitorHandle> {
        monitor::available_monitors()
    }

    #[inline]
    pub fn primary_monitor(&self) -> Option<MonitorHandle> {
        let monitor = monitor::primary_monitor();
        Some(monitor)
    }

    #[inline]
    pub fn raw_window_handle_raw_window_handle(
        &self,
    ) -> raw_window_handle::RawWindowHandle {
        let window_handle = raw_window_handle::AppKitWindowHandle::new({
            let ptr = Retained::as_ptr(&self.view()) as *mut _;
            std::ptr::NonNull::new(ptr).expect("Retained<T> should never be null")
        });
        raw_window_handle::RawWindowHandle::AppKit(window_handle)
    }

    fn toggle_style_mask(&self, mask: NSWindowStyleMask, on: bool) {
        let current_style_mask = self.window().styleMask();
        if on {
            self.set_style_mask(current_style_mask | mask);
        } else {
            self.set_style_mask(current_style_mask & !mask);
        }
    }

    #[inline]
    pub fn theme(&self) -> Option<Theme> {
        self.ivars().current_theme.get()
    }

    #[inline]
    pub fn has_focus(&self) -> bool {
        self.window().isKeyWindow()
    }

    pub fn set_theme(&self, theme: Option<Theme>) {
        let mtm = MainThreadMarker::from(self);
        set_ns_theme(theme, mtm);
        self.ivars()
            .current_theme
            .set(theme.or_else(|| Some(get_ns_theme(mtm))));
    }

    #[inline]
    pub fn set_content_protected(&self, protected: bool) {
        self.window().setSharingType(if protected {
            NSWindowSharingType::NSWindowSharingNone
        } else {
            NSWindowSharingType::NSWindowSharingReadOnly
        })
    }

    pub fn title(&self) -> String {
        self.window().title().to_string()
    }

    pub fn reset_dead_keys(&self) {
        // (Artur) I couldn't find a way to implement this.
    }
}

fn restore_and_release_display(monitor: &MonitorHandle) {
    let available_monitors = monitor::available_monitors();
    if available_monitors.contains(monitor) {
        unsafe {
            ffi::CGRestorePermanentDisplayConfiguration();
            assert_eq!(
                ffi::CGDisplayRelease(monitor.native_identifier()),
                ffi::kCGErrorSuccess
            );
        };
    } else {
        tracing::warn!(
            monitor = monitor.name(),
            "Tried to restore exclusive fullscreen on a monitor that is no longer available"
        );
    }
}

impl WindowExtMacOS for WindowDelegate {
    #[inline]
    fn simple_fullscreen(&self) -> bool {
        self.ivars().is_simple_fullscreen.get()
    }

    #[inline]
    fn set_simple_fullscreen(&self, fullscreen: bool) -> bool {
        let mtm = MainThreadMarker::from(self);

        let app = NSApplication::sharedApplication(mtm);
        let is_native_fullscreen = self.ivars().fullscreen.borrow().is_some();
        let is_simple_fullscreen = self.ivars().is_simple_fullscreen.get();

        // Do nothing if native fullscreen is active.
        if is_native_fullscreen
            || (fullscreen && is_simple_fullscreen)
            || (!fullscreen && !is_simple_fullscreen)
        {
            return false;
        }

        if fullscreen {
            // Remember the original window's settings
            // Exclude title bar
            self.ivars().standard_frame.set(Some(
                self.window().contentRectForFrameRect(self.window().frame()),
            ));
            self.ivars()
                .saved_style
                .set(Some(self.window().styleMask()));
            self.ivars()
                .save_presentation_opts
                .set(Some(app.presentationOptions()));

            // Tell our window's state that we're in fullscreen
            self.ivars().is_simple_fullscreen.set(true);

            // Simulate pre-Lion fullscreen by hiding the dock and menu bar
            let presentation_options =
                NSApplicationPresentationOptions::NSApplicationPresentationAutoHideDock
                    | NSApplicationPresentationOptions::NSApplicationPresentationAutoHideMenuBar;
            app.setPresentationOptions(presentation_options);

            // Hide the titlebar
            self.toggle_style_mask(NSWindowStyleMask::Titled, false);

            // Set the window frame to the screen frame size
            let screen = self
                .window()
                .screen()
                .expect("expected screen to be available");
            self.window().setFrame_display(screen.frame(), true);

            // Fullscreen windows can't be resized, minimized, or moved
            self.toggle_style_mask(NSWindowStyleMask::Miniaturizable, false);
            self.toggle_style_mask(NSWindowStyleMask::Resizable, false);
            self.window().setMovable(false);

            true
        } else {
            let new_mask = self.saved_style();
            self.set_style_mask(new_mask);
            self.ivars().is_simple_fullscreen.set(false);

            let save_presentation_opts = self.ivars().save_presentation_opts.get();
            let frame = self
                .ivars()
                .standard_frame
                .get()
                .unwrap_or(DEFAULT_STANDARD_FRAME);

            if let Some(presentation_opts) = save_presentation_opts {
                app.setPresentationOptions(presentation_opts);
            }

            self.window().setFrame_display(frame, true);
            self.window().setMovable(true);

            true
        }
    }

    #[inline]
    fn has_shadow(&self) -> bool {
        self.window().hasShadow()
    }

    #[inline]
    fn set_background_color(&self, r: f64, g: f64, b: f64, a: f64) {
        let color_value =
            unsafe { NSColor::colorWithSRGBRed_green_blue_alpha(r, g, b, a) };
        let mut background_color = self.ivars().background_color.borrow_mut();
        *background_color = color_value.clone();
        self.window().setBackgroundColor(Some(&color_value));
    }

    #[inline]
    fn set_has_shadow(&self, has_shadow: bool) {
        self.window().setHasShadow(has_shadow)
    }

    #[inline]
    fn set_tabbing_identifier(&self, identifier: &str) {
        self.window()
            .setTabbingIdentifier(&NSString::from_str(identifier))
    }

    #[inline]
    fn tabbing_identifier(&self) -> String {
        self.window().tabbingIdentifier().to_string()
    }

    #[inline]
    fn select_next_tab(&self) {
        self.window().selectNextTab(None)
    }

    #[inline]
    fn select_previous_tab(&self) {
        unsafe { self.window().selectPreviousTab(None) }
    }

    #[inline]
    fn select_tab_at_index(&self, index: usize) {
        if let Some(group) = self.window().tabGroup() {
            if let Some(windows) = unsafe { self.window().tabbedWindows() } {
                if index < windows.len() {
                    group.setSelectedWindow(Some(&windows[index]));
                }
            }
        }
    }

    #[inline]
    fn num_tabs(&self) -> usize {
        unsafe { self.window().tabbedWindows() }
            .map(|windows| windows.len())
            .unwrap_or(1)
    }

    fn is_document_edited(&self) -> bool {
        self.window().isDocumentEdited()
    }

    fn set_document_edited(&self, edited: bool) {
        self.window().setDocumentEdited(edited)
    }

    fn set_option_as_alt(&self, option_as_alt: OptionAsAlt) {
        self.view().set_option_as_alt(option_as_alt);
    }

    fn option_as_alt(&self) -> OptionAsAlt {
        self.view().option_as_alt()
    }

    fn set_unified_titlebar(&self, unified_titlebar: bool) {
        let window = self.window();
        if unified_titlebar {
            let mtm = MainThreadMarker::from(self);
            unsafe {
                // The toolbar style is ignored if there is no toolbar, so it is
                // necessary to add one.
                window.setToolbar(Some(&NSToolbar::new(mtm)));
                window.setToolbarStyle(NSWindowToolbarStyle::Unified);
            }
        } else {
            unsafe {
                window.setToolbar(None);
                window.setToolbarStyle(NSWindowToolbarStyle::Automatic);
            }
        }
    }

    fn set_colorspace(&self, colorspace: crate::platform::macos::Colorspace) {
        let window = self.window();
        configure_window_colorspace(window, colorspace);
    }

    fn unified_titlebar(&self) -> bool {
        let window = self.window();
        unsafe {
            window.toolbar().is_some()
                && window.toolbarStyle() == NSWindowToolbarStyle::Unified
        }
    }
}

const DEFAULT_STANDARD_FRAME: NSRect =
    NSRect::new(NSPoint::new(50.0, 50.0), NSSize::new(800.0, 600.0));

pub(super) fn get_ns_theme(mtm: MainThreadMarker) -> Theme {
    let app = NSApplication::sharedApplication(mtm);
    if !app.respondsToSelector(sel!(effectiveAppearance)) {
        return Theme::Light;
    }
    let appearance = app.effectiveAppearance();
    let name = appearance
        .bestMatchFromAppearancesWithNames(&NSArray::from_id_slice(&[
            NSString::from_str("NSAppearanceNameAqua"),
            NSString::from_str("NSAppearanceNameDarkAqua"),
        ]))
        .unwrap();
    match &*name.to_string() {
        "NSAppearanceNameDarkAqua" => Theme::Dark,
        _ => Theme::Light,
    }
}

fn set_ns_theme(theme: Option<Theme>, mtm: MainThreadMarker) {
    let app = NSApplication::sharedApplication(mtm);
    if app.respondsToSelector(sel!(effectiveAppearance)) {
        let appearance = theme.map(|t| {
            let name = match t {
                Theme::Dark => NSString::from_str("NSAppearanceNameDarkAqua"),
                Theme::Light => NSString::from_str("NSAppearanceNameAqua"),
            };
            NSAppearance::appearanceNamed(&name).unwrap()
        });
        app.setAppearance(appearance.as_ref().map(|a| a.as_ref()));
    }
}

fn dark_appearance_name() -> &'static NSString {
    // Don't use the static `NSAppearanceNameDarkAqua` to allow linking on macOS < 10.14
    ns_string!("NSAppearanceNameDarkAqua")
}

pub fn appearance_to_theme(appearance: &NSAppearance) -> Theme {
    let best_match =
        appearance.bestMatchFromAppearancesWithNames(&NSArray::from_id_slice(&[
            unsafe { NSAppearanceNameAqua.copy() },
            dark_appearance_name().copy(),
        ]));
    if let Some(best_match) = best_match {
        if *best_match == *dark_appearance_name() {
            Theme::Dark
        } else {
            Theme::Light
        }
    } else {
        tracing::warn!(
            ?appearance,
            "failed to determine the theme of the appearance"
        );
        // Default to light in this case
        Theme::Light
    }
}

fn configure_window_colorspace(
    window: &WinitWindow,
    colorspace: crate::platform::macos::Colorspace,
) {
    // Configure the window's content view for colorspace support
    if let Some(content_view) = window.contentView() {
        // Enable layer backing which is required for colorspace configuration
        content_view.setWantsLayer(true);

        // The colorspace will be applied at the Metal rendering level in sugarloaf
        // Here we ensure the window is properly configured for wide color support

        tracing::info!(
            "Configured window {:?} for colorspace: {:?} (layer-backed view enabled)",
            unsafe { window.windowNumber() },
            colorspace
        );
    } else {
        tracing::warn!("Window has no content view for colorspace configuration");
    }
}

impl DisplayLinkSupport for WindowDelegate {
    fn setup_display_link(&self) -> Result<(), &'static str> {
        tracing::info!("Setting up display link for window {:?}", self.id());

        // Get the display ID for the current monitor
        let display_id = match self.current_monitor_inner() {
            Some(monitor) => {
                // Get the actual display ID from the monitor
                let display_id = monitor.native_identifier();
                tracing::info!(
                    "Using display ID {} from current monitor for window {:?}",
                    display_id,
                    self.id()
                );
                display_id
            }
            None => {
                tracing::warn!("Could not get current monitor, using main display");
                use core_graphics::display::CGDisplay;
                CGDisplay::main().id
            }
        };

        // Get window ID
        let window_id = self.id();

        // Create GCD callback for VSync-timed redraws
        unsafe extern "C" fn vsync_callback(context: *mut std::ffi::c_void) {
            if context.is_null() {
                return;
            }

            unsafe {
                let user_data =
                    &*(context as *const super::display_link::DisplayLinkUserData);

                // Get the view directly from the user data
                let view = user_data.view_ptr;

                // Get window delegate from the view (similar to Zed's approach)
                use super::view::get_window_delegate;
                if let Some(window_delegate) = get_window_delegate(view) {
                    // Check if window needs redraw (dirty state) OR if we're within
                    // the 1-second presentation window after input to prevent display downclocking
                    let needs_redraw = window_delegate.ivars().needs_redraw.get();
                    let present_after_input =
                        window_delegate.should_present_after_input();

                    if needs_redraw || present_after_input {
                        // Clear dirty flag and trigger redraw
                        window_delegate.ivars().needs_redraw.set(false);
                        window_delegate
                            .ivars()
                            .app_delegate
                            .handle_redraw(user_data.window_id);

                        tracing::trace!(
                            "VSync redraw triggered {:?} - needs_redraw {:?}",
                            user_data.window_id,
                            needs_redraw
                        );
                    } else {
                        tracing::trace!(
                            "VSync callback skipped - window {:?} not dirty",
                            user_data.window_id
                        );
                    }
                } else {
                    tracing::warn!(
                        "VSync callback could not get window delegate from view for window {:?}",
                        user_data.window_id
                    );
                }
            }
        }

        // Get the view pointer for direct access
        let view = self.view();
        let view_ptr = Retained::as_ptr(&view) as *mut std::ffi::c_void;

        // Create the display link with GCD-based communication
        let display_link =
            DisplayLink::new(display_id, window_id, view_ptr, vsync_callback)?;

        // Store the display link
        *self.ivars().display_link.borrow_mut() = Some(display_link);

        Ok(())
    }

    fn start_display_link(&self) -> Result<(), &'static str> {
        if let Some(display_link) = self.ivars().display_link.borrow().as_ref() {
            display_link.start()
        } else {
            Err("Display link not initialized")
        }
    }

    fn stop_display_link(&self) -> Result<(), &'static str> {
        if let Some(display_link) = self.ivars().display_link.borrow().as_ref() {
            display_link.stop()
        } else {
            Err("Display link not initialized")
        }
    }
}
