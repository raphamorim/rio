use lazycell::{AtomicLazyCell, LazyCell};
use mio::net::TcpStream;
use mio::{event::Source, Interest, Registry, Token};
use std::any::Any;
use std::error;
use std::net::SocketAddr;
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::{fmt, io};

/// Creates a new asynchronous channel, where the `Receiver` can be registered
/// with `Poll`.
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let (tx_ctl, rx_ctl) = ctl_pair();
    let (tx, rx) = mpsc::channel();

    let tx = Sender { tx, ctl: tx_ctl };

    let rx = Receiver { rx, ctl: rx_ctl };

    (tx, rx)
}

/// Creates a new synchronous, bounded channel where the `Receiver` can be
/// registered with `Poll`.
pub fn sync_channel<T>(bound: usize) -> (SyncSender<T>, Receiver<T>) {
    let (tx_ctl, rx_ctl) = ctl_pair();
    let (tx, rx) = mpsc::sync_channel(bound);

    let tx = SyncSender { tx, ctl: tx_ctl };

    let rx = Receiver { rx, ctl: rx_ctl };

    (tx, rx)
}

fn ctl_pair() -> (SenderCtl, ReceiverCtl) {
    let inner = Arc::new(Inner {
        pending: AtomicUsize::new(0),
        senders: AtomicUsize::new(1),
        interest: AtomicLazyCell::new(),
    });

    let tx = SenderCtl {
        inner: Arc::clone(&inner),
    };

    let rx = ReceiverCtl {
        registration: LazyCell::new(),
        inner,
    };

    (tx, rx)
}

/// Tracks messages sent on a channel in order to update readiness.
struct SenderCtl {
    inner: Arc<Inner>,
}

/// Tracks messages received on a channel in order to track readiness.
struct ReceiverCtl {
    registration: LazyCell<mio::unix::pipe::Receiver>,
    inner: Arc<Inner>,
}

/// The sending half of a channel.
pub struct Sender<T> {
    tx: mpsc::Sender<T>,
    ctl: SenderCtl,
}

/// The sending half of a synchronous channel.
pub struct SyncSender<T> {
    tx: mpsc::SyncSender<T>,
    ctl: SenderCtl,
}

/// The receiving half of a channel.
pub struct Receiver<T> {
    rx: mpsc::Receiver<T>,
    ctl: ReceiverCtl,
}

/// An error returned from the `Sender::send` or `SyncSender::send` function.
pub enum SendError<T> {
    /// An IO error.
    Io(io::Error),

    /// The receiving half of the channel has disconnected.
    Disconnected(T),
}

/// An error returned from the `SyncSender::try_send` function.
pub enum TrySendError<T> {
    /// An IO error.
    Io(io::Error),

    /// Data could not be sent because it would require the callee to block.
    Full(T),

    /// The receiving half of the channel has disconnected.
    Disconnected(T),
}

struct Inner {
    // The number of outstanding messages for the receiver to read
    pending: AtomicUsize,
    // The number of sender handles
    senders: AtomicUsize,
    // The set readiness handle
    interest: AtomicLazyCell<Interest>,
}

impl<T> Sender<T> {
    /// Attempts to send a value on this channel, returning it back if it could not be sent.
    pub fn send(&self, t: T) -> Result<(), SendError<T>> {
        self.tx.send(t).map_err(SendError::from).and_then(|_| {
            self.ctl.inc()?;
            Ok(())
        })
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Sender<T> {
        Sender {
            tx: self.tx.clone(),
            ctl: self.ctl.clone(),
        }
    }
}

impl<T> SyncSender<T> {
    /// Sends a value on this synchronous channel.
    ///
    /// This function will *block* until space in the internal buffer becomes
    /// available or a receiver is available to hand off the message to.
    pub fn send(&self, t: T) -> Result<(), SendError<T>> {
        self.tx.send(t).map_err(From::from).and_then(|_| {
            self.ctl.inc()?;
            Ok(())
        })
    }

    /// Attempts to send a value on this channel without blocking.
    ///
    /// This method differs from `send` by returning immediately if the channel's
    /// buffer is full or no receiver is waiting to acquire some data.
    pub fn try_send(&self, t: T) -> Result<(), TrySendError<T>> {
        self.tx.try_send(t).map_err(From::from).and_then(|_| {
            self.ctl.inc()?;
            Ok(())
        })
    }
}

impl<T> Clone for SyncSender<T> {
    fn clone(&self) -> SyncSender<T> {
        SyncSender {
            tx: self.tx.clone(),
            ctl: self.ctl.clone(),
        }
    }
}

impl<T> Receiver<T> {
    /// Attempts to return a pending value on this receiver without blocking.
    pub fn try_recv(&self) -> Result<T, mpsc::TryRecvError> {
        self.rx.try_recv().map(|res| {
            let _ = self.ctl.dec();
            res
        })
    }
}

impl<T> Source for Receiver<T> {
    fn register(
        self: &mut Receiver<T>,
        registry: &Registry,
        token: Token,
        interest: Interest,
    ) -> io::Result<()> {
        self.ctl.register(registry, token, interest)
    }

    fn reregister(
        self: &mut Receiver<T>,
        registry: &Registry,
        token: Token,
        interest: Interest,
    ) -> io::Result<()> {
        self.ctl.reregister(registry, token, interest)
    }

    fn deregister(self: &mut Receiver<T>, registry: &Registry) -> io::Result<()> {
        self.ctl.deregister(registry)
    }
}

/*
 *
 * ===== SenderCtl / ReceiverCtl =====
 *
 */

impl SenderCtl {
    /// Call to track that a message has been sent
    fn inc(&self) -> io::Result<()> {
        let cnt = self.inner.pending.fetch_add(1, Ordering::Acquire);

        if 0 == cnt {
            // Toggle readiness to readable
            if let Some(interest) = self.inner.interest.borrow() {
                interest.add(Interest::READABLE);
            }
        }

        Ok(())
    }
}

impl Clone for SenderCtl {
    fn clone(&self) -> SenderCtl {
        self.inner.senders.fetch_add(1, Ordering::Relaxed);
        SenderCtl {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl Drop for SenderCtl {
    fn drop(&mut self) {
        if self.inner.senders.fetch_sub(1, Ordering::Release) == 1 {
            let _ = self.inc();
        }
    }
}

impl ReceiverCtl {
    fn dec(&self) -> io::Result<()> {
        let first = self.inner.pending.load(Ordering::Acquire);

        if first == 1 {
            // Unset readiness
            if let Some(interest) = self.inner.interest.borrow() {
                interest.remove(Interest::READABLE);
            }
        }

        // Decrement
        let second = self.inner.pending.fetch_sub(1, Ordering::AcqRel);

        if first == 1 && second > 1 {
            // There are still pending messages. Since readiness was
            // previously unset, it must be reset here
            if let Some(interest) = self.inner.interest.borrow() {
                interest.add(Interest::READABLE);
            }
        }

        Ok(())
    }
}

impl Source for ReceiverCtl {
    fn register(
        self: &mut ReceiverCtl,
        registry: &Registry,
        token: Token,
        interest: Interest,
    ) -> io::Result<()> {
        if self.registration.borrow_mut().is_some() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "receiver already registered",
            ));
        }

        // let address: SocketAddr = "127.0.0.1:34255".parse().unwrap();
        // let listener = TcpListener::bind(address).unwrap();
        // listener.set_nonblocking(true).expect("Cannot set non-blocking");
        // let mut stream = TcpStream::connect(listener.local_addr()?)?;
        // let (mut stream1, stream2) = mio::net::UnixStream::pair()?;
        let (sender, mut receiver) = mio::unix::pipe::new()?;
        receiver.set_nonblocking(true).expect("Cannot set non-blocking");

        registry.register(&mut receiver, token, interest)?;

        if self.inner.pending.load(Ordering::Relaxed) > 0 {
            // TODO: Don't drop readiness
            let _ = interest.add(Interest::READABLE);
        }

        self.registration
            .fill(receiver)
            .expect("unexpected state encountered");
        self.inner
            .interest
            .fill(interest)
            .expect("unexpected state encountered");

        Ok(())
    }

    fn reregister(
        self: &mut ReceiverCtl,
        registry: &Registry,
        token: Token,
        interest: Interest,
    ) -> io::Result<()> {
        match self.registration.borrow_mut() {
            Some(registration) => registry.reregister(registration, token, interest),
            None => Err(io::Error::new(
                io::ErrorKind::Other,
                "receiver not registered",
            )),
        }
    }

    fn deregister(self: &mut ReceiverCtl, registry: &Registry) -> io::Result<()> {
        match self.registration.borrow_mut() {
            Some(registration) => registry.deregister(registration),
            None => Err(io::Error::new(
                io::ErrorKind::Other,
                "receiver not registered",
            )),
        }
    }
}

/*
 *
 * ===== Error conversions =====
 *
 */

impl<T> From<mpsc::SendError<T>> for SendError<T> {
    fn from(src: mpsc::SendError<T>) -> SendError<T> {
        SendError::Disconnected(src.0)
    }
}

impl<T> From<io::Error> for SendError<T> {
    fn from(src: io::Error) -> SendError<T> {
        SendError::Io(src)
    }
}

impl<T> From<mpsc::TrySendError<T>> for TrySendError<T> {
    fn from(src: mpsc::TrySendError<T>) -> TrySendError<T> {
        match src {
            mpsc::TrySendError::Full(v) => TrySendError::Full(v),
            mpsc::TrySendError::Disconnected(v) => TrySendError::Disconnected(v),
        }
    }
}

impl<T> From<mpsc::SendError<T>> for TrySendError<T> {
    fn from(src: mpsc::SendError<T>) -> TrySendError<T> {
        TrySendError::Disconnected(src.0)
    }
}

impl<T> From<io::Error> for TrySendError<T> {
    fn from(src: io::Error) -> TrySendError<T> {
        TrySendError::Io(src)
    }
}

/*
 *
 * ===== Implement Error, Debug and Display for Errors =====
 *
 */

impl<T: Any> error::Error for SendError<T> {}

impl<T: Any> error::Error for TrySendError<T> {}

impl<T> fmt::Debug for SendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        format_send_error(self, f)
    }
}

impl<T> fmt::Display for SendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        format_send_error(self, f)
    }
}

impl<T> fmt::Debug for TrySendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        format_try_send_error(self, f)
    }
}

impl<T> fmt::Display for TrySendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        format_try_send_error(self, f)
    }
}

#[inline]
fn format_send_error<T>(e: &SendError<T>, f: &mut fmt::Formatter) -> fmt::Result {
    match *e {
        SendError::Io(ref io_err) => write!(f, "{}", io_err),
        SendError::Disconnected(..) => write!(f, "Disconnected"),
    }
}

#[inline]
fn format_try_send_error<T>(e: &TrySendError<T>, f: &mut fmt::Formatter) -> fmt::Result {
    match *e {
        TrySendError::Io(ref io_err) => write!(f, "{}", io_err),
        TrySendError::Full(..) => write!(f, "Full"),
        TrySendError::Disconnected(..) => write!(f, "Disconnected"),
    }
}
