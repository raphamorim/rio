use crate::{
    error::EventLoopError,
    event::Event,
    event_loop::{EventLoop, EventLoopWindowTarget},
};

#[cfg(doc)]
use crate::{platform::pump_events::EventLoopExtPumpEvents, window::Window};

/// Additional methods on [`EventLoop`] to return control flow to the caller.
pub trait EventLoopExtRunOnDemand {
    /// A type provided by the user that can be passed through [`Event::UserEvent`].
    type UserEvent;

    /// Runs the event loop in the calling thread and calls the given `event_handler` closure
    /// to dispatch any window system events.
    ///
    /// Unlike [`EventLoop::run`], this function accepts non-`'static` (i.e. non-`move`) closures
    /// and it is possible to return control back to the caller without
    /// consuming the `EventLoop` (by using [`exit()`]) and
    /// so the event loop can be re-run after it has exit.
    ///
    /// It's expected that each run of the loop will be for orthogonal instantiations of your
    /// Winit application, but internally each instantiation may re-use some common window
    /// system resources, such as a display server connection.
    ///
    /// This API is not designed to run an event loop in bursts that you can exit from and return
    /// to while maintaining the full state of your application. (If you need something like this
    /// you can look at the [`EventLoopExtPumpEvents::pump_events()`] API)
    ///
    /// Each time `run_on_demand` is called the `event_handler` can expect to receive a
    /// `NewEvents(Init)` and `Resumed` event (even on platforms that have no suspend/resume
    /// lifecycle) - which can be used to consistently initialize application state.
    ///
    /// See the [`set_control_flow()`] docs on how to change the event loop's behavior.
    ///
    /// # Caveats
    /// - This extension isn't available on all platforms, since it's not always possible to return
    ///   to the caller (specifically this is impossible on iOS and Web - though with the Web
    ///   backend it is possible to use `EventLoopExtWebSys::spawn()`[^1] more than once instead).
    /// - No [`Window`] state can be carried between separate runs of the event loop.
    ///
    /// You are strongly encouraged to use [`EventLoop::run()`] for portability, unless you specifically need
    /// the ability to re-run a single event loop more than once
    ///
    /// # Supported Platforms
    /// - Windows
    /// - Linux
    /// - macOS
    /// - Android
    ///
    /// # Unsupported Platforms
    /// - **Web:**  This API is fundamentally incompatible with the event-based way in which
    ///   Web browsers work because it's not possible to have a long-running external
    ///   loop that would block the browser and there is nothing that can be
    ///   polled to ask for new events. Events are delivered via callbacks based
    ///   on an event loop that is internal to the browser itself.
    /// - **iOS:** It's not possible to stop and start an `NSApplication` repeatedly on iOS.
    ///
    #[cfg_attr(
        not(wasm_platform),
        doc = "[^1]: `spawn()` is only available on `wasm` platforms."
    )]
    ///
    /// [`exit()`]: EventLoopWindowTarget::exit()
    /// [`set_control_flow()`]: EventLoopWindowTarget::set_control_flow()
    fn run_on_demand<F>(&mut self, event_handler: F) -> Result<(), EventLoopError>
    where
        F: FnMut(Event<Self::UserEvent>, &EventLoopWindowTarget<Self::UserEvent>);
}

impl<T> EventLoopExtRunOnDemand for EventLoop<T> {
    type UserEvent = T;

    fn run_on_demand<F>(&mut self, event_handler: F) -> Result<(), EventLoopError>
    where
        F: FnMut(Event<Self::UserEvent>, &EventLoopWindowTarget<Self::UserEvent>),
    {
        self.event_loop.run_on_demand(event_handler)
    }
}
