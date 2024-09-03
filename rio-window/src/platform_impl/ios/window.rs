#![allow(clippy::unnecessary_cast)]

use std::collections::VecDeque;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{
    class, declare_class, msg_send, msg_send_id, mutability, ClassType, DeclaredClass,
};
use objc2_foundation::{
    CGFloat, CGPoint, CGRect, CGSize, MainThreadBound, MainThreadMarker, NSObjectProtocol,
};
use objc2_ui_kit::{
    UIApplication, UICoordinateSpace, UIResponder, UIScreen,
    UIScreenOverscanCompensation, UIViewController, UIWindow,
};
use tracing::{debug, warn};

use super::app_state::EventWrapper;
use super::view::WinitView;
use super::view_controller::WinitViewController;
use crate::cursor::Cursor;
use crate::dpi::{
    LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize, Position, Size,
};
use crate::error::{ExternalError, NotSupportedError, OsError as RootOsError};
use crate::event::{Event, WindowEvent};
use crate::icon::Icon;
use crate::platform::ios::{ScreenEdge, StatusBarStyle, ValidOrientations};
use crate::platform_impl::platform::{
    app_state, monitor, ActiveEventLoop, Fullscreen, MonitorHandle,
};
use crate::window::{
    CursorGrabMode, ImePurpose, ResizeDirection, Theme, UserAttentionType,
    WindowAttributes, WindowButtons, WindowId as RootWindowId, WindowLevel,
};

declare_class!(
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub(crate) struct WinitUIWindow;

    unsafe impl ClassType for WinitUIWindow {
        #[inherits(UIResponder, NSObject)]
        type Super = UIWindow;
        type Mutability = mutability::MainThreadOnly;
        const NAME: &'static str = "WinitUIWindow";
    }

    impl DeclaredClass for WinitUIWindow {}

    unsafe impl WinitUIWindow {
        #[method(becomeKeyWindow)]
        fn become_key_window(&self) {
            let mtm = MainThreadMarker::new().unwrap();
            app_state::handle_nonuser_event(
                mtm,
                EventWrapper::StaticEvent(Event::WindowEvent {
                    window_id: RootWindowId(self.id()),
                    event: WindowEvent::Focused(true),
                }),
            );
            let _: () = unsafe { msg_send![super(self), becomeKeyWindow] };
        }

        #[method(resignKeyWindow)]
        fn resign_key_window(&self) {
            let mtm = MainThreadMarker::new().unwrap();
            app_state::handle_nonuser_event(
                mtm,
                EventWrapper::StaticEvent(Event::WindowEvent {
                    window_id: RootWindowId(self.id()),
                    event: WindowEvent::Focused(false),
                }),
            );
            let _: () = unsafe { msg_send![super(self), resignKeyWindow] };
        }
    }
);

impl WinitUIWindow {
    pub(crate) fn new(
        mtm: MainThreadMarker,
        window_attributes: &WindowAttributes,
        frame: CGRect,
        view_controller: &UIViewController,
    ) -> Retained<Self> {
        let this: Retained<Self> =
            unsafe { msg_send_id![mtm.alloc(), initWithFrame: frame] };

        this.setRootViewController(Some(view_controller));

        match window_attributes.fullscreen.clone().map(Into::into) {
            Some(Fullscreen::Exclusive(ref video_mode)) => {
                let monitor = video_mode.monitor();
                let screen = monitor.ui_screen(mtm);
                screen.setCurrentMode(Some(video_mode.screen_mode(mtm)));
                this.setScreen(screen);
            }
            Some(Fullscreen::Borderless(Some(ref monitor))) => {
                let screen = monitor.ui_screen(mtm);
                this.setScreen(screen);
            }
            _ => (),
        }

        this
    }

    pub(crate) fn id(&self) -> WindowId {
        (self as *const Self as usize as u64).into()
    }
}

pub struct Inner {
    window: Retained<WinitUIWindow>,
    view_controller: Retained<WinitViewController>,
    view: Retained<WinitView>,
    gl_or_metal_backed: bool,
}

impl Inner {
    pub fn set_title(&self, _title: &str) {
        debug!("`Window::set_title` is ignored on iOS")
    }

    pub fn set_transparent(&self, _transparent: bool) {
        debug!("`Window::set_transparent` is ignored on iOS")
    }

    pub fn set_blur(&self, _blur: bool) {
        debug!("`Window::set_blur` is ignored on iOS")
    }

    pub fn set_visible(&self, visible: bool) {
        self.window.setHidden(!visible)
    }

    pub fn is_visible(&self) -> Option<bool> {
        warn!("`Window::is_visible` is ignored on iOS");
        None
    }

    pub fn request_redraw(&self) {
        if self.gl_or_metal_backed {
            let mtm = MainThreadMarker::new().unwrap();
            // `setNeedsDisplay` does nothing on UIViews which are directly backed by CAEAGLLayer or
            // CAMetalLayer. Ordinarily the OS sets up a bunch of UIKit state before
            // calling drawRect: on a UIView, but when using raw or gl/metal for drawing
            // this work is completely avoided.
            //
            // The docs for `setNeedsDisplay` don't mention `CAMetalLayer`; however, this has been
            // confirmed via testing.
            //
            // https://developer.apple.com/documentation/uikit/uiview/1622437-setneedsdisplay?language=objc
            app_state::queue_gl_or_metal_redraw(mtm, self.window.clone());
        } else {
            self.view.setNeedsDisplay();
        }
    }

    pub fn pre_present_notify(&self) {}

    pub fn inner_position(&self) -> Result<PhysicalPosition<i32>, NotSupportedError> {
        let safe_area = self.safe_area_screen_space();
        let position = LogicalPosition {
            x: safe_area.origin.x as f64,
            y: safe_area.origin.y as f64,
        };
        let scale_factor = self.scale_factor();
        Ok(position.to_physical(scale_factor))
    }

    pub fn outer_position(&self) -> Result<PhysicalPosition<i32>, NotSupportedError> {
        let screen_frame = self.screen_frame();
        let position = LogicalPosition {
            x: screen_frame.origin.x as f64,
            y: screen_frame.origin.y as f64,
        };
        let scale_factor = self.scale_factor();
        Ok(position.to_physical(scale_factor))
    }

    pub fn set_outer_position(&self, physical_position: Position) {
        let scale_factor = self.scale_factor();
        let position = physical_position.to_logical::<f64>(scale_factor);
        let screen_frame = self.screen_frame();
        let new_screen_frame = CGRect {
            origin: CGPoint {
                x: position.x as _,
                y: position.y as _,
            },
            size: screen_frame.size,
        };
        let bounds = self.rect_from_screen_space(new_screen_frame);
        self.window.setBounds(bounds);
    }

    pub fn inner_size(&self) -> PhysicalSize<u32> {
        let scale_factor = self.scale_factor();
        let safe_area = self.safe_area_screen_space();
        let size = LogicalSize {
            width: safe_area.size.width as f64,
            height: safe_area.size.height as f64,
        };
        size.to_physical(scale_factor)
    }

    pub fn outer_size(&self) -> PhysicalSize<u32> {
        let scale_factor = self.scale_factor();
        let screen_frame = self.screen_frame();
        let size = LogicalSize {
            width: screen_frame.size.width as f64,
            height: screen_frame.size.height as f64,
        };
        size.to_physical(scale_factor)
    }

    pub fn request_inner_size(&self, _size: Size) -> Option<PhysicalSize<u32>> {
        Some(self.inner_size())
    }

    pub fn set_min_inner_size(&self, _dimensions: Option<Size>) {
        warn!("`Window::set_min_inner_size` is ignored on iOS")
    }

    pub fn set_max_inner_size(&self, _dimensions: Option<Size>) {
        warn!("`Window::set_max_inner_size` is ignored on iOS")
    }

    pub fn resize_increments(&self) -> Option<PhysicalSize<u32>> {
        None
    }

    #[inline]
    pub fn set_resize_increments(&self, _increments: Option<Size>) {
        warn!("`Window::set_resize_increments` is ignored on iOS")
    }

    pub fn set_resizable(&self, _resizable: bool) {
        warn!("`Window::set_resizable` is ignored on iOS")
    }

    pub fn is_resizable(&self) -> bool {
        warn!("`Window::is_resizable` is ignored on iOS");
        false
    }

    #[inline]
    pub fn set_enabled_buttons(&self, _buttons: WindowButtons) {
        warn!("`Window::set_enabled_buttons` is ignored on iOS");
    }

    #[inline]
    pub fn enabled_buttons(&self) -> WindowButtons {
        warn!("`Window::enabled_buttons` is ignored on iOS");
        WindowButtons::all()
    }

    pub fn scale_factor(&self) -> f64 {
        self.view.contentScaleFactor() as _
    }

    pub fn set_cursor(&self, _cursor: Cursor) {
        debug!("`Window::set_cursor` ignored on iOS")
    }

    pub fn set_cursor_position(&self, _position: Position) -> Result<(), ExternalError> {
        Err(ExternalError::NotSupported(NotSupportedError::new()))
    }

    pub fn set_cursor_grab(&self, _: CursorGrabMode) -> Result<(), ExternalError> {
        Err(ExternalError::NotSupported(NotSupportedError::new()))
    }

    pub fn set_cursor_visible(&self, _visible: bool) {
        debug!("`Window::set_cursor_visible` is ignored on iOS")
    }

    pub fn drag_window(&self) -> Result<(), ExternalError> {
        Err(ExternalError::NotSupported(NotSupportedError::new()))
    }

    pub fn drag_resize_window(
        &self,
        _direction: ResizeDirection,
    ) -> Result<(), ExternalError> {
        Err(ExternalError::NotSupported(NotSupportedError::new()))
    }

    #[inline]
    pub fn show_window_menu(&self, _position: Position) {}

    pub fn set_cursor_hittest(&self, _hittest: bool) -> Result<(), ExternalError> {
        Err(ExternalError::NotSupported(NotSupportedError::new()))
    }

    pub fn set_minimized(&self, _minimized: bool) {
        warn!("`Window::set_minimized` is ignored on iOS")
    }

    pub fn is_minimized(&self) -> Option<bool> {
        warn!("`Window::is_minimized` is ignored on iOS");
        None
    }

    pub fn set_maximized(&self, _maximized: bool) {
        warn!("`Window::set_maximized` is ignored on iOS")
    }

    pub fn is_maximized(&self) -> bool {
        warn!("`Window::is_maximized` is ignored on iOS");
        false
    }

    pub(crate) fn set_fullscreen(&self, monitor: Option<Fullscreen>) {
        let mtm = MainThreadMarker::new().unwrap();
        let uiscreen = match &monitor {
            Some(Fullscreen::Exclusive(video_mode)) => {
                let uiscreen = video_mode.monitor.ui_screen(mtm);
                uiscreen.setCurrentMode(Some(video_mode.screen_mode(mtm)));
                uiscreen.clone()
            }
            Some(Fullscreen::Borderless(Some(monitor))) => monitor.ui_screen(mtm).clone(),
            Some(Fullscreen::Borderless(None)) => {
                self.current_monitor_inner().ui_screen(mtm).clone()
            }
            None => {
                warn!("`Window::set_fullscreen(None)` ignored on iOS");
                return;
            }
        };

        // this is pretty slow on iOS, so avoid doing it if we can
        let current = self.window.screen();
        if uiscreen != current {
            self.window.setScreen(&uiscreen);
        }

        let bounds = uiscreen.bounds();
        self.window.setFrame(bounds);

        // For external displays, we must disable overscan compensation or
        // the displayed image will have giant black bars surrounding it on
        // each side
        uiscreen.setOverscanCompensation(UIScreenOverscanCompensation::None);
    }

    pub(crate) fn fullscreen(&self) -> Option<Fullscreen> {
        let mtm = MainThreadMarker::new().unwrap();
        let monitor = self.current_monitor_inner();
        let uiscreen = monitor.ui_screen(mtm);
        let screen_space_bounds = self.screen_frame();
        let screen_bounds = uiscreen.bounds();

        // TODO: track fullscreen instead of relying on brittle float comparisons
        if screen_space_bounds.origin.x == screen_bounds.origin.x
            && screen_space_bounds.origin.y == screen_bounds.origin.y
            && screen_space_bounds.size.width == screen_bounds.size.width
            && screen_space_bounds.size.height == screen_bounds.size.height
        {
            Some(Fullscreen::Borderless(Some(monitor)))
        } else {
            None
        }
    }

    pub fn set_decorations(&self, _decorations: bool) {}

    pub fn is_decorated(&self) -> bool {
        true
    }

    pub fn set_window_level(&self, _level: WindowLevel) {
        warn!("`Window::set_window_level` is ignored on iOS")
    }

    pub fn set_window_icon(&self, _icon: Option<Icon>) {
        warn!("`Window::set_window_icon` is ignored on iOS")
    }

    pub fn set_ime_cursor_area(&self, _position: Position, _size: Size) {
        warn!("`Window::set_ime_cursor_area` is ignored on iOS")
    }

    pub fn set_ime_allowed(&self, _allowed: bool) {
        warn!("`Window::set_ime_allowed` is ignored on iOS")
    }

    pub fn set_ime_purpose(&self, _purpose: ImePurpose) {
        warn!("`Window::set_ime_allowed` is ignored on iOS")
    }

    pub fn focus_window(&self) {
        warn!("`Window::set_focus` is ignored on iOS")
    }

    pub fn request_user_attention(&self, _request_type: Option<UserAttentionType>) {
        warn!("`Window::request_user_attention` is ignored on iOS")
    }

    // Allow directly accessing the current monitor internally without unwrapping.
    fn current_monitor_inner(&self) -> MonitorHandle {
        MonitorHandle::new(self.window.screen())
    }

    pub fn current_monitor(&self) -> Option<MonitorHandle> {
        Some(self.current_monitor_inner())
    }

    pub fn available_monitors(&self) -> VecDeque<MonitorHandle> {
        monitor::uiscreens(MainThreadMarker::new().unwrap())
    }

    pub fn primary_monitor(&self) -> Option<MonitorHandle> {
        #[allow(deprecated)]
        Some(MonitorHandle::new(UIScreen::mainScreen(
            MainThreadMarker::new().unwrap(),
        )))
    }

    pub fn id(&self) -> WindowId {
        self.window.id()
    }

    pub fn raw_window_handle_rwh_06(&self) -> raw_window_handle::RawWindowHandle {
        let mut window_handle = raw_window_handle::UiKitWindowHandle::new({
            let ui_view = Retained::as_ptr(&self.view) as _;
            std::ptr::NonNull::new(ui_view).expect("Retained<T> should never be null")
        });
        window_handle.ui_view_controller =
            std::ptr::NonNull::new(Retained::as_ptr(&self.view_controller) as _);
        raw_window_handle::RawWindowHandle::UiKit(window_handle)
    }

    pub fn theme(&self) -> Option<Theme> {
        warn!("`Window::theme` is ignored on iOS");
        None
    }

    pub fn set_content_protected(&self, _protected: bool) {}

    pub fn has_focus(&self) -> bool {
        self.window.isKeyWindow()
    }

    #[inline]
    pub fn set_theme(&self, _theme: Option<Theme>) {
        warn!("`Window::set_theme` is ignored on iOS");
    }

    pub fn title(&self) -> String {
        warn!("`Window::title` is ignored on iOS");
        String::new()
    }

    pub fn reset_dead_keys(&self) {
        // Noop
    }
}

pub struct Window {
    inner: MainThreadBound<Inner>,
}

impl Window {
    pub(crate) fn new(
        event_loop: &ActiveEventLoop,
        window_attributes: WindowAttributes,
    ) -> Result<Window, RootOsError> {
        let mtm = event_loop.mtm;

        if window_attributes.min_inner_size.is_some() {
            warn!("`WindowAttributes::min_inner_size` is ignored on iOS");
        }
        if window_attributes.max_inner_size.is_some() {
            warn!("`WindowAttributes::max_inner_size` is ignored on iOS");
        }

        // TODO: transparency, visible

        #[allow(deprecated)]
        let main_screen = UIScreen::mainScreen(mtm);
        let fullscreen = window_attributes.fullscreen.clone().map(Into::into);
        let screen = match fullscreen {
            Some(Fullscreen::Exclusive(ref video_mode)) => {
                video_mode.monitor.ui_screen(mtm)
            }
            Some(Fullscreen::Borderless(Some(ref monitor))) => monitor.ui_screen(mtm),
            Some(Fullscreen::Borderless(None)) | None => &main_screen,
        };

        let screen_bounds = screen.bounds();

        let frame = match window_attributes.inner_size {
            Some(dim) => {
                let scale_factor = screen.scale();
                let size = dim.to_logical::<f64>(scale_factor as f64);
                CGRect {
                    origin: screen_bounds.origin,
                    size: CGSize {
                        width: size.width as _,
                        height: size.height as _,
                    },
                }
            }
            None => screen_bounds,
        };

        let view = WinitView::new(mtm, &window_attributes, frame);

        let gl_or_metal_backed = view.isKindOfClass(class!(CAMetalLayer))
            || view.isKindOfClass(class!(CAEAGLLayer));

        let view_controller = WinitViewController::new(mtm, &window_attributes, &view);
        let window = WinitUIWindow::new(mtm, &window_attributes, frame, &view_controller);

        app_state::set_key_window(mtm, &window);

        // Like the Windows and macOS backends, we send a `ScaleFactorChanged` and `Resized`
        // event on window creation if the DPI factor != 1.0
        let scale_factor = view.contentScaleFactor();
        let scale_factor = scale_factor as f64;
        if scale_factor != 1.0 {
            let bounds = view.bounds();
            let screen = window.screen();
            let screen_space = screen.coordinateSpace();
            let screen_frame = view.convertRect_toCoordinateSpace(bounds, &screen_space);
            let size = LogicalSize {
                width: screen_frame.size.width as f64,
                height: screen_frame.size.height as f64,
            };
            let window_id = RootWindowId(window.id());
            app_state::handle_nonuser_events(
                mtm,
                std::iter::once(EventWrapper::ScaleFactorChanged(
                    app_state::ScaleFactorChanged {
                        window: window.clone(),
                        scale_factor,
                        suggested_size: size.to_physical(scale_factor),
                    },
                ))
                .chain(std::iter::once(EventWrapper::StaticEvent(
                    Event::WindowEvent {
                        window_id,
                        event: WindowEvent::Resized(size.to_physical(scale_factor)),
                    },
                ))),
            );
        }

        let inner = Inner {
            window,
            view_controller,
            view,
            gl_or_metal_backed,
        };
        Ok(Window {
            inner: MainThreadBound::new(inner, mtm),
        })
    }

    pub(crate) fn maybe_queue_on_main(&self, f: impl FnOnce(&Inner) + Send + 'static) {
        // For now, don't actually do queuing, since it may be less predictable
        self.maybe_wait_on_main(f)
    }

    pub(crate) fn maybe_wait_on_main<R: Send>(
        &self,
        f: impl FnOnce(&Inner) -> R + Send,
    ) -> R {
        self.inner.get_on_main(|inner| f(inner))
    }

    #[inline]
    pub(crate) fn raw_window_handle_rwh_06(
        &self,
    ) -> Result<raw_window_handle::RawWindowHandle, raw_window_handle::HandleError> {
        if let Some(mtm) = MainThreadMarker::new() {
            Ok(self.inner.get(mtm).raw_window_handle_rwh_06())
        } else {
            Err(raw_window_handle::HandleError::Unavailable)
        }
    }

    #[inline]
    pub(crate) fn raw_display_handle_rwh_06(
        &self,
    ) -> Result<raw_window_handle::RawDisplayHandle, raw_window_handle::HandleError> {
        Ok(raw_window_handle::RawDisplayHandle::UiKit(
            raw_window_handle::UiKitDisplayHandle::new(),
        ))
    }
}

// WindowExtIOS
impl Inner {
    pub fn set_scale_factor(&self, scale_factor: f64) {
        assert!(
            dpi::validate_scale_factor(scale_factor),
            "`WindowExtIOS::set_scale_factor` received an invalid hidpi factor"
        );
        let scale_factor = scale_factor as CGFloat;
        self.view.setContentScaleFactor(scale_factor);
    }

    pub fn set_valid_orientations(&self, valid_orientations: ValidOrientations) {
        self.view_controller.set_supported_interface_orientations(
            MainThreadMarker::new().unwrap(),
            valid_orientations,
        );
    }

    pub fn set_prefers_home_indicator_hidden(&self, hidden: bool) {
        self.view_controller
            .set_prefers_home_indicator_auto_hidden(hidden);
    }

    pub fn set_preferred_screen_edges_deferring_system_gestures(
        &self,
        edges: ScreenEdge,
    ) {
        self.view_controller
            .set_preferred_screen_edges_deferring_system_gestures(edges);
    }

    pub fn set_prefers_status_bar_hidden(&self, hidden: bool) {
        self.view_controller.set_prefers_status_bar_hidden(hidden);
    }

    pub fn set_preferred_status_bar_style(&self, status_bar_style: StatusBarStyle) {
        self.view_controller
            .set_preferred_status_bar_style(status_bar_style);
    }

    pub fn recognize_pinch_gesture(&self, should_recognize: bool) {
        self.view.recognize_pinch_gesture(should_recognize);
    }

    pub fn recognize_pan_gesture(
        &self,
        should_recognize: bool,
        minimum_number_of_touches: u8,
        maximum_number_of_touches: u8,
    ) {
        self.view.recognize_pan_gesture(
            should_recognize,
            minimum_number_of_touches,
            maximum_number_of_touches,
        );
    }

    pub fn recognize_doubletap_gesture(&self, should_recognize: bool) {
        self.view.recognize_doubletap_gesture(should_recognize);
    }

    pub fn recognize_rotation_gesture(&self, should_recognize: bool) {
        self.view.recognize_rotation_gesture(should_recognize);
    }
}

impl Inner {
    fn screen_frame(&self) -> CGRect {
        self.rect_to_screen_space(self.window.bounds())
    }

    fn rect_to_screen_space(&self, rect: CGRect) -> CGRect {
        let screen_space = self.window.screen().coordinateSpace();
        self.window
            .convertRect_toCoordinateSpace(rect, &screen_space)
    }

    fn rect_from_screen_space(&self, rect: CGRect) -> CGRect {
        let screen_space = self.window.screen().coordinateSpace();
        self.window
            .convertRect_fromCoordinateSpace(rect, &screen_space)
    }

    fn safe_area_screen_space(&self) -> CGRect {
        let bounds = self.window.bounds();
        if app_state::os_capabilities().safe_area {
            let safe_area = self.window.safeAreaInsets();
            let safe_bounds = CGRect {
                origin: CGPoint {
                    x: bounds.origin.x + safe_area.left,
                    y: bounds.origin.y + safe_area.top,
                },
                size: CGSize {
                    width: bounds.size.width - safe_area.left - safe_area.right,
                    height: bounds.size.height - safe_area.top - safe_area.bottom,
                },
            };
            self.rect_to_screen_space(safe_bounds)
        } else {
            let screen_frame = self.rect_to_screen_space(bounds);
            let status_bar_frame = {
                let app =
                    UIApplication::sharedApplication(MainThreadMarker::new().unwrap());
                #[allow(deprecated)]
                app.statusBarFrame()
            };
            let (y, height) = if screen_frame.origin.y > status_bar_frame.size.height {
                (screen_frame.origin.y, screen_frame.size.height)
            } else {
                let y = status_bar_frame.size.height;
                let height = screen_frame.size.height
                    - (status_bar_frame.size.height - screen_frame.origin.y);
                (y, height)
            };
            CGRect {
                origin: CGPoint {
                    x: screen_frame.origin.x,
                    y,
                },
                size: CGSize {
                    width: screen_frame.size.width,
                    height,
                },
            }
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WindowId {
    window: *mut WinitUIWindow,
}

impl WindowId {
    pub const unsafe fn dummy() -> Self {
        WindowId {
            window: std::ptr::null_mut(),
        }
    }
}

impl From<WindowId> for u64 {
    fn from(window_id: WindowId) -> Self {
        window_id.window as u64
    }
}

impl From<u64> for WindowId {
    fn from(raw_id: u64) -> Self {
        Self {
            window: raw_id as _,
        }
    }
}

unsafe impl Send for WindowId {}
unsafe impl Sync for WindowId {}

impl From<&AnyObject> for WindowId {
    fn from(window: &AnyObject) -> WindowId {
        WindowId {
            window: window as *const _ as _,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PlatformSpecificWindowAttributes {
    pub scale_factor: Option<f64>,
    pub valid_orientations: ValidOrientations,
    pub prefers_home_indicator_hidden: bool,
    pub prefers_status_bar_hidden: bool,
    pub preferred_status_bar_style: StatusBarStyle,
    pub preferred_screen_edges_deferring_system_gestures: ScreenEdge,
}
