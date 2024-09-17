use crate::dpi::{PhysicalPosition, PhysicalSize, Position, Size};
use crate::error::{ExternalError, NotSupportedError, OsError as RootOE};
use crate::icon::Icon;
use crate::window::{
    Cursor, CursorGrabMode, ImePurpose, ResizeDirection, Theme, UserAttentionType,
    WindowAttributes, WindowButtons, WindowId as RootWI, WindowLevel,
};

use super::main_thread::{MainThreadMarker, MainThreadSafe};
use super::monitor::MonitorHandle;
use super::r#async::Dispatcher;
use super::{backend, ActiveEventLoop, Fullscreen};
use web_sys::HtmlCanvasElement;

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;

pub struct Window {
    inner: Dispatcher<Inner>,
}

pub struct Inner {
    id: WindowId,
    pub window: web_sys::Window,
    canvas: Rc<RefCell<backend::Canvas>>,
    destroy_fn: Option<Box<dyn FnOnce()>>,
}

impl Window {
    pub(crate) fn new(
        target: &ActiveEventLoop,
        mut attr: WindowAttributes,
    ) -> Result<Self, RootOE> {
        let id = target.generate_id();

        let window = target.runner.window();
        let document = target.runner.document();
        let canvas = backend::Canvas::create(
            target.runner.main_thread(),
            id,
            window.clone(),
            document.clone(),
            &mut attr,
        )?;
        let canvas = Rc::new(RefCell::new(canvas));

        target.register(&canvas, id);

        let runner = target.runner.clone();
        let destroy_fn = Box::new(move || runner.notify_destroy_window(RootWI(id)));

        let inner = Inner {
            id,
            window: window.clone(),
            canvas,
            destroy_fn: Some(destroy_fn),
        };

        inner.set_title(&attr.title);
        inner.set_maximized(attr.maximized);
        inner.set_visible(attr.visible);
        inner.set_window_icon(attr.window_icon);
        inner.set_cursor(attr.cursor);

        let canvas = Rc::downgrade(&inner.canvas);
        let (dispatcher, runner) =
            Dispatcher::new(target.runner.main_thread(), inner).unwrap();
        target.runner.add_canvas(RootWI(id), canvas, runner);

        Ok(Window { inner: dispatcher })
    }

    pub(crate) fn maybe_queue_on_main(&self, f: impl FnOnce(&Inner) + Send + 'static) {
        self.inner.dispatch(f)
    }

    pub(crate) fn maybe_wait_on_main<R: Send>(
        &self,
        f: impl FnOnce(&Inner) -> R + Send,
    ) -> R {
        self.inner.queue(f)
    }

    pub fn canvas(&self) -> Option<HtmlCanvasElement> {
        self.inner
            .value()
            .map(|inner| inner.canvas.borrow().raw().clone())
    }

    pub(crate) fn prevent_default(&self) -> bool {
        self.inner
            .queue(|inner| inner.canvas.borrow().prevent_default.get())
    }

    pub(crate) fn set_prevent_default(&self, prevent_default: bool) {
        self.inner.dispatch(move |inner| {
            inner.canvas.borrow().prevent_default.set(prevent_default)
        })
    }

    #[inline]
    pub fn raw_window_handle_raw_window_handle(
        &self,
    ) -> Result<raw_window_handle::RawWindowHandle, raw_window_handle::HandleError> {
        self.inner
            .value()
            .map(|inner| {
                let canvas = inner.canvas.borrow();
                // SAFETY: This will only work if the reference to `HtmlCanvasElement` stays valid.
                let canvas: &wasm_bindgen::JsValue = canvas.raw();
                let window_handle = raw_window_handle::WebCanvasWindowHandle::new(
                    std::ptr::NonNull::from(canvas).cast(),
                );
                raw_window_handle::RawWindowHandle::WebCanvas(window_handle)
            })
            .ok_or(raw_window_handle::HandleError::Unavailable)
    }

    #[inline]
    pub(crate) fn raw_display_handle_raw_window_handle(
        &self,
    ) -> Result<raw_window_handle::RawDisplayHandle, raw_window_handle::HandleError> {
        Ok(raw_window_handle::RawDisplayHandle::Web(
            raw_window_handle::WebDisplayHandle::new(),
        ))
    }
}

impl Inner {
    pub fn set_title(&self, title: &str) {
        self.canvas.borrow().set_attribute("alt", title)
    }

    pub fn set_transparent(&self, _transparent: bool) {}

    pub fn set_blur(&self, _blur: bool) {}

    pub fn set_visible(&self, _visible: bool) {
        // Intentionally a no-op
    }

    #[inline]
    pub fn is_visible(&self) -> Option<bool> {
        None
    }

    pub fn request_redraw(&self) {
        self.canvas.borrow().request_animation_frame();
    }

    pub fn pre_present_notify(&self) {}

    pub fn outer_position(&self) -> Result<PhysicalPosition<i32>, NotSupportedError> {
        Ok(self
            .canvas
            .borrow()
            .position()
            .to_physical(self.scale_factor()))
    }

    pub fn inner_position(&self) -> Result<PhysicalPosition<i32>, NotSupportedError> {
        // Note: the canvas element has no window decorations, so this is equal to `outer_position`.
        self.outer_position()
    }

    pub fn set_outer_position(&self, position: Position) {
        let canvas = self.canvas.borrow();
        let position = position.to_logical::<f64>(self.scale_factor());

        backend::set_canvas_position(
            canvas.document(),
            canvas.raw(),
            canvas.style(),
            position,
        )
    }

    #[inline]
    pub fn inner_size(&self) -> PhysicalSize<u32> {
        self.canvas.borrow().inner_size()
    }

    #[inline]
    pub fn outer_size(&self) -> PhysicalSize<u32> {
        // Note: the canvas element has no window decorations, so this is equal to `inner_size`.
        self.inner_size()
    }

    #[inline]
    pub fn request_inner_size(&self, size: Size) -> Option<PhysicalSize<u32>> {
        let size = size.to_logical(self.scale_factor());
        let canvas = self.canvas.borrow();
        backend::set_canvas_size(canvas.document(), canvas.raw(), canvas.style(), size);
        None
    }

    #[inline]
    pub fn set_min_inner_size(&self, dimensions: Option<Size>) {
        let dimensions =
            dimensions.map(|dimensions| dimensions.to_logical(self.scale_factor()));
        let canvas = self.canvas.borrow();
        backend::set_canvas_min_size(
            canvas.document(),
            canvas.raw(),
            canvas.style(),
            dimensions,
        )
    }

    #[inline]
    pub fn set_max_inner_size(&self, dimensions: Option<Size>) {
        let dimensions =
            dimensions.map(|dimensions| dimensions.to_logical(self.scale_factor()));
        let canvas = self.canvas.borrow();
        backend::set_canvas_max_size(
            canvas.document(),
            canvas.raw(),
            canvas.style(),
            dimensions,
        )
    }

    #[inline]
    pub fn resize_increments(&self) -> Option<PhysicalSize<u32>> {
        None
    }

    #[inline]
    pub fn set_resize_increments(&self, _increments: Option<Size>) {
        // Intentionally a no-op: users can't resize canvas elements
    }

    #[inline]
    pub fn set_resizable(&self, _resizable: bool) {
        // Intentionally a no-op: users can't resize canvas elements
    }

    pub fn is_resizable(&self) -> bool {
        true
    }

    #[inline]
    pub fn set_enabled_buttons(&self, _buttons: WindowButtons) {}

    #[inline]
    pub fn enabled_buttons(&self) -> WindowButtons {
        WindowButtons::all()
    }

    #[inline]
    pub fn scale_factor(&self) -> f64 {
        super::backend::scale_factor(&self.window)
    }

    #[inline]
    pub fn set_cursor(&self, cursor: Cursor) {
        self.canvas.borrow_mut().cursor.set_cursor(cursor)
    }

    #[inline]
    pub fn set_cursor_position(&self, _position: Position) -> Result<(), ExternalError> {
        Err(ExternalError::NotSupported(NotSupportedError::new()))
    }

    #[inline]
    pub fn set_cursor_grab(&self, mode: CursorGrabMode) -> Result<(), ExternalError> {
        let lock = match mode {
            CursorGrabMode::None => false,
            CursorGrabMode::Locked => true,
            CursorGrabMode::Confined => {
                return Err(ExternalError::NotSupported(NotSupportedError::new()))
            }
        };

        self.canvas
            .borrow()
            .set_cursor_lock(lock)
            .map_err(ExternalError::Os)
    }

    #[inline]
    pub fn set_cursor_visible(&self, visible: bool) {
        self.canvas.borrow_mut().cursor.set_cursor_visible(visible)
    }

    #[inline]
    pub fn drag_window(&self) -> Result<(), ExternalError> {
        Err(ExternalError::NotSupported(NotSupportedError::new()))
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
    pub fn set_cursor_hittest(&self, _hittest: bool) -> Result<(), ExternalError> {
        Err(ExternalError::NotSupported(NotSupportedError::new()))
    }

    #[inline]
    pub fn set_minimized(&self, _minimized: bool) {
        // Intentionally a no-op, as canvases cannot be 'minimized'
    }

    #[inline]
    pub fn is_minimized(&self) -> Option<bool> {
        // Canvas cannot be 'minimized'
        Some(false)
    }

    #[inline]
    pub fn set_maximized(&self, _maximized: bool) {
        // Intentionally a no-op, as canvases cannot be 'maximized'
    }

    #[inline]
    pub fn is_maximized(&self) -> bool {
        // Canvas cannot be 'maximized'
        false
    }

    #[inline]
    pub(crate) fn fullscreen(&self) -> Option<Fullscreen> {
        if self.canvas.borrow().is_fullscreen() {
            Some(Fullscreen::Borderless(None))
        } else {
            None
        }
    }

    #[inline]
    pub(crate) fn set_fullscreen(&self, fullscreen: Option<Fullscreen>) {
        let canvas = &self.canvas.borrow();

        if fullscreen.is_some() {
            canvas.request_fullscreen();
        } else {
            canvas.exit_fullscreen()
        }
    }

    #[inline]
    pub fn set_decorations(&self, _decorations: bool) {
        // Intentionally a no-op, no canvas decorations
    }

    pub fn is_decorated(&self) -> bool {
        true
    }

    #[inline]
    pub fn set_window_level(&self, _level: WindowLevel) {
        // Intentionally a no-op, no window ordering
    }

    #[inline]
    pub fn set_window_icon(&self, _window_icon: Option<Icon>) {
        // Currently an intentional no-op
    }

    #[inline]
    pub fn set_ime_cursor_area(&self, _position: Position, _size: Size) {
        // Currently a no-op as it does not seem there is good support for this on web
    }

    #[inline]
    pub fn set_ime_allowed(&self, _allowed: bool) {
        // Currently not implemented
    }

    #[inline]
    pub fn set_ime_purpose(&self, _purpose: ImePurpose) {
        // Currently not implemented
    }

    #[inline]
    pub fn focus_window(&self) {
        let _ = self.canvas.borrow().raw().focus();
    }

    #[inline]
    pub fn request_user_attention(&self, _request_type: Option<UserAttentionType>) {
        // Currently an intentional no-op
    }

    #[inline]
    pub fn current_monitor(&self) -> Option<MonitorHandle> {
        None
    }

    #[inline]
    pub fn available_monitors(&self) -> VecDeque<MonitorHandle> {
        VecDeque::new()
    }

    #[inline]
    pub fn primary_monitor(&self) -> Option<MonitorHandle> {
        None
    }

    #[inline]
    pub fn id(&self) -> WindowId {
        self.id
    }

    #[inline]
    pub fn set_theme(&self, _theme: Option<Theme>) {}

    #[inline]
    pub fn theme(&self) -> Option<Theme> {
        backend::is_dark_mode(&self.window).map(|is_dark_mode| {
            if is_dark_mode {
                Theme::Dark
            } else {
                Theme::Light
            }
        })
    }

    pub fn set_content_protected(&self, _protected: bool) {}

    #[inline]
    pub fn has_focus(&self) -> bool {
        self.canvas.borrow().has_focus.get()
    }

    pub fn title(&self) -> String {
        String::new()
    }

    pub fn reset_dead_keys(&self) {
        // Not supported
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        if let Some(destroy_fn) = self.destroy_fn.take() {
            destroy_fn();
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WindowId(pub(crate) u32);

impl WindowId {
    pub const unsafe fn dummy() -> Self {
        Self(0)
    }
}

impl From<WindowId> for u64 {
    fn from(window_id: WindowId) -> Self {
        window_id.0 as u64
    }
}

impl From<u64> for WindowId {
    fn from(raw_id: u64) -> Self {
        Self(raw_id as u32)
    }
}

#[derive(Clone, Debug)]
pub struct PlatformSpecificWindowAttributes {
    pub(crate) canvas: Option<Arc<MainThreadSafe<backend::RawCanvasType>>>,
    pub(crate) prevent_default: bool,
    pub(crate) focusable: bool,
    pub(crate) append: bool,
}

impl PlatformSpecificWindowAttributes {
    pub(crate) fn set_canvas(&mut self, canvas: Option<backend::RawCanvasType>) {
        let Some(canvas) = canvas else {
            self.canvas = None;
            return;
        };

        let main_thread = MainThreadMarker::new()
            .expect("received a `HtmlCanvasElement` outside the window context");

        self.canvas = Some(Arc::new(MainThreadSafe::new(main_thread, canvas)));
    }
}

impl Default for PlatformSpecificWindowAttributes {
    fn default() -> Self {
        Self {
            canvas: None,
            prevent_default: true,
            focusable: true,
            append: false,
        }
    }
}
