# A multibuffer protocol for Rio — terminal multiplexing at the protocol level

*2026-08-01. Four-agent research sweep (rio architecture, tmux control mode,
modern mux architectures, historical/academic prior art) synthesized. Sibling
document to `rio-tui-protocol-research.md`; the house protocol rules
established there and in `specs/glyph-protocol.md` bind this design too.*

## TL;DR

Every multiplexer that draws panes into one grid inherits the same three sins:
every byte is parsed twice, two copies of terminal state are perpetually
reconciled, and every terminal feature that lives outside the grid (images,
keyboard protocols, color stacks) is gatekept to the lowest common
denominator. The industry is converging on the fix from two directions at
once: terminals are absorbing tmux control mode (iTerm2 2013, WezTerm 2025,
Ghostty 2026 — rio's own ask is issue #919), and Mitchell Hashimoto's
Superlogical (launched **this week**) is building a libghostty-based mux where
client and server share one emulation engine. Rio is unusually well-placed for
the same move: rio-vt is already fully per-instance, librio already drives a
Crosswords without a PTY (`inject_output`), and the Glyph Protocol established
the escape-namespace house style. The design that survives all the evidence is
a **three-layer split**: (1) an in-band wire protocol where buffers are
first-class objects over one PTY-ish connection — DEC solved the framing in
1987; (2) per-buffer state typed as *snapshot* (grids — skippable, mosh-style)
or *log* (input, scrollback — gap-free, credit-controlled) — mosh's deepest
lesson, learned negatively across 14 years of missing scrollback; (3) an
optional resident engine (rio-vt in a daemon) that makes detach/reattach a
state resync instead of a byte replay. Flow control, security framing, and
nesting are designed in on day one because every predecessor retrofitted them
painfully or died without them.

## Why protocol level (the problem, and the moment)

**The double-emulation tax.** tmux/screen/Zellij sit between the application
and the terminal: they parse every escape into their own grid, then re-emit
ANSI for the outer terminal to parse again. Costs measured across the survey:
CPU (two parsers), latency (frame reconciliation), and feature loss — kitty's
graphics protocol is still unusable through tmux (tmux #4902 open) because its
state (terminal-allocated image/placement IDs, chunked megabyte payloads,
readback replies) lives outside the grid, which is the only state a
grid-scraper preserves. The one workaround that survived (Unicode placeholder
cells, U+10EEEE) works precisely because it smuggles the state *into* cells.

**The native-pane wave.** iTerm2 proved for a decade that "terminal renders
mux objects natively" works over a plain ssh pipe (tmux -CC), and its scar
tissue documents every design mistake (below). WezTerm merged its own tmux-CC
client in March 2025; Ghostty landed one in early 2026 and is retrofitting
command batching. Rio has the same request open (#919). Everyone is building a
*client for tmux's accidental protocol* — nobody except Superlogical is yet
shipping the protocol that should exist instead.

**Rio's asymmetric advantage.** rio-vt/librio is the same play as
libghostty-vt: an embeddable engine both ends of a mux can share (the zmx
pattern — a ~1k-LoC daemon embedding the VT engine as a passive observer,
emitting a state snapshot on reattach). rio-vt is in production at Lovable;
canario already ships state restore built on `dump()`/`inject_output()`.

## Prior art — the load-bearing lessons

### DEC TD/SMP + SSU (1987-98): the direct ancestor
Two host sessions over one RS-232 line, VT330/340/420/520 rendering both
natively (split screen or F4-toggle), host side in VMS or DECserver firmware.
The wire protocol (patent US5165020): **sticky session selection** —
SELECT-SESSION switches the current session, all following data bytes belong
to it (near-zero per-byte overhead on 9600 baud); OPEN/CLOSE/RESTORE-SESSION
lifecycle (RESTORE = warm reattach, 1987!); **per-session byte credit**
(ADD-CREDIT / VERIFY-CREDIT / ZERO-CREDIT) so one session flooding never
starves the other; byte stuffing so payload can't forge control. Cross-session
copy/paste via a terminal-local buffer. It died of patents and deliberate
non-documentation ("the TDSMP commands are not documented" — Leichter), then
the platform shifted; not of design flaws. Every element of its design
reappears independently in SSH channels and QUIC.

### Plan 9's 8½/rio: multiplexing as namespace, and the recursion test
The window system consumes `/dev/cons`+`/dev/mouse` and serves per-window
virtual copies of the same files; a client cannot tell it is multiplexed, and
because the system consumes exactly the interface it serves, **it runs inside
itself unmodified**. That recursion property is the acceptance test for a
multibuffer protocol: each buffer's inside must be a complete ordinary
terminal, so the mux nests in a buffer, remoting is just re-pointing the
transport, and terminal-wg's composability rule ("pictures must work inside
the multiplexer") holds by construction.

### tmux control mode + iTerm2: the shipped reference, warts as curriculum
Mechanics: line protocol, `%begin/%end/%error` guard blocks with command
numbers, `%output %pane <octal-escaped bytes>`, `%`-notifications carrying
only IDs, checksummed layout strings, DCS handshake, no version negotiation.
What a decade of iTerm2 issues teaches — copy: native per-pane scrollback
(seeded by `capture-pane`, then accumulated locally — the killer feature),
guard-block request/response, server-owned canonical layout broadcast as
diffs. Avoid: no negotiation (clients sniff `%begin` arity per version);
octal-armored text for bulk both directions (1 byte of keystroke → ~5 bytes;
pastes crawl); notifications that force N follow-up round trips (attach
latency; Ghostty is retrofitting batching); **one shared window size across
clients** (the "gray dead zones" jank — per-client geometry must be
decoupled); in-band text with no resync marker (worst failure: protocol spills
into a shell as typed input); flow control bolted on eight years late (3.2's
`%pause`/`%continue`, drop-based with buffered-age stamps — the design itself
is decent).

### WezTerm mux: server-owned state, line-delta sync, missing backpressure
Server parses escapes into termwiz state; clients are mirrors keyed by
`StableRowIndex` (scrollback-stable rows) + per-pane `SequenceNo`, pulling
lines lazily into an LRU cache — reattach is "list panes, mark stale, refetch"
with no replay log. Framing: varint-length PDUs, serial-numbered
request/response where serial 0 marks server push — one stream, both
semantics. Mistakes to avoid: `CODEC_VERSION` checked by strict equality (no
negotiation), and **no server→client backpressure whatsoever** — one
firehosing pane freezes the whole domain (#2048, #7692); line deltas are also
worst-case for full-screen TUI churn (#2503) — frame-snapshot semantics are
needed alongside row deltas.

### kitty: the strongest counter-position, and the auth bar
Kovid's argument against muxes ("a second VT state machine that double-parses
every byte") is answered, not refuted, by the shared-engine design — his own
objection ("a persistence daemon is basically a terminal emulator itself")
concedes the architecture; the fix is using the *same* emulator library on
both ends. What kitty contributes positively: the remote-control envelope
(cmd, version triple, no_response, async, stream_id — a complete request
lifecycle in five fields) and the only serious in-band auth model
(AES-256-GCM + X25519, per-action scoped passwords, replay windows). Any verb
that creates/redirects buffers needs that bar, not "socket permissions are the
auth."

### Zellij: server-side grids, the ANSI round-trip trap, protobuf lesson
Full VTE grid per pane server-side (real multi-client, per-client focus), but
the client protocol ships pre-composed ANSI frames — so when the client is a
real terminal, it's parse→grid→re-serialize→re-parse. Its MessagePack IPC was
exact-build lock-in until a protobuf retrofit (0.44); its plugin ABI had been
protobuf-versioned years earlier — version the client protocol like you
version the plugin ABI, from day one. Resurrection ships layout (KDL,
re-run-commands behind a safety prompt) but not scrollback by default —
durability must cover both structure and state.

### mosh SSP: the theory
Two sync instances, one per direction, over **different object types**:
server→client syncs the *screen* (snapshot; intermediate states legally
skipped), client→server syncs *input* (log; every keystroke). Diffs are
idempotent numbered-state transforms (`old_num → new_num`, ack, throwaway) —
no replay cache, reorder/duplicate tolerance, roaming = "any authentic
datagram retargets the reply address," reattach after an hour = one diff.
Flow control is implicit: state-skipping paced by RTT. Its 14-year hole — no
scrollback (#122) — is exactly what snapshot-typing discards, and users run
tmux *inside* mosh to get multiplexing. Generalized per-buffer
(`{buffer_id, old_num, new_num, ack_num, throwaway_num, diff}`), SSP gives
per-buffer flow control and focus prioritization for free: the focused editor
syncs at 50 Hz while a background build log legally drops to 4 fps.

### SSH channels, QUIC, HTTP/2: the flow-control benchmarks
RFC 4254: per-channel byte credit (window adjust), independent per-side
channel numbering (no allocation races), half-close distinct from close,
control requests exempt from data credit — all correct; but one TCP stream
underneath means head-of-line blocking across channels. QUIC's two-level
credit (per-stream + connection) is the modern shape. HTTP/2's priority
*trees* failed in the field and were replaced by RFC 9218's minimal urgency
integer — prioritization must stay dead simple: focused buffer wins.

### Security analyses: the response is the wound
The CVE taxonomy (HD Moore 2003; Leadbeater 2023, 10 CVEs across every major
terminal): query/echoback weaponization (any reply the terminal emits can be
aimed at a shell), unescaped OSC payloads reaching execution, clipboard/screen
exfiltration, memory unsafety in complex sub-protocols (Sixel). For a mux
protocol specifically: buffer payload must be structurally unable to forge mux
control (`cat evil.bin` must never open/close/refocus buffers — TD/SMP solved
this with byte stuffing, tmux with octal armor); every protocol reply must be
un-typeable as shell input; per-buffer emulation state fully isolated.

### Arcan: why in-band wins deployment
Arcan's diagnosis of terminal protocol sins is correct and its clean-slate
answer (out-of-band events, API subwindows) stays niche for the same reason:
billions of programs target the pty. The conclusion for rio: the multibuffer
protocol must be reachable through an ordinary pty, degrade silently on
unaware terminals, and be openly specified — the three properties TD/SMP
(proprietary), Arcan (not incremental), and tmux -CC (accidental, but open and
pty-borne — the one that shipped) jointly prove out.

## The design space — three architectures that compose

**A. Remote UI control** (kitty `kitten @`, wezterm cli): protocol verbs that
make the *frontend* spawn native splits/tabs, each with its own new PTY.
Cheap (rio has `RioEvent::CreateWindow/CreateNativeTab` and
`ContextManager::split` already), but it is not multiplexing: useless over a
single ssh connection, no detach. Worth shipping as a degenerate mode
(local-only verbs), not the destination.

**B. In-band virtual buffers** (TD/SMP reborn): one PTY carries framed,
buffer-tagged streams; the terminal instantiates one rio-vt instance per
buffer and renders them as native panes/tabs. This is the wire protocol — it
works over plain ssh with zero server-side installation beyond the
application that speaks it (a mux-aware shell wrapper, a build tool fanning
out logs, an AI agent running parallel tasks).

**C. Resident engine** (zmx/Superlogical shape): a rio-vt-based daemon owns
the buffers and their scrollback; frontends attach/detach; reattach is a
state snapshot + resync, not a byte replay. This is what makes B durable —
and because B's state is *defined* as enumerable and re-emittable, C is "a
process that holds B's state," not a second protocol. **C already exists:
`oj` (../mux), Raphael's tmux replacement built on rio-vt — client/server
over a unix socket, session-per-host-process, detach/reattach, proxy pty per
attachment for capability passthrough, per-pane rio-vt grids server-side,
byte-budgeted scrollback. Today oj composes frames and diffs them onto the
host terminal (grid-scraping with aggressive passthrough); rmx gives it a
second render path: when the host terminal negotiates rmx, oj stops
compositing and opens one buffer per pane, streaming raw pane bytes as
bursts — native panes, native scrollback, images and keyboard protocols
passing straight into each buffer's own terminal instance (retiring oj's
kitty-placeholder re-encoding inside Rio). When the host terminal is not
rmx-aware, oj falls back to its current composed rendering. oj works
everywhere today; it becomes protocol-native inside Rio.**

The synthesis: **B is the protocol, C is persistence for it, A is B's
local-only trivial case.** One spec covers all three; implementation can land
in that order.

## Design principles (distilled, binding)

1. **Framing**: length-declared or strictly-terminated frames tagged with
   buffer ID, inside rio's APC house grammar; payload can never spell a
   control frame (stuffing/armor generalized). Guard-block replies with
   command numbers. Bulk data as base64/chunked (glyph-protocol precedent) —
   never octal-armored per byte.
2. **Buffer typing**: every stream is *snapshot* (grid state; skippable,
   idempotent numbered diffs) or *log* (input, scrollback; gap-free, byte
   credit). Sync policy, retention, and backpressure attach to the type.
3. **Flow control on day one**: per-buffer credit for logs (TD/SMP ≡ RFC 4254
   ≡ QUIC), state-skipping for snapshots (mosh), a connection-level cap, and
   control verbs exempt from data credit. Prioritization = one urgency
   integer; focused buffer wins.
4. **Identity**: client-chosen buffer keys, idempotent create (resend =
   replace) — the glyph-protocol v1.6 lesson forbids terminal-allocated IDs
   the client can only learn from replies (transcript replayability).
   Independent per-side numbering where both sides create objects.
5. **Recursion**: each buffer is a complete ordinary terminal (DA1, modes,
   graphics, this protocol itself). The Plan 9 test: rio-mux inside a
   rio-mux buffer must compose, and unaware-terminal degradation must be
   silent (DECRQM-0 / APC-ignored fallback — inside tmux it simply doesn't
   activate).
6. **Layout is advisory and host-owned**: the app *proposes* (split
   direction, weights, urgency), the terminal solves and reports solved cell
   rects (Taffy in rioterm; SwiftUI weights in canario; both must be legal
   implementations). One-directional, loop-free — the house rule. Per-client
   geometry decoupled (the tmux shared-size jank is the #1 thing to not
   rebuild).
7. **Input**: typed, framed, buffer-tagged key/paste/mouse events on the
   write side that can never be mistaken for typed bytes; per-buffer keyboard
   protocol state (each Crosswords already owns its kitty-keyboard stack);
   focus changes only by user gesture, never by escape sequence; input
   serials + echo-ack hooks per buffer reserved for latency masking.
8. **Security**: fixed-alphabet replies that never reflect app strings;
   privileged verbs (create/close/redirect) gated by negotiated session
   nonce at minimum, kitty-style scoped auth for socket transports; quotas
   (buffer count, per-buffer scrollback budget) with documented eviction;
   per-buffer state isolation; declarative-only (no exec, no filesystem).
9. **Durability**: scrollback is inside the durability story (mosh's hole,
   Zellij's default, tmux -CC's attach-flood are the three failure modes) —
   windowed, on-demand history pull via stable row indices, never
   all-at-once.
10. **Adoption mechanics**: open spec in `specs/`, silent degradation,
    DA1-fenced negotiation with capability names (not bitfields),
    implementation-first with a fast second implementation (librio/canario is
    the built-in second consumer; a `rio-mux` CLI shim could bring the
    protocol to non-rio terminals the way tmux -CC clients work today).

## Sketch: the Rio Multiplex Protocol (named `rmx` — spec draft at `specs/rio-multiplex-protocol.md`)

Following the Glyph Protocol template — APC transport, fixed prefix, `verb;
key=value;…` grammar, `s` support-ping, `reply=` modes, hard caps:

- **Verbs (session plane)**: `s` (support ping/negotiate: version, max
  buffers, features as name list) · `o` (open buffer: client key, kind
  snapshot|log-view, size proposal, title, cwd hint) · `c` (close: key,
  disposition keep-corpse|discard) · `f` (focus request — advisory,
  user-gesture rule applies) · `l` (layout proposal: split tree with weights;
  reply = solved rects, fixed alphabet) · `q` (enumerate: full re-emittable
  state dump — the mux-replay/reattach primitive).
- **Data plane**: `w` (write bytes to buffer: key, base64 chunk, continuation
  flag — ≤4 KB chunks per house wire economics) · sticky variant `t` (select
  target: subsequent raw APC data frames belong to buffer k — the TD/SMP
  optimization for bulk streams, with the stuffing rule).
- **Flow plane**: `a` (add credit for a log buffer) · `p`/`r` (pause/resume
  snapshot sync, age-stamped like tmux 3.2) · per-buffer `sync` scoping
  (mode-2026 semantics move per-buffer — today it's per-Processor).
- **Input plane (terminal→app)**: framed events `I;k=<key>;…` for keys,
  paste, mouse, resize (in-band configure report per buffer — virtual buffers
  have no SIGWINCH), focus/visibility notifications gated by the
  subscription ∧ focus ∧ visibility rule.
- **Lifecycle table** (the never-wedged guarantee): child exit / pty EOF →
  all buffers reap (corpse-to-scrollback per disposition); RIS → protocol
  reset; OSC 133 span close → app-scoped buffers reap; user veto keybinding
  unconditionally tears down; unaware terminal → APC ignored, app detects via
  `s` timeout and falls back to single-buffer behavior.

## Where it lands in the code (from the architecture recon)

- **Parser**: zero changes — APC already streams (`apc_start/put/end`); the
  protocol is one new prefix branch in `Performer::process_apc_buffer`
  (handler.rs:728) plus a typed `ansi/mux.rs` module on the
  `glyph_protocol.rs` pattern.
- **The demux seam is the `Handler` trait**: a router object implementing
  `Handler`, owning `FxHashMap<BufferKey, Crosswords>` plus the sticky
  target, behind the existing `parser.advance()` call. Everything downstream
  already copes: all rio-vt state is per-instance (verified — only two
  harmless globals), events are route_id-keyed, and
  `librio::Surface::inject_output` proves a Crosswords runs without a PTY.
- **Frontend**: a PTY-less Context flavor (the dead-context path already
  proves `main_fd = -1` works) whose writes route as protocol frames over the
  *parent* PTY; buffer create/close verbs surface as RioEvents exactly like
  the `GlyphProtocolQuery` pattern; native rendering reuses ContextGrid/Taffy
  as-is.
- **librio**: new `Action` variants (the ABI grows freely) + the RenderState
  contract generalizing from "one grid" to "a set of grids"; canario's
  SessionStore is the persistence prototype.
- **Known plumbing debts**: `PtyWrite(route_id, …)` replies must gain buffer
  tagging; per-buffer fairness on the single reader (`MAX_LOCKED_READ` is
  per-connection today); reply-collision on the shared write side is the
  hard input-routing problem (framed input plane is the answer, with a
  legacy-app story per buffer).

## Open questions

1. ~~Wire the resident engine (layer C) as a rioterm-embedded server or a
   separate daemon?~~ Answered: **oj** (../mux) is the resident engine — it
   already owns sessions, PTYs, detach, and per-pane rio-vt state. rmx's job
   is to be the render path oj negotiates with Rio; oj's job is persistence
   and the everywhere-fallback. Remaining sub-question: whether rioterm also
   embeds a minimal server later ("all GUIs can be servers").
2. Scrollback ownership for virtual buffers: terminal-side (rio-vt
   per-instance, quota'd) vs engine-side with windowed pull — likely both,
   negotiated.
3. How much of the input plane can reuse kitty keyboard protocol encodings
   verbatim inside frames vs needing its own event grammar?
4. The legacy-app story inside a virtual buffer: a nested pty allocated
   terminal-side (real fd per buffer, terminal bridges it) would make
   arbitrary programs work unmodified — but reintroduces fd-per-buffer on
   the client. Possibly the single most consequential fork left.
5. Auth tiering: is session-nonce enough for the in-band pty transport
   (payload can't forge frames), reserving kitty-grade crypto for socket
   transports only?
6. Naming/prefix registration and whether to co-design with another terminal
   (the implementation-first + fast-second-implementation rule; Ghostty went
   from co-design candidate to competitor this week).

## Phased roadmap

1. **Spec draft** in `specs/rio-multiplex-protocol.md` — DONE 2026-08-01 (v0.1: B-only scope,
   layer C kept reachable via idempotent client keys + `q` enumeration) —
   plus `s`-ping negotiation implemented in rio-vt behind a feature flag.
2. **Layer B minimal**: open/write/close + narrow input plane; router-Handler
   demux in rio-vt; PTY-less Context rendering in rioterm; a demo client
   (Rust crate + shell tool `rio-open`) proving splits-over-ssh with zero
   server install.
3. **Flow + layout planes**: credits, snapshot pacing, layout proposals via
   Taffy; librio Action/RenderState generalization; canario rendering (the
   second implementation).
4. **Layer C**: `rio-muxd` over librio — attach/detach/roam via `q`
   enumeration + state resync; session persistence unifying with canario's
   SessionStore.
5. **Ecosystem**: publish spec, `rio-mux` compatibility shim for other
   terminals, upstream conversations (answers #919, #322/#1704, #868/#1691
   in one architecture).

## Sources

Synthesized from four research reports (2026-08-01): rio architecture
inventory (file:line references throughout), tmux control mode + iTerm2
(protocol mechanics, flow control history, WezTerm/Ghostty adoption), modern
mux architectures (WezTerm codec/StableRowIndex, kitty remote control + the
graphics cautionary tale, Zellij, mosh SSP, Superlogical/zmx, dtach/abduco/
Eternal Terminal), and historical/academic prior art (DEC TD/SMP patent
US5165020 + VT420/VT520 manuals, Plan 9 8½/rio papers, RFC 4254/9000/9218,
mosh USENIX ATC'12, terminal security CVE taxonomies, Arcan essays,
terminal-wg). Full URLs live in the agent reports (session scratchpad
`mux-report-*.md`); key primary documents: patent US5165020, vt100.net VT420
manual ch.7, 9p.io 8½ paper, mosh-paper.pdf, tmux Control-Mode wiki, RFC 4254.
