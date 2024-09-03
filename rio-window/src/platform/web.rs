//! # Web
//!
//! The officially supported browsers are Chrome, Firefox and Safari 13.1+,
//! though forks of these should work fine.
//!
//! Winit supports compiling to the `wasm32-unknown-unknown` target with
//! `web-sys`.
//!
//! On the web platform, a Winit window is backed by a `<canvas>` element. You
//! can either [provide Winit with a `<canvas>` element][with_canvas], or
//! [let Winit create a `<canvas>` element which you can then retrieve][get]
//! and insert it into the DOM yourself.
//!
//! Currently, there is no example code using Winit on Web, see [#3473]. For
//! information on using Rust on WebAssembly, check out the [Rust and
//! WebAssembly book].
//!
//! [with_canvas]: WindowAttributesExtWebSys::with_canvas
//! [get]: WindowExtWebSys::canvas
//! [#3473]: https://github.com/rust-windowing/winit/issues/3473
//! [Rust and WebAssembly book]: https://rustwasm.github.io/book/
//!
//! ## CSS properties
//!
//! It is recommended **not** to apply certain CSS properties to the canvas:
//! - [`transform`](https://developer.mozilla.org/en-US/docs/Web/CSS/transform)
//! - [`border`](https://developer.mozilla.org/en-US/docs/Web/CSS/border)
//! - [`padding`](https://developer.mozilla.org/en-US/docs/Web/CSS/padding)
//!
//! The following APIs can't take them into account and will therefore provide inaccurate results:
//! - [`WindowEvent::Resized`] and [`Window::(set_)inner_size()`]
//! - [`WindowEvent::Occluded`]
//! - [`WindowEvent::CursorMoved`], [`WindowEvent::CursorEntered`], [`WindowEvent::CursorLeft`], and
//!   [`WindowEvent::Touch`].
//! - [`Window::set_outer_position()`]
//!
//! [`WindowEvent::Resized`]: crate::event::WindowEvent::Resized
//! [`Window::(set_)inner_size()`]: crate::window::Window::inner_size
//! [`WindowEvent::Occluded`]: crate::event::WindowEvent::Occluded
//! [`WindowEvent::CursorMoved`]: crate::event::WindowEvent::CursorMoved
//! [`WindowEvent::CursorEntered`]: crate::event::WindowEvent::CursorEntered
//! [`WindowEvent::CursorLeft`]: crate::event::WindowEvent::CursorLeft
//! [`WindowEvent::Touch`]: crate::event::WindowEvent::Touch
//! [`Window::set_outer_position()`]: crate::window::Window::set_outer_position

use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

#[cfg(web_platform)]
use web_sys::HtmlCanvasElement;

use crate::application::ApplicationHandler;
use crate::cursor::CustomCursorSource;
use crate::event::Event;
use crate::event_loop::{self, ActiveEventLoop, EventLoop};
#[cfg(web_platform)]
use crate::platform_impl::CustomCursorFuture as PlatformCustomCursorFuture;
use crate::platform_impl::PlatformCustomCursorSource;
use crate::window::{CustomCursor, Window, WindowAttributes};

#[cfg(not(web_platform))]
#[doc(hidden)]
pub struct HtmlCanvasElement;

pub trait WindowExtWebSys {
    /// Only returns the canvas if called from inside the window context (the
    /// main thread).
    fn canvas(&self) -> Option<HtmlCanvasElement>;

    /// Returns [`true`] if calling `event.preventDefault()` is enabled.
    ///
    /// See [`Window::set_prevent_default()`] for more details.
    fn prevent_default(&self) -> bool;

    /// Sets whether `event.preventDefault()` should be called on events on the
    /// canvas that have side effects.
    ///
    /// For example, by default using the mouse wheel would cause the page to scroll, enabling this
    /// would prevent that.
    ///
    /// Some events are impossible to prevent. E.g. Firefox allows to access the native browser
    /// context menu with Shift+Rightclick.
    fn set_prevent_default(&self, prevent_default: bool);
}

impl WindowExtWebSys for Window {
    #[inline]
    fn canvas(&self) -> Option<HtmlCanvasElement> {
        self.window.canvas()
    }

    fn prevent_default(&self) -> bool {
        self.window.prevent_default()
    }

    fn set_prevent_default(&self, prevent_default: bool) {
        self.window.set_prevent_default(prevent_default)
    }
}

pub trait WindowAttributesExtWebSys {
    /// Pass an [`HtmlCanvasElement`] to be used for this [`Window`]. If [`None`],
    /// [`WindowAttributes::default()`] will create one.
    ///
    /// In any case, the canvas won't be automatically inserted into the web page.
    ///
    /// [`None`] by default.
    #[cfg_attr(
        not(web_platform),
        doc = "",
        doc = "[`HtmlCanvasElement`]: #only-available-on-wasm"
    )]
    fn with_canvas(self, canvas: Option<HtmlCanvasElement>) -> Self;

    /// Sets whether `event.preventDefault()` should be called on events on the
    /// canvas that have side effects.
    ///
    /// See [`Window::set_prevent_default()`] for more details.
    ///
    /// Enabled by default.
    fn with_prevent_default(self, prevent_default: bool) -> Self;

    /// Whether the canvas should be focusable using the tab key. This is necessary to capture
    /// canvas keyboard events.
    ///
    /// Enabled by default.
    fn with_focusable(self, focusable: bool) -> Self;

    /// On window creation, append the canvas element to the web page if it isn't already.
    ///
    /// Disabled by default.
    fn with_append(self, append: bool) -> Self;
}

impl WindowAttributesExtWebSys for WindowAttributes {
    fn with_canvas(mut self, canvas: Option<HtmlCanvasElement>) -> Self {
        self.platform_specific.set_canvas(canvas);
        self
    }

    fn with_prevent_default(mut self, prevent_default: bool) -> Self {
        self.platform_specific.prevent_default = prevent_default;
        self
    }

    fn with_focusable(mut self, focusable: bool) -> Self {
        self.platform_specific.focusable = focusable;
        self
    }

    fn with_append(mut self, append: bool) -> Self {
        self.platform_specific.append = append;
        self
    }
}

/// Additional methods on `EventLoop` that are specific to the web.
pub trait EventLoopExtWebSys {
    /// A type provided by the user that can be passed through `Event::UserEvent`.
    type UserEvent: 'static;

    /// Initializes the winit event loop.
    ///
    /// Unlike
    #[cfg_attr(all(web_platform, target_feature = "exception-handling"), doc = "`run_app()`")]
    #[cfg_attr(
        not(all(web_platform, target_feature = "exception-handling")),
        doc = "[`run_app()`]"
    )]
    /// [^1], this returns immediately, and doesn't throw an exception in order to
    /// satisfy its [`!`] return type.
    ///
    /// Once the event loop has been destroyed, it's possible to reinitialize another event loop
    /// by calling this function again. This can be useful if you want to recreate the event loop
    /// while the WebAssembly module is still loaded. For example, this can be used to recreate the
    /// event loop when switching between tabs on a single page application.
    #[rustfmt::skip]
    ///
    #[cfg_attr(
        not(all(web_platform, target_feature = "exception-handling")),
        doc = "[`run_app()`]: EventLoop::run_app()"
    )]
    /// [^1]: `run_app()` is _not_ available on WASM when the target supports `exception-handling`.
    fn spawn_app<A: ApplicationHandler<Self::UserEvent> + 'static>(self, app: A);

    /// See [`spawn_app`].
    ///
    /// [`spawn_app`]: Self::spawn_app
    #[deprecated = "use EventLoopExtWebSys::spawn_app"]
    fn spawn<F>(self, event_handler: F)
    where
        F: 'static + FnMut(Event<Self::UserEvent>, &ActiveEventLoop);
}

impl<T> EventLoopExtWebSys for EventLoop<T> {
    type UserEvent = T;

    fn spawn_app<A: ApplicationHandler<Self::UserEvent> + 'static>(self, mut app: A) {
        self.event_loop.spawn(move |event, event_loop| {
            event_loop::dispatch_event_for_app(&mut app, event_loop, event)
        });
    }

    fn spawn<F>(self, event_handler: F)
    where
        F: 'static + FnMut(Event<Self::UserEvent>, &ActiveEventLoop),
    {
        self.event_loop.spawn(event_handler)
    }
}

pub trait ActiveEventLoopExtWebSys {
    /// Sets the strategy for [`ControlFlow::Poll`].
    ///
    /// See [`PollStrategy`].
    ///
    /// [`ControlFlow::Poll`]: crate::event_loop::ControlFlow::Poll
    fn set_poll_strategy(&self, strategy: PollStrategy);

    /// Gets the strategy for [`ControlFlow::Poll`].
    ///
    /// See [`PollStrategy`].
    ///
    /// [`ControlFlow::Poll`]: crate::event_loop::ControlFlow::Poll
    fn poll_strategy(&self) -> PollStrategy;

    /// Async version of [`ActiveEventLoop::create_custom_cursor()`] which waits until the
    /// cursor has completely finished loading.
    fn create_custom_cursor_async(
        &self,
        source: CustomCursorSource,
    ) -> CustomCursorFuture;
}

impl ActiveEventLoopExtWebSys for ActiveEventLoop {
    #[inline]
    fn create_custom_cursor_async(
        &self,
        source: CustomCursorSource,
    ) -> CustomCursorFuture {
        self.p.create_custom_cursor_async(source)
    }

    #[inline]
    fn set_poll_strategy(&self, strategy: PollStrategy) {
        self.p.set_poll_strategy(strategy);
    }

    #[inline]
    fn poll_strategy(&self) -> PollStrategy {
        self.p.poll_strategy()
    }
}

/// Strategy used for [`ControlFlow::Poll`][crate::event_loop::ControlFlow::Poll].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PollStrategy {
    /// Uses [`Window.requestIdleCallback()`] to queue the next event loop. If not available
    /// this will fallback to [`setTimeout()`].
    ///
    /// This strategy will wait for the browser to enter an idle period before running and might
    /// be affected by browser throttling.
    ///
    /// [`Window.requestIdleCallback()`]: https://developer.mozilla.org/en-US/docs/Web/API/Window/requestIdleCallback
    /// [`setTimeout()`]: https://developer.mozilla.org/en-US/docs/Web/API/setTimeout
    IdleCallback,
    /// Uses the [Prioritized Task Scheduling API] to queue the next event loop. If not available
    /// this will fallback to [`setTimeout()`].
    ///
    /// This strategy will run as fast as possible without disturbing users from interacting with
    /// the page and is not affected by browser throttling.
    ///
    /// This is the default strategy.
    ///
    /// [Prioritized Task Scheduling API]: https://developer.mozilla.org/en-US/docs/Web/API/Prioritized_Task_Scheduling_API
    /// [`setTimeout()`]: https://developer.mozilla.org/en-US/docs/Web/API/setTimeout
    #[default]
    Scheduler,
}

pub trait CustomCursorExtWebSys {
    /// Returns if this cursor is an animation.
    fn is_animation(&self) -> bool;

    /// Creates a new cursor from a URL pointing to an image.
    /// It uses the [url css function](https://developer.mozilla.org/en-US/docs/Web/CSS/url),
    /// but browser support for image formats is inconsistent. Using [PNG] is recommended.
    ///
    /// [PNG]: https://en.wikipedia.org/wiki/PNG
    fn from_url(url: String, hotspot_x: u16, hotspot_y: u16) -> CustomCursorSource;

    /// Crates a new animated cursor from multiple [`CustomCursor`]s.
    /// Supplied `cursors` can't be empty or other animations.
    fn from_animation(
        duration: Duration,
        cursors: Vec<CustomCursor>,
    ) -> Result<CustomCursorSource, BadAnimation>;
}

impl CustomCursorExtWebSys for CustomCursor {
    fn is_animation(&self) -> bool {
        self.inner.animation
    }

    fn from_url(url: String, hotspot_x: u16, hotspot_y: u16) -> CustomCursorSource {
        CustomCursorSource {
            inner: PlatformCustomCursorSource::Url {
                url,
                hotspot_x,
                hotspot_y,
            },
        }
    }

    fn from_animation(
        duration: Duration,
        cursors: Vec<CustomCursor>,
    ) -> Result<CustomCursorSource, BadAnimation> {
        if cursors.is_empty() {
            return Err(BadAnimation::Empty);
        }

        if cursors.iter().any(CustomCursor::is_animation) {
            return Err(BadAnimation::Animation);
        }

        Ok(CustomCursorSource {
            inner: PlatformCustomCursorSource::Animation { duration, cursors },
        })
    }
}

/// An error produced when using [`CustomCursor::from_animation`] with invalid arguments.
#[derive(Debug, Clone)]
pub enum BadAnimation {
    /// Produced when no cursors were supplied.
    Empty,
    /// Produced when a supplied cursor is an animation.
    Animation,
}

impl fmt::Display for BadAnimation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "No cursors supplied"),
            Self::Animation => write!(f, "A supplied cursor is an animtion"),
        }
    }
}

impl Error for BadAnimation {}

#[cfg(not(web_platform))]
struct PlatformCustomCursorFuture;

#[derive(Debug)]
pub struct CustomCursorFuture(pub(crate) PlatformCustomCursorFuture);

impl Future for CustomCursorFuture {
    type Output = Result<CustomCursor, CustomCursorError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.0)
            .poll(cx)
            .map_ok(|cursor| CustomCursor { inner: cursor })
    }
}

#[derive(Clone, Debug)]
pub enum CustomCursorError {
    Blob,
    Decode(String),
    Animation,
}

impl Display for CustomCursorError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Blob => write!(f, "failed to create `Blob`"),
            Self::Decode(error) => write!(f, "failed to decode image: {error}"),
            Self::Animation => {
                write!(f, "found `CustomCursor` that is an animation when building an animation")
            }
        }
    }
}
