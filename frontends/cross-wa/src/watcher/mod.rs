use crate::event::RioEvent;
use corcovado::Events;
use corcovado::PollOpt;
use corcovado::Ready;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use rio_backend::superloop::Superloop;
use std::io::ErrorKind;
use std::thread::{Builder, JoinHandle};
use std::time::Duration;
use std::time::Instant;

const CONFIG_POLLING_TIMEOUT: Duration = Duration::from_secs(2);
const APP_POLLING_TIMEOUT: Duration = Duration::from_secs(1);

use wa::native::apple::menu::RepresentedItem;
use wa::KeyAssignment;

/// Like `thread::spawn`, but with a `name` argument.
pub fn spawn_named<F, T, S>(name: S, f: F) -> JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
    S: Into<String>,
{
    Builder::new()
        .name(name.into())
        .spawn(f)
        .expect("thread spawn works")
}

pub fn watch_app(receiver: corcovado::channel::Receiver<RepresentedItem>) {
    #[cfg(target_os = "macos")]
    {
        spawn_named("App connection reader", move || {
            let poll = corcovado::Poll::new().unwrap();

            let poll_opts = PollOpt::edge() | PollOpt::oneshot();
            let mut tokens = (0..).map(Into::into);

            let channel_token = tokens.next().unwrap();
            poll.register(&receiver, channel_token, Ready::readable(), poll_opts)
                .unwrap();

            let mut events = Events::with_capacity(1024);

            'event_loop: loop {
                let sync_timeout = Some(Instant::now() + APP_POLLING_TIMEOUT);
                let timeout =
                    sync_timeout.map(|st| st.saturating_duration_since(Instant::now()));

                if let Err(err) = poll.poll(&mut events, timeout) {
                    match err.kind() {
                        ErrorKind::Interrupted => continue,
                        _ => panic!("EventLoop polling error: {err:?}"),
                    }
                }

                if events.is_empty() {
                    continue;
                }

                for event in events.iter() {
                    match event.token() {
                        token if token == channel_token => {
                            while let Ok(msg) = receiver.try_recv() {
                                match msg {
                                    RepresentedItem::KeyAssignment(
                                        KeyAssignment::SpawnWindow,
                                    ) => {
                                        println!("SpawnWindow");
                                    }
                                    _ => {}
                                }
                            }

                            if poll
                                .reregister(
                                    &receiver,
                                    token,
                                    Ready::readable(),
                                    PollOpt::edge() | PollOpt::oneshot(),
                                )
                                .is_err()
                            {
                                break 'event_loop;
                            }

                            // In case should shutdown by message
                            // break 'event_loop;
                        }
                        _ => {
                            #[cfg(unix)]
                            use corcovado::unix::UnixReady;

                            #[cfg(unix)]
                            if UnixReady::from(event.readiness()).is_hup() {
                                // Don't try to do I/O on a dead PTY.
                                continue;
                            }
                        }
                    }

                    poll.reregister(
                        &receiver,
                        channel_token,
                        Ready::readable(),
                        poll_opts,
                    )
                    .unwrap();
                }
            }

            // The evented instances are not dropped here so deregister them explicitly.
            let _ = poll.deregister(&receiver);
        });
    }
}

pub fn watch_config_file(mut event_proxy: Superloop) -> notify::Result<()> {
    let path = rio_backend::config::config_dir_path();
    let (tx, rx) = std::sync::mpsc::channel();

    // Automatically select the best implementation for your platform.
    // You can also access each implementation directly e.g. INotifyWatcher.
    let mut watcher = RecommendedWatcher::new(
        tx,
        Config::default().with_poll_interval(CONFIG_POLLING_TIMEOUT),
    )?;

    tokio::spawn(async move {
        // Add a path to be watched. All files and directories at that path and
        // below will be monitored for changes.
        if let Err(err_message) =
            watcher.watch(path.as_ref(), RecursiveMode::NonRecursive)
        {
            log::warn!("unable to watch config directory {err_message:?}");
        };

        for res in rx {
            match res {
                Ok(event) => match event.kind {
                    EventKind::Any
                    | EventKind::Create(_)
                    | EventKind::Modify(_)
                    | EventKind::Other => {
                        log::info!("config directory has dispatched an event {event:?}");
                        // TODO: Refactor to send_global_event
                        event_proxy.send_event(RioEvent::UpdateConfig, 0);
                    }
                    _ => (),
                },
                Err(err_message) => {
                    log::error!("unable to watch config directory: {err_message:?}")
                }
            }
        }
    });

    Ok(())
}
