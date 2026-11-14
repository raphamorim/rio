# Corcovado

Corcovado is a maintained fork of mio 0.6.x (along mio-signal-hook and mio-extras) trimmed down to what Rio's PTY event loop needs:

- `Poll`/`Events` readiness polling backed by epoll, kqueue, and IOCP (works on Windows 11).
- A pollable cross-thread `channel`.
- User-space readiness via `Registration`/`SetReadiness`.
- `EventedFd` and a Unix-domain `stream::UnixStream` (used for signal handling).

Compared to mio 0.6.x, the networking types (TCP/UDP), the timer, and the Fuchsia backend were removed, it uses the Rust standard library for net and io, and it builds with Rust edition 2021.
