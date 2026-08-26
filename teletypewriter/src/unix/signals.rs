use corcovado::event::Evented;
use corcovado::stream;
use corcovado::{Poll, PollOpt, Ready, Token};
use std::io::Error;

macro_rules! implement_signals_with_pipe {
    ($pipe:path) => {
        use std::borrow::Borrow;

        use signal_hook::iterator::backend::{self, SignalDelivery};
        use signal_hook::iterator::exfiltrator::{Exfiltrator, SignalOnly};

        use $pipe as Pipe;

        use libc::c_int;

        /// A struct which mimics [`signal_hook::iterator::SignalsInfo`]
        /// but also allows usage together with MIO runtime.
        pub struct SignalsInfo<E: Exfiltrator = SignalOnly>(SignalDelivery<Pipe, E>);

        pub use backend::Pending;

        impl<E: Exfiltrator> SignalsInfo<E> {
            /// Create a `Signals` instance.
            ///
            /// This registers all the signals listed. The same restrictions (panics, errors) apply
            /// as with [`Handle::add_signal`][backend::Handle::add_signal].
            pub fn new<I, S>(signals: I) -> Result<Self, Error>
            where
                I: IntoIterator<Item = S>,
                S: Borrow<c_int>,
                E: Default,
            {
                Self::with_exfiltrator(signals, E::default())
            }

            /// A constructor with specifying an exfiltrator to pass information out of the signal
            /// handlers.
            pub fn with_exfiltrator<I, S>(
                signals: I,
                exfiltrator: E,
            ) -> Result<Self, Error>
            where
                I: IntoIterator<Item = S>,
                S: Borrow<c_int>,
            {
                let (read, write) = Pipe::pair()?;
                let delivery =
                    SignalDelivery::with_pipe(read, write, exfiltrator, signals)?;
                Ok(Self(delivery))
            }

            /// Registers another signal to the set watched by this [`Signals`] instance.
            ///
            /// The same restrictions (panics, errors) apply as with
            /// [`Handle::add_signal`][backend::Handle::add_signal].
            #[allow(unused)]
            pub fn add_signal(&self, signal: c_int) -> Result<(), Error> {
                self.0.handle().add_signal(signal)
            }

            /// Returns an iterator of already received signals.
            ///
            /// This returns an iterator over all the signal numbers of the signals received since last
            /// time they were read (out of the set registered by this `Signals` instance). Note that they
            /// are returned in arbitrary order and a signal number is returned only once even if it was
            /// received multiple times.
            ///
            /// This method returns immediately (does not block) and may produce an empty iterator if there
            /// are no signals ready. So you should register an instance of this struct at an instance of
            /// [`corcovado::Poll`] to query for readability of the underlying self pipe.
            pub fn pending(&mut self) -> Pending<E> {
                self.0.pending()
            }
        }

        /// A simplified signal iterator.
        ///
        /// This is the [`SignalsInfo`], but returning only the signal numbers. This is likely the
        /// one you want to use.
        pub type Signals = SignalsInfo<SignalOnly>;
    };
}

implement_signals_with_pipe!(stream::UnixStream);

impl Evented for Signals {
    fn register(
        &self,
        poll: &Poll,
        token: Token,
        interest: Ready,
        opts: PollOpt,
    ) -> Result<(), Error> {
        self.0.get_read().register(poll, token, interest, opts)
    }

    fn reregister(
        &self,
        poll: &Poll,
        token: Token,
        interest: Ready,
        opts: PollOpt,
    ) -> Result<(), Error> {
        self.0.get_read().reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> Result<(), Error> {
        self.0.get_read().deregister(poll)
    }
}
