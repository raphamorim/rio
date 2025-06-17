//! The [`EventLoop`] struct and assorted supporting types, including
//! [`ControlFlow`].
//!
//! If you want to send custom events to the event loop, use
//! [`EventLoop::create_proxy`] to acquire an [`EventLoopProxy`] and call its
//! [`send_event`][EventLoopProxy::send_event] method.
//!
//! See the root-level documentation for information on how to create and use an event loop to
//! handle events.
use std::marker::PhantomData;
#[cfg(any(x11_platform, wayland_platform))]
use std::os::unix::io::{AsFd, AsRawFd, BorrowedFd, RawFd};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::{error, fmt};

#[cfg(not(web_platform))]
use std::time::{Duration, Instant};
#[cfg(web_platform)]
use web_time::{Duration, Instant};

use crate::application::ApplicationHandler;
use crate::error::{EventLoopError, OsError};
use crate::event::Event;
use crate::monitor::MonitorHandle;
use crate::platform_impl;
use crate::window::Theme;
use crate::window::{CustomCursor, CustomCursorSource, Window, WindowAttributes};

/// Provides a way to retrieve events from the system and from the windows that were registered to
/// the events loop.
///
/// An `EventLoop` can be seen more or less as a "context". Calling [`EventLoop::new`]
/// initializes everything that will be required to create windows. For example on Linux creating
/// an event loop opens a connection to the X or Wayland server.
///
/// To wake up an `EventLoop` from a another thread, see the [`EventLoopProxy`] docs.
///
/// Note that this cannot be shared across threads (due to platform-dependant logic
/// forbidding it), as such it is neither [`Send`] nor [`Sync`]. If you need cross-thread access,
/// the [`Window`] created from this _can_ be sent to an other thread, and the
/// [`EventLoopProxy`] allows you to wake up an `EventLoop` from another thread.
///
/// [`Window`]: crate::window::Window
pub struct EventLoop<T: 'static> {
    pub(crate) event_loop: platform_impl::EventLoop<T>,
    pub(crate) _marker: PhantomData<*mut ()>, // Not Send nor Sync
}

/// Target that associates windows with an [`EventLoop`].
///
/// This type exists to allow you to create new windows while Winit executes
/// your callback.
pub struct ActiveEventLoop {
    pub(crate) p: platform_impl::ActiveEventLoop,
    pub(crate) _marker: PhantomData<*mut ()>, // Not Send nor Sync
}

/// Object that allows building the event loop.
///
/// This is used to make specifying options that affect the whole application
/// easier. But note that constructing multiple event loops is not supported.
///
/// This can be created using [`EventLoop::new`] or [`EventLoop::with_user_event`].
#[derive(Default)]
pub struct EventLoopBuilder<T: 'static> {
    pub(crate) platform_specific: platform_impl::PlatformSpecificEventLoopAttributes,
    _p: PhantomData<T>,
}

static EVENT_LOOP_CREATED: AtomicBool = AtomicBool::new(false);

impl EventLoopBuilder<()> {
    /// Start building a new event loop.
    #[inline]
    #[deprecated = "use `EventLoop::builder` instead"]
    pub fn new() -> Self {
        EventLoop::builder()
    }
}

impl<T> EventLoopBuilder<T> {
    /// Builds a new event loop.
    ///
    /// ***For cross-platform compatibility, the [`EventLoop`] must be created on the main thread,
    /// and only once per application.***
    ///
    /// Calling this function will result in display backend initialisation.
    ///
    /// ## Panics
    ///
    /// Attempting to create the event loop off the main thread will panic. This
    /// restriction isn't strictly necessary on all platforms, but is imposed to
    /// eliminate any nasty surprises when porting to platforms that require it.
    /// `EventLoopBuilderExt::any_thread` functions are exposed in the relevant
    /// [`platform`] module if the target platform supports creating an event
    /// loop on any thread.
    ///
    /// ## Platform-specific
    ///
    /// - **Wayland/X11:** to prevent running under `Wayland` or `X11` unset `WAYLAND_DISPLAY` or
    ///   `DISPLAY` respectively when building the event loop.
    /// - **Android:** must be configured with an `AndroidApp` from `android_main()` by calling
    ///   [`.with_android_app(app)`] before calling `.build()`, otherwise it'll panic.
    ///
    /// [`platform`]: crate::platform
    #[cfg_attr(
        android_platform,
        doc = "[`.with_android_app(app)`]: \
               crate::platform::android::EventLoopBuilderExtAndroid::with_android_app"
    )]
    #[cfg_attr(
        not(android_platform),
        doc = "[`.with_android_app(app)`]: #only-available-on-android"
    )]
    #[inline]
    pub fn build(&mut self) -> Result<EventLoop<T>, EventLoopError> {
        let _span = tracing::debug_span!("rio_window::EventLoopBuilder::build").entered();

        if EVENT_LOOP_CREATED.swap(true, Ordering::Relaxed) {
            return Err(EventLoopError::RecreationAttempt);
        }

        // Certain platforms accept a mutable reference in their API.
        #[allow(clippy::unnecessary_mut_passed)]
        Ok(EventLoop {
            event_loop: platform_impl::EventLoop::new(&mut self.platform_specific)?,
            _marker: PhantomData,
        })
    }

    #[cfg(web_platform)]
    pub(crate) fn allow_event_loop_recreation() {
        EVENT_LOOP_CREATED.store(false, Ordering::Relaxed);
    }
}

impl<T> fmt::Debug for EventLoop<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("EventLoop { .. }")
    }
}

impl fmt::Debug for ActiveEventLoop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("ActiveEventLoop { .. }")
    }
}

/// Set through [`ActiveEventLoop::set_control_flow()`].
///
/// Indicates the desired behavior of the event loop after [`Event::AboutToWait`] is emitted.
///
/// Defaults to [`Wait`].
///
/// [`Wait`]: Self::Wait
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum ControlFlow {
    /// When the current loop iteration finishes, immediately begin a new iteration regardless of
    /// whether or not new events are available to process.
    Poll,

    /// When the current loop iteration finishes, suspend the thread until another event arrives.
    #[default]
    Wait,

    /// When the current loop iteration finishes, suspend the thread until either another event
    /// arrives or the given time is reached.
    ///
    /// Useful for implementing efficient timers. Applications which want to render at the
    /// display's native refresh rate should instead use [`Poll`] and the VSync functionality
    /// of a graphics API to reduce odds of missed frames.
    ///
    /// [`Poll`]: Self::Poll
    WaitUntil(Instant),
}

impl ControlFlow {
    /// Creates a [`ControlFlow`] that waits until a timeout has expired.
    ///
    /// In most cases, this is set to [`WaitUntil`]. However, if the timeout overflows, it is
    /// instead set to [`Wait`].
    ///
    /// [`WaitUntil`]: Self::WaitUntil
    /// [`Wait`]: Self::Wait
    pub fn wait_duration(timeout: Duration) -> Self {
        match Instant::now().checked_add(timeout) {
            Some(instant) => Self::WaitUntil(instant),
            None => Self::Wait,
        }
    }
}

impl EventLoop<()> {
    /// Create the event loop.
    ///
    /// This is an alias of `EventLoop::builder().build()`.
    #[inline]
    pub fn new() -> Result<EventLoop<()>, EventLoopError> {
        Self::builder().build()
    }

    /// Start building a new event loop.
    ///
    /// This returns an [`EventLoopBuilder`], to allow configuring the event loop before creation.
    ///
    /// To get the actual event loop, call [`build`][EventLoopBuilder::build] on that.
    #[inline]
    pub fn builder() -> EventLoopBuilder<()> {
        Self::with_user_event()
    }
}

impl<T> EventLoop<T> {
    /// Start building a new event loop, with the given type as the user event
    /// type.
    pub fn with_user_event() -> EventLoopBuilder<T> {
        EventLoopBuilder {
            platform_specific: Default::default(),
            _p: PhantomData,
        }
    }

    /// See [`run_app`].
    ///
    /// [`run_app`]: Self::run_app
    #[inline]
    #[deprecated = "use `EventLoop::run_app` instead"]
    #[cfg(not(all(web_platform, target_feature = "exception-handling")))]
    pub fn run<F>(self, event_handler: F) -> Result<(), EventLoopError>
    where
        F: FnMut(Event<T>, &ActiveEventLoop),
    {
        let _span = tracing::debug_span!("rio_window::EventLoop::run").entered();

        self.event_loop.run(event_handler)
    }

    /// Run the application with the event loop on the calling thread.
    ///
    /// See the [`set_control_flow()`] docs on how to change the event loop's behavior.
    ///
    /// ## Platform-specific
    ///
    /// - **iOS:** Will never return to the caller and so values not passed to this function will
    ///   *not* be dropped before the process exits.
    /// - **Web:** Will _act_ as if it never returns to the caller by throwing a Javascript
    ///   exception (that Rust doesn't see) that will also mean that the rest of the function is
    ///   never executed and any values not passed to this function will *not* be dropped.
    ///
    ///   Web applications are recommended to use
    #[cfg_attr(
        web_platform,
        doc = "[`EventLoopExtWebSys::spawn_app()`][crate::platform::web::EventLoopExtWebSys::spawn_app()]"
    )]
    #[cfg_attr(not(web_platform), doc = "`EventLoopExtWebSys::spawn()`")]
    ///   [^1] instead of [`run_app()`] to avoid the need
    ///   for the Javascript exception trick, and to make it clearer that the event loop runs
    ///   asynchronously (via the browser's own, internal, event loop) and doesn't block the
    ///   current thread of execution like it does on other platforms.
    ///
    ///   This function won't be available with `target_feature = "exception-handling"`.
    ///
    /// [`set_control_flow()`]: ActiveEventLoop::set_control_flow()
    /// [`run_app()`]: Self::run_app()
    /// [^1]: `EventLoopExtWebSys::spawn_app()` is only available on Web.
    #[inline]
    #[cfg(not(all(web_platform, target_feature = "exception-handling")))]
    pub fn run_app<A: ApplicationHandler<T>>(
        self,
        app: &mut A,
    ) -> Result<(), EventLoopError> {
        self.event_loop
            .run(|event, event_loop| dispatch_event_for_app(app, event_loop, event))
    }

    /// Creates an [`EventLoopProxy`] that can be used to dispatch user events
    /// to the main event loop, possibly from another thread.
    pub fn create_proxy(&self) -> EventLoopProxy<T> {
        EventLoopProxy {
            event_loop_proxy: self.event_loop.create_proxy(),
        }
    }

    /// Gets a persistent reference to the underlying platform display.
    ///
    /// See the [`OwnedDisplayHandle`] type for more information.
    pub fn owned_display_handle(&self) -> OwnedDisplayHandle {
        OwnedDisplayHandle {
            platform: self.event_loop.window_target().p.owned_display_handle(),
        }
    }

    /// Change if or when [`DeviceEvent`]s are captured.
    ///
    /// See [`ActiveEventLoop::listen_device_events`] for details.
    ///
    /// [`DeviceEvent`]: crate::event::DeviceEvent
    pub fn listen_device_events(&self, allowed: DeviceEvents) {
        let _span = tracing::debug_span!(
            "rio_window::EventLoop::listen_device_events",
            allowed = ?allowed
        )
        .entered();

        self.event_loop
            .window_target()
            .p
            .listen_device_events(allowed);
    }

    #[inline]
    #[cfg(target_os = "macos")]
    pub fn set_confirm_before_quit(&self, confirmation: bool) {
        let _span =
            tracing::debug_span!("rio_window::EventLoop::set_confirm_before_quit")
                .entered();

        self.event_loop.set_confirm_before_quit(confirmation)
    }

    /// Sets the [`ControlFlow`].
    pub fn set_control_flow(&self, control_flow: ControlFlow) {
        self.event_loop
            .window_target()
            .p
            .set_control_flow(control_flow)
    }

    /// Create a window.
    ///
    /// Creating window without event loop running often leads to improper window creation;
    /// use [`ActiveEventLoop::create_window`] instead.
    #[deprecated = "use `ActiveEventLoop::create_window` instead"]
    #[inline]
    pub fn create_window(
        &self,
        window_attributes: WindowAttributes,
    ) -> Result<Window, OsError> {
        let _span = tracing::debug_span!(
            "rio_window::EventLoop::create_window",
            window_attributes = ?window_attributes
        )
        .entered();

        let window = platform_impl::Window::new(
            &self.event_loop.window_target().p,
            window_attributes,
        )?;
        Ok(Window { window })
    }

    /// Create custom cursor.
    pub fn create_custom_cursor(
        &self,
        custom_cursor: CustomCursorSource,
    ) -> CustomCursor {
        self.event_loop
            .window_target()
            .p
            .create_custom_cursor(custom_cursor)
    }
}

impl<T> raw_window_handle::HasDisplayHandle for EventLoop<T> {
    fn display_handle(
        &self,
    ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError>
    {
        raw_window_handle::HasDisplayHandle::display_handle(
            self.event_loop.window_target(),
        )
    }
}

#[cfg(any(x11_platform, wayland_platform))]
impl<T> AsFd for EventLoop<T> {
    /// Get the underlying [EventLoop]'s `fd` which you can register
    /// into other event loop, like [`calloop`] or [`mio`]. When doing so, the
    /// loop must be polled with the [`pump_app_events`] API.
    ///
    /// [`calloop`]: https://crates.io/crates/calloop
    /// [`mio`]: https://crates.io/crates/mio
    /// [`pump_app_events`]: crate::platform::pump_events::EventLoopExtPumpEvents::pump_app_events
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.event_loop.as_fd()
    }
}

#[cfg(any(x11_platform, wayland_platform))]
impl<T> AsRawFd for EventLoop<T> {
    /// Get the underlying [EventLoop]'s raw `fd` which you can register
    /// into other event loop, like [`calloop`] or [`mio`]. When doing so, the
    /// loop must be polled with the [`pump_app_events`] API.
    ///
    /// [`calloop`]: https://crates.io/crates/calloop
    /// [`mio`]: https://crates.io/crates/mio
    /// [`pump_app_events`]: crate::platform::pump_events::EventLoopExtPumpEvents::pump_app_events
    fn as_raw_fd(&self) -> RawFd {
        self.event_loop.as_raw_fd()
    }
}

impl ActiveEventLoop {
    /// Create the window.
    ///
    /// Possible causes of error include denied permission, incompatible system, and lack of memory.
    ///
    /// ## Platform-specific
    ///
    /// - **Web:** The window is created but not inserted into the web page automatically. Please
    ///   see the web platform module for more information.
    #[inline]
    pub fn create_window(
        &self,
        window_attributes: WindowAttributes,
    ) -> Result<Window, OsError> {
        let _span = tracing::debug_span!(
            "rio_window::ActiveEventLoop::create_window",
            window_attributes = ?window_attributes
        )
        .entered();

        let window = platform_impl::Window::new(&self.p, window_attributes)?;
        Ok(Window { window })
    }

    /// Create custom cursor.
    pub fn create_custom_cursor(
        &self,
        custom_cursor: CustomCursorSource,
    ) -> CustomCursor {
        let _span =
            tracing::debug_span!("rio_window::ActiveEventLoop::create_custom_cursor",)
                .entered();

        self.p.create_custom_cursor(custom_cursor)
    }

    /// Returns the list of all the monitors available on the system.
    #[inline]
    pub fn available_monitors(&self) -> impl Iterator<Item = MonitorHandle> {
        let _span =
            tracing::debug_span!("rio_window::ActiveEventLoop::available_monitors",)
                .entered();

        #[allow(clippy::useless_conversion)] // false positive on some platforms
        self.p
            .available_monitors()
            .into_iter()
            .map(|inner| MonitorHandle { inner })
    }

    /// Returns the primary monitor of the system.
    ///
    /// Returns `None` if it can't identify any monitor as a primary one.
    ///
    /// ## Platform-specific
    ///
    /// **Wayland / Web:** Always returns `None`.
    #[inline]
    pub fn primary_monitor(&self) -> Option<MonitorHandle> {
        let _span = tracing::debug_span!("rio_window::ActiveEventLoop::primary_monitor",)
            .entered();

        self.p
            .primary_monitor()
            .map(|inner| MonitorHandle { inner })
    }

    /// Change if or when [`DeviceEvent`]s are captured.
    ///
    /// Since the [`DeviceEvent`] capture can lead to high CPU usage for unfocused windows, winit
    /// will ignore them by default for unfocused windows on Linux/BSD. This method allows changing
    /// this at runtime to explicitly capture them again.
    ///
    /// ## Platform-specific
    ///
    /// - **Wayland / macOS / iOS / Android / Orbital:** Unsupported.
    ///
    /// [`DeviceEvent`]: crate::event::DeviceEvent
    pub fn listen_device_events(&self, allowed: DeviceEvents) {
        let _span = tracing::debug_span!(
            "rio_window::ActiveEventLoop::listen_device_events",
            allowed = ?allowed
        )
        .entered();

        self.p.listen_device_events(allowed);
    }

    /// Returns the current system theme.
    ///
    /// Returns `None` if it cannot be determined on the current platform.
    ///
    /// ## Platform-specific
    ///
    /// - **iOS / Android / Wayland / x11 / Orbital:** Unsupported.
    pub fn system_theme(&self) -> Option<Theme> {
        self.p.system_theme()
    }

    /// Sets the [`ControlFlow`].
    pub fn set_control_flow(&self, control_flow: ControlFlow) {
        self.p.set_control_flow(control_flow)
    }

    /// Gets the current [`ControlFlow`].
    pub fn control_flow(&self) -> ControlFlow {
        self.p.control_flow()
    }

    /// This exits the event loop.
    ///
    /// See [`LoopExiting`][Event::LoopExiting].
    pub fn exit(&self) {
        let _span = tracing::debug_span!("rio_window::ActiveEventLoop::exit",).entered();

        self.p.exit()
    }

    /// Returns if the [`EventLoop`] is about to stop.
    ///
    /// See [`exit()`][Self::exit].
    pub fn exiting(&self) -> bool {
        self.p.exiting()
    }

    /// Gets a persistent reference to the underlying platform display.
    ///
    /// See the [`OwnedDisplayHandle`] type for more information.
    pub fn owned_display_handle(&self) -> OwnedDisplayHandle {
        OwnedDisplayHandle {
            platform: self.p.owned_display_handle(),
        }
    }
}

impl raw_window_handle::HasDisplayHandle for ActiveEventLoop {
    fn display_handle(
        &self,
    ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError>
    {
        let raw = self.p.raw_display_handle_raw_window_handle()?;
        // SAFETY: The display will never be deallocated while the event loop is alive.
        Ok(unsafe { raw_window_handle::DisplayHandle::borrow_raw(raw) })
    }
}

/// A proxy for the underlying display handle.
///
/// The purpose of this type is to provide a cheaply clonable handle to the underlying
/// display handle. This is often used by graphics APIs to connect to the underlying APIs.
/// It is difficult to keep a handle to the [`EventLoop`] type or the [`ActiveEventLoop`]
/// type. In contrast, this type involves no lifetimes and can be persisted for as long as
/// needed.
///
/// For all platforms, this is one of the following:
///
/// - A zero-sized type that is likely optimized out.
/// - A reference-counted pointer to the underlying type.
#[derive(Clone)]
pub struct OwnedDisplayHandle {
    #[allow(dead_code)]
    platform: platform_impl::OwnedDisplayHandle,
}

impl fmt::Debug for OwnedDisplayHandle {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OwnedDisplayHandle").finish_non_exhaustive()
    }
}

impl raw_window_handle::HasDisplayHandle for OwnedDisplayHandle {
    #[inline]
    fn display_handle(
        &self,
    ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError>
    {
        let raw = self.platform.raw_display_handle_raw_window_handle()?;

        // SAFETY: The underlying display handle should be safe.
        let handle = unsafe { raw_window_handle::DisplayHandle::borrow_raw(raw) };

        Ok(handle)
    }
}

/// Used to send custom events to [`EventLoop`].
pub struct EventLoopProxy<T: 'static> {
    event_loop_proxy: platform_impl::EventLoopProxy<T>,
}

impl<T: 'static> Clone for EventLoopProxy<T> {
    fn clone(&self) -> Self {
        Self {
            event_loop_proxy: self.event_loop_proxy.clone(),
        }
    }
}

impl<T: 'static> EventLoopProxy<T> {
    /// Send an event to the [`EventLoop`] from which this proxy was created. This emits a
    /// `UserEvent(event)` event in the event loop, where `event` is the value passed to this
    /// function.
    ///
    /// Returns an `Err` if the associated [`EventLoop`] no longer exists.
    ///
    /// [`UserEvent(event)`]: Event::UserEvent
    pub fn send_event(&self, event: T) -> Result<(), EventLoopClosed<T>> {
        let _span =
            tracing::debug_span!("rio_window::EventLoopProxy::send_event",).entered();

        self.event_loop_proxy.send_event(event)
    }
}

impl<T: 'static> fmt::Debug for EventLoopProxy<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("EventLoopProxy { .. }")
    }
}

/// The error that is returned when an [`EventLoopProxy`] attempts to wake up an [`EventLoop`] that
/// no longer exists.
///
/// Contains the original event given to [`EventLoopProxy::send_event`].
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct EventLoopClosed<T>(pub T);

impl<T> fmt::Display for EventLoopClosed<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Tried to wake up a closed `EventLoop`")
    }
}

impl<T: fmt::Debug> error::Error for EventLoopClosed<T> {}

/// Control when device events are captured.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub enum DeviceEvents {
    /// Report device events regardless of window focus.
    Always,
    /// Only capture device events while the window is focused.
    #[default]
    WhenFocused,
    /// Never capture device events.
    Never,
}

/// A unique identifier of the winit's async request.
///
/// This could be used to identify the async request once it's done
/// and a specific action must be taken.
///
/// One of the handling scenarios could be to maintain a working list
/// containing [`AsyncRequestSerial`] and some closure associated with it.
/// Then once event is arriving the working list is being traversed and a job
/// executed and removed from the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AsyncRequestSerial {
    serial: usize,
}

impl AsyncRequestSerial {
    // TODO(kchibisov): Remove `cfg` when the clipboard will be added.
    #[allow(dead_code)]
    pub(crate) fn get() -> Self {
        static CURRENT_SERIAL: AtomicUsize = AtomicUsize::new(0);
        // NOTE: We rely on wrap around here, while the user may just request
        // in the loop usize::MAX times that's issue is considered on them.
        let serial = CURRENT_SERIAL.fetch_add(1, Ordering::Relaxed);
        Self { serial }
    }
}

/// Shim for various run APIs.
#[inline(always)]
pub(crate) fn dispatch_event_for_app<T: 'static, A: ApplicationHandler<T>>(
    app: &mut A,
    event_loop: &ActiveEventLoop,
    event: Event<T>,
) {
    match event {
        Event::NewEvents(cause) => app.new_events(event_loop, cause),
        Event::WindowEvent { window_id, event } => {
            app.window_event(event_loop, window_id, event)
        }
        Event::DeviceEvent { device_id, event } => {
            app.device_event(event_loop, device_id, event)
        }
        Event::UserEvent(event) => app.user_event(event_loop, event),
        Event::Suspended => app.suspended(event_loop),
        Event::Resumed => app.resumed(event_loop),
        Event::AboutToWait => app.about_to_wait(event_loop),
        Event::LoopExiting => app.exiting(event_loop),
        Event::MemoryWarning => app.memory_warning(event_loop),
        Event::Opened { urls } => app.open_urls(event_loop, urls),
        Event::HookEvent(hook, modifiers) => {
            app.hook_event(event_loop, &hook, &modifiers)
        }
        Event::OpenConfig => app.open_config(event_loop),
    }
}
