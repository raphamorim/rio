# rmx — Rio Multiplex Protocol

**Author:** Raphael Amorim
**Year:** 2026
**Last updated:** 2026-08-01
**Status:** DRAFT v0.1 — nothing here is stable yet

**See also:**
- Research and prior-art survey: `rio-multibuffer-protocol-research.md`
  (repo root) — tmux control mode, DEC TD/SMP, Plan 9, mosh SSP,
  WezTerm/kitty/Zellij, and the security taxonomy this design answers.
- Protocol house style: `specs/glyph-protocol.md` (transport, framing,
  negotiation, and lifetime conventions established there apply here).
- Reference implementation (planned): [Rio terminal](https://raphamorim.io/rio),
  with librio/canario as the mandatory second consumer.
- Intended resident-engine consumer (layer C in the research doc): `oj`
  (../mux) — a rio-vt-based multiplexer whose server owns sessions and
  per-pane state, composes frames for non-rmx terminals, and negotiates
  rmx as its native render path.

---

## Abstract

rmx is a terminal protocol that lets an application running behind a
single PTY connection create, write to, and receive input for
**multiple terminal buffers**, which the terminal renders natively as
panes, tabs, or windows. Each buffer is a complete, ordinary terminal
(its own grid, scrollback, modes, keyboard protocol state, and
graphics), so everything that works in a terminal works inside a
buffer — the composability that grid-scraping multiplexers
structurally cannot provide.

rmx is transported over APC (Application Program Command) sequences on
the ordinary PTY byte stream. It requires no daemon, no socket, and no
installation on either end beyond an application that emits the
sequences and a terminal that understands them; on unaware terminals
it degrades silently to the primary stream. Six verbs are defined in
v1: support-negotiation (`s`), open (`o`), write (`w`), raw burst
(`t`), close (`c`), and enumerate (`q`), plus a credit verb (`a`) and
a terminal→application input-event frame (`i`).

## 1. Motivation

Multiplexers that repaint panes into one grid (screen, tmux, Zellij)
parse every byte twice, reconcile two copies of terminal state, and
gatekeep every feature whose state lives outside the grid — images,
keyboard protocols, color stacks — to the lowest common denominator.
The industry is converging on terminal-rendered multiplexing from two
directions: terminals are absorbing tmux's control mode (iTerm2,
WezTerm, Ghostty; Rio's own request is issue #919), and shared-engine
mux daemons are appearing. tmux -CC proves the model works over a
plain ssh pipe, and a decade of iTerm2 integration documents every
protocol mistake to avoid: no negotiation, octal-armored bulk data,
ID-only notifications forcing round-trip storms, one shared window
size across clients, flow control retrofitted eight years late.

rmx is the protocol tmux -CC is by accident: openly specified,
negotiated, framed for safety, flow-controlled from day one, and
defined so that a resident session engine (detach/reattach) can be
built on top of it later without a spec revision.

The historical ancestor is DEC's TD/SMP (VT330/VT420/VT520, 1987):
multiple sessions over one serial line, terminal-rendered split
screen, sticky session selection, per-session byte credit, and byte
stuffing so payload could never forge control. It died of patents and
deliberate non-documentation, not of design. rmx openly re-derives
that design for the PTY era.

## 2. Design goals

1. **Zero-install accessibility.** Works over anything that moves a
   byte stream: ssh, serial, `docker exec`, nested terminals. The
   only requirements are a mux-aware application and a mux-aware
   terminal. This goal outranks all others; features that require a
   daemon, a socket, or out-of-band setup are out of scope for v1.
2. **Buffers are complete terminals.** Every buffer supports the full
   escape repertoire the terminal supports — DA1, modes, graphics,
   the Glyph Protocol, scrollback. The acceptance test is Plan 9's:
   the protocol must compose with itself and with every other
   terminal feature ("pictures must work inside the multiplexer").
3. **Silent degradation.** On unaware terminals every rmx sequence is
   an ignored APC; the primary stream renders normally. Inside an
   unaware multiplexer (tmux filters APC) the `s` ping times out and
   the application falls back to single-buffer behavior.
4. **Flow control from day one.** Per-buffer byte credit on the data
   plane, control verbs exempt, primary stream exempt. Every surveyed
   predecessor either retrofitted this painfully (tmux 3.2, Zellij
   #525) or still lacks it (WezTerm's frozen domains).
5. **Un-forgeable control.** Buffer payload is structurally unable to
   spell a control frame (base64 armor or length-prefixed bursts).
   Terminal replies and input events use a fixed alphabet and never
   reflect application strings. A malicious `cat` can, at worst, open
   visible quota-bounded buffers — never redirect input, steal focus,
   or corrupt a sibling buffer.
6. **The resident engine stays reachable.** Two properties are load-
   bearing for a future detach/reattach engine (layer C in the
   research doc) and are therefore REQUIRED even though v1 ships no
   daemon: buffer identity is **client-chosen and idempotent**
   (re-open = adopt, never duplicate), and session state is **fully
   enumerable and re-emittable** (`q`). A resident engine is then an
   implementation that holds rmx state, not a new protocol.

## 3. Transport

rmx uses APC (`ESC _ ... ESC \`), following the precedent and
rationale in `specs/glyph-protocol.md` §3: terminals that do not
implement an APC command are required to ignore it.

### 3.1 Identifier

Every rmx message begins with the lowercase ASCII string `rmx`.
Terminals MUST ignore any APC message whose body does not begin with
an identifier they recognize; applications MUST NOT rely on any
behavior for unknown identifiers.

### 3.2 Framing

```
ESC _ rmx ; <verb> [ ; key=value ]* [ ; <payload> ] ESC \
```

Parameter keys are lowercase ASCII. Values are decimal for integers,
base64 for binary payloads, and lowercase ASCII names for enums.
Unknown keys MUST be ignored (forward compatibility). The total
length of one rmx frame MUST NOT exceed 4096 bytes; larger payloads
use continuation (§6.2) or raw bursts (§7).

**Buffer keys** (`k=`) are client-chosen strings matching
`[a-z0-9-]{1,16}`. The restricted alphabet is a security property:
keys are the only application-originated value the terminal ever
reflects back (in replies and input events), and this alphabet is
harmless to any parser downstream. Keys outside the alphabet MUST be
rejected with `reason=bad_key`.

The **primary stream** — every byte outside rmx frames — is itself a
buffer, addressed by the reserved key `main`. It is exempt from
credit (§9) and cannot be closed; this preserves ordinary PTY
semantics, degradation, and nesting.

### 3.3 Verbs

| Verb | Direction | Meaning |
|------|-----------|---------|
| `s`  | app → term | Support ping / capability negotiation. Any reply confirms rmx; timeout means unsupported. |
| `o`  | app → term | Open (or adopt) a buffer. |
| `w`  | app → term | Write base64 content to a buffer. |
| `t`  | app → term | Redirect the next N raw bytes to a buffer (bulk path). |
| `c`  | app → term | Close a buffer. |
| `q`  | app → term | Enumerate session state (the reattach/replay primitive). |
| `a`  | term → app | Grant data credit for a buffer. |
| `i`  | term → app | Input/event frame (keys, paste, resize, focus, replies, closure). |

Replies honor the Glyph Protocol `reply=0|1|2` convention
(silent / always / errors-only); the same response-draining rationale
applies. Default is `reply=2`.

## 4. Negotiation (`s`)

Request: `ESC _ rmx ; s ESC \`

Reply:

```
ESC _ rmx ; s ; v=1 ; cap=core[,raw][,layout][,nest][,scrollback] ;
  max=<buffers> ; credit=<initial-bytes> ESC \
```

- `v` — highest protocol version supported.
- `cap` — capability names as an extensible set (never a bitfield;
  the Glyph Protocol v1.8/v1.9 lesson). `core` = §5–§10. `raw` = the
  `t` verb. `layout` = placement hints honored (§5.2). `nest` = rmx
  frames inside buffer content are interpreted by that buffer's
  terminal instance (§11). `scrollback` = `disp=scrollback` on close.
- `max` — maximum simultaneous buffers (excluding `main`).
- `credit` — initial per-buffer data credit in bytes (§9).

Applications MUST treat a timeout (RECOMMENDED: 500 ms after DA1
round-trip confirms liveness) as "terminal does not implement rmx"
and fall back to single-buffer behavior. Clients MUST ignore
capability names they do not recognize.

## 5. Open (`o`)

### 5.1 Request

```
ESC _ rmx ; o ; k=<key> [; cols=<n>] [; rows=<n>] [; title=<base64>]
  [; at=<key>] [; dir=right|down] [; weight=<1-100>]
  [; u=<0-7>] [; focus=1] [; reply=<0|1|2>] ESC \
```

- `k` — client-chosen key. **Idempotent**: opening an existing key
  adopts the existing buffer (parameters other than `k` update it);
  it MUST NOT create a duplicate. This is what makes re-emission
  after reconnect, and a future resident engine, possible.
- `cols`/`rows` — size *proposal*. The terminal decides the real size
  and reports it via an `i;ev=resize` event (§8). One-directional
  layout: the application proposes, the terminal disposes.
- `title` — base64 UTF-8, subject to the same sanitization as OSC 0.
  Never reflected in any reply or event.
- `at`+`dir`+`weight` — placement hint relative to an existing buffer
  (or `main`), honored only when `cap=layout`. Advisory: the terminal
  (and the user, afterward) own real geometry.
- `u` — urgency 0 (highest) to 7, RFC 9218 style. A single integer by
  design; priority trees are a documented failure (HTTP/2). Default 3.
- `focus=1` — request initial focus **at creation only**. Runtime
  focus stealing does not exist in this protocol (§12).

### 5.2 Response (`reply=1`, or `reply=2` on failure)

```
ESC _ rmx ; o ; k=<key> ; status=<u8> [; reason=<name>]
  [; cols=<n> ; rows=<n>] [; credit=<bytes>] ESC \
```

`status=0` success. Failure reasons: `bad_key`, `quota` (max buffers
reached — the terminal MUST NOT evict to satisfy an open),
`unsupported`. The reply reflects only the key, fixed-alphabet names,
and integers.

## 6. Write (`w`)

### 6.1 Request

```
ESC _ rmx ; w ; k=<key> [; m=1] ; <base64 payload> ESC \
```

The payload is bytes for the buffer's terminal instance, exactly as
if they had arrived on a PTY of its own — escape sequences included.
`m=1` marks continuation (more chunks follow for one logical write);
chunking follows the house wire economics (≤4 KB frames).

### 6.2 Semantics

- Writes to an unknown key are dropped and reported (`reply=2`
  default) with `reason=no_such_buffer` — they MUST NOT auto-open.
- Writes consume credit (§9); a write exceeding available credit is
  truncated at the credit boundary and reported with
  `reason=no_credit` (the application should have paced itself; the
  report is diagnostic, and precise loss accounting is the
  application's job via credit arithmetic).
- Ordering is preserved per buffer and across buffers relative to the
  frames' positions in the stream.

## 7. Raw burst (`t`) — capability `raw`

```
ESC _ rmx ; t ; k=<key> ; n=<bytes> ESC \
<exactly n raw bytes>
```

The next `n` bytes after the frame terminator belong verbatim to
buffer `k`; the stream then reverts to `main`. This is the bulk path
(no base64 overhead) and the bridge path for running unmodified
programs in a buffer (a helper allocates a pty, runs the program, and
pumps its output as bursts — see §14). The length prefix is the
anti-forgery mechanism: content is never scanned for terminators, so
no content can escape the burst — DEC's byte-stuffing goal achieved
with modern framing. `n` MUST NOT exceed 65536; bursts consume credit
like writes, and a burst exceeding available credit is a protocol
error that closes the buffer (`i;ev=closed;reason=no_credit`) — raw
mode is for applications that do credit accounting correctly.

## 8. Input and events (`i`) — terminal → application

All terminal→application traffic for non-`main` buffers is carried in
`i` frames on the PTY read side. Applications not expecting rmx never
receive them (events exist only after a successful `o`), and `main`
input remains raw bytes — ordinary PTY semantics untouched.

```
ESC _ rmx ; i ; k=<key> ; ev=<name> [; key=value]* [; <base64>] ESC \
```

Events (v1):

| `ev` | Payload | Meaning |
|------|---------|---------|
| `in`     | base64 | Input for the buffer: exactly the bytes the terminal would write to a dedicated PTY for the user's keys/paste/mouse, honoring **that buffer's** modes (kitty keyboard state, bracketed paste, mouse encoding are per-buffer). |
| `reply`  | base64 | The buffer's own VT query responses (DA1, DSR, DECRQM… issued by content inside the buffer). Tagging replies per buffer solves the shared-write-side collision. |
| `resize` | `cols=`, `rows=` | Authoritative size (at open, and whenever layout changes). Virtual buffers have no SIGWINCH; this is their configure event. |
| `focus`  | `state=in\|out` | Focus crossed into/out of this buffer (user gesture only). |
| `closed` | `reason=<name>` | Buffer ended: `user` (veto/close gesture), `app` (your `c`), `quota`, `no_credit`, `teardown`. |

Delivery of `in` follows the pre-decided gating rule: a buffer
receives input only when it is subscribed (opened), focused, and
visible. Input frames are generated exclusively by the terminal;
there is no verb by which stream content can synthesize input to any
buffer (the echoback/TIOCSTI class is structurally absent).

## 9. Flow control (`a`)

Each buffer (except `main`) carries a byte-credit account, initialized
to the negotiated `credit` value at open. `w` payload bytes (decoded
length) and `t` burst bytes decrement it. The terminal replenishes
with grants as it consumes/renders:

```
ESC _ rmx ; a ; k=<key> ; n=<bytes> ESC \
```

Control verbs never consume credit (the RFC 4254 rule: signaling must
survive data backpressure). `main` is uncredited — its backpressure
is the PTY itself. Credit is the mechanism by which one flooding
buffer cannot starve siblings or the control plane; the terminal's
grant policy (favoring focused/urgent buffers) is implementation-
defined. Terminals SHOULD size grants so an interactive buffer never
stalls (≥ 2 × typical frame of output) and MAY starve invisible
low-urgency buffers arbitrarily long.

## 10. Close (`c`) and enumerate (`q`)

### 10.1 Close

```
ESC _ rmx ; c ; k=<key> [; disp=discard|scrollback] ESC \
```

`disp=discard` (default) removes the buffer and its state.
`disp=scrollback` (capability `scrollback`) renders the buffer's
final screen into `main`'s scrollback as plain annotated lines before
discarding — the "corpse" option for build logs and finished tasks.

### 10.2 Enumerate

```
ESC _ rmx ; q ESC \
```

Reply: one frame per buffer plus a terminator frame, fixed alphabet
only:

```
ESC _ rmx ; q ; k=<key> ; cols=<n> ; rows=<n> ; u=<n> ; more=1 ESC \
ESC _ rmx ; q ; more=0 ESC \
```

`q` exists for state re-emission: an application that reconnects (or
a future resident engine that adopts a session) re-opens its keys
idempotently and repopulates content it owns. The terminal never
echoes buffer *content* or titles in `q` — content restoration is the
application's job (it is the source of truth for its own output), and
`inject_output`-style seeding rides ordinary `w`/`t`.

## 11. Lifecycle

| Event | Effect |
|-------|--------|
| PTY EOF / child exit | All buffers reap with `ev=closed;reason=teardown`; disposition per buffer's last `disp` hint. The terminal MUST never be left wedged. |
| RIS (`ESC c`) on `main` | Full rmx reset: all buffers discarded. RIS *inside* a buffer resets only that buffer. |
| ED/ clears on `main` | No effect on buffers (they are not grid content of `main`). |
| User veto | Terminals MUST provide a user gesture that unconditionally closes any or all rmx buffers, regardless of application state. |
| Unaware terminal | All frames ignored; `s` timeout; application falls back. |
| Terminal/PTY hangup (application side) | The application MUST treat every buffer as closed with `reason=teardown`. Recovery on a new connection is `s` → `q` (empty = fresh terminal) → idempotent re-`o` → re-seed via `w`/`t`. Applications holding authoritative pane state (a resident engine such as oj) recover fully; others recover what they retained. |
| Inside unaware mux (tmux) | APC filtered → same as unaware terminal. rmx does not attempt passthrough in v1. |
| Nesting (`cap=nest`) | rmx frames arriving *inside a buffer's content* are interpreted by that buffer's terminal instance, creating sub-buffers scoped to it. Terminals MAY cap depth (RECOMMENDED ≥ 2) and MUST count nested buffers against the session quota. Without `nest`, such frames are ignored APC inside that buffer. |

Buffer state is session-scoped: it MUST NOT persist across terminal
restarts and MUST NOT leak between tabs, windows, panes, or PTY
sessions (house rule).

## 12. Security considerations

The threat model is untrusted bytes on the PTY (a malicious `cat`)
and untrusted buffer content (a compromised program inside one
buffer).

- **No input synthesis.** Nothing in the protocol converts stream
  content into input — `i` frames are terminal-generated only, and
  every reply uses fixed-alphabet names, integers, and the
  restricted-alphabet key. No reply reflects application strings
  (titles are write-only).
- **No focus stealing.** Focus moves only by user gesture; `focus=1`
  applies at creation only, and terminals MAY ignore it (RECOMMENDED
  when the user is actively typing — the same class of protection as
  focus-stealing prevention in window managers).
- **Forgery containment.** `w` is armored; `t` is length-prefixed and
  never scanned; a stray frame in cat'd junk can at worst open
  visible, quota-bounded, credit-bounded buffers, which the user can
  veto. Terminals MAY additionally gate rmx behind a config option
  (default on with quotas is the reference stance; stricter
  deployments can require per-session confirmation on first `o`).
- **Isolation.** Each buffer's emulation state (modes, palettes,
  keyboard protocol, graphics stores, glyph glossaries) is fully
  per-buffer; no verb reads or writes another buffer's state.
- **Resource bounds.** `max` buffers, per-buffer credit, per-buffer
  scrollback budgets (terminal-owned), 4 KB frames, 64 KB bursts.
  Open-when-full fails; nothing evicts silently.
- **No execution, no filesystem.** rmx is declarative; no frame
  references files or causes the terminal to execute anything.

## 13. Non-goals (v1)

- **No resident engine / detach-reattach.** Layer C in the research
  doc. Kept reachable by §2.6's two required properties; shipped as a
  separate program (`rio-muxd`) speaking rmx, later.
- **No runtime focus verb**, no window management verbs (move,
  minimize), no cross-buffer clipboard verbs.
- **No socket transport or cryptographic auth.** In-band on the PTY
  only; kitty-grade scoped auth becomes relevant with sockets, later.
- **No full layout tree grammar.** Placement hints only; a declared
  layout plane (`l`) is reserved for a future version, expected to
  follow the one-directional rule (app proposes tree, terminal
  reports solved rects).
- **No scrollback sync/pull.** Each buffer's scrollback lives in the
  terminal, quota-bounded. Windowed history pull (stable row indices)
  is a layer-C concern.
- **No predictive echo.** Input serials / echo-ack are anticipated
  (mosh/WezTerm lesson) but not specified; the `i` frame grammar
  leaves room (`serial=` reserved key).
- **No multi-client semantics.** One PTY, one application, one
  terminal in v1.

## 14. Companion tool (non-normative): `rio-open`

The legacy-application bridge is deliberately outside the protocol: a
small helper that allocates a local pty pair, spawns an arbitrary
program on it, opens an rmx buffer, pumps program output as `t`
bursts, and unwraps `ev=in`/`ev=resize` frames back onto the pty
(TIOCSWINSZ included). `rio-open vim file.txt` over plain ssh then
yields vim in a native pane with no daemon and no remote install
beyond the helper binary. The helper dies with the connection — 
persistence is layer C's job.

## 15. Conformance

A terminal is rmx v1 conformant if it:

1. Recognizes the `rmx` identifier in APC and ignores unknown verbs
   and keys within it.
2. Implements `s`, `o`, `w`, `c`, `q`, `a`, and `i` with the
   semantics above; `t` if and only if it advertises `raw`.
3. Treats `o` as idempotent by key, enforces the key alphabet, and
   never evicts to satisfy an open.
4. Renders each buffer with a full terminal instance whose emulation
   state is isolated per buffer.
5. Emits `resize` on open and on every geometry change; delivers
   `in` only under the subscribed ∧ focused ∧ visible rule; never
   synthesizes input from stream content.
6. Enforces credit on non-`main` buffers, exempts control frames,
   and replenishes via `a`.
7. Applies the lifecycle table in §11, including the user veto and
   the never-wedged guarantee.
8. Uses only fixed-alphabet names, integers, and buffer keys in
   replies and events.

An application is rmx v1 conformant if it:

1. Negotiates via `s` before any other verb and treats timeout as
   unsupported.
2. Uses keys from the restricted alphabet, re-opens idempotently, and
   tracks credit before writing.
3. Handles every `reason=` and `ev=closed` without wedging, and
   tolerates the terminal ignoring placement/focus hints.
4. Never assumes a buffer's size; consumes `resize` events.

## 16. Open questions (tracked for v0.2)

1. `t` burst interaction with the credit floor: a `t` declared larger
   than remaining credit is currently a buffer-closing error — too
   harsh? Alternative: clamp-and-report like `w`.
2. Should `q` include a monotonic session epoch so a layer-C engine
   can detect terminal restarts vs. reconnects?
3. Key-encoding inside `ev=in`: current design delegates entirely to
   the buffer's negotiated keyboard modes (terminal encodes as if to
   a private PTY). Verify this round-trips the kitty keyboard
   protocol without loss.
4. `disp=scrollback` rendering format (annotation of corpse lines).
5. Whether `main` should be addressable by `w` (currently NO — the
   primary stream is the only writer to `main`).
6. Prefix registration etiquette with other terminals; co-design
   partner for the second implementation beyond librio/canario.
