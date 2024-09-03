use crate::application::ApplicationHandler;
use crate::error::EventLoopError;
use crate::event::Event;
use crate::event_loop::{self, ActiveEventLoop, EventLoop};

#[cfg(doc)]
use crate::{platform::pump_events::EventLoopExtPumpEvents, window::Window};

/// Additional methods on [`EventLoop`] to return control flow to the caller.
pub trait EventLoopExtRunOnDemand {
    /// A type provided by the user that can be passed through [`Event::UserEvent`].
    type UserEvent: 'static;

    /// See [`run_app_on_demand`].
    ///
    /// [`run_app_on_demand`]: Self::run_app_on_demand
    #[deprecated = "use EventLoopExtRunOnDemand::run_app_on_demand"]
    fn run_on_demand<F>(&mut self, event_handler: F) -> Result<(), EventLoopError>
    where
        F: FnMut(Event<Self::UserEvent>, &ActiveEventLoop);

    /// Run the application with the event loop on the calling thread.
    ///
    /// Unlike [`EventLoop::run_app`], this function accepts non-`'static` (i.e. non-`move`)
    /// closures and it is possible to return control back to the caller without
    /// consuming the `EventLoop` (by using [`exit()`]) and
    /// so the event loop can be re-run after it has exit.
    ///
    /// It's expected that each run of the loop will be for orthogonal instantiations of your
    /// Winit application, but internally each instantiation may re-use some common window
    /// system resources, such as a display server connection.
    ///
    /// This API is not designed to run an event loop in bursts that you can exit from and return
    /// to while maintaining the full state of your application. (If you need something like this
    /// you can look at the [`EventLoopExtPumpEvents::pump_app_events()`] API)
    ///
    /// Each time `run_app_on_demand` is called the startup sequence of `init`, followed by
    /// `resume` is being preserved.
    ///
    /// See the [`set_control_flow()`] docs on how to change the event loop's behavior.
    ///
    /// # Caveats
    /// - This extension isn't available on all platforms, since it's not always possible to return
    ///   to the caller (specifically this is impossible on iOS and Web - though with the Web
    ///   backend it is possible to use `EventLoopExtWebSys::spawn()`[^1] more than once instead).
    /// - No [`Window`] state can be carried between separate runs of the event loop.
    ///
    /// You are strongly encouraged to use [`EventLoop::run_app()`] for portability, unless you
    /// specifically need the ability to re-run a single event loop more than once
    ///
    /// # Supported Platforms
    /// - Windows
    /// - Linux
    /// - macOS
    /// - Android
    ///
    /// # Unsupported Platforms
    /// - **Web:**  This API is fundamentally incompatible with the event-based way in which Web
    ///   browsers work because it's not possible to have a long-running external loop that would
    ///   block the browser and there is nothing that can be polled to ask for new events. Events
    ///   are delivered via callbacks based on an event loop that is internal to the browser itself.
    /// - **iOS:** It's not possible to stop and start an `UIApplication` repeatedly on iOS.
    #[cfg_attr(not(web_platform), doc = "[^1]: `spawn()` is only available on `wasm` platforms.")]
    #[rustfmt::skip]
    ///
    /// [`exit()`]: ActiveEventLoop::exit()
    /// [`set_control_flow()`]: ActiveEventLoop::set_control_flow()
    fn run_app_on_demand<A: ApplicationHandler<Self::UserEvent>>(
        &mut self,
        app: &mut A,
    ) -> Result<(), EventLoopError> {
        #[allow(deprecated)]
        self.run_on_demand(|event, event_loop| {
            event_loop::dispatch_event_for_app(app, event_loop, event)
        })
    }
}

impl<T> EventLoopExtRunOnDemand for EventLoop<T> {
    type UserEvent = T;

    fn run_on_demand<F>(&mut self, event_handler: F) -> Result<(), EventLoopError>
    where
        F: FnMut(Event<Self::UserEvent>, &ActiveEventLoop),
    {
        self.event_loop.window_target().clear_exit();
        self.event_loop.run_on_demand(event_handler)
    }
}

impl ActiveEventLoop {
    /// Clear exit status.
    pub(crate) fn clear_exit(&self) {
        self.p.clear_exit()
    }
}

/// ```compile_fail
/// use rio_window::event_loop::EventLoop;
/// use rio_window::platform::run_on_demand::EventLoopExtRunOnDemand;
///
/// let mut event_loop = EventLoop::new().unwrap();
/// event_loop.run_on_demand(|_, _| {
///     // Attempt to run the event loop re-entrantly; this must fail.
///     event_loop.run_on_demand(|_, _| {});
/// });
/// ```
#[allow(dead_code)]
fn test_run_on_demand_cannot_access_event_loop() {}
