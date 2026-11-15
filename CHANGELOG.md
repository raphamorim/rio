# Changelog

## 0.5.32 (2026-11-15)

- **Windows ships a modern ConPTY.** The installer and the new portable `.zip` put `conpty.dll` and `OpenConsole.exe` from the Windows Terminal project (1.25, MIT) next to `rio.exe`, so Rio drives a current pseudoconsole instead of the one built into Windows: sixel, iTerm2 and kitty images render, the screen is restored when a TUI exits, and the IME candidate window follows the caret, with no manual sideload. The portable `.exe` remains for this release. Rio loads `conpty.dll` from its own directory only, so a copy on `PATH` is never picked up, and a pseudoconsole that fails to open or resize is reported instead of aborting Rio ([#1767](https://github.com/raphamorim/rio/pull/1767), [#1764](https://github.com/raphamorim/rio/pull/1764), closes [#1759](https://github.com/raphamorim/rio/issues/1759), [#2031](https://github.com/raphamorim/rio/issues/2031)).

- **A long command can ring the bell or send a desktop notification when it finishes.** The new `[notify-on-command-finish]` section is off by default ([#2043](https://github.com/raphamorim/rio/pull/2043)). `when` is `never`, `unfocused` (the command's pane is not the one you are looking at: another window has focus, or another tab or split is active) or `always`. `bell` (default `true`) rings the bell through the [`[bell]`](https://rioterm.com/docs/config#bell) settings, so the audio bell and the tab indicator follow them; `notify` (default `false`) sends a notification such as "Command Failed: Command took 2m 3s and exited with code 1."; `after` (default `5000`) skips commands that ran that many milliseconds or less. It needs a shell that marks commands with OSC 133 `C` and `D`. Rio's zsh integration now sends those marks itself, so prompt navigation (`ScrollToPrevPrompt`, `ScrollToNextPrompt`) and this notification work in zsh with no setup: `A` and `B` around the prompt, carried inside PS1 so a redraw repeats them, `P;k=s` on continuation lines and PS2, `C` when a command line runs and `D` with its exit status when it ends. Hooks installed by your own rc files never see the marks, and a theme's PS1 is marked after the theme sets it. fish 4 already sends the marks; the PowerShell script still only reports the directory. librio hosts get `Action::CommandFinished` (`RIO_ACTION_COMMAND_FINISHED`, with the exit status in `data_a` and the duration in milliseconds in `data_b`).

- **librio: Sixel and iTerm2 images reach embedders, in one placement list with kitty.** `rio_render_state_image_count`, `rio_render_state_image_placement`, `rio_render_state_image_info` and `rio_render_state_image_rgba` give every image placement on screen in draw order, each with its protocol, layer, z-index, viewport geometry, a change stamp to cache textures by, and RGBA pixels on request; `rio_render_state_image_placements_generation` says when the list changed so a host keeps its draw list otherwise. Sixel and iTerm2 pixels, which the terminal hands over once, were dropped by librio before; the `rio_render_state_kitty_*` calls stay as the kitty subset ([#2045](https://github.com/raphamorim/rio/pull/2045), closes [#2034](https://github.com/raphamorim/rio/issues/2034)).

- **Rio no longer claims OSC 7501 support it does not act on.** The program status query is answered only in librio hosts that set `program_status_cb`, and the reply lists the accepted states and kinds so a program can fall back ([#2040](https://github.com/raphamorim/rio/pull/2040)).

- **More input bounds.** An XTGETTCAP request over 1 MiB gets no reply and the next one is served normally; an OSC 8 hyperlink whose params and URI exceed 2047 bytes is ignored rather than opening or closing a link, and when `id=` repeats the last non-empty value wins; OSC 1337 `SetUserVar` is recognized and ignored instead of stored for the session, as other terminals do ([#2042](https://github.com/raphamorim/rio/pull/2042)).

## 0.5.31 (2026-11-13)

- **OSC 8 hyperlinks keep their whole URI.** Everything after the second `;` is now the URI, taken verbatim, so a link like `https://x.dev/a;b;` no longer opens as `https://x.dev/a`, however many `;` it contains ([#1983](https://github.com/raphamorim/rio/pull/1983)). Clicking or hint-opening an OSC 8 link opens exactly what the program sent: `post-processing` (trailing punctuation, unmatched brackets) now only applies to regex matches. A URI with invalid UTF-8 starts a new link instead of being dropped, and an empty URI always closes the link, even when an `id` is given. The same parser change covers every OSC whose payload is free text, which used to be split on `;` and cut at 16 parts: a window title (OSC 0/2) is kept exactly as sent, and one with invalid UTF-8 or over 255 bytes is dropped rather than shown as a guess; an OSC 7 or `9;9` working directory survives any number of `;`, and an empty one clears the stored directory instead of leaving it stale; an OSC 9 or 777 notification shows its whole body, `9;4` progress splits as before, a bare `9;4` or `9;9` is a one-character notification, and the ConEmu sub-commands Rio does not implement are dropped instead of popping a notification with a junk body.

- **Pathological output can no longer exhaust memory or stall the parser.** A long run of combining marks on one cell (`e` U+0301 repeated 65535 times with `REP`) took 8.6 s and 3.3 GB because every mark re-interned the cell's extras; a cell now keeps at most 64 marks and the same input takes 7 ms and 3 MB. An unterminated APC or OSC grew its buffer until the process died; sequences over 64 MiB are discarded whole and both buffers release capacity above 1 MiB between sequences, so one large image or paste no longer pins that memory. Kitty graphics chunked uploads follow kitty: one transmission in flight at a time, the declared `S=` size is no longer reserved up front (a bogus size tried to allocate gigabytes before any data arrived), and on Linux a FIFO under `/dev/shm` is rejected instead of blocking the parser ([#2041](https://github.com/raphamorim/rio/pull/2041)).

- **librio: OSC 7501 program status reports reach the host.** Programs report their state (`idle`, `working`, `done`, `blocked`, `error`, with a `kind`, a `progress` and a message) in one escape sequence, so a host can show which tab needs attention without per-tool hooks ([#2039](https://github.com/raphamorim/rio/pull/2039)). Reports are validated against the spec (rev 0.4) with its size limits, and the host gets `Action::ProgramStatus` with the report, a prompt start on OSC 133 `A`/`N` and a reset on RIS; the C ABI adds `program_status_cb`. Rio's own windows do not display it yet.

## 0.5.30 (2026-11-12)

- **Numpad Enter no longer sends Ctrl+C on macOS.** AppKit reports the text of numpad Enter (and of fn+Return on laptops) as ETX (`0x03`), and Rio wrote it to the PTY as is, killing the foreground process. Contributed by [@shyim](https://github.com/shyim) ([#2007](https://github.com/raphamorim/rio/pull/2007), landed as [#2016](https://github.com/raphamorim/rio/pull/2016)). The key now reports `\r` like Return, while its logical key and `Numpad` location are unchanged, so `numpadenter` bindings and the kitty keyboard encoding still work. The default search bindings (Enter for the next match, Shift+Enter for the previous one, Enter to confirm in Vi search) now also match numpad Enter. In application keypad mode (`ESC =`) numpad Enter keeps sending `\r`, matching xterm's default, since terminfo turns that mode on for most shells and TUIs.

- **`xdg-terminal-exec` can launch Rio with an app id, title, working directory and command.** The desktop entry gains the `X-TerminalArg*` keys, contributed by [@parker-brown-family](https://github.com/parker-brown-family) ([#2017](https://github.com/raphamorim/rio/pull/2017), landed as [#2019](https://github.com/raphamorim/rio/pull/2019), closes [#1933](https://github.com/raphamorim/rio/issues/1933)). Two new flags fill the keys Rio was missing. `--title` sets a fixed window title that the shell's own OSC 0/2 titles no longer replace, which `--title-placeholder` could not do. `--hold` (and a `hold` config option) keeps a pane open after its command exits with "Process exited. Press any key to close the terminal.", first undoing vi mode, text styling and charset, mouse and focus reporting and the kitty keyboard protocol. Bindings that don't write to the program (copy, scroll, search, new tab, menu items) still work on a held pane, and the first key or paste that would send input closes it. A title without `{{ }}` is rendered as is, never parsed as a template. `-e`, `--working-dir`, `--title`, `--title-placeholder` and `--hold` now survive a config reload, which also applies the `[platform]` blocks and the flatpak `use-fork` override. The man pages document `--title`, `--hold`, `--app-id` and `hold`.

- **Closing a terminal hangs up its pty even while a later shell is running.** `openpty` and `forkpty` return their descriptors without close-on-exec, so every shell spawned later inherited the pty of each terminal already open, and programs left on a closed terminal never got SIGHUP. Diagnosed and fixed by [@ezemtsov](https://github.com/ezemtsov) ([#2024](https://github.com/raphamorim/rio/pull/2024), landed as [#2025](https://github.com/raphamorim/rio/pull/2025)): both ends are marked close-on-exec with `fcntl` right after `openpty`, as ghostty does, and the `use-fork` path marks its main descriptor too. The child still gets its end through `dup2`, which never carries the flag.

- **Glyph protocol sizing and placement (§8.5) are implemented, as spec v1.12.** Rio accepted `aw`, `lh`, `size`, `align` and `pad` on registrations and ignored them, scaling every glyph by `min(span width, cell height) / upm` and centring it on its own outline, so text-like glyphs came out too small unless the client misreported `upm`, and letters in the same word didn't share a baseline ([#2032](https://github.com/raphamorim/rio/pull/2032)). Implementing it showed a gap in the spec: `lh` gives the height of the authored extent but not where it sits. Spec v1.12 adds a `ymin` key for the bottom of that extent. Declaring any of `aw`, `lh` or `ymin` makes placement ignore the outline's bounds, so the glyphs of a set stop shifting relative to each other; declaring none uses the outline's bounding box, with `contain` capped by the em. The `size` default changes from `height` to `contain`, glyphs are clipped to their render span (a `width=2` glyph with no blank cell after it, including in the last column, renders in one cell), and an invalid sizing value is rejected with `malformed_payload`. `align=_,baseline` uses the primary font's ascent so glyphs sit on the surrounding text's line. Each rasterisation is identified by registration, span width, cell height, baseline and, when the paint graph asks for it, the foreground, which also fixes colour glyphs keeping the first foreground they were drawn in and old bitmaps being served after 2048 registrations. A COLR payload with an empty CPAL (`cpal_len = 0`) now renders. A client that sends only `upm` renders as before, except that a `width=2` glyph needs a blank cell after it; a v1.11 client that sends `lh` for an outline with descenders must add `ymin`. Ghostty defaults to `size=height` and has no `ymin` until it adopts v1.12.

- Build and packaging: the `librio` workflow runs on `macos-latest`, since GitHub no longer accepts `macos-14`, and builds the xcframework only on `main` ([#2032](https://github.com/raphamorim/rio/pull/2032)). Nix flake inputs were updated ([#2006](https://github.com/raphamorim/rio/pull/2006), [#2030](https://github.com/raphamorim/rio/pull/2030)).

## 0.5.29 (2026-11-01)

- **Synchronized output no longer flashes a half-drawn screen.** When a program redraws inside a synchronized update (mode 2026), for example Claude Code repainting after a resize, it sends the begin marker, a clear and the new screen in one write, and the kernel splits that into several reads. Rio buffered the raw bytes after the begin marker, but the rest of the first read could leak through, so a mostly cleared screen was presented until the update timed out. Diagnosed by [@ezemtsov](https://github.com/ezemtsov) ([#1991](https://github.com/raphamorim/rio/pull/1991)), fixed in [#2000](https://github.com/raphamorim/rio/pull/2000): mode 2026 is now a plain terminal mode that only holds presentation. Bytes are always parsed into the grid, and the renderer keeps drawing the last frame until the update ends, while Rio's own UI (search, command palette, assistant, cursor) keeps updating on top. An update ends on its end marker, after 1 second without a new begin marker, on a full reset, or on a real resize (grid, cell or panel size). There is no byte buffer and no 2 MiB cap anymore, and librio's `RenderState` follows the same rules.

- **The `vi-cursor` color is used in Vi mode.** It was parsed from `[colors]` and never read, so the cursor kept the `cursor` color (or the OSC 12 one) in Vi mode. Contributed by [@TanbirRamim](https://github.com/TanbirRamim) ([#1932](https://github.com/raphamorim/rio/pull/1932), landed as [#2004](https://github.com/raphamorim/rio/pull/2004)). Vi mode is tracked per split, and the trail cursor now uses the same color as the cursor it trails, OSC 12 included.

- **The OSC 9;4 progress bar animates.** It used to switch on and off: the bar appeared at full length, jumped on every value change and vanished on dismissal. Contributed by [@aymanbagabas](https://github.com/aymanbagabas) ([#1868](https://github.com/raphamorim/rio/pull/1868), landed as [#2005](https://github.com/raphamorim/rio/pull/2005)). The bar now eases into place when it appears, between values, between states and away on dismissal, and a report arriving while it leaves brings it back from where it is. The indeterminate block bounces back and forth with an ease at each end. A report without a percentage follows its state: `Pause` draws a full bar, `Set` and `Error` show the moving block, so a bare `OSC 9;4;2` no longer makes the bar disappear. A bar left behind by a program that exited without clearing it is now dismissed after 15 seconds even if nothing else redraws the window.

- **Variable fonts render the style you asked for on macOS.** With a variable font such as JetBrains Mono, `style = "Medium"` and `style = "Bold"` both reopened the font file at its first face and rendered as Thin, so regular and bold text looked identical. Rio now keeps the face CoreText matched, with its variable axes and collection index, and fonts CoreText provides without a file path load too. Contributed by [@aymanbagabas](https://github.com/aymanbagabas) ([#1963](https://github.com/raphamorim/rio/pull/1963)).

- **Kitty graphics shared memory and file transfers are hardened.** On unix, an `O=` offset was ignored by the range check, so a request past the end of a segment crashed the terminal with SIGBUS (any program writing to the terminal could trigger it). Unaligned offsets failed, and a failed mapping leaked the segment ([#1997](https://github.com/raphamorim/rio/pull/1997)). Offsets and sizes are now handled the same way for Windows shared memory, files and temp files: `O=` is honored without `S=`, uncompressed pixels read only the bytes their size implies, and file paths are checked on the file actually opened, rejecting `/proc`, `/sys`, `/dev` (except `/dev/shm`) and anything that is not a regular file ([#1998](https://github.com/raphamorim/rio/pull/1998), [#2001](https://github.com/raphamorim/rio/pull/2001)).

- **Bad `scroll.multiplier` or `scroll.divider` values no longer break scrolling.** A zero, negative, extreme or NaN value could hang Rio in a huge loop or leave scrolling dead until restart. Both are now clamped to `0.01..10000` with a logged warning, and a `[scroll]` table setting only one of them no longer throws away the whole config ([#1999](https://github.com/raphamorim/rio/pull/1999), refs [#890](https://github.com/raphamorim/rio/issues/890) and [#1975](https://github.com/raphamorim/rio/issues/1975)).

- Build and packaging: rio-vt compiles for Android, contributed by [@marc2332](https://github.com/marc2332) ([#1996](https://github.com/raphamorim/rio/pull/1996)), and for wasm32 again ([#2001](https://github.com/raphamorim/rio/pull/2001)). The Nix package uses `stdenv.hostPlatform` checks, which silences the nixpkgs-unstable deprecation warnings, contributed by [@pinpox](https://github.com/pinpox) ([#1984](https://github.com/raphamorim/rio/pull/1984)). The minimum Rust version is now 1.99.1 ([#1993](https://github.com/raphamorim/rio/pull/1993)).

## 0.5.28 (2026-09-17)

- **A bell in a background tab shows on the tab.** A `BEL` from a pane you are not looking at — a background tab, or the focused tab of an unfocused window — now marks that tab with a small dot next to its title, cleared the instant the tab is shown; ringing the pane you are already watching never marks it. The dot is drawn as a shape rather than a glyph, so it renders identically whatever fonts are installed (a 🔔 only exists where an emoji font resolves), and it sits gated behind `bell.tab-indicator` (default `true`), with a live config toggle that also retracts marks already set. Started by [@aymanbagabas](https://github.com/aymanbagabas) ([#1881](https://github.com/raphamorim/rio/pull/1881), landed as [#1922](https://github.com/raphamorim/rio/pull/1922)). In the same motion the entire title pipeline became event-driven: the 2-second title poll is gone and title rendering never inspects the foreground process. OSC 0/2 titles and OSC 7 directories apply the moment they arrive instead of up to 2s late, `{{columns}}`/`{{lines}}` re-render on the reflow that changes them (window resize, font-size change, splits), editing `title.content` in the config applies live for the first time, and `{{program}}` now names the command the pane spawned — the semantics ghostty and kitty use — instead of probing the process tree. A title changing in a hidden split renders lazily when the split surfaces, so a background pane streaming titles costs nothing, and a fully idle terminal does zero title work: no timer ever wakes the process.

- **Shell integration is automatic.** The title path variables and open-new-tab-in-current-directory used to work only if you wired your shell to emit OSC 7 yourself; rio now injects that wiring at spawn ([#1923](https://github.com/raphamorim/rio/pull/1923)). zsh loads through `ZDOTDIR` (your own `ZDOTDIR` and `.zshenv` are restored and run first), fish through a `vendor_conf.d` entry on `XDG_DATA_DIRS` (restored after load, so the entry never leaks to child processes), and PowerShell — powershell.exe and pwsh, on Windows and unix — through `-NoExit -EncodedCommand` carrying the script inline after your `$PROFILE` runs: inline means the default Windows execution policy cannot block it, and the prompt wrapper preserves `$?` and `$LASTEXITCODE` so failure-colored prompts keep working, while macOS keeps `login(1)` semantics for default shells. Every pane exports `RIO_SHELL_INTEGRATION` pointing at the scripts so any other shell (or a nested one) can source the integration manually, and `shell-integration = false` turns the whole thing off. The parser side grew to match the ecosystem: OSC 7 accepts the `kitty-shell-cwd://` flavor (a verbatim path — spaces, `#` and `%` need no encoding, and `;` survives) and the ConEmu/Windows-Terminal `OSC 9;9` sequence is understood as a working-directory report (absolute paths only; it previously fell into the notification path and popped a bogus toast), so a prompt already configured for kitty, ghostty, or Windows Terminal works in rio unchanged. Flatpak spawns forward the integration across the sandbox boundary via `flatpak-spawn --env`.

- **Closing panes and quitting no longer orphans shells.** Contributed by [@rawkode](https://github.com/rawkode) ([#1927](https://github.com/raphamorim/rio/pull/1927), landed as [#1930](https://github.com/raphamorim/rio/pull/1930)): quitting the app now runs destructors instead of a bare `process::exit`, closing a pane delivers exactly one synchronous SIGHUP to the child's process group (`killpg`, like ghostty and kitty, reaching descendants), the grace before escalation grew from 100ms to 1s so shell HUP traps and history flushes actually finish, the post-SIGKILL reap is bounded so a child in uninterruptible sleep cannot wedge the UI, and BSD's `killpg` EPERM quirk (setuid `login(1)` members) no longer aborts the escalation and leaks a zombie. A pane's final output before exit is drained fully on both unix and ConPTY, and a PTY reader error now closes the terminal instead of leaving a live-looking pane whose keystrokes go nowhere.

- **`xterm-rio` advertises 256 colors and truecolor.** The installed terminfo's `xterm-rio` entry inherited only the 8-color base, and since rio prefers it when present, real sessions were capped: zsh's `%F{8}` rendered as default foreground, tmux clamped to 8 colors, vim saw `t_Co=8`. The 256-color block now applies to both entries, ordered so even strict SVr4 `tic` resolves it, plus the `Tc`/`setrgbf`/`setrgbb` truecolor caps (the same values ghostty and kitty ship) so tmux over ssh keeps truecolor without a forwarded `COLORTERM`. Contributed by [@Brumor](https://github.com/Brumor) ([#1924](https://github.com/raphamorim/rio/pull/1924), landed as [#1925](https://github.com/raphamorim/rio/pull/1925)).

- **IME composition renders inline at the cursor.** Composing with a dead key or a CJK input method used to show nothing until commit: a US-International `'`, Japanese romaji, pinyin, all typed blind ([#1883](https://github.com/raphamorim/rio/issues/1883)). The composition now draws at the cursor as a cursor-colored block, laid out per grapheme cluster (a ZWJ emoji or a decomposed accent occupies one slot and shapes as one unit) on exactly one row: it slides left at the right edge, drops leading clusters when wider than the whole row so the tail under active editing stays visible, and never wraps or paints over scrollback. Arrowing between conversion segments shows a thick underline caret spanning the full width of a wide cluster; a caret the IME reports hidden (candidate paging on Wayland and Windows) draws none; the OS candidate popup follows the caret and knows the composition's extent, so it stops opening tens of cells away on top of freshly drawn text. IME state is window-level, so a background split can never keep a frozen composition painted or swallow keys after refocus, and composing snaps the viewport out of scrollback so a live composition is never invisible while its key gate holds input. The overlays caught up too: committed dead-key and CJK text now reaches the command palette and the tab rename field, which previously accepted only ASCII; keyboard and IME dispatch derive from one modal roster, so error toasts block both symmetrically and dismiss with Enter, warnings never block a working terminal, and the welcome screen no longer leaks keystrokes into the live shell hidden behind it. On Windows, the OS floating composition window is now actually suppressed while composing (the flag was being masked off the wrong message argument), so composition happens inline instead of in a detached system box ([#1849](https://github.com/raphamorim/rio/pull/1849)).

- **Blinking text is preserved.** SGR 5 (slow) and 6 (rapid) were parsed and thrown away; they are now tracked per cell in the style table, cancelled by SGR 25, and carried through every path that reproduces a screen: `contents_formatted` snapshots, librio's `Surface::serialize()` reconnect flow, and the C ABI, which exposes them as `RIO_STYLE_SLOW_BLINK`/`RIO_STYLE_RAPID_BLINK` alongside a newly documented `RIO_STYLE_*` layout for the whole style word, pinned by compile-time asserts so the header can never drift from the Rust bits. Contributed by [@gold-silver-copper](https://github.com/gold-silver-copper) ([#1911](https://github.com/raphamorim/rio/pull/1911)). Review hardening around it closed neighboring serialization gaps in the same motion: snapshots emit underline kinds (double, curly, dotted, dashed) and SGR 58 underline colors instead of flattening them to a plain underline; a bg-colored or underlined blank (a TUI status bar's trailing fill) is no longer trimmed out of snapshots, while an `ESC[31m` + `clear` screen no longer serializes every erased cell as a styled space; and rio-vt, librio and rio-grid share one SGR emitter and one underline-kind ladder instead of three hand-synced copies. Benchmarked flat against the parser suite: no regression, with the SGR-heavy paths marginally faster.

- **librio: underline colors, cursor shape, and the child pid over the C ABI.** `rio_render_state_cell_underline_color` hands hosts a cell's explicit SGR 58 color, so undercurls can finally render in their own color the way Rio itself draws them; absence is self-describing (a new `RIO_COLOR_NONE` kind, never confusable with black) and a `RIO_CELL_HAS_UNDERLINE_COLOR` marker bit lets a full-frame scan skip the call for the overwhelming majority of cells. Missing-cell reads return `RIO_COLOR_NONE` fg/bg instead of a fake opaque black, `rio_render_state_cursor_shape` reports the DECSCUSR shape (block, underline, beam, or hidden), and `rio_surface_child_pid` returns the spawned program's pid on every platform: a session leader for `killpg` on unix, and the real conpty child on Windows, where it previously required reading state the surface never tracked.

## 0.5.27 (2026-08-30)

- **`window.opacity` works on Windows.** Setting it below 1 rendered the window nearly invisible on the wgpu backend and fully invisible on the CPU one, for three stacked reasons. Default Windows builds never took the GPU path at all: the backend dispatch gated on a cargo feature that default builds don't enable, silently falling back to the CPU rasterizer even though the wgpu code was compiled in. The CPU rasterizer wrote an alpha byte of zero into every pixel and masked it back out of every blend, which DWM — composited per-pixel once `window.opacity < 1` engages blur-behind — reads as a fully transparent window. And the wgpu clear color went out straight on surfaces that DWM interprets as premultiplied, so the background read as nearly transparent. All three are fixed by [@shiena](https://github.com/shiena) ([#1583](https://github.com/raphamorim/rio/pull/1583), landed as [#1905](https://github.com/raphamorim/rio/pull/1905)): Windows defaults now take the wgpu path, the clear color is premultiplied exactly when the surface composites premultiplied, and the CPU framebuffer carries premultiplied alpha end to end — the convention every terminal that does transparency converged on (alacritty premultiplies its clear color the same way, kitty and ghostty premultiply everything). The alpha channel costs nothing: it rides the SWAR lane pair the green channel occupied alone (`00AA00GG` mirroring the existing `00RR00BB` trick), so all four blend kernels do four channels in the same two multiplies as the old opaque three, held bit-exact to the scalar reference by randomized agreement tests. Around the fix, review hardening: wgpu picks Vulkan-class adapters whose surface can actually carry alpha when transparency is requested — DX12 swapchains on a plain window expose no such mode, the same gap behind wezterm's long-open WebGPU transparency bug — with a deterministic ranking that never lands on a software rasterizer and falls back gracefully when a broken driver refuses a device; `[renderer] filters` edits live-reload on Windows now; and since a filter chain's final pass overwrites the frame's alpha, combining filters with opacity warns once instead of cancelling silently.

- Images drawn with kitty's Unicode-placeholder flow show up in Neovim. The protocol says a placeholder cell carrying no placement id may resolve to *any* virtual placement of its image, and both kitty and ghostty honor that; Rio only tried the exact id, so a placement registered as `p=N` was never found for cells without one — which is the common case, because Neovim's TUI only emits the id-carrying underline color for cells that also have an underline attribute, and snacks.image doesn't underline its placeholders: the cells reserved space and nothing drew. Id 0 now falls back to the lowest placement id of the image, deterministic and still letting an explicit `p=0` win; placeholders also draw when the placement arrives after the transmission (`a=t` then `a=p,U=1`), and GPU uploads are tracked per image ([#1893](https://github.com/raphamorim/rio/pull/1893)).

- **librio: bracketed paste, mode bits, dynamic colors.** `rio_surface_paste` gives C embedders what `rio_surface_text` couldn't: a paste that arrives atomically, wrapped in `ESC[200~`/`ESC[201~` when the program enabled mode 2004 (contributed by [@ezemtsov](https://github.com/ezemtsov), [#1896](https://github.com/raphamorim/rio/pull/1896)). Review hardened it into the shape rioterm itself uses: the payload travels verbatim inside the brackets — editors are entitled to distinguish a pasted newline from Enter — minus ESC, ETX and the 8-bit CSI, so a malicious clipboard can never close the bracket early and inject keystrokes; without brackets, newlines normalize to the CR the Enter key produces. Alongside it, `rio_surface_mode_bits` reports mouse reporting, application cursor keys, alternate screen and bracketed paste for input decisions the host makes on its own, and `rio_render_state_dynamic_color` exposes the OSC 10/11/12 dynamic foreground, background and cursor colors — returning unset after an OSC 110/111/112 reset so the host falls back to its own scheme.

- **librio can no longer get its host killed by SIGPIPE.** Stress-running the new C-ABI test gate crashed roughly one run in eight with exit 141: during pty teardown, the signal machinery closes its internal wake pipe while a SIGCHLD from the dying child is in flight, and the signal handler writes into that pipe expecting `EPIPE` back — an assumption that holds in Rust programs, which ignore SIGPIPE at startup, and kills C and Swift embedders, which don't. The wake sockets now opt out per-fd (`SO_NOSIGPIPE`, the same thing upstream mio does on Apple targets), and the C gate that found the bug stays SIGPIPE-naive on purpose, doubling as the regression test ([#1899](https://github.com/raphamorim/rio/pull/1899)). The gate also stopped depending on the login shell's mood: it pastes into a `/bin/cat` child where the tty's own echo is the assertion, covering the unbracketed and bracketed paths end to end — mode 2004 enabled by a real child, verified through the new mode-bits API. And `nix build` runs the test suite green again: the sandbox ships no `/bin/sleep`, so the tests' quiet child spawns through `/bin/sh` ([#1900](https://github.com/raphamorim/rio/pull/1900)).

- `cargo install rioterm` works on Windows again. The window icon was compiled into the published `rio-window` library crate from a path outside the crate — a file cargo cannot package — so every registry build of the crate on Windows died in its build script ([#1887](https://github.com/raphamorim/rio/issues/1887), diagnosed by [@milkowski](https://github.com/milkowski)). The resource now lives in the binary crate, embedding the identical icon it already ships for the runtime window, which fixes two quieter bugs in the same motion: on the GNU toolchain the linker could prune the icon out of the library archive entirely, and right-clicking rio.exe → Properties used to advertise `rio-window` — "Winit fork maintained for Rio terminal" — as the product, because the resource compiler stamps version info from whichever crate runs it ([#1898](https://github.com/raphamorim/rio/pull/1898)).

## 0.5.26 (2026-08-23)

- Massive PTY throughput improvement, most visible in programs that repaint the whole screen with truecolor (gradients, animations, terminal video, heavy TUIs). Rio's pseudo-terminal was created with a baud rate of 0, which makes the BSD/XNU kernel clamp the tty output queue to its ~100-byte floor — every write/read handshake moved ~150 bytes through a syscall round trip, capping drain throughput at a fraction of what the kernel can do. The pty now opens at 230400 baud (the kernel sizes its queue from the baud rate on macOS/BSD; on Linux it only fixes `stty` reporting "0 baud"), and the reader drains until the queue is empty before returning to poll. An unthrottled full-screen truecolor benchmark went from ~340 fps to ~1900 fps.
- Per-cell truecolor styles no longer degrade over long sessions: the style intern table is now garbage-collected with a mark-and-sweep driven by per-row hints, so gradient- and animation-heavy output can run indefinitely. Previously the table could saturate at 65,535 distinct styles, after which new colors silently rendered with the default style.
- The renderer's frame snapshot now carries fully resolved per-row style values instead of a shared style table, removing an entire class of stale-color windows around alternate-screen switches, resets, and style-table growth.
- Copying text that includes color-erased blanks (e.g. `ESC[48;2;…m` + `ESC[K`) no longer inserts NUL characters or another cell's combining marks into the clipboard, and session serialization now preserves those fills' background colors instead of emitting a wrong style. Hovering or clicking such a blank can no longer resolve an unrelated hyperlink's URL.
- Embedder note (librio): `RenderState::style_of(square)` is replaced by `style_at(line, column, square)` and per-row `row_styles(line)`, matching the new resolved-style snapshot.
- Hovering a URL underlines the URL, not a spot to its right. The hover path used regex byte offsets directly as cell columns, so any multi-byte character left of the match — box-drawing prefixes, CJK, emoji — dragged the underline right by one cell per extra UTF-8 byte; and it searched one visual row at a time, so soft-wrapped URLs matched truncated or not at all. Matching now runs over the logical unwrapped line under the pointer with a byte-to-cell map recording every byte's source cell, so bounds cannot drift regardless of encoding and a wrapped URL resolves as one whole link ([#1884](https://github.com/raphamorim/rio/pull/1884)).
- macOS: resizing no longer flashes a stretched stale frame. The layer scaled its last-presented contents to the new bounds while the next frame rendered; its gravity is pinned top-left now, so old pixels stay put until real ones replace them.
- The [glyph protocol](https://github.com/raphamorim/rio/blob/main/specs/glyph-protocol.md) spec caught up with its implementations. v1.10 caps decoded outlines at 64 KiB — the wire form is compressed, so the existing wire cap bounded nothing: a crafted payload expanded to ~768 KiB of points per slot, ~768 MiB per session — with the cap requested by Mitchell Hashimoto against the expansion while implementing the protocol in Ghostty, and enforced by the reference implementation with a `reason=outline_too_large` rejection ([#1878](https://github.com/raphamorim/rio/pull/1878)). v1.11 settles `width` as a render-level span: layout stays on system `wcwidth`, a `width=2` glyph overflows the following cell in pixels, and the client keeps that cell blank — withdrawing v1.7's "authoritative for all layout decisions" wording ([#1879](https://github.com/raphamorim/rio/pull/1879)).


## 0.5.25 (2026-08-15)

- **The cursor trail actually works now.** Five open issues traced to one pair of root causes: the animation's continuation flag was read before the trail advanced, so a trail could freeze mid-flight and stay painted until unrelated output arrived — the "trail covers the whole screen" screenshots ([#1597](https://github.com/raphamorim/rio/issues/1597)) were that freeze photographing a scroll-sized jump — and the spring integration snapped to its destination whenever a frame gap exceeded the animation length, which at event-driven frame cadence was most of the time, the "feels like 15 FPS" choppiness of [#1830](https://github.com/raphamorim/rio/issues/1830). The motion model is now kitty's, verified line-by-line against its source: an exponential ease-out that composes exactly across irregular frames (no fixed-cadence render loop to depend on), with per-corner speed picked by direction alignment so the quad stretches into the smear. Around it, the behaviors the issue tracker asked for: the trail fades out when a program hides the cursor — yazi and lazygit redraws no longer animate a phantom ([#1511](https://github.com/raphamorim/rio/issues/1511)) — jumps at or under a threshold of cells don't trail so typing stays trail-free, and four new `[effects]` options: [`trail-cursor-color`, `trail-cursor-opacity`](https://rioterm.com/docs/config#effects) ([#1594](https://github.com/raphamorim/rio/issues/1594)), `trail-cursor-decay = [fast_ms, slow_ms]`, and `trail-cursor-start-threshold` ([#1520](https://github.com/raphamorim/rio/issues/1520)). One invariant runs through the rework: layout is never travel — window resizes, font-size changes, display rescales, split-divider drags and tab-strip reflows snap the trail in place, and panel switches teleport it, so the smear only ever draws where the cursor actually went; and beam and underline trails converge onto the exact cursor geometry the grid draws, reusing its thickness and baseline math rather than approximating them ([#1871](https://github.com/raphamorim/rio/pull/1871)).

- Rio launches on macOS 10.15 again. wgpu 30 references HDR color-space constants (`kCGColorSpaceExtendedDisplayP3`, the BT.2100 pair) that only exist on macOS 11+; their use is runtime-gated, but the references linked as strong dyld imports, so the loader aborted the launch before a single line of Rio ran — the `Symbol not found` crash of [#1873](https://github.com/raphamorim/rio/issues/1873) and [#1711](https://github.com/raphamorim/rio/issues/1711), broken since the wgpu upgrade in 0.4.9. CoreGraphics is now weak-linked for the rio binary: the missing constants resolve to null instead of aborting, every CG symbol Rio actually calls has existed since 10.11, and an audit of the binary's import table found no other post-10.15 strong reference waiting behind this one. Upstream fixed the root cause on trunk a week after v30 shipped; [gfx-rs/wgpu#10067](https://github.com/gfx-rs/wgpu/issues/10067) asks for a patch release, and the next wgpu upgrade retires the workaround.

- `tmux new -ADXs` no longer crashes Rio. The `-X` flag SIGHUPs the shell of a *background* tab, and the close path only adjusted the focused-tab index when the dying tab was itself the focused one: removing a background tab shifted every index left while the focused index stayed put, and the very next tab lookup panicked out of bounds ([#1848](https://github.com/raphamorim/rio/issues/1848)). The rewrite also closes two adjacent holes the crash exposed: a background tab's PTY exiting while the focused tab had splits used to leave a zombie tab that never closed, and a background tab that itself had splits could lose the whole tab, living splits included, when just one of its shells exited. The close path now searches every panel of every tab and removes exactly the panel that died.

- Hovering a link shows its target in a bottom-left pill, the way a browser status bar does. An OSC 8 link's display text is arbitrary, so its destination stayed invisible until you had already opened it; the pill shows the real URI, tail-elided so the scheme and host always survive truncation, and it never draws while a modal overlay (search, command palette, assistant, quit confirmation) would consume the click anyway ([#1874](https://github.com/raphamorim/rio/pull/1874), by [@aymanbagabas](https://github.com/aymanbagabas)).

- librio: mouse reporting for embedders. `mouse_button()` and `mouse_motion()` encode presses, releases and drags for programs that grab the mouse (X10 and DEC 1000/1002/1003), honoring shift-to-bypass so local selection stays reachable. Both return whether a report was written, telling the host when not to start its own selection. Exposed over the C ABI.

## 0.5.24 (2026-08-13)

- tmux redraws are atomic in Rio now. Rio's terminfo has advertised the `Sync` capability for years in the legacy iTerm2 DCS form (`ESC P =1s ESC \`), a form Rio's parser never implemented — the synchronized-update support that landed later spoke only the modern `CSI ? 2026` dialect and nobody went back to reconcile the two. tmux trusts the terminfo entry, so it dutifully bracketed every redraw with begin/end markers Rio swallowed and ignored: no tmux redraw was ever synchronized, and each frame painted as the bytes trickled in, visible as flicker and momentarily inconsistent screens during heavy TUI updates under tmux. Both sides are fixed: the parser honors the DCS bracketing form (including markers split across reads, which arrives constantly from a PTY), and the terminfo entry now advertises the `CSI ? 2026` form, so an updated terminfo gets atomic redraws from tmux and anything else that consults `Sync`. Upstream, [tmux #5489](https://github.com/tmux/tmux/pull/5489) proposes Rio for tmux's default terminal-features so the capability set stops depending on which terminfo happens to be installed; until a tmux carrying it ships, `set -as terminal-features 'rio*:sync'` in tmux.conf does the same by hand.

- Legacy widths are pure wcwidth again. When grapheme clustering is off (`grapheme-clustering = false`, or a program's `DECRST 2027`), an emoji variation selector no longer flips the width of the cell before it — that behavior was a stopgap from before mode 2027 existed, and in legacy mode it is exactly wrong: applications that position text by wcwidth get a cell layout they cannot predict, and every full-screen redraw from tmux desyncs by one column at each flipped cell, leaving stray characters behind. Width changes from variation selectors now happen only inside the mode-2027 cluster path, where programs opt in knowingly. Alongside it, legacy mode stopped attaching selectors to cells that are not emoji bases: a selector after a plain letter is presentation noise, and storing it polluted copy, serialization and round-trips for zero render effect.

- **librio: measure a grapheme cluster without printing it.** `cluster_width()` takes a UTF-32 buffer and returns how many codepoints its first grapheme cluster spans and how many terminal cells it occupies, applying exactly the rules the mode-2027 print path applies: VS16 widens and VS15 narrows a valid emoji base, any width-bearing continuation makes the cluster wide, invalid selectors are consumed without effect. It is the measurement half an embedder needs to shape and place text for cells without replaying bytes through a terminal, exposed as `cluster_width` in Rust, `rio_cluster_width` over the C ABI, and `cluster_width` (returning `[len, width]`) on the wasm class. The FFI edge is defined rather than assumed: values that are not Unicode scalars measure as one narrow codepoint when they lead and terminate the cluster when they follow, so untrusted input cannot wedge the walk. The parity claim is enforced, not aspirational — a differential test prints each measured cluster into a live mode-2027 grid and demands agreement on codepoint count and cell width, and a randomized 2,000-sequence soak over bases, joiners, selectors and modifiers holds cell width and cursor advance to the measured answer. One documented limit: a cluster led by a zero-width Prepend codepoint (Arabic number signs and kin) measures by pure segmentation while the print path drops the orphan lead — the same open limitation ghostty's measurement API carries.

- Modifier-click opens links even when the program owns the mouse. With mouse reporting on (vim, tmux with `mouse on`), a hint-modifier click was forwarded to the program instead of opening the link under the pointer; it now bypasses mouse mode. The pointer icon and hint highlight also update the moment the modifier is pressed or released mid-hover rather than on the next mouse move ([#1869](https://github.com/raphamorim/rio/pull/1869), by [@aymanbagabas](https://github.com/aymanbagabas)). A pre-release adversarial review then hardened the whole click contract around a press-time latch: the press records *which* link it landed on, and the release opens exactly that link, only when it lands back on the same span — so a drag that ends over a different link opens nothing, a modifier change mid-click cannot swap which configured action runs, and a press swallowed by window chrome (tab strip, dialogs) can never open a highlight it never touched. The same latch keeps the bypass airtight for the application underneath: a mouse-mode program now sees a hint click as *nothing at all* — previously the press and release were hidden but drag motion still leaked through as button events with no press around them — and a highlight can no longer outlive its modifier via the window padding and hijack a later plain click. Links open on release rather than press now, the way buttons behave everywhere else.

- macOS: double-clicking the top of the window zooms it again when the tab strip is hidden. With `hide-if-single` hiding the strip, the reserved band at the top is window chrome for dragging, but the standard double-click-to-zoom gesture had been lost with it; the band now routes through one navigation predicate that keeps both behaviors ([#1867](https://github.com/raphamorim/rio/pull/1867), by [@jxdones](https://github.com/jxdones)).

## 0.5.23 (2026-08-12)

- Scrolling back through history no longer corrupts the view. The viewport's scrollback position is an offset counted from the live bottom, and any scroll that pushes rows into history grows it so the same content stays on screen. The bookkeeping had one wrong branch: scrolls that add nothing to history, an editor's insert/delete line, any scroll region that does not start at the top row, grew the offset anyway. Once it drifted past the rows that actually exist, the storage index arithmetic wrapped and the renderer was handed arbitrary rows, painting fragments of other lines at wrong positions until a full redraw washed them out. The offset now moves only when history does. Two neighbours came out of the same investigation: a viewport pinned at the scrollback cap slides when a region scroll evicts history, and that frame now takes full damage since every visible row changed; and growing the window while scrolled back adjusted the offset by the intended row count rather than the rows history could actually supply, with both resize paths now clamping against what remains.

- Idle terminals stop rendering at PTY rate. Consuming a frame never recorded the cursor position it carried, so after the first cursor movement every damage check reported the cursor as moved, and every PTY read fired a redraw event, woke the renderer, and took the terminal lock even when nothing had changed. The consumed frame now marks its cursor as seen, and a drain that changed nothing fires nothing.

- These came out of a new damage-pipeline test harness rather than staring at screenshots: it replicates the renderer's exact frame-consumption and row-rebuild decisions against a painted mirror, drives randomized editor-shaped traffic (region scrolls, insert/delete lines, alt-screen flips, resizes, viewport scrolling, racing redraws) across 200 seeds, and holds three invariants at every quiet point: the offset never exceeds available history, no content change is left without a wakeup to paint it, and what was painted equals what the grid holds. It found all of the above by itself, prints the exact operation trace when something diverges, and now runs in CI so the next damage bug fails a test instead of leaving glyph leftovers in someone's editor.

## 0.5.22 (2026-08-12)

- The release that actually shipped 0.5.21: identical fixes, plus the repair of the crates.io publish pipeline that burned the 0.5.21 tag. The new `rio-unicode` crate had its own frozen version number, and a fixed version can only be published once while the release pipeline republishes every workspace member per release; it now rides the shared workspace version like every other published crate, so `cargo publish --workspace` goes through cleanly on this and every future tag. Along the way the crate lost its `unicode-width` alias: manifests, imports, and the published metadata all say `rio-unicode` now, so the code and the crates.io dependency list finally tell the same story.

## 0.5.21 (2026-08-12, tag superseded by 0.5.22)

- Stale glyphs no longer linger until you scroll. Attaching a combining mark, ZWJ continuation, or variation selector to an already-painted cell mutated the cell without recording damage, so the renderer was never told the row changed: in Emacs, NeoTree hover kept showing the previous directory name and stray characters survived until a scroll forced a full repaint. The gap had been latent in the legacy zero-width path for years and became hot when 0.5.20 turned grapheme clustering on by default, since TUI icon fonts lean on exactly those sequences and a chunked redisplay routinely lands an attach in its own damage window. The attach path now marks the row, and a new property test guards the whole bug class: randomized VT streams, with bare continuation codepoints arriving in their own damage windows on purpose, asserting that every row whose content changed is covered by reported damage.

- Large escape-sequence payloads parse at memory speed. The parser handled OSC, APC, SOS, PM, and DCS passthrough one byte at a time, paying two state dispatches and a push per byte, which is where kitty-graphics images, OSC 52 clipboard writes, and sixel data spent their time. Those states now consume payload spans in bulk: an 8-bytes-per-step boundary scan finds the run up to the next terminator or control byte and appends it in one slice, with the per-byte state machine still deciding every boundary byte, so semantics are unchanged by construction. Measured on 4 MiB payloads: kitty APC 481 MiB/s to 29 GiB/s, OSC 628 MiB/s to 17 GiB/s, sixel DCS 824 MiB/s to 15 GiB/s. A differential test pins the batched paths to the per-byte machine across random chunk splits, the same discipline ghostty uses for its SIMD parser, whose string states remain per-byte.

## 0.5.20 (2026-08-12)

- **rioterm: Rio's terminal engine, on the web.** `librio` now compiles to WebAssembly and ships as [`rioterm`](https://github.com/raphamorim/riotermjs) on npm, with a `react-rioterm` wrapper. It is the same Rust VT core the desktop app runs, parsing your bytes in the browser, with a canvas or DOM renderer selectable per instance, and it is not a JavaScript reimplementation of a terminal: `rio-vt` and `librio` cross-compile to `wasm32-unknown-unknown` unchanged behind a wasm-bindgen `RioTerm` class, and the browser is handed the transport (a WebSocket to a real shell, an in-page interpreter) instead of a PTY. On byte-identical 120x40 workloads against xterm.js with its WebGL renderer it parses and paints plain text about 3x faster and cold-inits ~1.8x faster, with headless VT parsing 6x faster; the benchmark suite is in the [repo](https://github.com/raphamorim/riotermjs/tree/main/benchmark). It runs in production in Lovable's admin terminal, and there is a [write-up](https://rioterm.com/blog/2026/08/10/riotermjs-rio-terminal-for-the-web).

- **librio: reconnect and search.** `Surface::serialize()` dumps scrollback and screen as a VT byte stream that reproduces content, SGR styling and OSC 8 hyperlinks when replayed into a same-width terminal. It is the reconnect-restore flow cloud IDEs need, where a dropped socket should come back with the buffer intact. `Surface::search()` runs a regex over the whole buffer and returns matches in ring-relative coordinates, so a hit stays valid however the viewport is scrolled. Both are exposed on the wasm `RioTerm` and wrapped by rioterm's `serialize()`, `search()`, `findNext()` and `findPrevious()`.

- **librio: plain-text links and cursor visibility for renderers.** `Surface::url_at()` regex-detects a URL (`http`, `https`, `mailto`, and friends) under a pointer cell, resolving a URL that wrapped across rows as one whole link, so a host gets hover-and-click links without an addon and without a per-frame cost; it is hit-test shaped, run only on pointer events. `RenderState::cursor_visible()` reports when the cursor is hidden, whether by DECTCEM (`CSI ?25l`) or by a scrolled-back viewport, so a renderer stops painting a phantom caret where the program wanted none.

- **librio: a leaner web build, and a chunk-safe injector.** Image-codec and glyph-protocol decoding moved behind a `graphics` feature (on by default for native embedders, off for the web build), which halves the wasm from 2.2 MB to 1.1 MB and cuts cold init roughly in half, since the browser had been compiling png/jpeg/webp/tiff and a font-outline parser that a text terminal never runs. Separately, `Surface::inject_output()` now keeps its VT parser across calls: an escape sequence split across two writes, which is the norm the moment output arrives over a WebSocket rather than a PTY, now resumes mid-sequence instead of leaking its tail into the grid as literal text. `teletypewriter` also retries a transient `openpty` failure (seen when spawning many PTYs at once) and reports the real errno if it still fails.

- The PTY event loop wastes fewer syscalls. Rio inherited alacritty's mio-era loop shape: every source registered edge+oneshot, so every wakeup paid a re-registration syscall to re-arm it, and every read loop paid one extra read just to hear EAGAIN. The PTY fd is now registered level-triggered and re-registered only when write interest actually changes, the control channel sits on plain edge (it is drained to empty on every wakeup, which re-arms it for free), and a read that returns fewer bytes than the buffer holds is trusted to mean the PTY is drained rather than confirmed with another syscall; the call nginx has made for two decades on every non-edge event model, and kitty makes more aggressively still with a single read per poll. Measured against a real PTY the confirmation read was pure waste: macOS hands the loop about 150 bytes per wakeup, so the old loop did exactly twice the reads for the same bytes, and dropping it takes bulk output from 23 to 26 MiB/s with per-wakeup latency down 18%. Two edge cases found by review and now spelled out in the code: the final drain after a child exits still reads to EAGAIN, because there is no next poll behind it to catch leftovers, and an interrupted read says nothing about the fd being drained, so only WouldBlock does.

- `corcovado`, Rio's fork of mio 0.6, went on a diet: about 7,700 lines gone. The timer, the Fuchsia backend, the Windows TCP/UDP sample bindings and a directory of tests that were already commented out served nobody; the fork exists to poll a PTY, a signal pipe and a channel. With them went `iovec` (unmaintained since 2018), `cfg-if 0.1`, `slab`, `socket2` and the `windows` crate. Migrating to mio 1.x or smol's `polling` was on the table first, and the benchmarks closed it: mio 1.x removed the pollable channel, the user-space readiness the Windows pipes are built on and level-triggered mode outright, and with the loop fixes above corcovado matches or beats both crates on every shape Rio's loop actually runs. The only thing a migration had left to offer was less fork to maintain, and the trim delivered that without one.

- Combining marks finally render. The grid renderer hashed a cell's attached marks into its glyph-cache key but never handed them to the font shaper, so a decomposed `é` — an `e` plus a combining accent, the form `ls` output and git logs are full of — drew as a bare `e`. Marks now shape together with their base on both the CoreText and Swash paths, which meant teaching the glyph-to-cell mapping to use explicit per-cell offsets: the old per-character cursor assumed one char per cell and would have pinned every glyph after a mark to the wrong column ([#1854](https://github.com/raphamorim/rio/pull/1854)).

- Erasing half a wide character erases the whole character. `write_cell` always repaired a wide pair split by a single-cell overwrite, but the range operations — ECH, DCH, ICH, EL, and ED's partial rows — never did, leaving stranded halves behind: a lead rendering with a live cell inside its right half, orphaned spacers, stale pre-wrap padding dragged into the row by a shift. The repair is ghostty's design (`splitCellBoundary`): each operation names the exact seams it cuts before mutating, O(1) per seam — a worst-case EL on a 320-column grid costs 61ns against 42ns with no repair at all — and a repaired cell keeps its WRAPLINE bit, so resize-reflow can't mistake a soft wrap for a hard break. Alacritty does none of this; rio now sits with ghostty and xterm ([#1854](https://github.com/raphamorim/rio/pull/1854)).

- A combining mark typed inside an OSC 8 hyperlink attached itself to every cell of the link. All cells written under one hyperlink share a slot in the grid's extras table, and the mark-attach path mutated that slot in place. Slots are interned by content now — repeated emoji and combining sequences cost one slot total instead of marching the table toward its silent 65,535-slot cliff — which makes shared slots immutable by construction: writers copy, extend, and re-intern, so the marked cell gets its own entry and its neighbors keep theirs. Hyperlink detection stops keying on raw slot ids (a marked cell mid-link legitimately differs now) and compares the link itself; a link left open across an alt-screen switch is re-interned into the alt grid's table instead of dangling a foreign id; and a mark aimed at a color-only blank — whose bits store the background, not an id — is dropped instead of cloning a stranger's slot onto the cell ([#1854](https://github.com/raphamorim/rio/pull/1854)).

- One Unicode version for the whole terminal: the new `rio-unicode` crate carries both the width tables and grapheme cluster segmentation, generated from the same Unicode 17 release, ending the skew where cell widths answered to Unicode 16 while string segmentation answered to 17. The width side keeps the exact per-character semantics rio has always shipped — an every-codepoint audit against the old tables pins the difference to 196 codepoints, all genuine 16-to-17 data changes, and fails CI if a regeneration ever drifts further — deliberately rejecting upstream unicode-width's newer semantics (soft hyphens going zero-width, conjoining jamo collapsing) that the tree pinned a fork to avoid. The grapheme side is a pairwise break state machine built for feeding a terminal grid codepoint by codepoint, ghostty's shape, passing all 766 Unicode conformance vectors; it's the substrate for grapheme clustering (mode 2027) if rio goes there ([#1854](https://github.com/raphamorim/rio/pull/1854)).

- **Rio went there: grapheme clusters, opt-in via DEC private mode 2027.** A program that enables it (`CSI ? 2027 h`, discoverable through DECRQM) gets extended grapheme clusters as the unit of cell layout: a ZWJ emoji like 🧑‍🌾 occupies one wide cell instead of shattering into per-codepoint cells, flag pairs consume exactly two columns with regional-indicator parity honored, Indic conjuncts join across their linker, and emoji variation selectors flip a cluster's width — VS16 wide, VS15 narrow, judged against the codepoint they actually follow — rather than being blindly attached. The design deviates from ghostty's reset-hooks on purpose: Rio keeps no cross-call segmentation state at all. A no-break sequence accumulates into a single cell, so everything the break rules can look behind at *is* the previous cell's contents, rebuilt per codepoint from `rio-unicode`'s conformance-tested machine; cursor movement, clears, scrolling and screen switches invalidate clusters by construction, with no hooks to forget. The hot path gets the treatment from ghostty's [devlog-006](https://mitchellh.com/writing/ghostty-devlog-006): a flat per-codepoint class table plus a precomputed 5,832-entry transition table that folds the entire break-rule chain into one indexed byte, generated at first use by driving the reference implementation so it can never drift from the rules it replaces — cutting the mode's worst-case overhead (pure CJK plus emoji, no ASCII in sight) from 3.05x to 1.53x, with the mode off staying bit-identical and cost-free. The rest of the grid keeps up: search feeds every codepoint a cell carries to the regex engine so a decomposed accent or a ZWJ sequence finally matches, reflow and serialization round-trip clusters intact, and embedders read cluster text through new wasm (`cluster_text()`) and C ABI (`rio_render_state_cell_cluster()`) channels. It ships on by default, the ghostty path (`grapheme-width-method = unicode`): a new top-level [`grapheme-clustering`](https://rioterm.com/docs/config#grapheme-clustering) config option turns it off for legacy wcwidth layout, every rio-vt consumer can configure its own default (librio exposes `set_grapheme_clustering` on the surface, the C ABI and the wasm class), a program's DECSET/DECRST always wins at runtime, and RIS restores the configured default rather than a hardcoded one. On the web it ships in [rioterm 0.1.8](https://www.npmjs.com/package/rioterm), whose canvas and DOM renderers now read the full cluster text off flagged cells instead of drawing only the base codepoint ([#1857](https://github.com/raphamorim/rio/pull/1857)).

- **Nix: a binary cache.** `nix run github:raphamorim/rio` no longer means compiling the whole workspace: every merge to main pushes the build to [rioterm.cachix.org](https://rioterm.cachix.org), and the flake advertises the substituter through `nixConfig`, so nix offers to enable it on first use and the build becomes a download ([#1829](https://github.com/raphamorim/rio/discussions/1829)). Two flake-CI repairs landed alongside: FlakeHub cache authentication had been silently disabled by missing OIDC permissions (the action logs told nobody in particular), and the flake build stopped charging every PR push, running on PRs only when they touch what the flake consumes while every merge to main still builds the full tree. Also fixed for flake users building with tests: the context tests were signalling PID 1 inside the nix sandbox, which is the builder itself, killing the build mid-run with no failure output.

- **CI costs an order of magnitude less per pull request.** A push to a PR branch fired both `push` and `pull_request` runs of everything, and each run built three MSYS2 flavors in release mode under full LTO: roughly 130 runner-minutes per push across up to 14 jobs, saturating the account-wide concurrency cap that GitHub applies even to public repos where minutes themselves are free. Pushes now trigger on main only, PRs smoke-test the one MSYS2 flavor upstream recommends (UCRT64, debug profile, cached) with the full three-flavor release matrix moved to the release gate where its LTO coverage belongs, stacked Nix builds cancel instead of piling up behind a contributor's pushes, `cargo fmt` runs once instead of three times, and CI builds drop line-level debug info so caches stay under the eviction threshold. The result is about 15 minutes across 5 jobs per PR push, with a build-timings report uploaded from every run so a future slowdown is one artifact download to diagnose.

- Wayland: background blur, beyond KDE. `window.blur = true` now works through `ext-background-effect-v1`, the cross-desktop staging protocol COSMIC and KWin ≥ 6.4 implement, so a translucent Rio gets frosted glass behind it on those compositors instead of only under the KDE-specific blur protocol, which stays as the fallback for older Plasma. The protocol starts every surface with an *empty* blur region, so Rio sets a full-surface one explicitly and lets the compositor clip it ([#1841](https://github.com/raphamorim/rio/pull/1841), by [@milkowski](https://github.com/milkowski)).

- Wayland: a compositor that drives key repeat itself no longer crashes Rio. `wl_keyboard` v10 lets the compositor take over repeat: it disables client-side repeat with `repeat_info { rate: 0 }`, then delivers held keys in a third `repeated` key state, which landed in a `_ => unreachable!()` arm and aborted the whole process. Repeated keys are now forwarded exactly as the client-side repeat timer forwards them, and unhandled keyboard events log a warning instead of taking the terminal down with them ([#1774](https://github.com/raphamorim/rio/pull/1774), by [@nikicat](https://github.com/nikicat)).

- Color emoji render correctly from bitmap fonts. Every strike format was pushed through Swash as if it were the same thing, and none of them were: PNG strikes (Noto Color Emoji) decode to straight RGBA that Rio's premultiplied atlas then over-darkened, raw CBDT strikes arrive as premultiplied *BGRA* that drew with red and blue swapped, and the packed CBDT formats make Swash under-allocate its decode buffer and panic outright. Each path is now handled for what it is: PNG premultiplied after decode, raw strikes channel-swapped and taken only at their exact strike size (resampling premultiplied data is what corrupted them), packed formats skipped before Swash can crash on them. The strike selection walks the font's CBLC table with the same nearest-size logic Swash uses, so the format decision always matches the strike Swash actually renders — and a synthetic-font test suite assembles real CBLC tables byte by byte to prove it, including truncated and lying ones. For the record: cosmic-text, the largest Swash consumer, still has all three bugs ([#1843](https://github.com/raphamorim/rio/pull/1843), by [@mirsella](https://github.com/mirsella)).

- macOS: SKK works. Its hiragana mode commits text directly without a preedit — input Rio dropped on the floor — and its mode-switch keys (Ctrl+J, `q`) reached the shell as control sequences because the IME consumes them through `selectMode()` without any text callback Rio could see. Direct commits are now detected by comparing what the IME inserts against what the key actually says. For the mode switches, Rio borrows the detection ghostty has shipped for over a year: each SKK mode is its own macOS input source, so a key that *changes the input source* mid-event belongs to the IME and never reaches the shell — and nothing else changes, so IMEs that quietly inspect Ctrl combinations (Chinese, Korean) can't swallow app-bound keys like Ctrl+C, the regression that pattern caused elsewhere. A new `keyboard.forward-to-ime-modifier-mask` additionally decides which modifier combinations go to the IME at all — matched as a subset, so listing `ctrl` doesn't drag Cmd+Ctrl combos along. The default forwards everything, exactly as before ([#1845](https://github.com/raphamorim/rio/pull/1845), by [@shiena](https://github.com/shiena)).

- The OSC 9;4 progress bar sits at the top of the window when the tab strip is hidden, instead of floating at the strip's height below nothing. With `hide-if-single` now the macOS default, a lone tab is exactly when this matters ([#1844](https://github.com/raphamorim/rio/pull/1844), by [@mirsella](https://github.com/mirsella)).

- `navigation.hide-if-single` now defaults per platform: `true` on macOS, where hiding the strip for a lone tab is the native-app feel, and `false` on Linux and Windows, where the strip stays as a centred title. Either value can still be set explicitly, globally or per platform.

- Windows: no flash of blank window at startup. Rio could show the bare native window before the first frame was ready, a white blink that made launch feel slower than it was. The window now stays cloaked (DWM's mechanism for exactly this) until the first frame has rendered, so it appears with content already in place ([#1814](https://github.com/raphamorim/rio/pull/1814), by [@lalvarezt](https://github.com/lalvarezt)).

- An unfocused window shows a hollow cursor. The cursor style decision consulted panel focus but not window focus, so the active panel kept drawing a solid block after you switched apps; it now checks both, giving an unfocused window the same hollow outline an unfocused split gets: steady, not blinking, until focus returns ([#1835](https://github.com/raphamorim/rio/pull/1835), by [@mirsella](https://github.com/mirsella)).

- `librio` builds on Windows again. `Surface::foreground_process_name` leaned on a process lookup and PTY fields that only exist on Unix, which broke the Windows build the moment it landed. On Windows it now returns an empty name, the same unavailable-result convention `working_dir` already follows there, with the Rust and C APIs unchanged ([#1838](https://github.com/raphamorim/rio/pull/1838), by [@mirsella](https://github.com/mirsella)).

- The Nix package build works again, and the OOM theory was wrong. Every Nix build died with exit code 129 partway through the test suite, which read as the kernel reclaiming a 7 GB runner; but 129 is SIGHUP, not the OOM killer's SIGKILL, and the death always landed on the context tests. The real cause: a dead context stores `1` as its placeholder `shell_pid`, and dropping it SIGHUPed that pid unconditionally — and in the Nix sandbox the builder shell *is* PID 1, so `cargo test` killed its own parent. The kill is now guarded against placeholder pids, the check phase that was briefly skipped as a stopgap ([#1828](https://github.com/raphamorim/rio/pull/1828), by [@bet4it](https://github.com/bet4it)) is back on, and the rio 0.5.x update in nixpkgs is unblocked ([#1842](https://github.com/raphamorim/rio/pull/1842), by [@pinpox](https://github.com/pinpox)).

## 0.5.19 (2026-08-07)

- Canario, security: a `canario://run` link could show one command in the confirmation and run a different one. Percent-encoded terminal control characters survived into the alert, where they have no glyph to give themselves away, and the approved string was then replayed into the shell as keystrokes: `canario://run?cmd=echo+Foo%15ls` reads as `echo Fools` and runs `ls`, because ^U kills the line the user was just shown. ^P recalls a history entry instead, and the bidi overrides reorder the rendered line without changing a byte of it. Deep-link parameters now reject every Unicode control and format character (categories Cc and Cf, which covers C0, DEL, C1, the bidi overrides and the zero-width formatters), so a link carrying one never opens at all ([#1824](https://github.com/raphamorim/rio/issues/1824), reported by [@lalvarezt](https://github.com/lalvarezt)).

- Canario: an approved command is no longer typed into the shell at all, which is the deeper half of that fix. It is handed over at spawn as a single `-c` argument, so no part of it is ever read by a line editor; `-i` keeps the interactive rc files sourced so aliases and PATH still apply, and a trailing exec leaves the usual interactive shell behind once it finishes. The old "wait 600ms and hope the shell has settled" timer is gone with it. The confirmation now renders the command in a bounded monospaced box, so a long one scrolls rather than pushing its tail out of a dialog that looks fully read, and Return activates nothing (Escape still cancels). Fixed on the way: `sendText` measured with `strlen`, so an embedded NUL silently truncated what was sent, a mismatch between what was displayed and what was transmitted that the paste path shared.

- `librio`: `rio_surface_config_s` gained `args` and `args_len`, an argv for the spawned program, so an embedder with a command to run can spawn it instead of typing it into the terminal. The fields are appended to the struct, so zero-initialized callers keep working untouched.

- Security issues can now be reported privately, through the repository's Security tab rather than a public issue, and the project has a security policy describing scope and disclosure.

## 0.5.18 (2026-08-07)

- Canario: color schemes, picked the way Arc and Zed pick them. ⇧⌘T (or "Change Theme" in the command bar) opens a floating picker with no scrim and no preview pane, because the live window is the preview: arrowing or hovering recolors every terminal instantly, Enter keeps it, Esc or a click outside puts everything back the way it was. Each row is a mini terminal rendered from the scheme's own tokens (a prompt line, ls colors, an error line, a selection run and the 16-color strip with the cursor block), so the colors a swatch can never show (selection and cursor over the real background) are visible before committing. The prebuilt set is the classics: Dracula, Catppuccin, Nord, Solarized, Gruvbox, Tokyo Night, One Half, Rosé Pine, GitHub, Monokai, Snazzy, Everforest, Kanagawa (vendored from the same iTerm2-Color-Schemes exports Ghostty and kitty bundle theirs from), plus Rio's default and [Lucario](https://github.com/raphamorim/lucario). The choice persists and new, split and restored sessions all start on it.

- `librio`: `rio_set_colors()` swaps the palette that named and indexed cell colors resolve against, at runtime and process-wide; NULL restores Rio's default theme. The render snapshot keeps each cell's original color form and resolves at query time, so a swap re-themes the entire scrollback on the host's next draw with no re-snapshot. Found on the way: the dim-color fallback (2/3 luminance) did its arithmetic in u8 and overflowed for any component of 128 or more (invisible with the default palette the tests exercised, guaranteed with brighter schemes), now widened, with the swap/reset path under test.

## 0.5.17 (2026-08-06)

- Canario: input methods actually work now. The in-progress composition (dead keys, Japanese, Chinese, Korean) was tracked but never drawn, so composing was invisible until commit; it now renders at the cursor the way IME-aware apps draw it, an inverted box with a thick underline that slides left when the composition outgrows the row. Option composes characters by default (the macOS convention Terminal.app, iTerm2 and ghostty share), so dead keys and international layouts work out of the box; alt-as-meta remains available through `librio`'s existing switch. The composition state machine follows ghostty's: a backspace that cancels romaji no longer also deletes terminal characters, committed text arrives as text-only key events rather than raw bytes (a `librio` path that existed but encoded nothing, now fixed with a test), bare control characters an input method consumed are never forwarded, arrows that commit Korean input still move the cursor afterwards, and a keystroke that switched keyboard layouts encodes nothing.

- Canario: keys report completely. Repeats are repeats rather than fresh presses, releases are delivered (programs using the kitty keyboard protocol's event types were receiving none), and bare modifier presses are reported in the protocol's report-all mode, with left/right distinguished the way ghostty distinguishes them. `librio`'s key model gained the nine modifier keys with kitty's assigned codes, encoded only where the protocol says they exist.

- Canario: no more flicker under heavy output. Rendering is paced to the display: PTY wakeups mark the session dirty and a per-view display link performs exactly one grid snapshot and draw per refresh, pausing when idle, so a flood of output can no longer outpace the screen or saturate the main thread with redundant snapshots (`seq 1 200000` renders clean in 0.24s). Applications that wrap redraws in synchronized output (`ESC[?2026`) were already buffered whole by Rio's parser, timeout and all; the pacing covers everything that doesn't.

- Canario: watchers, which are breakpoints for output. Select any text, right-click, "Watch for", and when that text next appears in the terminal's output, the sidebar tab badges with an eye and a hit count, and the Dock asks for attention if Canario is in the background. The scanner diffs each render's visible rows against the previous ones, so re-renders of an unchanged screen never refire, text already on screen when the watcher was created doesn't count, a chatty log is held to one hit per two seconds, and a terminal with no watchers pays nothing. The badge's context menu stops individual watchers or clears the count. iTerm2 has carried this power for a decade as regex Triggers buried in profile preferences; here it's two clicks on the text itself.

- Canario: picture-in-picture, for panes. Right-click a pane, "Pop Out Panel", and the running pane moves into a small always-on-top panel, the way videos pop out of a browser: watch the deploy from any app, and close the panel (or press Bring Back on the placeholder tile) to slide the pane back into its tab. It is the same live view, not a copy: the PTY, scrollback and selection all come along and come back.

- Canario: long commands report progress with OSC 9;4 (the ConEmu sequence winget, systemd and a growing list of CLIs emit), and Canario carries it beyond the window: a spinner or determinate ring on the sidebar tab, a live percentage on the Dock icon, and a menu-bar pill that jumps to the reporting terminal when clicked. Error state turns the pill into a warning and the ring red; indeterminate shows a spinner; state 0 clears all three. Ghostty draws these in-window; the point here is that progress follows you out of the app. `librio` forwards the reports over the C ABI as `RIO_ACTION_PROGRESS` (the action struct gained generic `data_a`/`data_b` payload fields).

- Canario: deep links. `canario://quick` toggles the quick terminal, `canario://new?space=Work&cwd=~/api` opens a terminal in the named space at a directory, `canario://terminal?title=htop` jumps to a session by fuzzy title, and `canario://run?cmd=npm+test&space=Work` opens a terminal and runs a command, after a confirmation showing the exact command, because URLs arrive from browsers and other apps and a shell one-liner should never execute on a click alone. Launchers, Shortcuts, other apps and README badges can now drive the terminal.

- Canario: ctrl+click opens the context menu everywhere in a pane, matching right-click. AppKit performs that translation in NSView's default mouse handling, which a terminal's own mouse handling replaces, so it is now done explicitly.

- `librio`: `rio_render_state_alt_screen()` reports whether the alternate screen (full-screen TUIs) is active, so embedders can gate prompt-output affordances away from applications that draw their own UI.

- `rio-vt`: the PTY driver is a feature now. Embedders that bring their own PTY layer and use rio-vt purely as a VT state machine can set `default-features = false` and drop `teletypewriter`, `corcovado` and 19 transitive dependencies, several of them unmaintained 2018-era crates that were a hard question for anyone auditing their tree. The `pty` feature stays in the default set, so nothing changes for Rio, Canario, or existing embedders ([#1819](https://github.com/raphamorim/rio/issues/1819), reported by [@gold-silver-copper](https://github.com/gold-silver-copper)).

- macOS: local network access works from programs running inside Rio and Canario. macOS attributes local-network traffic from child processes (`ssh host.local`, `nc` to a LAN address, mDNS lookups) to the responsible terminal application, and without `NSLocalNetworkUsageDescription` in the app's metadata the permission prompt never appears and connections quietly fail, and the terminal doesn't even show up under Privacy & Security → Local Network to allow by hand. Both apps now declare it, so the prompt appears the first time a program reaches for the local network ([#1821](https://github.com/raphamorim/rio/pull/1821), by [@Goooyi](https://github.com/Goooyi)).

## 0.5.16 (2026-08-06)

- Canario: images render in the terminal via the kitty graphics protocol. `chafa -f kitty`, `kitten icat`, yazi's previews and every other kitty-speaking tool now draw real pixels instead of character art: chunked transmissions reassemble, RGB and RGBA payloads both decode, retransmissions of the same image id refresh in place, and placements are anchored to the scrollback like every kitty terminal anchors them, so an image scrolls away with its prompt and comes back when you scroll up. Layering follows the protocol's z-index rules the way ghostty's renderer splits them: below cell backgrounds, below text (a chart can sit under its own axis labels), or above everything.

- Canario: virtual placements (`U=1`, the Unicode-placeholder mode `kitten icat` uses under multiplexers) work too. The renderer collapses runs of U+10EEEE placeholder cells, whose foreground color and combining diacritics say which image and which slice each cell shows, into image draws, with the aspect-fit and centering rules matching kitty's and ghostty's.

- Canario: images are objects, not just pixels. Click one and it opens in an Image Peek: an Arc-style lightbox that springs out of the cell rect, dims the window, and pinch-zooms up to 12x, with Esc or Space to close and arrow keys to walk every image in the buffer. Inside the peek, Live Text runs on the image (the same interface Photos uses), so text inside a matplotlib chart or a screenshot of an error is selectable and copyable, and QR codes a CLI prints are clickable. Right-click an image in the terminal for Copy Image, Save to Downloads, Open in Preview and Share, or press and drag it straight out into Finder, Slack or Figma as a PNG, the way images drag out of Messages.

- `librio`: the kitty pipeline is exposed over the C ABI for any embedder drawing its own cells: `rio_render_state_kitty_count()` and `rio_render_state_kitty_placement()` iterate placements as viewport-resolved pixel rectangles (z-sorted, with normalized source rects for crops), `rio_render_state_kitty_image_info()` carries dimensions plus a change stamp so decoded bitmaps can be cached until a retransmission, and `rio_render_state_kitty_image_rgba()` copies pixels out as tightly-packed RGBA. Virtual placeholder runs surface through the same iterator, so an embedder that draws direct placements gets `icat` under tmux for free.

- `librio`: two placement bugs found on the way. Surfaces created through the C API reported zero cell dimensions to the graphics subsystem, which silently dropped every kitty placement (the reason no embedder had images until now); cell metrics now derive from the surface's pixel size. And placement rows are absolute counting lines evicted from the scrollback ring, so once a long-lived session filled its scrollback, every image quietly culled as off-screen; the viewport math now accounts for evicted lines, with a regression test that forces eviction.

## 0.5.15 (2026-08-06)

- Canario: a settings window (⌘,), in the shape Arc uses: native preference tabs over dark grouped sections. The Text tab holds the terminal font: a size slider and every monospace family installed on the machine, each row rendered in its own typeface above a live sample, applied to running terminals as you click and persisted across launches. Families are found by asking each font whether it is fixed-pitch, so the list is exactly what a terminal can use.

- Canario: the Appearance tab makes the window's colors yours: window, text, selection and focused-panel border each get a picker, applied live and persisted. Text is picked once and the interface derives its opacity tiers from it, so a dark window with light text stays legible everywhere without adjusting five shades by hand. The preset swatches (salmon, blush, rosewood, cocoa, ultramarine, from the chrome study) each carry a complete four-color palette rather than just a window color, so one click can't strand black text on a dark window.

- Canario: Nerd Font icons render out of the box, sized and positioned like a patched font. Two things had to be true at once. First, the glyphs have to exist: the symbols-only Nerd Font is embedded in `librio` itself, the way libghostty embeds it, and Canario cascades to it from the terminal font, so Powerline separators, Font Awesome and Material icons resolve on a machine with no patched font installed and no font file in the app bundle. Second, they have to be placed correctly: raw icon glyphs are designed against wildly different boxes, which is what the Nerd Fonts patcher's per-glyph scaling rules are for. Rio carries a port of that table (by way of ghostty's generated version); Canario now applies it at render time, so Powerline triangles stretch to the full cell and butt seamlessly against their coloured background, icons centre on the patcher's icon height, and an icon with a free cell beside it may span two. Private-use glyphs the table doesn't know are fitted to their cells so nothing bleeds into the neighbour.

- `librio`: two additions for embedders drawing their own cells. `rio_symbols_nerd_font()` returns the embedded symbols-only Nerd Font as raw TTF bytes, so icon fallback needs no file management on the embedder's side. The Nerd Font constraint table moved from sugarloaf into `rio-fonts`, a new dependency-free crate for font assets and glyph placement rules (the symbols font lives there too, behind a `symbols-nerd-font` feature so only renderers that want the ~2 MB opt in), and is exposed as `rio_nerd_constrain()`: pass a codepoint and the glyph's measured bounding box, get back the box the patcher's rules would produce; the maths stays in one place, so any embedder's renderer lines up with Rio and ghostty by construction. One convention to know: boxes are y-up from the cell bottom, ghostty's coordinate space, so CoreText-style baseline-relative measurements shift by the descent on the way in and out.

## 0.5.14 (2026-08-05)

- Canario updates itself. A quiet pill appears beside the sidebar toggle when a new release is out (GitHub Releases is the feed; the notarized `Canario.dmg` per tag is the artifact); its dropdown offers installing or skipping that version. Installing downloads the image, swaps the app bundle in place and relaunches, with progress shown in the pill and a fall-back to revealing the image in Finder if any step refuses. Checks run shortly after launch and at most daily, plus a "Check for Updates…" menu item that also reports "up to date"; development builds never check. Sparkle remains the eventual destination, but this covers the same user story with no new dependencies while the release pipeline already publishes everything the updater needs.

- Canario: dragging a selection past the top or bottom edge now scrolls the view and keeps extending, ghostty's selection-scroll-tick behavior: speed grows with how far past the edge the pointer sits, and the anchor stays put because librio stores selections in absolute grid coordinates.

- Canario: the command palette is a real fuzzy finder now. Every whitespace-separated token must match, the space or parent name is part of the haystack (so "wrk" narrows to the Work space), scoring rewards word starts, prefixes and consecutive runs, and the matched characters light up in the results so it's visible why a row is there.

## 0.5.13 (2026-08-05)

- `librio`: shells now spawn with a TERM that actually resolves. The child inherited the host process's environment, which for an app launched from Finder contains no TERM at all, and for one launched from another terminal contains whatever that terminal used, such as `rio` on a machine where rio's terminfo was never installed. Either way programs that query terminfo got nothing back, which showed up as prompt frameworks drawing fragments of their glyphs, and ssh forwarding a TERM the remote host answered with "unknown terminal type". TERM is now resolved at spawn the way Rio resolves it: `xterm-rio` when that terminfo is installed, `rio` when only the older entry is, and `xterm-256color` otherwise, which is the case for a fresh Canario install and is known to every local terminfo database and every ssh host. `COLORTERM=truecolor` rides along.

- Canario: shells receive `TERM_PROGRAM=canario` and `TERM_PROGRAM_VERSION`, the same convention Rio follows, so shell integrations can tell which frontend they are running in.

## 0.5.12 (2026-08-05)

- Canario: the cursor no longer floats over whichever history row happens to occupy its grid position while the view is scrolled up. It is drawn at its viewport-relative position, sliding down as you scroll and not drawn at all once it leaves the viewport, which is where Ghostty draws the line too. `librio` gained `rio_render_state_display_offset()` so any renderer can make the same call.

- `librio`: input snaps a scrolled view back to the live screen and drops any selection, the scroll-to-bottom-on-input behaviour of every terminal. It lives in the one path all keys, text and paste go through, so it only fires when a key actually produced PTY bytes (Ghostty gates its equivalent the same way), and every embedder gets it rather than each frontend reimplementing it.

- Canario: ⌘W closes the focused panel rather than the whole terminal, falling back to closing the terminal when the panel is the last one, and to the window when nothing is open. ⇧⌘W closes the terminal with all its splits.

## 0.5.11 (2026-08-05)

- Canario, an experimental macOS frontend built on `librio`, ships from this release as `Canario.dmg`. It borrows its workflow from browsers: terminals live in coloured spaces with ⌘1 to ⌘9 to jump between them, ⌘K opens a command bar that finds any terminal or pane by title and runs app actions, hovering a tab or pane in the sidebar shows a live preview, ⌥⌘T summons a floating quick terminal over whatever app is frontmost, new shells can be routed into spaces by working directory, and quitting asks first. Sessions restore in full: the sidebar tree, splits, scrollback and each pane's working directory.

- `librio`: cells that carry only a background colour (the fills erase-to-end-of-line leaves behind, htop's header bar, blank lines after `clear`) resolved their colour through the style table, misreading the inline colour bits as a style id, so erase fills rendered with the wrong colour or none. The snapshot accessors now decode them as the bg-only cells they are. The same cells previously leaked NULs into persisted scrollback text, which now records them as trimmable spaces.

- `librio`: `Surface::working_dir()` no longer requires OSC 7. When the shell has not reported a directory, the kernel's view of the foreground process's cwd is used instead (proc_pidinfo on macOS, /proc elsewhere, via the same helper Rio already used for spawning daemons), so session restore and directory-based routing work with entirely unconfigured shells.

- `librio`: named and low-indexed colours resolve from Rio's default theme (`rio-vt`'s `Colors::default()`) rather than a hardcoded xterm palette, so an embedder's terminal matches Rio's look out of the box, pink cursor included, and the two can never drift apart. Everything downstream reads through one resolved table, which is where theme-file loading will plug in.

- The tab strip stays visible with a single tab, drawn as the tab title centred with nothing behind it, and `navigation.hide-if-single` defaults to `false` to match. A lone tab has nothing to be distinguished from, so the tab shape carried no information worth the space, and dropping it hands the title the width of the window rather than one tab slot, which is where long titles used to lose their ends. A custom tab colour, whether from colour automation or the picker, has no fill left to sit in and colours the title text instead. Set `navigation.hide-if-single = true` to keep the strip hidden on a single tab as before ([#1817](https://github.com/raphamorim/rio/pull/1817)).

- With a single tab, right-clicking anywhere on the strip opens the colour picker for it. The whole strip belongs to that tab now that its title is centred, where before only the first tab slot responded.

## 0.5.10 (2026-08-04)

- Windows: opening a hint could run arbitrary commands. The default hint launched `cmd /c start` with the matched text appended as an argument, and Rust quotes an argument only when it contains whitespace or a quote, so cmd metacharacters in the match reached cmd's parser as syntax: everything after an `&` in a URL ran as its own command. Nothing needed to be configured for this. The shipped hint rule covers OSC 8 hyperlinks, binds ctrl+shift+O, and enables mouse hints, so the requirements were that text arrived in the terminal (`cat` of a file, `curl` output, a log tail, an ssh banner) and that it was opened once. With OSC 8 the visible link text is chosen by whoever sent it, so it need not resemble what runs. Matches now go to the Windows default handler through `ShellExecuteW`, which takes the target as a single string rather than a command line, and `open` and `xdg-open` receive it as one argument with no shell involved. Nothing changes on macOS or Linux, which were never affected ([#1816](https://github.com/raphamorim/rio/pull/1816), reported by [@lalvarezt](https://github.com/lalvarezt) in [#1811](https://github.com/raphamorim/rio/issues/1811)).

- Windows: a hint that fails to open now says so in the log. `ShellExecuteW` reports failure in its return value, which was discarded, so the only symptom was a click that did nothing at all.

## 0.5.9 (2026-08-04)

- Applications that narrow the scroll region no longer lose rows from the screen. Rio derived the visible viewport from the DECSTBM scroll region rather than the screen height, so once vim, less, htop, top, tmux or man narrowed it, the snapshot the renderer draws from was short by however many rows the region excluded, while the terminal kept reporting its full height. The usual `CSI 1;Nr` shape showed up as the bottom row or two going stale; a region narrowed at the top (`CSI 2;8r`) shifted the whole viewport up. The region bounds where scrolling happens, not what is displayed, which is how alacritty and Ghostty both draw the line ([#1805](https://github.com/raphamorim/rio/pull/1805), reported by ratty).

- X10 mouse reporting (`CSI ? 9 h`) is now supported. Mode 9 was parsed as an unknown private mode and dropped, so a program that asked for it received nothing and had no way to find out why: DECRQM answered 0, not recognized. It joins 1000, 1002 and 1003 as a fourth protocol and reports what X10 defines and no more, meaning presses of the left, middle and right buttons, with no release, no motion and no modifier bits in the button code. Setting 1005 or 1006 alongside it still selects the UTF-8 or SGR wire format, since the format and the event mode are tracked separately ([#1809](https://github.com/raphamorim/rio/pull/1809), reported by ratty).

- Resetting a mouse protocol other than the one in use no longer leaves mouse reporting on. The four protocols are one setting, so setting any of them already cleared the others, but a reset cleared only its own bit: after `CSI ? 9 h`, a program that sent `CSI ? 1000 l` to stop reporting kept receiving reports. A reset now turns reporting off whichever of the four it names, since there is no protocol underneath to fall back to, which is how Ghostty models it. Sequences that reset the protocol they set, which is what programs actually send, behave as before.

- `rio-vt`: a grid one column wide no longer panics or hangs on double-width characters. A wide glyph needs a second cell for its trailing spacer, and on one column there is never one, nor can the usual escape of wrapping to make room help, because the new row's column 0 is also its last column. Writing `你` wrote the spacer past the end of the row and panicked, promoting an already-written cell with a variation selector did the same, and shrinking a row that already held a wide character displaced it into the next row where it landed at column 0 and was displaced again on every iteration, growing the row buffer without bound. That last one hangs rather than crashing, so it was the worse of the two. Writing and reflow now both drop the glyph for a blank narrow cell and a VS16 promotion is declined, which is what Ghostty settled on for the same degenerate size. Embedders are entitled to ask for such a grid: drag-resize and tiling layouts produce them transiently, and nothing announces that the floor is two columns ([#1807](https://github.com/raphamorim/rio/pull/1807), reported by ratty).

- `rio-vt`: `EventListener::event()` is gone. It was the trait's one required method, yet nothing in the tree called it and every implementation returned `(None, false)`, so the only method an embedder was obliged to write was the one that did nothing. Every other method on the trait already had a default. Implementors should delete their own override, which is the whole migration ([#1808](https://github.com/raphamorim/rio/pull/1808), reported by ratty).

- `rio-vt`: three accessors for embedders that encode their own key events or read cells directly. `keyboard_mode()` returns the active kitty keyboard flags, the same byte the terminal reports to the application, instead of every embedder rebuilding it from `Mode` bits and inheriting a bit order that drifts. xterm's `modifyOtherKeys` (`CSI > 4 ; n m`) is now parsed and readable through `modify_other_keys()`, where before it had no mode bit and no handler, leaving embedders to scan raw PTY bytes outside the parser for it. `Grid::cell_text()` yields a cell's base character followed by its zero-width marks, since `Square::c()` returns only the base and silently drops combining accents, ZWJ joiners and variation selectors.

## 0.5.8 (2026-08-04)

- `rio-vt` re-exports `teletypewriter` and `corcovado`, so an embedder that drives the PTY loop no longer has to declare them itself. `Machine` is generic over a teletypewriter trait and hands back a corcovado channel, which previously forced consumers to name both crates in their own manifest and keep those versions in lockstep with rio-vt's, or get type errors that read as `expected Sender<Msg>, found Sender<Msg>`. They are now reachable as `rio_vt::teletypewriter` and `rio_vt::corcovado`.

- `rio-vt` owns its terminal size type. `Msg::Resize`, `RioEvent::TextAreaSizeRequest` and `OnResize` carried `teletypewriter::WinsizeBuilder`, so anything touching the event model named the PTY crate's type for four `u16`s; they now carry `rio_vt::event::WindowSize` and the PTY driver converts at the one place it calls `set_winsize` ([#1802](https://github.com/raphamorim/rio/pull/1802)). Together with the re-exports, an embedder that supplies its own PTY needs `rio-vt` as its only rio dependency.

## 0.5.7 (2026-08-03)

- OSC 7 working directories with characters outside `[A-Za-z0-9/._-]` now resolve correctly. Shells percent-encode the path they report (that is what `vte-urlencode-cwd` and its equivalents are for), but Rio read it back through a URL parser whose `path()` hands the encoding straight through, so `/home/My Files` arrived as `/home/My%20Files` and anything downstream of the reported directory (the title, opening a new tab in the same place) was working from a path that did not exist. The path is now decoded, with a malformed escape passed through literally rather than dropping the directory over one stray `%`.

- OSC 7 payloads that are not `file://` are no longer accepted. The URL parser took whatever path came back for any scheme, so `OSC 7 ; http://elsewhere/tmp` would move Rio's idea of the working directory. Both foot and Ghostty require the `file` scheme; Rio now does too.

- A shell on the far side of an ssh session can no longer set the local working directory. Anything at the other end of the PTY can emit OSC 7, and Rio applied it unconditionally, so a remote prompt would report a directory that only exists on the remote machine. The host is now checked against this machine's hostname, accepting `localhost` and the empty host that `file://$PWD` produces (RFC 8089 defines that as the local machine, and rejecting it would break a common shell setup). A refused payload is logged with the host so a working directory that stops updating can be diagnosed rather than guessed at. This follows what foot and Ghostty already do; Ghostty's own note calls validating the host the best practice terminals follow.

- Vi mode gained the `{` and `}` paragraph motions, which skip any blank lines under the cursor and then run to the far side of the paragraph they land in. A line of spaces or tabs delimits a paragraph the same way an untouched one does, matching the other motions.

- macOS: a shell you configure yourself with [`shell.program`](https://rioterm.com/docs/config#shell) is now started directly rather than through `/usr/bin/login`. Rio wrapped every spawn, so a named program collected a utmp entry and a `Last login` banner it never asked for. Only the default shell, meaning no `shell.program` set, which is what Rio ships with, still goes through `login` for its login-session environment, so a default configuration behaves exactly as before. Alacritty draws the line in the same place.

- `shell.program` is now optional rather than an empty string standing in for "use my default shell"; an existing `program = ""` in a config keeps working and still means the default.

- A command that writes and exits in the same breath no longer loses its last chunk of output. The PTY reader broke out of its loop as soon as the child-exit event arrived, which races the final readable event, so the tail of a short-lived command could be dropped before it was parsed. The PTY is now drained once more after the child exits.

- `rio-vt` is a little over a third smaller: 84 crates to 54. `url` accounted for 29 of them: it pulled `idna`, the ICU normalizer and collections, `zerovec`, `yoke` and a handful of proc-macro crates that also serialize the build, all to parse the one URL the terminal core ever looks at. `bytemuck` and `lazy_static` were declared but never referenced anywhere in the crate.

- `rio-vt`: `ChildEvent::Exited` now carries the child's raw wait status (`waitpid` on unix, `GetExitCodeProcess` on Windows) and `Machine` forwards it as `RioEvent::ChildExited`, so an embedder can report the exit code of whatever it ran. `create_pty_with_spawn` and `create_pty` take an optional set of environment variables to apply on top of the inherited environment, and "no program configured" is now `None` instead of an empty string across the config type and all three `create_pty` entry points.

- `rio-vt`: the child's signal mask is reset before `exec`. Ignored signal dispositions were already reset, but a *blocked* mask survives `exec` too, so a shell spawned from a thread that had `SIGINT` blocked came up unable to see ctrl+c. Rio spawns from the main thread and was unaffected; embedders that build their terminal off-thread were not.

## 0.5.6 (2026-08-03)

- macOS: waking from sleep no longer leaves the font, grid, or scale wrong. Rio read the display scale once at window creation and then relied on a single AppKit notification to learn about changes, but wake sequences rebuild the backing store without the numeric scale changing, so nothing ever fired. The scale is now re-checked at cheap checkpoints (window focus, un-occlusion, screen changes, and a new app-level display-reconfiguration observer that sweeps every window), the Metal layer's `contentsScale` is re-asserted on every rescale instead of trusting its creation-time value, and the first frame after wake (which can fail to acquire a drawable) is retried instead of silently dropped (closes [#1506](https://github.com/raphamorim/rio/issues/1506)). Idle terminals also repaint immediately when the window becomes visible again, rather than waiting for shell output ([#1792](https://github.com/raphamorim/rio/pull/1792)).

- Switching to a tab that wasn't visible no longer shows glyphs at a stale size after a DPI change. Background tabs always received the new dimensions, but nothing forced their rendered cells to rebuild: a scale change now invalidates every pane in every tab, tab switches reliably trigger a full refresh (the change detector compared layout node ids across tabs' separate layout trees, which collide by construction), and per-pane events (damage marks, titles, color requests) now reach panes in background tabs instead of being dropped until that pane's own shell produced output.

- Split paddings, gaps, and divider hit-boxes now follow the current display scale; they previously kept the DPI of the display the tab was created on.

## 0.5.5 (2026-08-01)

- Fixed typing echo in synchronized-output TUIs arriving ~150-200 ms late through Windows ConPTY. ConPTY coalesces output, so a frame's begin/end synchronized-update pair (mode 2026) often arrives inside one chunk; the begin armed Rio's 150 ms timeout but the inline end never disarmed it, so every following frame waited out the timer. An inline end now disarms the timeout, and replaying a buffered update re-arms it when a new begin is still pending (closes [#1753](https://github.com/raphamorim/rio/issues/1753): thanks to @cmoron for the original diagnosis and fix in [#1754](https://github.com/raphamorim/rio/pull/1754), ported to `rio-vt` by @marc2332 in [#1788](https://github.com/raphamorim/rio/pull/1788), landed via [#1789](https://github.com/raphamorim/rio/pull/1789)).

- `rio-vt`: synchronized updates whose 150 ms deadline has passed now flush on the next `advance()` call. Rio's event loop already flushed at the deadline, but embedders that drive the processor straight from a read loop had no timeout driver at all: a client that armed mode 2026 and stalled could freeze their screen until 2 MB of output accumulated. The latency cap now works out of the box for every embedder.

## 0.5.4 (2026-08-01)

- `rio-vt`: inserting a sixel/iTerm2 image with unset cell dimensions no longer panics. Headless embedders never report a cell size, so arbitrary input containing image sequences could hit a `step_by(0)` panic in the row-fill loop; it is now a no-op, matching the existing guard on the kitty placement path. Found by atuin's property tests during its vt100 → `rio-vt` migration.

## 0.5.3 (2026-07-31)

- The terminal engine (`rio-vt`) is substantially faster across the board. Plain text and scrolling-heavy output parse ~2.7x faster (865 → 2340 MiB/s on plain ASCII, 266 → 772 MiB/s on scroll-dominated streams), CJK and emoji text 3x (243 → 726 MiB/s), color-change-heavy streams 1.6x, and full-screen TUI redraws 1.6x. Under the hood: scrolling no longer pays per-row dirty and damage bookkeeping (a full-screen scroll marks damage once), recycled rows reset only their occupied cells, printable runs and CSI parameters are scanned a word at a time instead of a byte at a time, SGR dispatch no longer heap-allocates per sequence, wide characters are written as bulk cell pairs the way ASCII runs already were, UTF-8 decodes in large chunks instead of per-fragment, and the character width table covers emoji directly. Measured with [rio-vt-benchmark](https://github.com/raphamorim/rio-vt-benchmark).

- `rio-vt` now exposes its VT parser and the `Perform` trait, so embedders can drive the escape-sequence parser with their own handler instead of the built-in grid. The `Handler` trait also gained a provided `input_ascii_str` method that carries the parser's printable-ASCII guarantee, letting implementations skip revalidating the bytes.

- macOS: quake mode now opens with the right size and position on setups with mixed-DPI monitors. The window geometry was computed in physical pixels for the monitor under the cursor but applied using the window's own backing scale, so the dropdown came out mis-sized when the two differed; it is now converted through the target monitor's scale factor.

## 0.5.2 (2026-07-28)

- Maintenance release fixing the release pipeline: the `rio-vt` crate publish and the macOS dmg artifact were both missing from the 0.5.1 release.

## 0.5.1 (2026-07-28)

- Windows: resizing the window while the terminal sat idle could leave the newly exposed area unpainted (the desktop showed through behind Rio) until the next keystroke or output. A resize reflows the grid, but presenting a frame was gated on terminal damage, so an idle resize produced no repaint into the freshly reconfigured swapchain; the resize path now marks the grid dirty so it always presents (closes [#1773](https://github.com/raphamorim/rio/issues/1773), and the resize symptom in [#1759](https://github.com/raphamorim/rio/issues/1759)).

- The bundled `corcovado` event-loop crate now uses `socket2` in place of the unmaintained `net2`, clearing the RUSTSEC-2020-0016 advisory for every project that depends on `rio-vt` or `teletypewriter`.

## 0.5.0 (2026-07-27)

- Rio's terminal core is now an embeddable library, split out of the app. **`rio-vt`** is a standalone, dependency-light Rust crate: the VT state machine, ANSI/escape parser, grid with scrollback, selection, search, PTY driver, and the sixel / Kitty / iTerm2 image protocols, with no renderer, GPU, or font-shaping code, so building it with no features is just the terminal. **`librio`** wraps that same core behind a C ABI for Swift, C, and any language that speaks C, shipped as a `RioKit.xcframework` plus a bare `librio.a` + `librio.h` on each release rather than a published crate. Rio's own frontend and other products now run the same engine; `rio-vt` is already in production at [Lovable](https://lovable.dev). Full write-up in the [announcement post](https://rioterm.com/blog/2026/07/27/rio-vt-and-librio).

- Glyph Protocol registrations accept a `width` parameter again (`1` or `2`), honored purely at render time: a wide glyph paints across two cells while the codepoint's logical width stays at one cell (its `wcwidth`), so cursor position, wrapping and selection never desync from width-unaware applications like shells and line editors. Authors of wide glyphs emit a trailing space so the overflow lands on an empty cell. Registrations and clears now also repaint cells already showing the codepoint, and glyph rasterization is hardened against hostile payloads (a bounding-box contain guard plus a hard raster dimension ceiling make degenerate `upm` values harmless). This settles [#1649](https://github.com/raphamorim/rio/issues/1649) by keeping `width` in the spec at render level only ([#1650](https://github.com/raphamorim/rio/pull/1650)).

- Fixed the mouse wheel needing two notches per scrolled line with notched mice: wheel ticks were converted to pixels with the font size but consumed in cell-height units, so a single notch floored to zero lines. One notch is now exactly one line before the configured [`scroll.multiplier`](https://rioterm.com/docs/config#scroll), so the default scrolls the conventional three lines per notch (closes [#1350](https://github.com/raphamorim/rio/issues/1350)).

- macOS: clicking the dock icon while every Rio window is minimized now restores and focuses one, instead of doing nothing (closes [#1151](https://github.com/raphamorim/rio/issues/1151)).

- Fixed a stale selection surviving panel switches: clicking into an unfocused split never reached the click handler that resets selections, so the next drag in that panel extended a leftover anchor instead of starting fresh (closes [#1638](https://github.com/raphamorim/rio/issues/1638)).

- Fixed `copy-on-select` silently doing nothing when a selection ends on top of a hyperlink or hint: the hint fired first and its early return skipped the copy. Finishing a selection now copies it and no longer activates whatever sits under the release point, so sweeping a selection across a URL doesn't open it; plain clicks trigger hints exactly as before (closes [#1494](https://github.com/raphamorim/rio/issues/1494)).

- Hints rules now accept the TOML shapes the docs always showed. `[hints.rules.action]` with `command = "open"` parses (it previously failed with "missing field action" and silently reverted the whole `[hints]` table to defaults), and both keys can be written inline on the rule: `action = "Copy"` or `command = { program = "code", args = ["--goto"] }`. Misconfigured rules produce a clear error message (closes [#1407](https://github.com/raphamorim/rio/issues/1407), [#1618](https://github.com/raphamorim/rio/issues/1618)).

- Fixed dead ctrl+c and ctrl+\ in shells when Rio was launched as a background job (`rio &` from a script, some desktop launchers): ignored signal dispositions survive exec and the fork pty child never reset them. The child now resets the full set before exec, which also restores default `SIGPIPE` handling (the Rust runtime ignores it in the parent), fixing pipeline semantics like `yes | head` for every shell spawned through the default Linux/BSD fork path (closes [#1120](https://github.com/raphamorim/rio/issues/1120)).

- Windows: hyperlinks open through `ShellExecuteW` instead of `cmd /c start`, whose argument re-quoting corrupted URLs (`https://` became `tps://`) or made clicks silently do nothing (closes [#1457](https://github.com/raphamorim/rio/issues/1457), [#1278](https://github.com/raphamorim/rio/issues/1278)).

- OSC 52 clipboard support is now advertised in the primary device attributes response, so applications that probe DA1 before enabling remote clipboard writes (tcell-based tools over ssh, for example) will use it (closes [#1398](https://github.com/raphamorim/rio/issues/1398)).

## 0.4.12 (2026-07-22)

- Quake mode: bind `ToggleQuake` and a dropdown terminal slides in from the top of the monitor under your cursor. The binding also works as a system wide hotkey while Rio is unfocused (no accessibility permission needed on macOS), hiding it restores focus to the app you were in, and the size is configurable via [`window.quake-width-percentage`](https://rioterm.com/docs/config#windowquake-width-percentage) and [`window.quake-height-percentage`](https://rioterm.com/docs/config#windowquake-height-percentage). Binding changes live reload (closes [#89](https://github.com/raphamorim/rio/issues/89)).

  ```toml
  [bindings]
  keys = [{ key = "'", with = "super", action = "ToggleQuake" }]
  ```

- Shell integration: OSC 133 semantic prompt zones are now tracked (closes [#975](https://github.com/raphamorim/rio/issues/975)), with two new binding actions, `ScrollToPrevPrompt` and `ScrollToNextPrompt`, that jump between shell prompts in the scrollback. OSC 1337 `SetUserVar` is parsed and stored per terminal (closes [#976](https://github.com/raphamorim/rio/issues/976)); these sequences were previously dropped by the image protocol parser.

  ```toml
  [bindings]
  keys = [
    { key = "up", with = "super | shift", action = "ScrollToPrevPrompt" },
    { key = "down", with = "super | shift", action = "ScrollToNextPrompt" },
  ]
  ```

- Sixel and iTerm2 inline images are back (broken across 0.4.x, closes [#1591](https://github.com/raphamorim/rio/issues/1591)), rebuilt so images behave as grid content: they scroll with the text including inside scroll regions, are clipped by characters printed over them and by erase/delete sequences, survive in scrollback until their rows scroll off, and stay inside their split panel. Cursor movement after an image follows each protocol: sixel lands on the image's last row (with mode 8452 and DECSDM honored), iTerm2 moves right of the image and supports `doNotMoveCursor`.

- Kitty graphics protocol fixes: images no longer drift once scrollback saturates, they expire when their rows leave scrollback instead of lingering forever, deletions repaint immediately, and images no longer bleed across split panel borders. Images also render on the CPU fallback renderer now, and GPU image textures are bounded by a least recently used budget instead of growing without limit.

- Fixed yazi image previews collapsing into a tiny single-cell thumbnail: kitty virtual placements transmitted without the `c=`/`r=` grid parameters (what yazi's kitty protocol driver emits) now imply their grid from the image and cell size, instead of squeezing the whole image into a 1×1 placement box ([#1314](https://github.com/raphamorim/rio/issues/1314), [#1530](https://github.com/raphamorim/rio/issues/1530)).

- Fixed `[shell] args` being silently dropped on Linux and BSD: the fork pty path (the default there via `use-fork = true`) never passed the configured arguments to the shell. This also fixes `OpenConfigEditor` launching the editor without the config file (closes [#1016](https://github.com/raphamorim/rio/issues/1016), [#1423](https://github.com/raphamorim/rio/issues/1423)).

- Fixed two crashes: the rich text atlas textures now respect the GPU's texture size limit instead of hardcoding 4096 (startup crash on Raspberry Pi class GPUs, closes [#1641](https://github.com/raphamorim/rio/issues/1641)), and the grid iterator no longer panics on positions stale relative to the live grid, such as during tiling window manager resizes (closes [#1713](https://github.com/raphamorim/rio/issues/1713)).

- Fixed the tab bar overlapping the last terminal lines after opening a tab (and the leftover gap after closing back to one tab). Margin changes from the tab bar appearing or disappearing never reached the layout engine until a window resize; they now trigger a full relayout of every tab (closes [#1495](https://github.com/raphamorim/rio/issues/1495), [#1528](https://github.com/raphamorim/rio/issues/1528)).

- Tabs grow up to 240px wide (was 180px) so titles crop later, and the cap is now configurable via [`navigation.max-tab-width`](https://rioterm.com/docs/config#navigationmax-tab-width).

- Dropped file paths are now shell escaped: paths containing spaces, parentheses, quotes or other shell metacharacters get each sensitive character prefixed with a backslash instead of being pasted raw (closes [#1730](https://github.com/raphamorim/rio/issues/1730)).

- Fixed `[shell] args` containing spaces being word split on macOS: custom commands are now passed directly to `login(1)` instead of going through an intermediate shell string, so arguments like `args = ["-c", "tmux attach || tmux"]` survive intact.

## 0.4.11 (2026-07-20)

- Font caches and glyph atlases are now cleared when the font library changes. Stale entries could serve the old font's shaping, metrics and bitmaps after a config reload, or crash on font ids past the new library's length (closes [#1639](https://github.com/raphamorim/rio/issues/1639), [#1110](https://github.com/raphamorim/rio/issues/1110), [#818](https://github.com/raphamorim/rio/issues/818)).
- Glyph atlases recover when full: instead of silently dropping characters once the atlas hit its size limit, it is cleared and every row re-emitted against the fresh atlas.
- Fonts inside TrueType collections (.ttc) load the right face. The face index was ignored when loading configured fonts, so families like Sarasa rendered whichever face sat first in the file (closes [#1302](https://github.com/raphamorim/rio/issues/1302)).
- The configured bold/italic/bold-italic font wins over weight metadata, so families that ship their bold at weight 600 (Nerd Font patches, Operator Mono) no longer render bold cells with the regular face or stack faux bold on a real bold face.
- [`fonts.features`](https://rioterm.com/docs/config#fontsfeatures) works again and now live reloads. Features reach both shapers (swash and CoreText), and can be disabled with a `-` prefix: `features = ["-calt", "-liga"]` turns ligatures off (closes [#1125](https://github.com/raphamorim/rio/issues/1125), [#1032](https://github.com/raphamorim/rio/issues/1032), [#1258](https://github.com/raphamorim/rio/issues/1258)).
- [`fonts.hinting`](https://rioterm.com/docs/config#fontshinting) now applies to the terminal grid; it was previously hardcoded on.
- Per-slot font `weight` is back: `[fonts.bold] weight = 600` steers face selection and pins the `wght` axis on variable fonts, which also fixes variable-font weights never being applied on Linux and Windows (closes [#1577](https://github.com/raphamorim/rio/issues/1577)).
- Font families are matched by any of their names, so CJK fonts like "Source Han Mono SC" or "LXGW WenKai Mono" are found by their English alias, not only the localized name listed first in the file (closes [#1466](https://github.com/raphamorim/rio/issues/1466), thanks [@qiuzhiqian](https://github.com/qiuzhiqian) for the analysis).
- ctrl+digit and ctrl+punctuation combos reach the terminal: Rio computes the C0 control byte itself (ctrl+6 sends 0x1E, ctrl+/ sends 0x1F, same table kitty uses) instead of relying on inconsistent platform behavior. The kitty keyboard protocol encoding is untouched (closes [#863](https://github.com/raphamorim/rio/issues/863), [#1328](https://github.com/raphamorim/rio/issues/1328)).
- `[bindings]` edits live reload instead of requiring a new window, unknown binding actions are rejected loudly instead of silently unbinding the default, and `f1` through `f20` can be used as binding keys (plus `enter`, `escape` and `backspace` aliases).
- Fixed font size changing by two steps per keypress: the increase/decrease defaults were registered twice on every platform.
- Updated `swash` to 0.2.10, which fixes a hinting cache regression that rebuilt hinting state per glyph.

## 0.4.10 (2026-07-09)

- New `SelectAll` action: selects the entire terminal content, including the scrollback history, so it can be copied. Bound to `Command + A` on macOS by default, and available on every platform for custom [key bindings](https://rioterm.com/docs/key-bindings) via `action = "SelectAll"`.
- Fixed hint label rendering: keyboard hint labels (hyperlink hints) are now drawn as proper grid overlays using the configured hint foreground/background colors, with the leading character of each label highlighted.
- Tab strip refresh: tab island backgrounds are now derived automatically from the window background color. `colors.tabs` and `colors.tabs-active` now set the inactive and active tab title colors, and the `bar`, `tab-border`, `tabs-foreground`, `tabs-active-foreground` and `tabs-active-highlight` color options were removed.

## 0.4.9 (2026-07-07)

- Updated `wgpu` to 30.0.0 and the `librashader` filter chain to 0.12 (the wgpu path used for RetroArch shaders on Windows/WASM and behind the `wgpu` feature flag elsewhere). librashader 0.12 brings a smarter framebuffer pool that only allocates intermediate framebuffers for the shader passes that actually need them.

## 0.4.8 (2026-06-27)

- Adaptive theme on Linux: Rio now follows the system light/dark preference on Wayland and X11. With [`adaptive-theme`](https://rioterm.com/docs/config#adaptive-theme) configured, Rio watches the XDG Desktop Portal `color-scheme` setting (the same one Firefox and GNOME/KDE use) and switches between your light and dark themes live when the system appearance changes, no restart required. This previously worked only on macOS and Windows ([#408](https://github.com/raphamorim/rio/issues/408); based on [#1358](https://github.com/raphamorim/rio/pull/1358) by [@pinpox](https://github.com/pinpox)).

## 0.4.7 (2026-06-09)

- Tabs can now be reordered by dragging them along the tab strip. Displaced tabs slide into place with a spring animation, and a released tab settles into its new slot.
- Font fallback discovery now works on Linux and Windows: glyphs missing from your primary font (Nerd Font icons, CJK, emoji, and box/symbol characters) are discovered from installed system fonts (fontconfig on Linux, font-kit on Windows) and registered on the fly, instead of rendering as tofu (missing-glyph boxes). Previously only macOS did this. ([#1630](https://github.com/raphamorim/rio/pull/1630) by [@nikicat](https://github.com/nikicat), [#1642](https://github.com/raphamorim/rio/pull/1642) by [@pinpox](https://github.com/pinpox); closes [#1015](https://github.com/raphamorim/rio/issues/1015)).
- A tab's custom background color now follows the tab when tabs are reordered or closed, instead of staying pinned to a slot and painting under the wrong tab. The color picker also closes when its tab is closed.
- New tabs show `~` as a placeholder title until the running program sets one.
- The smooth (trail) cursor no longer animates between tabs and panels: switching tab or split places the cursor directly instead of sliding a trail across the screen.

## 0.4.6 (2026-06-05)

- Box-drawing and related symbols render again. Box-drawing, block elements, braille, powerline separators, geometric shapes, and legacy computing symbols are now drawn as crisp, pixel-aligned atlas sprites that tile seamlessly at any font size and scale factor: they no longer fall back to the font's (often gappy) glyphs. These had been dropped during the v0.4.x grid rewrite.
- Kitty graphics: the `X=`/`Y=` sub-cell pixel offset is now applied when placing images ([#1645](https://github.com/raphamorim/rio/pull/1645) by [@floens](https://github.com/floens)): the parser already understood it, but placements were pinned to the cell boundary. Offsets are clamped to the cell box per the kitty spec, and an offset that spills the image into an extra row/column is now accounted for in cursor movement and row occupation.
- Updated `wgpu` to 29.0.3 and the `librashader` filter chain to 0.11 (the wgpu path used for RetroArch shaders on Windows/WASM and behind the `wgpu` feature flag elsewhere).

## 0.4.5 (2026-05-21)

- Tab color picker now also opens on right-click (previously Control + left-click only).
- New reset swatch in the tab color picker: a slashed box at the end of the swatch row clears the tab's custom color back to the default.
- Fix clicks and cursor overrides being intercepted over the empty band at the top of the window when `navigation.hide-if-single` hides the tab strip on a single tab. New shared `Navigation::island_visible(num_tabs)` predicate keeps the renderer, click router, and cursor handler aligned: the empty band now passes through to the grid for selection and double-click maximize.

## 0.4.4 (2026-05-12)

- fix characters disappearing while typing on macOS: e.g. "agg" rendering as "ag", "355" as "35" ([#1595](https://github.com/raphamorim/rio/issues/1595)). The text-run iterator now breaks the run around the cursor cell so speculative substitutions on later cells can't blank earlier ones.
- fix panic on window resize / maximize ([#1593](https://github.com/raphamorim/rio/issues/1593), [#1596](https://github.com/raphamorim/rio/pull/1596) by [@kronberger-droid](https://github.com/kronberger-droid)). Grid dimensions are now sourced from the same snapshot that sized the row buffers, not from a live value that could race ahead.
- macOS: text selection at the top of the window now works when `navigation.hide-if-single` is on with a single tab ([#1512](https://github.com/raphamorim/rio/issues/1512), [#1516](https://github.com/raphamorim/rio/pull/1516) by [@0x0aa](https://github.com/0x0aa)). Click routing and the cursor override now share a single gate that matches what the renderer paints.
- reduce per-frame allocations in the rioterm renderer: per-cell styles are materialized once per snapshot, per-row dirty bits skip unchanged rows on partial damage, and visible-cell extras live in a per-frame map.
- text-run shape cache key is now position-independent, and runs no longer break on space: adjacent words shape in a single call and cache hits stay high as the cursor walks a row.

## 0.4.3 (2026-05-08)

- fix wrong font size and window chrome alignment after connecting/disconnecting an external monitor ([#1588](https://github.com/raphamorim/rio/pull/1588) by [@tauil](https://github.com/tauil)).
- drop the `copa` and `rio-proc-macros` crates: the escape-sequence parser now lives in
`rio-backend` with hot-path optimizations (bulk ASCII dispatch, inline OSC buffer, flat
codepoint-width table, fused validate+decode).
- adopt `simdutf` for SIMD UTF-8 transcoding, validation, and base64 decoding.

## 0.4.2 (2026-05-03)

- macOS Metal: window transparency now actually renders. The `CAMetalLayer` is flipped to non-opaque whenever `window.opacity < 1` or a blur style is set, so the alpha in the drawable's pixels reaches the compositor. Default windows stay opaque so the macOS opaque-window fast path is preserved.
- Per-cell background alpha: cells that don't paint an explicit background color now write `alpha = 0` instead of stamping the default theme background at full alpha. The drawable's translucent clear shows through, which is what makes the window actually look transparent (not just the margins). Cells with an SGR-set background, selection / search highlights, and inverse-video cells stay fully opaque.
- New `window.opacity-cells` config option. Off by default: cells with an SGR-set background stay fully opaque so syntax-highlighted regions and TUI panels (Neovim, tmux, lazygit) keep their contrast. Set to `true` to apply the opacity multiplier to those cells too, so the entire terminal, including TUI surfaces, shares the configured translucency.
- `window.blur` now accepts the macOS liquid-glass styles in addition to the existing bool. New string values: `"macos-glass-regular"` (regular opacity) and `"macos-glass-clear"` (highly transparent), both available on macOS 26 (Tahoe) and later. Older macOS / other platforms fall back to the standard system blur and emit a warning instead of failing. Existing `blur = true` / `blur = false` configs are unchanged.

## 0.4.1 (2026-05-02)

- small updates on goreleaser build config for perf optimizations.

## 0.4.0 (2026-04-29)

Note: There's a decent chance v0.4.0 may not work for many users. I've done my best to test it on my very old Windows and Linux machines, but it's difficult to cover all cases. This version includes many rewrites, so I recommend checking that it works properly before updating.

- reduced app size (dmg for example went from 16mb to 7.11mb).
- macos core text for searching and shaping.
- yeslogic-fontconfig-sys and windows core apis for searching. swash keeps for shaping.
- removed the extra ir for rendering so no more textruns, less memory and faster operations
- vulkan native support.
- no more font extras, rio will always find the glyph if the font is installed.
- wgpu now is optional via feature flag, this feature flag is required if you use rio with retro arch shaders. This is a movement to make rio possible for official debian.
- rio now only renders on Screen::render to ensure GPU is not stuck or overloaded during high render peaks.
- renderer.backend is only three options: `wgpu`, `metal` and `vulkan`.

tested with 7e222ef2f1bc0a9eb4ff7f111b8f4c94faa9be97

| Scenario                       | main                          | new-v4                          | Delta                    |
|--------------------------------|-------------------------------|---------------------------------|--------------------------|
| Idle frame                     | ~150 µs                       | ~100 µs                         | v4 ~33% faster           |
| Typing, light redraw           | ~300 µs                       | ~200 µs                         | v4 ~33% faster           |
| Shell output / row damage      | 1000–2000 µs                  | 100–300 µs                      | **v4 5–10× faster**      |
| 25-row scroll frame            | 5500 µs (renderer=3448 µs)    | 1661 µs (emit=1424 µs)          | **v4 ~3× faster**        |
| Trail animation tail (CPU work)| ~300 µs                       | ~200 µs                         | comparable, v4 slightly faster |
| GPU exec time (both)           | ~700–1000 µs                  | ~700–1500 µs                    | comparable               |
| First-frame setup              | 15 638 µs                     | 9 397 µs                        | v4 ~40% faster           |

## 0.3.11 (2026-04-17)

- Windows: native `MessageBoxW` close-confirmation dialog when `confirm-before-quit = true` (default), mirroring macOS's `applicationShouldTerminate` `NSAlert` in `rio-window/src/platform_impl/macos/app_delegate.rs`. Replaces rio's in-window GPU-rendered confirmation modal on Windows. New `EventLoopRunner.confirm_before_quit: Cell<bool>` set via the same public `EventLoop::set_confirm_before_quit` that macOS already uses (cfg gate widened to `any(target_os = "macos", target_os = "windows")`). The `WM_CLOSE` handler in `event_loop.rs` checks the flag and `vsync_state.window_count() <= 1` (only the *last* window prompts, matching macOS's app-wide quit semantics) and on confirm-No swallows the message so the window stays open. rioterm's `WindowEvent::CloseRequested` handler now follows the same fast path on Windows as on macOS (`routes.remove(&window_id)` + maybe-`exit()`) since the user already confirmed at the OS layer.
- Windows render loop now matches the macOS / CVDisplayLink model exactly: `Window::request_redraw` sets a per-window `Arc<AtomicBool>` dirty flag instead of immediately calling `RedrawWindow(RDW_INTERNALPAINT)`. The DwmFlush worker thread (`rio-window/src/platform_impl/windows/event_loop/vsync.rs`) is the single source of frame timing: per composition cycle it iterates the window registry and, for each window where `dirty || should_present_after_input` (1 s post-input window, identical to macOS), calls `RedrawWindow(.., RDW_INVALIDATE)`. Existing `WM_PAINT` → `RedrawRequested` path is unchanged. Plumbing: new `vsync::VSyncSharedState` (`Arc<RwLock<HashMap<HWND, Arc<AtomicBool>>>>` registry + `Mutex<Instant>` for last input) lives on `ActiveEventLoop`, threaded into `WindowData` and the `Window` struct so input handlers, `request_redraw`, and the worker all share it. `Window::Drop` unregisters before `DESTROY_MSG_ID`. Replaces the earlier "always invalidate per vsync" zed mirror; that approach worked but didn't compose with the app's own `request_redraw`, which made scroll feel locked to DWM tick. Now scroll-driven renders queue immediately into the next vsync (≤16 ms wait) and idle frames are skipped entirely. Net behaviour matches macOS one-to-one.
- Windows: `window.blur = true` now actually blurs (was a no-op stub). Implementation calls `DwmSetWindowAttribute` with `DWMWA_SYSTEMBACKDROP_TYPE = DWMSBT_TRANSIENTWINDOW` (Acrylic) when blur is on, `DWMSBT_NONE` when off: uses the existing `set_system_backdrop` plumbing in `rio-window/src/platform_impl/windows/window.rs`. Wired both at window creation (via `attributes.blur` in `on_create`) and at runtime via the cross-platform `Window::set_blur`. Requires Windows 11 22H2+; older builds silently no-op (matches the previous behaviour).
- Windows: vsync-driven render loop with 1-second post-input sustain (matches macOS / Linux / zed behaviour). All inside `rio-window`; frontends still just respond to `RedrawRequested`. New worker thread (`platform_impl/windows/event_loop/vsync.rs`) calls `DwmFlush()` per composition cycle; when DWM is disabled, the monitor is asleep, or the call returns under the 1 ms threshold (RDP / occluded), it falls back to `thread::sleep` at the queried `DwmGetCompositionTimingInfo.qpcRefreshPeriod` interval, with a `rateRefresh` numerator/denominator fallback for spuriously low values and a 16.6 ms (60 Hz) default if both queries fail. Same heuristic zed uses in `gpui_windows/src/vsync.rs`. Each tick `PostMessageW`'s a registered custom message (`Winit::VsyncTick`) to the existing `thread_msg_target`; the handler in `thread_event_target_callback` enumerates thread windows via `EnumThreadWindows`, filters with `IsWindowVisible`, and posts `RedrawWindow(hwnd, RDW_INTERNALPAINT)` per visible window so the existing `WM_PAINT` path emits `RedrawRequested` unchanged. The fan-out is gated on a new `EventLoopRunner.should_present_after_input()` (1 s window after the last input event), with the same field/method names as the macOS / Wayland / X11 implementations. `mark_input_received` is hooked into `public_window_callback_inner` for `WM_KEY{DOWN,UP}`, `WM_SYSKEY{DOWN,UP}`, `WM_MOUSE{MOVE,WHEEL,HWHEEL}`, all four button down/up pairs, `WM_TOUCH`, and `WM_POINTER{DOWN,UPDATE,UP}`. `EventLoop::drop` now joins the worker thread (via a `stub()`-and-`mem::replace` swap) before destroying `thread_msg_target` so the worker can never `PostMessageW` to a freed HWND. New windows-sys feature `Win32_System_Performance` for `QueryPerformanceFrequency`. `build.rs` no longer panics when cross-compiling between host and target OS (`#[cfg(target_os = ...)]` branches now also check `CARGO_CFG_TARGET_OS`), so `cargo check --target x86_64-pc-windows-gnu` actually works from a macOS workstation.
- Linux: vsync-driven render loop with 1-second post-input sustain (matches macOS / zed behaviour). Two changes inside `rio-window` (no rioterm churn; frontends still just respond to `RedrawRequested`). **Wayland** (`platform_impl/linux/wayland/event_loop/mod.rs`): the `wl_callback::Done` handler now unconditionally re-arms the next `wl_surface.frame()` and drives an auto-loop, modelled on zed's `gpui_linux/src/linux/wayland/window.rs:572-587`. Compositors stop delivering Done for occluded windows so the loop pauses naturally without an explicit visibility check. **X11** (`platform_impl/linux/x11/mod.rs`): a new `calloop::timer::Timer` source reschedules at the primary monitor's xrandr-derived refresh rate (`monitor::mode_refresh_rate_millihertz`, fallback 60 Hz); each tick sets `EventLoopState.vsync_pending` and the main pump fans the flag out to every visible window via the existing `redraw_sender` channel. Mirrors zed's `gpui_linux/src/linux/x11/client.rs:1934-1971`. Both paths gate the synthetic `RedrawRequested` on a new `should_present_after_input()` (1 s window after the last input event), with the same field/method names as the existing macOS implementation in `platform_impl/macos/window_delegate.rs:139,997`. `mark_input_received` is hooked into the Wayland `KeyboardHandler` and `PointerHandler` and into the X11 KeyPress/KeyRelease + XInput2 ButtonPress/Release/Motion dispatch sites in `event_processor.rs`. Net effect: ProMotion / 144 Hz / 240 Hz monitors now stay at peak refresh during typing/scroll on Wayland and X11; idle terminals don't burn extra CPU because the gate falls back to the existing dirty-driven path outside the 1 s window.
- Full kitty graphics protocol Unicode-placeholder support (`kitten icat --unicode-placeholder` and any tmux/yazi/etc. that uses U+10EEEE placement cells). Wire-protocol bugs fixed: (1) the APC parser stored `U=1` into a private `cmd.virtual_placement` field that was never propagated to `PlacementRequest`, so `place_graphic` always fell through to the direct-overlay branch; (2) `place_virtual_graphic` was auto-writing U+10EEEE cells to the grid, racing kitty's own writes (per the spec the application emits the cells, the terminal only stores metadata); (3) icat uses `a=T,U=1` (combined transmit-and-display) which goes through `kitty_transmit_and_display`, not `place_graphic`; that handler now also routes virtual placements correctly and pushes pixel data to `pending_images` (the regular overlay path did this implicitly via `place_kitty_overlay`); (4) the diacritics table had 299 entries instead of the canonical 297 (two stray entries `\u{06EA}` and `\u{06ED}` from earlier work), shifting every index past the divergence point so `image_id_high` decoded as the wrong byte (e.g. `0x5C` stored, `0x5E` decoded). Replaced with the 297 entries from `kitty/gen/rowcolumn-diacritics.txt`. Shader bug fixed: `image.metal` interpreted `source_rect.zw` as **size** but the new code passes it as end-coords; switched to `mix(.xy, .zw, corner)` (only worked before because every overlay used the full-image default `[0,0,1,1]`). Renderer side, modeled cell-by-cell on ghostty's `graphics_unicode.zig`: new `Row.kitty_virtual_placeholder` per-row dirty flag set in `Crosswords::input` and checked by the scan loop, so we only walk rows that contain a placeholder (`page.zig:1953-1958`); new `IncompletePlacement` + `can_append` / `append` / `complete` implements kitty's diacritic continuation rules: a cell with missing row/col/high-byte diacritics inherits from the previous cell on the row and the col field can be omitted to mean "auto-increment from prev" (`graphics_unicode.zig:407-535`); the renderer now walks each visible row left-to-right, builds runs of consecutive cells that belong to the same `(image_id, placement_id, image_row)` with sequential image columns, and pushes ONE `GraphicOverlay` per run instead of one per placement (matches ghostty's `PlacementIterator`); per-run aspect-preserving fit, centering, and source-rect clipping factored into `kitty_virtual::compute_run_geometry`, which handles partial visibility (placement scrolled half off-screen renders only the visible image slice), runs that fall entirely in the centering padding (returns `None`), and runs that straddle the padding boundary (clips both the screen rect and the source rect so the rendered slice exactly covers the fitted-image area). Placeholder cells render with `style.background_color = None` so the per-cell bg quad doesn't cover the BelowText image. `GraphicOverlay` gained a `source_rect: [f32; 4]` field (default `[0,0,1,1]` via `FULL_SOURCE_RECT`) wired through to the existing `ImageInstance.source_rect`. Tests cover: the 297-entry diacritic table, `IncompletePlacement::from_cell` across all input shapes (Indexed fg, Spec fg + 3rd diacritic, underline=placement_id, missing diacritics, Named fg → 0), all `can_append` cases (row inherit, col inherit, sequential col, col jump, row mismatch, image-id mismatch, image-id-high inherit), a 3-cell run with only the first cell carrying diacritics, parser-level `U=1` propagation, the metadata-only contract of `place_virtual_graphic`, an end-to-end pass that feeds icat's exact wire format and asserts both the resulting grid cells AND the per-row dirty flag, and `compute_run_geometry` across exact-fit / image-taller-than-grid (horizontal centering + left-padding cull) / image-wider-than-grid (vertical centering + top-row cull) / partial-visibility-scrolled-off-top / origin-offset / zero-sized-image cases.
- macOS Metal renderer: triple-buffered pipeline modeled on zed's `gpui_macos::InstanceBufferPool`. `CAMetalLayer.maximumDrawableCount` is now 3 (was 2). The six per-frame buffers (text vertices, quad instances, kitty/sixel image instances, bg-image instance, bg-fill instance, Globals uniform) collapsed into a single pooled `metal::Buffer` per frame: text/quad/image data bump-allocates from the pool buffer with 256-byte aligned offsets, and `Globals` (transform + `input_colorspace`, ~80 B) goes through `set_vertex_bytes` / `set_fragment_bytes` so no buffer is needed for it at all. The pool starts at 2 MiB, doubles on overflow up to a 256 MiB cap (matches zed). On overflow we end-encoding, drop the never-committed command buffer, grow the pool, and retry the frame from scratch; old smaller buffers in flight get rejected by `release` after `grow` and dropped naturally, no "all 3 slots must grow together" coordination. `command_buffer.add_completed_handler` releases the buffer back to the `Arc<Mutex<InstanceBufferPool>>` on the GPU completion thread. Net effect: CPU can stay up to 3 frames ahead of the GPU without racing the previously single `StorageModeShared` buffers (the old code was relying on luck: every frame shared one allocation), and ProMotion 120 Hz is now reachable without dropped frames during heavy scenes. `MetalRenderer::resize` is gone (no more uniform buffer to refresh on resize); `Renderer::render_metal` now owns command-buffer / encoder / drawable / commit, so `Sugarloaf::render_metal` is just a one-liner that hands `bg_color` and the `MetalContext` over.
- Fix Nerd Font glyphs from an `extras` family (e.g. `extras = [{family="JetBrainsMono Nerd Font Mono"}]`) overflowing into the next grid cell. `FontLibraryData::load` was loading every extras/symbol-map font with `is_emoji = true`, which made `font_cache::resolve_with` clamp every glyph from that font to `width = 2.0`; the terminal grid still budgeted one cell per PUA codepoint, so the layout advanced 2× per glyph and the next character painted on top of the right half. Extras and symbol-map fonts now load with `is_emoji = false`, and `FontData::from_data` / `from_slice` auto-promote to `is_emoji = true` when the SFNT carries a color table (`COLR`, `CBDT`, `CBLC`, or `sbix`): mirroring ghostty's `FT_HAS_COLOR()` / CoreText SBIX check, so a real emoji font in `extras` still gets the wide-cell / color-atlas treatment without needing a config flag.
- Removed the dedicated `fonts.emoji` config slot. Rio now relies entirely on `fonts.extras` plus the color-table auto-detection above: drop `extras = [{family="Apple Color Emoji"}]` (or any color family) and it's picked up as emoji; otherwise the bundled Twemoji continues to serve as the default color-emoji fallback. User `[fonts.emoji]` sections in existing configs are silently ignored instead of erroring.
- PUA / Nerd Font glyphs with an adjacent blank cell now fill the full 2-cell constraint crisply instead of rendering half-sized or blurry. `pua_constraint_width` in the rioterm renderer still picks 1 or 2 cells based on neighbour content; the compositor's fit pass (`sugarloaf/src/renderer/compositor.rs`) dropped the `.min(1.0)` cap so the glyph can scale up to the 2-cell slot: and to avoid stretching a bitmap atlas entry (which is blurry), `GlyphCacheSession::get_at_size(id, size)` now rasterizes the glyph fresh at `font_size × cells` when `cells > 1`. Cache key `(id, size)` already disambiguates the two rasterizations, so 1-cell and 2-cell lookups for the same codepoint coexist. Fixes JetBrainsMono NF Mono (patched to ~1-cell advance) showing as half-sized inside a 2-cell slot next to Cascadia NF whose glyphs render ~2 cells wide natively.
- Fix vertical positioning of constraint-fit PUA glyphs. The compositor was placing the scaled glyph at `baseline − entry.top × scale`, which drifts upward as the scale grows because the top-bearing scales faster than the descent-bearing. Switched to cell-centered placement (`topline + (line_height − sh) / 2.0`), matching ghostty's `.align_vertical = .center1` choice for `isSymbol(cp)`: symbols aren't baseline-anchored the way text characters are.
- macOS Metal renderer: wide-gamut pipeline with ghostty-compatible `native` alpha blending. `CAMetalLayer` uses plain `BGRA8Unorm` tagged with `kCGColorSpaceDisplayP3` (earlier iterations used `BGRA8Unorm_sRGB`, which makes Metal do linear-light blending: physically correct but visibly *brighter* on AA edges and translucent overlays than ghostty's and Terminal.app's default). The three pipeline color attachments moved back to `BGRA8Unorm` to match. Fragment shaders (`renderer.metal`, `image.metal`) now `srgb_to_linear → sRGB-to-P3 matrix → linear_to_srgb` before returning, emitting gamma-encoded DisplayP3 bytes that the drawable stores verbatim and the compositor displays directly. `MTLClearColor` goes through the same encode on the Rust side (`sugarloaf::prepare_output_rgb_f64`) so the first cleared pixel matches a shader-drawn quad of the same theme colour. Net effect: same saturation as before (P3 gamut preserved), same glyph weight as ghostty `alpha-blending = native` / Terminal.app (no more AA-edge brightening). The `metal` crate doesn't wrap `CAMetalLayer.colorspace` yet, so the setter goes through `msg_send!`; the `CGColorSpace` handle is `mem::forget`-ed to guarantee the pointer stays live for the layer's lifetime, and a post-set read-back logs a warning if the colorspace didn't take.
- **Breaking (macOS):** `[window] colorspace` default flipped from `display-p3` to `srgb`. The semantics also changed to match ghostty's `window-colorspace`: the setting now describes **how Rio interprets input color bytes** (hex values in config, ANSI direct-color sequences), not which colorspace the Metal surface uses. The Metal surface is always DisplayP3 regardless of this setting. With `srgb` (the new default), Rio applies a Bradford-adapted sRGB D65 → DisplayP3 D65 matrix (in linear light, after `srgb_to_linear`) so `#ff0000` renders as sRGB-standard red: matching ghostty and every other app. Users who preferred the previous more-saturated P3-interpreted look can pin it with `colorspace = "display-p3"`. The matrix runs in the fragment shader (`prepare_output_rgb` in `renderer.metal` / `image.metal`) driven by a new `input_colorspace: u8` field on the `Globals` uniform, and at the Rust boundary for the `MTLClearColor` (`sugarloaf::prepare_output_rgb_f64`) so the first HW-cleared pixel lands in the same colorspace as shader-emitted pixels. `MetalRenderer::new` takes the colorspace at construction and writes it into every uniform upload; the Metal encoder now binds the uniform buffer to fragment slot 1 (in addition to the existing vertex slot 1) so `fs_main` / `image_fs_main` can read the flag.
- New `OpenCommandPalette` key-binding action (default `Cmd+Shift+P` on macOS, `Ctrl+Shift+P` elsewhere), user-remappable via `opencommandpalette` in `[bindings]`. Replaces the hardcoded `Cmd/Ctrl+Shift+P` branch that used to sit in `screen::process_key_event` ahead of the binding system, so the palette shortcut now composes with mode-gating like every other action.
- New `List Fonts` command palette entry: selecting it keeps the palette open and swaps the list for the host system's font families (via `font-kit`'s `SystemSource`), fuzzy-filterable. Enter copies the selected family to the clipboard and closes the palette. Each font row shows a hand-drawn (rect-only) rounded copy icon on the right to advertise the action.
- Command palette scrollbar now reuses the terminal scrollbar's look and behaviour through shared primitives in `renderer::scrollbar` (`opacity_from_last_scroll`, `compute_thumb`, `draw_thumb`): 6 px wide, `[0.6, 0.6, 0.6, 0.5]` gray, no rounded corners, 2 s visibility + 300 ms fade after the last scroll event. Only appears when the user has actually scrolled and resets on palette close / query change.
- `Sugarloaf::font_family_names()` + `FontLibrary::family_names()` for enumerating the host font catalog.
- Rio `Attributes` re-exported at the sugarloaf crate root so downstream callers don't have to reach into `font_introspector`.

## 0.3.10 (2026-04-15)

- Fix tab-title overflow spilling into neighbour tabs / off-screen when the title is longer than the tab width (issue [#1508](https://github.com/raphamorim/rio/issues/1508)). The island was adding the full title as a text span, measuring it, and centering, with no truncation, so a wide `text_width` produced a negative x offset and the text bled left of the tab's origin. Added `Sugarloaf::char_advance(ch, attrs, font_size)` (and extended `ResolvedGlyph` with `advance_units` + `units_per_em`, populated once at resolve time alongside the existing fallback walk) so the tab bar can walk titles char-by-char and truncate with a trailing `…` to fit `tab_width - 2·TAB_PADDING_X`. Wired `TAB_PADDING_X` up and dropped its `#[allow(dead_code)]`.

## 0.3.9 (2026-04-15)

- Fix progress bar visually freezing at the left edge when a TUI heartbeats `OSC 9;4` faster than the indeterminate animation cycle (issue [#1509](https://github.com/raphamorim/rio/issues/1509)). The island had one `progress_last_update` field doing double duty as the animation phase reference *and* the stale-bar dismissal timestamp; every report yanked the animation back to t=0. Now split into `progress_started_at` (only reset on actual state transitions) and `progress_last_seen` (bumped every report, drives the 15 s timeout): same separation ghostty's GTK apprt gets for free from `GtkProgressBar.pulse()` plus a side `glib.timeoutAdd`.
- Fix emoji with VS16 variation selector (e.g. `🎟️`, `⚠️`) visually overflowing into the next grid cell. Rio's font shaper correctly uses cmap format 14 to pick the wide emoji glyph for `(base, U+FE0F)` clusters, but the grid still budgeted only one cell for the base, so the glyph painted over its neighbour: misaligning vim/tmux vertical split lines on any row containing a text-presentation emoji. Rio now promotes the cell to `Wide` + `Spacer` on VS16 (and narrows it back on VS15) for any sequence listed in Unicode's `emoji-variation-sequences.txt`, matching kitty and ghostty. New workspace crate `rio-grapheme-width` carries the table (forked from `wezterm-char-props`, MIT).
- Add `scrollback-history-limit` config option (default `10000`, set to `0` to disable scrollback). Fixes [#993](https://github.com/raphamorim/rio/issues/993).
- Harden Kitty graphics protocol: reject images exceeding 10000 px per axis or 400 MiB, add per-chunk base64 decoding so clients like `chafa --format=kitty` (which pads every chunk) merge correctly, time out stale chunked uploads after 10 s, combine `i=/I=/p=` response keys, validate `i=`/`I=` mutual exclusion, and require `i=` for `a=q` queries.
- Fix kitty image protocol whenever using transmission.
- Fix color picker not rendering correctly.
- Selection feel: bumped the click/drag side-of-cell threshold from 50% to 60% (matches ghostty), so selections no longer flip to the next cell as eagerly when the cursor crosses the midpoint.
- Default URL/path regex now matches local paths in addition to schemed URLs. Ported verbatim from ghostty's `src/config/url.zig`: rooted (`/abs`), explicitly-relative (`./x`, `../x`), home (`~/x`), env-var-rooted (`$VAR/x`), hidden-dir (`.config/x`), and bare relative paths with a dotted filename (`src/main.rs`). Lookbehinds reject mid-word starts (the `/bar` inside `foo/bar`) and trailing punctuation (`see https://example.com.` won't include the period).
- Switch hint regex matching from the Rust `regex` crate to `onig` (Oniguruma): same engine ghostty uses. Adds lookbehind/lookahead support so the default regex matches ghostty's behavior exactly, and user `[hints.rules]` regexes can now use lookarounds and backreferences.
- When a hint match looks like a local path, rio now resolves it against the terminal's OSC 7 working directory, expands `~/` and `$VAR/`, and only dispatches the resolved absolute path to the OS opener if the file exists; URL schemes (`https://…`, `mailto:`, …) are passed through unchanged.
- Restore OSC 8 hyperlink underlines. The cell repack moved hyperlink data into a side table and the renderer's `square.hyperlink().is_some()` check was disabled with a `// temporarily disabled` comment that was never lifted; the renderer now reads the cell's `HYPERLINK` flag directly so OSC 8 spans are underlined again.
- Fix hover hint underline not appearing on the very first hover after startup. `update_highlighted_hints` now marks the renderer's `pending_update` with full terminal damage when the hint changes, mirroring the pre-refactor `set_hyperlink_range` behaviour, so the partial-damage render path can no longer skip the re-shape.
- Fix two `context::title` tests panicking under sandboxed builds (e.g. Void's xbps-src) where `$HOME=/tmp` collapsed the test path to `~`. The tests now use a path prefix that can't plausibly be `$HOME`.

## 0.3.8 (2026-04-13)

- Fix click between panels not working if it's on alternative screen.
- Fix divider not appearing (regression of 0.3.7).

## 0.3.7 (2026-04-13)

- Reduced GPU upload.
  - ~17k quads before: **~8.5 MB**
  - ~17k quads after: **~1.55 MB**
  - 5.5x reduction.

## 0.3.6 (2026-04-13)

- Performance improvements.
- Now a cell is 8 bytes.
```
// Bit layout for Square(u64)
//
//   bits  0..20 (21): codepoint (Unicode scalar value, max 0x10_FFFF)
//                     OR low bits of bg color when content_tag != Codepoint
//   bits 21..22 (2):  wide                              (Wide enum)
//   bits 23..29 (7):  per-cell flag bits (CellFlags), incl WRAPLINE at bit 0
//   bits 30..31 (2):  content_tag (NEW)
//                       0 = Codepoint        (text cell, use style_id below)
//                       1 = BgPalette        (bg-only cell, palette index in 32..39)
//                       2 = BgRgb            (bg-only cell, RGB packed in 32..55)
//                       3 = reserved
//   bits 32..47 (16): style_id      (when tag == Codepoint)
//                     bg palette idx in low 8 (when tag == BgPalette)
//                     bg RGB.r:g    in low 16  (when tag == BgRgb)
//   bits 48..63 (16): extras_id     (when tag == Codepoint)
//                     bg RGB.b      in low 8   (when tag == BgRgb)
```
- Add support for `window.columns` and `window.rows` for window sizing. Ignores invalid `0` values with fallback to `window.width`/`window.height`, and guarantees a minimum startup size of `300x200` logical pixels.
- Avoid send damage events to Application when terminal already has damage found and unprocessed.
- Fix macOS window close button (red semaphore) showing quit confirmation dialog. Clicking the close button now always closes the window. Quit confirmation is only triggered by Cmd+Q.
- Remove `window.macos-use-quit-dialog` configuration option.

## 0.3.5 (2026-04-12)

- Fix underline cursor rendering.
- Fix underline cursor disappearing on blank cells when `window.background-image` is set or `window.opacity < 1.0`. The empty-run dispatch in the renderer now also paints decoration-only spans.
- Fix `navigation.unfocused-split-opacity` not dimming unfocused splits. The setting is now applied as a semi-transparent overlay drawn on top of inactive panes and works correctly on both opaque and transparent windows. Default raised from `0.4` to `0.7` and clamped to `[0.15, 1.0]`.
- Add `navigation.unfocused-split-fill`: RGB tint used for the unfocused-split overlay. Defaults to the terminal background color.
- Re-implement `window.background-image`. It had been a silent no-op since the renderer rewrite: setting it would clear the window to transparent and paint nothing on top. The image is now uploaded into a dedicated GPU texture sized exactly to the source dimensions (no glyph-atlas pollution, no atlas-size limit) and stretched to cover the full window.
- Add `window.background-image.opacity`: `0.0`–`1.0` multiplier applied to the image's alpha channel. Defaults to `1.0`.
- **Breaking:** removed `window.background-image.width`, `height`, `x`, and `y`. They had no effect under the old implementation and the new dedicated pipeline always fills the window.
- Surface background-image load failures (missing path, decode error) through the in-window assistant warning popup instead of a silent log line.
- Fix FreeBSD build.

## 0.3.4 (2026-04-11)

- Fix kitty image protocol overlay when has a new tab.
- Fix kitty image protocol placeholder content being rendered.

## 0.3.3 (2026-04-10)

- CPU rendering support by `renderer.use-cpu`.
- Removed Font char cache friction.
- Fix kitty image protocol and sixel.

## 0.3.2 (2026-04-08)
- Performance: Skip shaping for empty cells, perf gain can be over 50% depending of the visible rows.
- Private user area should adjust based on cells.
- Cells now by default are created with \0.
- Selection ignore \0 cells.

## 0.3.1 (2026-04-03)

- Fix crash when filters option is on.

## 0.3.0 (2026-04-03)

- **Quit Confirmation Dialog**: New in-window quit screen with Rio logo, triggered by Cmd+Q / confirm-before-quit
  - Respond with `y` to quit or `n` to cancel
  - Optional native macOS quit dialog via `window.macos-use-quit-dialog` config
- **Kitty Graphics Protocol**: Display images directly in your terminal
  - Direct placements (U=0)
  - Virtual placements (U=1)
  - Diacritic-based row/column encoding (283 combining characters)
  - RGB color encoding for image/placement IDs (24+8 bit support)
  - Virtual placement rendering (infrastructure complete, rendering pending)
- **Sixel Graphics**: Full support with proper scrolling and positioning
- **Graphics Rendering Improvements**:
  - Fixed vertical positioning alignment
  - Fixed scrolling (images persist when origin scrolls off-screen)
  - Fixed duplicate rendering with per-frame deduplication
  - LRU cache with automatic eviction
- **Native Metal Support** (macOS): Hardware-accelerated rendering with Metal
- **New GPU-Rendered Navigation**: Faster, smoother tab interface
- **Command Palette**: Quick access to terminal functions
- **Toggle Appearance Theme**: Switch between dark and light themes at runtime via key binding (`ToggleAppearanceTheme`) or command palette (only available when adaptive theme is configured)
- **Custom Mouse Cursor**: Configurable mouse cursor effects via `effects.custom-mouse-cursor`
- **Trail Cursor**: Smooth spring-animated cursor trail using neovide-style physics (enabled by default via `effects.trail-cursor`)
- **Desktop Notifications**: Support for OSC 9 (iTerm2) and OSC 777 (rxvt) terminal notifications using native platform APIs (macOS `UNUserNotificationCenter`, Linux D-Bus, Windows Toast)
- **Force Theme**: New `force-theme` configuration property to override the system theme when using adaptive themes
- **Quake Window Mode**: Drop-down terminal from top of screen
- **macOS Traffic Light Positioning**: Customize position of window control buttons
  - Configure via `macos-traffic-light-position-x` and `macos-traffic-light-position-y`
  - Defaults to standard macOS positioning (11.4, 16.1)
  - Not available in Tab navigation mode
- **OSC 9;4 Progress Bar Support**: Terminal progress indicator (ConEmu/Windows Terminal compatible)
- **Scroll Bar**: Overlay scroll bar that appears on scroll and fades out after 2s
  - Works per-panel in split views
  - Draggable thumb with click-on-track jump scrolling
  - Configure via `enable-scroll-bar` (enabled by default)
- **Tab Title `RELATIVE_PATH` variable**: New template variable that shows a home-relative shortened path (e.g. `~/Documents/a/rio` or `…/a/psone/starpsx`). Default tab title on macOS/Linux changed to `{{ TITLE || RELATIVE_PATH }}`.
- Wgpu now is always f32.
  - This fixes non arm chip macos use cases.

**Breaking Changes**

- Navigation modes simplified - if you use `TopTab`, `BottomTab`, or `Bookmark`, change to:
  ```toml
  [navigation]
  mode = "Tab"
  ```
- Default `Decorations` changed to `Transparent` on macOS (was `Enabled`)
- Removed: `TopTab`, `BottomTab`, and `Bookmark` navigation modes
- Available modes: `Plain`, `Tab`, `NativeTab` (macOS only)
- **Tab color configuration simplified**: Removed `tabs-foreground`, `tabs-active-foreground`, and `tabs-active-highlight`
  - Use `tabs` for inactive tab text and border color (default: `#cccccc`)
  - Use `tabs-active` for active tab text color (default: `#ffffff`)
- The old padding api became margin:
  ```toml
  # It will apply margin rules to the main container
  # CSS-Like
  margin = [10] # (10px to all)
  margin = [10, 5] # (top and bottom margin are 10px, right and left margin are 5px)
  margin = [10, 5, 15, 20] # (top margin is 10px, right margin is 5px, bottom margin is 15px, left margin is 20px)

  # It will apply margin rules to panels
  [panel]
  margin = [5] # (5px to all)
  row-gap = 0 # (0px)
  column-gap = 0 # (0px)
  ```

**Technical Details**

- Complete rendering architecture rewrite for GPU-based UI
- Parser now supports APC sequences for Kitty graphics protocol
- Removed legacy layer/quad rendering system
- Added Metal backend for macOS, split WebGPU backend for cross-platform
- New `kitty_virtual` module for placeholder encoding
- Graphics cleanup with LRU eviction strategy (evicts up to 5 oldest when atlas full)
- Added 5 unit tests for graphics rendering (positioning, LRU, deduplication)

## 0.2.38 (never tagged, released as 0.3.0)

- Update wgpu to v0.28.
- Update Rust to v1.92.

## 0.2.37 (2025-12-19)

- Support window bg color via OSC.
- Fix vi cursor not displayed when moving.
- Fix font loader for fallbacks and extra.
- Fix font size updating through config.

## 0.2.36 (2025-11-18)

- Fix handler should process two intermediate bytes in CSI sequences by [@aymanbagabas](https://github.com/aymanbagabas).
- Fix DECSCUSR.

## 0.2.35 (2025-11-03)

- GPU memory usage drop 83%.
- Sync input render logic (macos).

## 0.2.34 (2025-11-03)

- Fix issue for finding fonts introduced with the v0.2.33 new font loader.

## 0.2.33 (2025-10-30)

- **Platform-specific configuration improvements** [#1341](https://github.com/raphamorim/rio/issues/1341):
  - Added support for platform-specific environment variables via `env-vars` field in platform config
  - Platform-specific env-vars are now appended to global env-vars instead of replacing them
  - Fixed configuration inheritance: platform overrides now use field-level merging instead of replacing entire sections
  - Window, Navigation, and Renderer settings can now be partially overridden per platform without duplicating all fields
  - Added `theme` field to platform config for per-platform theme selection
  - Shell configuration continues to use complete replacement for simplicity
- Fix `ScrollPageUp` and `ScrollPageDown` actions not working in custom keybindings [#1275](https://github.com/raphamorim/rio/issues/1275).
- Fix Noticeably slower startup compared to wezterm, foot [#1346](https://github.com/raphamorim/rio/issues/1346).
- Fix Font loader taking a LOT of time to load fonts [#1339](https://github.com/raphamorim/rio/issues/1339).
- Fix Rio panics on launch on a Raspberry Pi 5 [#1332](https://github.com/raphamorim/rio/issues/1332).
- Fix kitty keyboard protocol.
- Support reporting terminal version via XTVERSION.

## 0.2.32 (2025-10-16)

- Updated WGPU to v27.0.1.
- Fix No backend are enabled on FreeBSD #1235.

## 0.2.31 (2025-09-27)

- Update Rust to v1.90.
- Fix kitty keyboard recognition.
- **Breaking: Simplified key binding escape sequences**
  - Replaced separate `text` and `bytes` fields with a single `esc` field
  - Escape sequences are now sent directly to the PTY without text manipulation
  - Migration: Replace `bytes = [27, 91, 72]` with `esc = "\u001b[H"`
  - Migration: Replace `text = "some text"` with `esc = "some text"`
  - Example: `{ key = "l", with = "control", esc = "\u001b[2J\u001b[H" }` to clear screen
- **Fix key binding conflicts**: Resolved issues where keys like `PageUp`, `PageDown`, and `Alt+Enter` required explicit `"None"` bindings before they could be reassigned
  - Simplified binding conflict resolution logic to automatically remove conflicting default bindings
  - User-defined bindings now always take precedence without requiring placeholder "None" entries

## 0.2.30 (2025-09-13)

- **Fix Debian/Ubuntu package installation**: Resolved terminfo conflicts with system packages [#1264](https://github.com/raphamorim/rio/issues/1264)
  - Debian (.deb) packages no longer include terminfo files to avoid conflicts with ncurses-term
  - Users on Ubuntu 22.04 and older need to manually install terminfo after package installation
  - Debian 13+ and Ubuntu 24.04+ users get terminfo from system's ncurses-term package
  - RPM packages continue to include terminfo as before
- Add audible & visual bell support [#1284](https://github.com/raphamorim/rio/pull/1284).

## 0.2.29 (2025-08-17)

- Fix blinking cursor issue [#1269](https://github.com/raphamorim/rio/issues/1269).
- Fix Rio uses UNC (\?\) path as working directory, breaking Neovim subprocesses on Windows.
- Add NSCameraUseContinuityCameraDeviceType to plist for macOS.

## 0.2.28 (2025-08-10)

- **Optimized rendering pipeline for improved performance**: Implemented deferred damage checking and render coalescing
  - Added Wakeup events to batch multiple rapid terminal updates into single render passes
  - Deferred damage calculation until render time to reduce unnecessary computations
  - Skip rendering for unfocused windows when `disable_unfocused_render` is enabled
  - Skip rendering for occluded windows when `disable_occluded_render` is enabled
  - Improved damage merging to always accumulate updates even when already marked dirty
  - Enhanced performance for rapid terminal output by coalescing non-synchronized updates

## 0.2.27 (2025-08-07)

- Breaking: If `xterm-rio` is installed we prioritized it over `rio` terminfo.
- **Fix sixel/iterm2 graphics persistence issue**: Fixed graphics remaining visible when overwritten by text
  - Graphics are now properly removed when cells containing them are overwritten
  - Fixes issues with file managers like Yazi where images would persist incorrectly
  - Simplified graphics cleanup logic by removing unused ClearSubregion functionality
- **CJK Font Metrics**: Fixed CJK characters displaying "higher" than Latin characters [#1071](https://github.com/raphamorim/rio/issues/1071)
  - Implemented comprehensive CJK font metrics handling with consistent baseline adjustment
  - Fixed scrolling issues for mixed Latin and CJK text content
  - Added CJK character width measurement using "水" (water ideograph) as reference
  - Created consistent cell dimensions across different font types
  - Developed extensive test suite with 40+ font-related tests to verify fixes

## 0.2.26 (2025-08-04)

- **Fix frame dropping in release builds**: Fixed an issue where release builds would drop frames due to damage event timing
  - Damage events are now emitted directly after parsing PTY data, ensuring proper batching
  - Removed redundant Wakeup event mechanism that was causing multiple renders per update
  - Synchronized update timeouts now properly emit damage events
  - Significantly improves rendering smoothness in optimized builds

## 0.2.25 (2025-08-03)

- Fix: Rio doesn't launch from context menu on Windows.
- Fix: Rio lacks embedded icon on Windows 10 by [@christianjann](https://github.com/christianjann).
- **Fix custom shells in /usr/local/bin not found on macOS**: Fixed an issue where custom shells installed in `/usr/local/bin` were not found when Rio was launched from Finder or other GUI applications
  - On macOS, Rio now uses `/usr/bin/login` to spawn shells, ensuring proper login shell environment with full PATH
  - Custom shells like Fish, Nushell, or custom Zsh installations in `/usr/local/bin` will now work correctly

## 0.2.24 (2025-08-01)

- Fix game mode regression.
- **Hint Label Damage Tracking**: Improved hint label rendering performance with proper damage tracking
  - Hint label areas are now properly marked for re-rendering when cleared
  - Eliminates visual artifacts when hint labels are removed
  - Optimized rendering to only update affected screen regions
- **Configurable Hyperlink Hover Keys**: Hyperlink hover modifier keys are now configurable
  - Configure custom modifier keys through the hints system in `config.toml`
  - Default behavior unchanged: Command on macOS, Alt on other platforms
  - Supports any combination of Shift, Control, Alt, and Super/Command keys
  - Example: `mouse = { enabled = true, mods = ["Shift"] }` to use Shift key
- **Hints Configuration**: Renamed `hints.enabled` to `hints.rules` for better clarity
  - Update your configuration: `[[hints.enabled]]` → `[[hints.rules]]`
  - All hint configuration sections now use `hints.rules.*` instead of `hints.enabled.*`
  - Functionality remains the same, only the configuration key names changed

## 0.2.23 (2025-07-23)

- Fix some rendering regressions introduced by 0.2.21.
- Improve performance by stopping locking on rendering run steps.
- Fix: [X11: WM_CLASS has an empty string property](https://github.com/raphamorim/rio/issues/1155).

## 0.2.22 (2025-07-18)

- Fix some regressions introduced by 0.2.21.

## 0.2.21 (2025-07-16)

- Breaking: `navigation.use-current-directory` has been renamed to `navigation.current-working-directory`.

### Performance Optimizations

- **Major**: Implemented efficient CVDisplayLink-based VSync synchronization for macOS
  - Perfect frame timing aligned with display hardware refresh cycles
  - Eliminates screen tearing and stuttering through hardware VSync synchronization
  - Adaptive refresh rate support: automatically handles 60Hz, 120Hz, ProMotion displays
  - Multi-display support: adapts when windows move between displays with different refresh rates
  - Grand Central Dispatch (GCD) integration for thread-safe cross-thread communication
  - **Smart rendering**: Only renders when content actually changes using dirty flag system
  - Power efficient: skips unnecessary redraws when content is static, reducing CPU usage
  - Professional rendering quality with smooth, tear-free visual updates
  - CVDisplayLink runs on dedicated background thread, never blocking UI operations
- **macOS VSync Optimization**: Disabled redundant software-based vsync calculations on macOS
  - CVDisplayLink already provides hardware-synchronized VSync timing
  - Eliminates unnecessary frame timing calculations and monitor refresh rate queries
  - Reduces CPU overhead and improves rendering performance
  - Software vsync logic remains active on other platforms for compatibility
- **Major**: Implemented a new text run caching system replacing line-based caching
  - Up to 96% reduction in text shaping overhead for repeated content
  - Individual text runs (words, operators, keywords) cached and reused across frames
  - 256-bucket hash table with LRU eviction for optimal memory usage
- **Cache Warming**: Pre-populate cache with 100+ common terminal patterns on startup
  - Programming keywords: `const`, `let`, `function`, `class`, `import`, `export`, etc.
  - Indentation patterns: 4/8/12/16 spaces, single/double/triple tabs
  - Shell commands: `ls`, `cd`, `git`, `npm`, `cargo`, `sudo`, etc.
  - Operators & punctuation: `=`, `==`, `=>`, `();`, `{}`, `[]`, etc.
  - File extensions: `.js`, `.ts`, `.rs`, `.py`, `.json`, `.md`, etc.
  - Error/log patterns: `Error:`, `[INFO]`, `FAILED`, `SUCCESS`, etc.
  - Immediate cache hits eliminate cold start shaping delays
- **SIMD-Optimized Whitespace Detection**: Multi-tier optimization for indentation processing
  - AVX2: 32 bytes per instruction (x86-64 with AVX2 support)
  - SSE2: 16 bytes per instruction (x86-64 with SSE2 support)
  - NEON: 16 bytes per instruction (ARM64/aarch64)
  - Optimized scalar: 8-byte chunks (universal fallback)
  - Up to 32x performance improvement for long indentation sequences
  - Critical for Python, nested JavaScript/TypeScript, YAML, and heavily indented code
- **Memory Pool for Vertices**: High-performance vertex buffer pooling system
  - Size-categorized pools: Small (64), Medium (256), Large (1024), XLarge (4096) vertices
  - Zero allocation overhead through buffer reuse across frames
  - LRU management with automatic cleanup when pools reach capacity
  - Thread-safe concurrent access with performance monitoring
  - Eliminates GC pressure and improves frame rate consistency
- **Background Font Operations**: Non-blocking font management
  - Font data release and cleanup in dedicated background thread
  - System font scanning and preloading without blocking main thread
  - Prevents frame rate drops during font operations
- **Occlusion-Based Rendering**: Skip rendering for occluded windows/tabs
  - Automatically detects when windows are completely hidden by other windows
  - Skips rendering for occluded windows to save GPU resources and improve performance
  - Renders one frame when window becomes visible again to ensure display is updated
  - Configurable via `[renderer] disable-occluded-render = true` (enabled by default)
  - Significantly improves performance when running multiple tabs or windows

### Other Improvements

- Optimize the character cluster cache for wide space characters.
- New font atlas, more efficient.
- Implemented around 75% Memory Reduction: Text glyphs now use R8 (1 byte) instead of RGBA (4 bytes).
- **Hint Label Damage Tracking**: Improved hint label rendering performance with proper damage tracking
  - Hint label areas are now properly marked for re-rendering when cleared
  - Eliminates visual artifacts when hint labels are removed
  - Optimized rendering to only update affected screen regions
- **IME Cursor Positioning**: Added configurable IME cursor positioning based on terminal cell coordinates
  - IME input popups now appear precisely at the cursor position
  - Improves input experience for CJK languages (Chinese, Japanese, Korean)
  - Configurable via `[keyboard] ime-cursor-positioning = true` (enabled by default)
- **Shift+Click Selection**: Added Shift+click support for expanding text selections
  - Shift+clicking now extends the current selection to the clicked cell
  - Provides standard terminal selection behavior expected by users
  - Regular clicking without Shift still clears selection and starts new one as before
- **CLI accepts relative paths for working directory CLI argument**: When invoking rio from other terminals using `rio --working-dir=<path>`, a relative path is now correctly processed

### Bug Fixes

- **Cursor Damage Tracking**: Fixed cursor rendering issues after `clear` command and during rapid typing
  - Replaced complex point-based damage tracking with simplified line-based approach
  - Eliminates edge cases where cursor updates were missed during fast typing sequences
  - Improved reliability by always damaging entire lines instead of tracking column ranges
  - Aligns with modern terminal design principles for more robust damage calculation
- **Selection Rendering**: Fixed selection highlight not appearing on first render
  - Selection changes now properly trigger damage tracking and rendering
  - Optimized selection damage to only redraw affected lines for better performance
  - Selection highlights now appear immediately when making selections
- **Text Selection**: Fixed selection behavior during input and paste operations
  - Selection properly clears when typing or pasting text (both bracketed and regular paste)
  - Selection coordinates remain stable during viewport scrolling
  - Prevents selection from being lost unexpectedly during normal terminal usage
- **Auto-scroll on Input**: Fixed issue where typing after scrolling up wouldn't automatically scroll to bottom
  - Now properly scrolls to bottom for both keyboard input and IME/paste operations
  - Ensures cursor remains visible when typing new content
- **Scroll Performance**: Improved scrolling performance by optimizing render event handling
  - Moved scroll display offset update before mouse cursor dirty event
  - Removed redundant render calls during scroll operations
  - Implemented centralized damage-based rendering in event loop for better performance
- **macOS IME Improvements**: Fixed emoji input and IME stability issues
  - Resolved `IMKCFRunLoopWakeUpReliable` errors when using emoji picker
  - Improved coordinate validation and error handling for IME positioning
  - Better handling of direct Unicode input (emoji picker, character viewer)
  - Added throttling to prevent excessive IME coordinate updates
- **Documentation**: Added comprehensive manual pages (man pages) for Unix-like systems
  - `man rio` - Main Rio terminal manual page with command-line options
  - `man 5 rio` - Complete configuration file format documentation
  - `man 5 rio-bindings` - Key bindings reference and customization guide
  - Available in `extra/man/` directory with build instructions
- **Terminfo Compatibility**: Improved terminal compatibility by adding `xterm-rio` terminfo entry
  - Added `xterm-rio` as primary terminfo entry with `rio` as alias for better application compatibility
  - Applications that look for "xterm-" prefixed terminals (like termwiz-based apps) now work correctly
  - Maintains `TERM=rio` environment variable for consistency with terminal identity
  - Fixes crashes with applications like `gitu` and other termwiz-based terminal programs
  - Follows same pattern as other modern terminals (Alacritty, Ghostty) for maximum compatibility

### Technical Details

The performance optimizations in this release represent a significant architectural improvement to Rio's text rendering pipeline:

- **Text Run Caching**: Replaces line-based caching with individual text run caching. Each unique text sequence (word, operator, keyword) is shaped once and reused across all occurrences.
- **SIMD Implementation**: Platform-adaptive SIMD instructions automatically detect and use the best available CPU features (AVX2 > SSE2 > NEON > optimized scalar) for maximum performance across different architectures.
- **Memory Management**: The vertex pool system uses size-categorized buffers with LRU eviction, eliminating allocation overhead while preventing memory bloat.
- **Cache Strategy**: Two-level caching (render data + text runs) with 256-bucket hash table using FxHasher for optimal lookup performance.
- **Compatibility**: All optimizations maintain full backward compatibility with existing Rio APIs and configurations.

These changes are particularly beneficial for:

- Programming workflows with repetitive code patterns
- Terminal sessions with heavy indentation (Python, nested JS/TS, YAML)
- Long-running sessions where cache warming provides sustained performance benefits
- Systems with limited memory where reduced allocation overhead improves overall responsiveness

### Bug Fixes

- **Backspace Key Compatibility**: Fixed backspace key not working properly in vim when `TERM=xterm-256color`
  - Changed backspace key bindings to send BS (0x08) instead of DEL (0x7F)
  - Updated Rio terminfo and termcap entries to match actual key behavior
  - Updated XTGETTCAP response to return `^H` for `kbs` capability
  - Ensures compatibility with applications expecting xterm-256color backspace behavior
  - Fixes issue where vim would display `^?` instead of performing backspace operation

## 0.2.20 (2025-06-20)

- Performance: Implemented SIMD-accelerated UTF-8 validation throughout Rio terminal using the `simdutf8` crate.
  - Architecture support: AVX2/SSE4.2 (x86-64), NEON (ARM64), SIMD128 (WASM)
  - Automatic optimization: Runtime detection selects fastest implementation available
- Support for XTGETTCAP (XTerm Get Termcap) escape sequence for querying terminal capabilities.
- Font library is now under a RWLock instead of Mutex to allow multiple tabs readings same font data.
- Fix: crash on openSUSE Tumbleweed [#1160](https://github.com/raphamorim/rio/issues/1160).

## 0.2.19 (2025-06-17)

- Reduced the bundle size by ~20.81% (MacOS, Linux, BSD).
- Performance: stop saving empty images in the image cache.
- Fix: On MacOS, keybind definition to ignore cmd-w does not work [#879](https://github.com/raphamorim/rio/issues/879).
- Fix: Build for MacOS 26 Tahoe.
- Fix: `Enter`,`Tab`, `Backspace` not disambiguated with `shift` in kitty keyboard's disambiguate mode.
- Fix: line-height adds small gaps for box-drawing characters [#1126](https://github.com/raphamorim/rio/issues/1126).
- Search matching a wrapping fullwidth character in the last column.
- Update Rust to 1.87.0.

## 0.2.18 (2025-06-05)

- Fix image display crashing the application whenever f16 is available.

## 0.2.17 (2025-06-03)

- _Breaking:_ Decorations as `Enabled` is default on MacOS (instead of `Transparent`).
- F16 Texture supports whenever is available.
- Clear font atlas whenever the font is changed.
- Skip passing sandbox env in Flatpak, fixes user environment in spawned shell [#1116](https://github.com/raphamorim/rio/pull/1116) by [@ranisalt](https://github.com/ranisalt).
- On Windows, fixed crash in should_apps_use_dark_mode() for Windows versions < 17763.

## 0.2.16 (2025-05-15)

- _Breaking_: support reading from config directory using `$XDG_CONFIG_HOME` on Linux [#1105](https://github.com/raphamorim/rio/pull/1105) by [@ranisalt](https://github.com/ranisalt).
- Fix: Crash on whenever attempting to clean an invalid line index.
- Add metainfo and screenshots for appstream by [@ranisalt](https://github.com/ranisalt).

## 0.2.15 (2025-05-08)

- Fix: In some cases, the first typed character doesn't display until after a delay, or until another key is hit [#1098](https://github.com/raphamorim/rio/issues/1098).
- Fix: Anomalous behavior occurs with the Bookmark tab style in the new versions 0.14 and 0.13. [#1094](https://github.com/raphamorim/rio/issues/1094).

## 0.2.14 (2025-05-05)

- Fix: panic and crash of terminal window during sudo apt update [#1093](https://github.com/raphamorim/rio/issues/1093).

## 0.2.13 (2025-05-05)

- _Breaking change_: For Windows and Linux users, hyperlink trigger whenever hovering a link was changed from `alt` to `shift`.
- Fix dimension for whenever a new tab is created from a view with splits.
- Drop subtables with empty coverage by [@xorgy](https://github.com/xorgy).
- Fix font size affecting tabs size.
- Support to drawable characters by using `fonts.use-drawable-chars = true`.
- Fix: Wrong unicode character alignment [#616](https://github.com/raphamorim/rio/issues/616).
- Fix: Built-in font for box drawing #974 [#974](https://github.com/raphamorim/rio/issues/974).
- Fix: U+E0B6 and U+E0B4 Unicode with different sizes [#895](https://github.com/raphamorim/rio/issues/895).
- Update wgpu to v25.
- Fix: Custom rendering (alignment) of Braille symbols [#1057](https://github.com/raphamorim/rio/issues/1057).
- Fix: Drawing char ⡿ in column 1 causes the entire terminal to stutter [#1033](https://github.com/raphamorim/rio/issues/1033).
- Fix: Some glyphs (e.g. braille symbol) are rendered with gaps in between [#930](https://github.com/raphamorim/rio/issues/930).
- Introduce `fonts.disable-warnings-not-found` to disable warning regarding fonts not found.
- Fix: Request: silently ignore missing fonts from fonts.family and fonts.family.extras [#1031](https://github.com/raphamorim/rio/issues/1031).
- Fix: Add branch drawing symbols to box characters [#761](https://github.com/raphamorim/rio/issues/761).
- Fix: macOS: fallback for missing font glyph? [#913](https://github.com/raphamorim/rio/issues/913).
- Fix: FPS calculation, before it was rendering avg 48 on 60fps screen, however it was due to wrong frame scheduling computations, now it's up to 56-58.
- Fix: Shift+Tab event is doubled, as if hit twice [#1061](https://github.com/raphamorim/rio/issues/1061).
- Fix: Request: Option to change click-link modifier key [#1059](https://github.com/raphamorim/rio/issues/1059).
- Fix: Unexpected tmux previous-window [#1062](https://github.com/raphamorim/rio/issues/1062).
- Rewrite the way Rio deals with line diff and updates computation.
- Support for setting a custom config directory using `$RIO_CONFIG_HOME`
- Support for additional font dirs using `fonts.additional-dirs`
- Rio's MSRV is 1.85.0.
- Support to Sextants.
- Fix: Octant support [#814](https://github.com/raphamorim/rio/issues/814).
- Fix: Issue regarding split not updating opacity style when getting unfocused.
- Add support for custom parsing of APC, SOS and PM sequences.

## 0.2.12 (2025-03-28)

- Fix crash regarding fonts not found whenever trying to run Rio.

## 0.2.11 (2025-03-27)

- Fix filter scanlines not appearing.
- rt(wgpu): clamp texture size to device limits by [@chyyran](https://github.com/chyyran).
- Support to builtin filters: `newpixiecrt` and `fubax_vr`.
- Fix dimension computation whenever resizing Rio.
- Removed `fonts.ui` property, now Rio will always use primary font for UI.
- Removed Text renderer mod by migrating to RichText renderer.
- _Breaking:_ `renderer.strategy = "Continuous"` was renamed to `renderer.strategy = "Game"`
- Fix search bar can't show chinese [#844](https://github.com/raphamorim/rio/issues/844).

## 0.2.10 (2025-03-13)

- Fix computation of lines on screen.
- Fix dimension of the first tab whenever TopTab or BottomTab is created.
- Fix flaky test issue, test_update_title_with_logical_or failing randomly on aarch64 [#994](https://github.com/raphamorim/rio/issues/994).
- Support to `navigation.unfocused_split_opacity`, default is `0.5`.
- Sugarloaf: Fix foreground color opacity not being computed.

## 0.2.9 (2025-03-10)

- Support to symbol map configuration: `fonts.symbol-map`:

```toml
# covers: '⊗','⊘','⊙'
fonts.symbol-map = [{ start = "2297", end = "2299", font-family = "Cascadia Code NF" }]
```

- Add Switch to Next/Prev Split or Tab command by [@vlabo](https://github.com/vlabo).
- Fix issue whenever the first main font cannot be found.

## 0.2.8 (2025-02-26)

- Support to `.rpm` files! (thanks [@vedantmgoyal9](https://github.com/vedantmgoyal9) and [@caarlos0](https://github.com/caarlos0))
- OSC 7 Escape sequences to advise the terminal of the working directory.
- Use [GoReleaser](https://goreleaser.com) to build & release Rio ([#921](https://github.com/raphamorim/rio/pull/921)), thanks [@caarlos0](https://github.com/caarlos0) and [@vedantmgoyal9](https://github.com/vedantmgoyal9)
- Cache GSUB and GPOS features independently.
- Updated `windows-sys` to `v0.59`.
  - To match the corresponding changes in `windows-sys`, the `HWND`, `HMONITOR`, and `HMENU` types now alias to `*mut c_void` instead of `isize`.

## 0.2.7 (2025-02-11)

- Shifted key reported without a shift when using kitty keyboard protocol.
- fix: Set cursor color via ANSI escape sequence [#945](https://github.com/raphamorim/rio/issues/945).
- fix: Can the "base 16" colors be changed at runtime through Ansi escape sequences? [#188](https://github.com/raphamorim/rio/issues/188)
- fix: Changing release and nightly build Ubuntu runners for x86 (`ubuntu-latest` to `ubuntu-22.04`) and arm (`ubuntu-24.04-arm` to `ubuntu-22.04-arm`)

## 0.2.6 (2025-02-08)

- Fix: 0.2.5 doesn't render grey scale font on macOS [#937](https://github.com/raphamorim/rio/issues/937).
- fix: fix duplicate tab_id by monotonic counter for unique tab IDs by [@hilaolu](https://github.com/hilaolu).
- Add backslash to invalid characters for URL regex.
- fix regression introduced by 0.2.5 on light colors.
- fix: CMD+W open new tab but not new window occasionally [#756](https://github.com/raphamorim/rio/issues/756).
- fix: Error getting window dimensions on Wayland [#768](https://github.com/raphamorim/rio/issues/768).

## 0.2.5 (2025-01-30)

- Introduced `draw-bold-text-with-light-colors` config, default is `false`.
- If light or dark colors are not specified Rio will try to convert it based on the regular color.
- Fix: Block writing to the shell when rendering the `Assistant` route.
- Fix: Immediately render the `Terminal` route when switching from the `Assistant`, `ConfirmToQuit` or `Welcome`, thus avoiding the need to double press `Enter`.
- Fix: MacOS Unable to type Option + Number for special characters [#916](https://github.com/raphamorim/rio/issues/916).
- Fix: Looking forward to having a color converter [#850](https://github.com/raphamorim/rio/issues/850).
- Fix: Unexpected basic 16 terminal colors displayed on some apps [#464](https://github.com/raphamorim/rio/issues/464).

## 0.2.4 (2025-01-17)

- Breaking: Rio now doesn't allow anymore disable kitty keyboard protocol.
- Fullwidth semantic escape characters.
- Fix: report of Enter/Tab/Backspace in kitty keyboard.
- Fix: use-kitty-keyboard-protocol = true doesn't work with tmux [#599](https://github.com/raphamorim/rio/issues/599).
- Fix: use-kitty-keyboard-protocol breaks F[5-12] on macOS [#904](https://github.com/raphamorim/rio/issues/904).
- Downgrade MSRV to 1.80.1
- Update wgpu to 24.0.0.

## 0.2.3 (2025-01-13)

- Rio now allows you to configure window title through configuration via template. Possible options:
  - `TITLE`: terminal title via OSC sequences for setting terminal title
  - `PROGRAM`: (e.g `fish`, `zsh`, `bash`, `vim`, etc...)
  - `ABSOLUTE_PATH`: (e.g `/Users/rapha/Documents/a/rio`)
  - `COLUMNS`: current columns
  - `LINES`: current lines
    - So, for example if you have: `{{COLUMNS}}x{{LINES}}` would show something like `88x66`.
- Perf improvement on text selection [#898](https://github.com/raphamorim/rio/pull/898) by [@marc2332](https://github.com/marc2332).
- Window title is now updated regardless the Navigation Mode.
- Performance: Background and foreground data are only retrieved if is asked (either color automation is enabled or `window.title` contains any request for it).
- Fixed: Nix build [#853](https://github.com/raphamorim/rio/pull/853).
- Support to `window.macos-use-shadow` (enable or disable shadow on MacOS).
- Support to `window.windows-corner-preference` (options: `Default`, `DoNotRound`,`Round` and `RoundSmall`).
- Support to `window.windows-use-undecorated-shadow` (default is enabled).
- Support to `window.windows-use-no-redirection-bitmap` (This sets `WS_EX_NOREDIRECTIONBITMAP`).
- Minimal stable rust version 1.84.0.
- Support for Unicode 16 characters.
- Support to line height.
- Renamed `--title` to `--title-placeholder` on CLI.
- Fixed: Deb package name 'rio' conflicts with existing one in Ubuntu [#876](https://github.com/raphamorim/rio/issues/876).
- Fixed: Unremovable bottom padding when using line-height [#449](https://github.com/raphamorim/rio/issues/449).
- On macOS, fixed undocumented cursors (e.g. zoom, resize, help) always appearing to be invalid and falling back to the default cursor.
- Introduce `SwitchCurrentTabToPrev` and `SwitchCurrentTabToNext` actions [#854](https://github.com/raphamorim/rio/pull/854/files) by [@agjini](https://github.com/agjini).
- On X11, Wayland, Windows and macOS, improved scancode conversions for more obscure key codes.
  - On macOS, fixed the scancode conversion for audio volume keys.
  - On macOS, fixed the scancode conversion for `IntlBackslash`.
- Kitty keyboard protocol is now enabled by default.
- Allow `Renderer` to be configured cross-platform by `Platform` property.
- Add `ToggleFullscreen` to configurable actions.
- Escape sequence to move cursor forward tabs ( CSI Ps I ).
- Always emit `1` for the first parameter when having modifiers in kitty keyboard protocol.
- Microsoft Windows: fix the event loop not waking on accessibility requests.
- Wayland: disable title text drawn with crossfont crate, use ab_glyph crate instead.
- Sugarloaf: Expose wgpu.

## 0.2.2 (2024-11-23)

- Fix iterm2 image protocol.
- Allow setting initial window title [#806](https://github.com/raphamorim/rio/pull/806) by [@xsadia](https://github.com/xsadia).
- Fix runtime error after changing to a specific retroarch shader on windows [#788](https://github.com/raphamorim/rio/issues/788) by [@chyyran](https://github.com/chyyran).
- Makes editor.args and shell.args optional in config.toml [#801](https://github.com/raphamorim/rio/pull/803) by [@Nylme](https://github.com/Nylme).
- Introduce `navigation.open-config-with-split`.

## 0.2.1 (2024-11-16)

- Fix: Search seems broken in 0.2.0 [#785](https://github.com/raphamorim/rio/issues/785).
- Regular font is now 400 as default weight.
- Support to chooseing font width [#507](https://github.com/raphamorim/rio/issues/507).
- Support to multiconfiguration. Rio now allows you to have different configurations per OS, you can write ovewrite `Shell`, `Navigation` and `Window`.

Example:

```toml
[shell]
# default (in this case will be used only on MacOS)
program = "/bin/fish"
args = ["--login"]

[platform]
# Microsoft Windows overwrite
windows.shell.program = "pwsh"
windows.shell.args = ["-l"]

# Linux overwrite
linux.shell.program = "tmux"
linux.shell.args = ["new-session", "-c", "/var/www"]
```

- Fix: Grey triangle in the titlebar [#778](https://github.com/raphamorim/rio/issues/778)
- Update window title straight away ([#779](https://github.com/raphamorim/rio/pull/779) by [@hunger](https://github.com/hunger))
- Always update the title on windows and MacOS ([#780](https://github.com/raphamorim/rio/pull/780) by [@hunger](https://github.com/hunger))

## 0.2.0 (2024-11-13)

- Note: The migration from 0.1.x to v0.2.x changed considerably the renderer source code, although it was tested for 3 weeks it's entirely possible that introduced bugs (hopefully not!).
- Performance gains!
  - Sugarloaf: Major rewrite of font glyph logic.
  - Sugarloaf: Removal of some unnecessary processing on shaping logic.
  - Sugarloaf: Rewrite/Change of render architecture, now sugarloaf does not have any reference to column/lines logic.
- _Breaking:_ Minimum MacOS version went from El Captain to Big Sur on ARM64 and Catalina on Intel x86.
- Microsoft Windows: [Rio terminal is now available on WinGet packages](https://github.com/microsoft/winget-pkgs/pull/184792).
- Microsoft Windows: [Rio terminal is now available on MINGW packages](https://github.com/msys2/MINGW-packages/pull/22248).
- Microsoft Windows: Rio support on ARM architecture by [@andreban](https://github.com/andreban).
- Allow MacOS automation via events.
- MacOS: Support titlebar unified: `window.macos-use-unified-titlebar = false`,
- Support disable font hinting: `fonts.hinting = false`.
- Fix: Configuration updates triggered multiple times on one save.
- Support to RetroArch shaders [@igorsaux](https://github.com/igorsaux).
- Fix: Set notepad as a default editor on Windows by [@igorsaux](https://github.com/igorsaux).
- Increased Linux font fallbacks list.
- Early initial split support (this feature is not yet stable).
- Fix: Preserve current working directory when opening new tabs [#725](https://github.com/raphamorim/rio/issues/725).
- Added `SplitDown`, `SplitRight`, `CloseSplitOrTab`, `SelectNextSplit` and `SelectPrevSplit` actions.
- Fix: Window doesn't receive mouse events on Windows 11 by [@igorsaux](https://github.com/igorsaux).
- Support to hex RGBA (example: `#43ff64d9`) on colors/theme by [@bio](https://github.com/bio) on [#696](https://github.com/raphamorim/rio/pull/696).
- Introduced `renderer.strategy`, options are `Events` and `Continuous`.
- Microsoft Windows: make `ControlFlow::WaitUntil` work more precisely using `CREATE_WAITABLE_TIMER_HIGH_RESOLUTION`.
- Fix: Window output lost when rio loses focus [#706](https://github.com/raphamorim/rio/issues/706).
- Updated wgpu to `23.0.0`.

## 0.1.17 (2024-10-06)

- Fix flash of white during startup on Microsoft Windows [#640](https://github.com/raphamorim/rio/issues/640).
- Add DWMWA_CLOAK support on Microsoft Windows.
- VI Mode now supports search by [@orhun](https://github.com/orhun).
- Use max frame per seconds based on the current monitor refresh rate.
- _breaking_ `renderer.max-fps` has been changed to `renderer.target-fps`.
- Fix background color for underline and beam cursors when using transparent window.
- Fix IME color for underline and beam cursors.
- Add default for Style property on Sugarloaf font.

## 0.1.16 (2024-09-24)

- Support auto bold on fonts.
- Support auto italic on fonts.
- Reduced default regular weight to 300 instead of 400.
- MacOS: Add dock menu.
- MacOS: Add Shell and Edit menu.
- MacOS: Support to native modal that asks if wants to close app.
- MacOS: Fix `confirm-before-quit` property.

## 0.1.15 (2024-09-19)

- Introduce `cursor.blinking-interval`, default value is 800ms.
- Fix blinking cursor lag issue.
- performance: Use `Vec` (std based) instead of ArrayVec for copa.
- Fix adaptive theme background color on macos.
- Decorations as `Transparent` is default on MacOS.
- Navigation mode as `NativeTab` is default on MacOS.
- `keyboard.use-kitty-keyboard-protocol` is now `false` by default.
- Add support for msys2/mingw builds release [#635](https://github.com/raphamorim/rio/issues/635) by [@Kreijstal](https://github.com/Kreijstal).

## 0.1.14 (2024-09-17)

- `developer.log-file` has been renamed to `developer.enable-log-file`.
- **breaking**: `CollapsedTab` has been renamed to `Bookmark`.
- Memory usage reduced by 75% (avg ~201mb to 48mb on first screen render).
- Implemented font data deallocator.
- Reduced font atlas buffer size to `1024`.
- Added lifetimes to application level (allowing to deallocate window structs once is removed).
- Migrated font context from `RwLock` to `Arc<FairMutex>`.
- MacOS does not clear with background operation anymore, instead it relies on window background.
- Background color has changed to `#0F0D0E`.
- Fix font emoji width.
- Fix MacOS tabbing when spawned from a new window.

## 0.1.13 (2024-09-09)

- Support to iTerm2 image protocol.
- Fix: Issue building rio for Void Linux [#656](https://github.com/raphamorim/rio/issues/656).
- Fix: Adaptive theme doesn't appear to work correctly on macOS [#660](https://github.com/raphamorim/rio/issues/660).
- Fix: Image background support to OpenGL targets.
- Fix: Unable to render images with sixel protocol & ratatui-image [#639](https://github.com/raphamorim/rio/issues/639).
- Implement LRU to cache on layout and draw methods.
- Reenable set subtitle on MacOS native tabs.

## 0.1.12 (2024-09-07)

- Introduce: `renderer.max-fps`.
- Fix: Cursor making text with ligatures hidden.
- Fix: Underline cursor not working.
- Fix: sixel: Text doesn't overwrite sixels [#636](https://github.com/raphamorim/rio/issues/636).
- Initial support to Sixel protocol.
- Support to `fonts.emoji`. You can also specify which emoji font you would like to use, by default will be loaded a built-in Twemoji color by Mozilla.

In case you would like to change:

```toml
# Apple
# [fonts.emoji]
# family = "Apple Color Emoji"

# In case you have Noto Color Emoji installed
# [fonts.emoji]
# family = "Noto Color Emoji"
```

- Support to `fonts.ui`. You can specify user interface font on Rio.

Note: `fonts.ui` does not have live reload configuration update, you need to close and open Rio again.

```toml
[fonts.ui]
family = "Departure Mono"
```

- **breaking:** Revamp the cursor configuration

Before:

```toml
cursor = '▇'
blinking-cursor = false
```

After:

```toml
[cursor]
shape = 'block'
blinking = false
```

## 0.1.11 (2024-09-04)

- Experimental support to Sixel protocol.
- Clipboard has been moved to Application level and shared to all windows.
- Replace `run` with `run_app`.
- Support CSI_t 16 (Report Cell Size in Pixels).
- Support CSI_t 14 (Report Terminal Window Size in Pixels).
- Fix on all the issues regarding whenever the font atlas reaches the limit.
- _breaking change_: collapsed tabs use now `tabs-active-highlight` instead of `tabs-active`.
- Default font for UI has changed to [DepartureMono](https://departuremono.com/).
- Performance: drop extra texture creation and manipulation.
- Fix on windows: If editor is not found, the app panics [#641](https://github.com/raphamorim/rio/issues/641).
- Improvements on `window.background-image` as respect width and height properties if were used.
- Macos: remove grab cursor when dragging and use default instead.
- Fix `tabs-active-highlight` config key [#618](https://github.com/raphamorim/rio/pull/618).
- Add `tabs-active-foreground` config key [#619](https://github.com/raphamorim/rio/pull/619).
- Add `tabs-foreground` config key.
- `use-kitty-keyboard-protocol` is now `true` as default.
- Remove tokio runtime.
- Allow configuring with lowercase values for enums.
- Rename `hide-cursor-when-typing` to `hide-mouse-cursor-when-typing`.
- Cleanup selection once happens a resize.
- Windows: Reduce WM_PAINT messages of thread target window.

## 0.1.10 (2024-08-27)

- Refactor/Simplify close tabs logic internally.
- Fix: NativeTab margin top when `hide-if-single` is true.
- Fix: Search bar width on 1.0 dpi screens.
- Fix: Windows - The behavior of using a complete shell command and a shell command with parameters is inconsistent [#533](https://github.com/raphamorim/rio/issues/533).
- X11: Replace libxcursor with custom cursor code.
- Fix: Kitty keyboard protocol shifted key codes are reported in wrong order [#596](https://github.com/raphamorim/rio/issues/596).
- Fix: Mouse pointer hidden (Ubuntu Wayland) / Cursor icon not changing [#383](https://github.com/raphamorim/rio/issues/383).
- Enable search functionality as default on Linux.
- Enable search functionality as default on Microsoft Windows.
- Add command for closing all tabs except the current one (`CloseUnfocusedTabs`)

## 0.1.9 (2024-08-25)

- Search support.
- New theme properties `search-match-background`, `search-match-foreground`, `search-focused-match-background` and `search-focused-match-foreground`.
- Fix bug Tab indicator doesn't disappear [#493](https://github.com/raphamorim/rio/issues/493).
- Fix color automation on tabs for linux.
- Update tabs UI styles (make it larger and able to show more text when necessary).
- Corrections on underline render proportions for different DPIs.
- Support writing the config to a custom/default location via `--write-config` (Ref: #605).
- Fix scale update on transitioning between screens with different DPI.
- Support a short variant (`-w`) for `--working-dir` argument.

## 0.1.8 (2024-08-21)

- **breaking:** Introduced a new property in theme called `tabs-active-highlight`, default color is `#ff00ff`.
- **breaking:** Removed breadcrumb navigation.
- **breaking:** Introduced a new property in theme called `bar`, default color changed is `#1b1a1a`.
- **breaking:** `CollapsedTab` is now default for all platforms.
- Tab UI got some updates.
- Introduce `navigation.hide-if-single` property (Ref: [#595](https://github.com/raphamorim/rio/issues/595)).
- Performance update: Remove lock dependencies on render calls.
- Performance update: Render repeated styled fragments as one rect.
- Sugarloaf API has changed from `Sugar` primitives to `Content`.
- Fix: `[editor]` overshadow headerless parameters in default config. (Ref: #601)

## 0.1.7 (2024-08-16)

**Breaking**

Editor property have changed from `String` to allow input arguments as well.

Before:

```toml
editor = "vi"
```

Now:

```toml
[editor]
program = "code"
args = ["-w"]
```

- Fix: editor doesn't handle arguments [#550](https://github.com/raphamorim/rio/issues/550).
- Fix: Weird rendering behaviour on setting padding-x in config [#590](https://github.com/raphamorim/rio/issues/590).
- Upgrade Rust to 1.80.1.

## 0.1.6 (2024-08-12)

- Support custom colors on all underlines.
- Support for advaned formatting (squiggly underline?) [#370](https://github.com/raphamorim/rio/issues/370)
- Performance improvements!
  - Cache strategy has improved to cover any line that have been previously rendered.
  - Render backgrounds and cursors in one pass.
- Update tokio

## 0.1.5 (2024-08-08)

- Fix Bug cell disappearance [#579](https://github.com/raphamorim/rio/issues/579).
- Fix Bug Rendering problem with TUIs using cursor movement control sequences in rio (v0.1.1+) [#574](https://github.com/raphamorim/rio/issues/574).
- Changed default font family to Cascadia Code.
- Changed default width to 800 and default height to 500.

## 0.1.4 (2024-08-05)

- Fix Bug Text Rendering Bug [#543](https://github.com/raphamorim/rio/issues/543).
- Fix Abnormal font display and incomplete Navigation content display [#554](https://github.com/raphamorim/rio/issues/554).
- Fix Bug switch tabs doesn't work [#536](https://github.com/raphamorim/rio/issues/536).
- Update Cascadia Code to 2404.23.
- Change Cascadia builtin font from ttf to otf.
- Improvements for mouse selection.
- Performance improvements for background renders for all navigations besides `Plain` and `NativeTab`.
- Fix Cursor blinking is triggered by changes in inactive tabs [#437](https://github.com/raphamorim/rio/issues/437).
- Fix key bindings when key is uppercased (`alt` or `shift` is inputted along).
- Support to padding-y (ref: [#400](https://github.com/raphamorim/rio/issues/400))

Define y axis padding based on a format `[top, bottom]`, default is `[0, 0]`.

Example:

```toml
padding-y = [30, 10]
```

- Update swash (0.1.18), ab_glyph (0.2.28) and remove double hashmap implementation.

## 0.1.3 (2024-07-27)

- Added support to font features (ref: #548 #551)

```toml
[fonts]
features = ["ss01", "ss02", "ss03", "ss04", "ss05", "ss06", "ss07", "ss08", "ss09"]
```

Note: Font features do not have support to live reload on configuration, so to reflect your changes, you will need to close and reopen Rio.

- fix: Wayland - No input after first run [#566](https://github.com/raphamorim/rio/issues/566).
- fix: Mouse pointer location differs from selected text #573.
- fix: IO Safety violation from dropping RawFd (fatal runtime error: IO Safety violation: owned file descriptor already closed).
- Upgrade to Rust 1.80.0.

## 0.1.2 (2024-07-21)

- Upgrade wgpu to v22.0.0.
- Restrict of cells width.
- Wayland: update dependencies.
- Wayland: avoid crashing when compositor is misbehaving. (ref: raphamorim/winit 22522c9b37e9734c9a2408fae8d34b2599ff4574).
- Performance upgrades for lines rendered previously.

## 0.1.1 (2024-06-16)

- Fix the validation errors whenever a surface is used with the vulkan backend.
- Clean up weak references to texture views and bind groups to prevent memory leaks.
- Fix crashes whenever reading binary files.
- Improvements on font loader (avoid set weight or style in the lookup if isn't defined).
- Fallbacks fonts doesn't trigger alerts anymore.

## 0.1.0 (2024-06-10)

**Breaking change: Opacity API has changed**

- `background-opacity` has been renamed to `opacity`. It sets window background opacity.
- Removed `foreground-opacity` property.
- Removed support to DX11.

Example:

```toml
[window]
opacity = 0.8
```

- Major rewrite on sugarloaf.
  - New rendering architecture.
  - Sugarloaf now uses same render pass for each render.
  - Ignore equal renderers.
  - Compute layout updates only if layout is different.
- `BottomTab` navigation is now default for Linux and Windows.
- Support to font ligatures.
- Support bluetooth access on MacOs.
- Upgraded wgpu to 0.20.0.
- Support "open here" for Microsoft Windows.
- Fixes on font search for Microsoft Windows.
- Open Url support for MacOS.
- All tabs/window instances now use same font data.
- Disabled `line-height` configuration in this version (it will be re added eventually).
- Updated ttf-parser and memmap2 on sugarloaf.

#### Bug fixes

- closed: #514 Odd background transparency on macOS (Intel)
- closed: #398 Neovim and Helix rendering with line spacing
- closed: #512 Visible lines on transparent background
- closed: #491 Noticeable text update
- closed: #476 Glyphs have very weird rendering
- closed: #422 Background opacity
- closed: #355 Issues with double-width chars
- closed: #259 Sugarloaf: Positioning glyphs
- closed: #167 Tab bar overlaps text
- closed: #328 Some font issues
- closed: #225 Doesn't work with touchscreen
- closed: #307 default offset height is above the bottom position since update
- closed: #392 Box drawing issue with Berkeley Mono on MacOS

## 0.0.39 (2024-05-23)

- Minor fix on fixed transparency on backgrounds for Welcome/Dialog.

## 0.0.38 (2024-05-23)

- Corrections for transparency and blur for MacOS windows.
- Apply dynamic background logic only for images and keep alpha channel on background.

## 0.0.37 (2024-05-01)

- _Breaking change:_ Reduced font size to `16.0`.
- _Breaking change:_ Set `VI mode` trigger with CTRL + SHIFT + SPACE on Windows.
- Update winit to 0.30.0.
- Update rust version to 1.77.2.
- Initial touch support by [@androw](https://github.com/androw) [#226](https://github.com/raphamorim/rio/pull/226)

## 0.0.36 (2024-03-14)

- fixes for x11 freeze issue.
- update winit to 0.29.15.
- update wix (toolset that builds Windows Installer) from 4.0.1 to 4.0.4.

## 0.0.35 (2024-02-27)

- Bump wayland dependencies: `wayland-backend`, `wayland-client`, `wayland-cursor` and `wayland-scanner`.
- Refactor: disable cursor blink on selection (ref #437) #441 by @hougesen .
- Rewrite hash logic to use `BuildHasher::hash_one`.
- Report focus change https://terminalguide.namepad.de/mode/p1004/.
- update rust version to 1.75.0.
- update winit to 0.29.11.

## 0.0.34 (2024-01-07)

- use Fowler–Noll–Vo hash function implementation for sugar cache (more efficient for smaller hash keys)
- update winit to 0.29.9

## 0.0.33 (2023-12-18)

- **Breaking**: Removed `macos-hide-toolbar-buttons` in favor of `window.decorations` api.
- Fix: Rio failing to draw blur upon launch #379
- Fix: Window transparency does not work on X11 #361
- Added support for path based color automation.
- Added `window.decorations` property, available options are `Enabled`, `Disabled`, `Transparent` and `Buttonless`.

## 0.0.32 (2023-12-13)

- Fix: font order priority.
- Fix: add default values to keyboard config (#382)

## 0.0.31 (2023-12-12)

- **Breaking**: Configuration `performance` has moved to `renderer.performance`.
- **Breaking**: Configuration `disable-renderer-when-unfocused` has moved to `renderer.disable-renderer-when-unfocused`.
- **Breaking**: Configuration `use-kitty-keyboard-protocol` has moved to `keyboard.use-kitty-keyboard-protocol`.

- Introduction of new configuration property called `keyboard`.

```toml
[keyboard]
use-kitty-keyboard-protocol = false
disable-ctlseqs-alt = false
```

- Introduction of `keyboard.disable-ctlseqs-alt`: Disable ctlseqs with ALT keys. It is useful for example if you would like Rio to replicate Terminal.app, since it does not deal with ctlseqs with ALT keys

- Introduction of new configuration property called `renderer`.

```toml
[renderer]
performance = "High"
disable-renderer-when-unfocused = false
backend = "Automatic"

# backend options:
# Automatic: Leave Sugarloaf/WGPU to decide
# GL: Supported on Linux/Android, and Windows and macOS/iOS via ANGLE
# Vulkan: Supported on Windows, Linux/Android
# DX12: Supported on Windows 10
# DX11: Supported on Windows 7+
# Metal: Supported on macOS/iOS
```

- Fix: update padding top on config change [#378](https://github.com/raphamorim/rio/pull/378) by [@hougesen](https://github.com/hougesen)
- Fixed bug where color automation did not work on Linux because of line ending character.
- Fix: Control + Up/Down don't works as expected on neovim [#371](https://github.com/raphamorim/rio/issues/371)
- Fix: remove duplicate kitty backspace keybinds [#375](https://github.com/raphamorim/rio/pull/375) by [@hougesen](https://github.com/hougesen)
- Fix: Kitty-keyboard-protocol causes Backspace to delete 2 characters. [#344](https://github.com/raphamorim/rio/issues/344) by [@hougesen](https://github.com/hougesen)

## 0.0.30 (2023-12-10)

- Fix regression with color ansi when transparency is off.
- **Breaking**: Config `navigation.macos-hide-window-buttons` has moved to `window.macos-hide-toolbar-buttons`.
- **Breaking**: Config property `padding-x` has been updated from 5.0 to 0.0 on MacOS.

## 0.0.29 (2023-12-05)

- Fix compiled binary shows nothing inside the app window [#366](https://github.com/raphamorim/rio/issues/366).
- Fix command key + left and right strange behavior [#359](https://github.com/raphamorim/rio/issues/359).
- **New scroll API**: Scroll calculation for canonical mode will be based on `(accumulated scroll * multiplier / divider)` so if you want quicker scroll, keep increasing the multiplier if you want to reduce you increase the divider. Can use both properties also to find the best scroll for you:

```toml
[scroll]
multiplier = 3.0
divider = 1.0
```

- Corrections for TMUX scroll calculations.

## 0.0.28 (2023-11-29)

- **Breaking**: Settings UI has been removed and `editor` property has been added.
- **Breaking**: default `padding-x` for MacOS has moved from `10.0` to `5.0`.
- **Breaking: Background API has moved to Window**

Example:

```toml
[window]
width = 600
height = 400
mode = "Windowed"
foreground-opacity = 1.0
background-opacity = 1.0
```

Using image as background:

```toml
[window.background-image]
path = "/Users/rapha/Desktop/eastward.jpg"
width = 200.0
height = 200.0
x = 0.0
y = 0.0
```

- **Breaking:** MacOS default navigation mode will become `NativeTab`.
- Support for blur background.
- Support opacity for foreground and background.
- Cursor hide feature is now behind configuration `hide-cursor-when-typing`.
- Confirm before quite (it can be disabled through configuration `confirm-before-quit`).
- Close the last tab in MacOS when using `command + w` (Ref: [#296](https://github.com/raphamorim/rio/issues/296))
- OSC 8 (Hyperlinks).
- Fix current path on new tab is not working when using Native Tab (Ref [#323](https://github.com/raphamorim/rio/issues/323)).
- Change `POLLING_TIMEOUT` for configuration update from 1s to 2s.
- Update `.icns` file with more format and add new icon (Ref: [#329](https://github.com/raphamorim/rio/pull/329)) by [@nix6839](https://github.com/nix6839).
- Update `.ico` files with more resolution and add new icon (Ref: [#329](https://github.com/raphamorim/rio/pull/329)) by [@nix6839](https://github.com/nix6839).

## 0.0.27 (2023-11-05)

- Activate the hyperlink check whenever a modifier is changed (`alt` for windows/linux/bsd and `command` for macos).
- Fix Error when Double click on terminal side (Ref [#316](https://github.com/raphamorim/rio/issues/316)).

## 0.0.26 (2023-10-28)

- Upgrade winit to 0.29.3.
- Support for `Run` actions key bindings for Microsoft Windows.
- Hyperlink support (Ref [#60](https://github.com/raphamorim/rio/issues/60))

## 0.0.25 (2023-10-25)

- Upgrade wgpu to 0.18.0.
- Desktop OpenGL 3.3+ Support on Windows through WebGPU.
- Display the shell name on the tab title for MacOS Native Tab (Ref [#311](https://github.com/raphamorim/rio/issues/311) by [@eduronqui](https://github.com/eduronqui)).
- Fix VI cursor disappearing whenever perform a scroll..
- Fix flagged dimmed colors (cases where it does not comes from rgb index).
- Fix MacOS fullscreen empty space on margin top.
- Upgrade winit to 0.29.2.

## 0.0.24 (2023-10-20)

- Improvements on selection text for scale factor >= 2.0.
- Improvements on cursor sugar creation, dropped unnecessary usage of clone.
- Colors/Themes got a new property called `vi-cursor`, you can specify any color you wish for VI Cursor.
- Alacritty's VI Mode.

## 0.0.23 (2023-10-18)

#### Breaking changes

- `navigation.mode = "Plain"` now only shutdowns the key bindings related to tab creation/manipulation.
- `ignore-selection-fg-color` has been renamed to `ignore-selection-foreground-color`.
- Kitty keyboard protocol has been disabled by default in this version, for enable it you need to use `use-kitty-keyboard-protocol = true`.
- `CollapsedTab` is not based on reverse order anymore.
- Actions `SelectTab1`, `SelectTab2`, ..., `SelectTab9` have been removed in favor of the new select tab API:

```toml
[bindings]
keys = [
    { key = "1", with = "super", action = "SelectTab(0)" },
    { key = "2", with = "super", action = "SelectTab(1)" },
    { key = "3", with = "super", action = "SelectTab(2)" }
]
```

- Actions `ScrollLineUp` and `ScrollLineDown` have been removed in favor of the new Scroll API:

```toml
[bindings]
keys = [
    # Scroll up 8 lines
    { key = "up", with = "super", action = "Scroll(8)" },
    # Scroll down 5 lines
    { key = "down", with = "super", action = "Scroll(-5)" }
]
```

#### Other changes

- Rendering performance small improvements towards to Sugar text for regular font, dropped in redundancy processing (avg 68ms to 22ms with tests using 155x94 without repetition like `vim Cargo.lock`).
- Rendering performance small improvements towards to Sugar rect calculation, dropped in redundancy processing. Now Sugarloaf computes better Rects duplication in a line. It gains significant performance for large screens (avg ~12ms).
- Fix Backspace behaviour misplace on Windows (Ref https://github.com/raphamorim/rio/issues/220).
- `ClearHistory` key binding is available to use per configuration file.
- Introduce Alacritty's VI Mode (Ref https://github.com/raphamorim/rio/issues/186).
- Implement `ClearSelection` key binding action.
- Fix Cursor shape isn't restored (Ref https://github.com/raphamorim/rio/issues/279).
- Fix color automation for breadcrumb mode (Ref https://github.com/raphamorim/rio/issues/251).
- Fix text copy (OSC 52) is broken (tmux, zellij) (Ref https://github.com/raphamorim/rio/issues/276).
- Fix lines calculation for different fonts.
- Fix bug whenever is not closing terminal for non native tabs (Ref https://github.com/raphamorim/rio/issues/255).
- Removal of hide cursor functionality when start to type for all platforms besides Apple MacOS.
- Support to new scroll action API key binding.
- Support to new select tab action API key binding.
- Support to execute programs as actions for key bindings:

```toml
[bindings]
keys = [
    { key = "p", with = "super", action = "Run(code)" },
    { key = "o", with = "super", action = "Run(sublime ~/.config/rio/config.toml)" }
]
```

- Upgrade rust to 1.73.0 by @igorvieira.

## 0.0.22 (2023-10-02)

- Now you can add extra fonts to load:

```toml
[fonts]
extras = [{ family = "Microsoft JhengHei" }]
```

- Added `ScrollLineUp`, `ScrollLineDown`, `ScrollHalfPageUp`, `ScrollHalfPageDown`, `ScrollToTop`and `ScrollToBottom` to bindings.
- Fix japanese characters on Microsoft Windows (Ref: https://github.com/raphamorim/rio/issues/266).
- Navigation fonts now use the CascadiaCode built-in font and cannot be changed.
- Proper select adapter with `is_srgb` filter check.
- Switched to queue rendering instead of use staging_belt.
- Fixed leaks whenever buffer dropped map callbacks.
- Forked and embedded glyph-brush project to sugarloaf. Glyph-brush was originally created @alexheretic and is licensed under Apache-2.0 license.
- Upgrade wgpu to 0.17.1.

## 0.0.21 (2023-09-30)

- Hide other applications in MacOS #262 by @sonbui00.
- Implemented `working-dir` parameter to cli https://github.com/raphamorim/rio/issues/258.
- Remove legacy icns icons from bundle.

## 0.0.20 (2023-09-24)

- Fix retrieve foreground process name to tabs.
- Fix cursor disappearing in the first tab whenever a new tab is created with NativeTab.
- Fix settings for NativeTabs.
- New docs.
- Removal of RIO_CONFIG environment variable.
- Add ToggleFullscreen Action #229 (Ref: https://github.com/raphamorim/rio/pull/249)
- fix: Command + H can't hide rio on macOS (Ref: https://github.com/raphamorim/rio/pull/244).
- Added fontconfig to font loader.
- New Rio terminal logo.
- Update Rust to 1.72.1 (Ref: https://github.com/raphamorim/rio/pull/238).
- Enable CPU-specific optimizations on aarch64-apple-darwin (Ref: https://github.com/raphamorim/rio/pull/235).
- Use release profile with optimization level as 3 (Ref: https://github.com/raphamorim/rio/pull/236).
- Use fixed dependency versions in sugarloaf
- Added split support along with the following actions `SplitVertically`, `SplitHorizontally` and `ClosePane` (support to split is still not available).

## 0.0.19 (2023-09-19)

**Breaking change**

Configuration properties: `window_height`, `window_width` and `window_opacity` has been moved to a new window/background API:

```toml
# Window configuration
#
# • width - define the initial window width.
#   Default: 600
#
# • height - define the initial window height.
#   Default: 400
#
# • mode - define how the window will be created
#     - "Windowed" (default) is based on width and height
#     - "Maximized" window is created with maximized
#     - "Fullscreen" window is created with fullscreen
#
[window]
width = 600
height = 400
mode = "Windowed"

# Background configuration
#
# • opacity - changes the background transparency state
#   Default: 1.0
#
# • mode - defines background mode between "Color" and "Image"
#   Default: Color
#
# • image - Set an image as background
#   Default: None
#
[background]
mode = "Image"
opacity = 1.0
[background.image]
path = "/Users/rapha/Desktop/eastward.jpg"
width = 200.0
height = 200.0
x = 0.0
```

- Fix for retrieving shell environment variable when running inside of Flatpak sandbox (Ref: https://github.com/raphamorim/rio/issues/198).
- Rio terminal is now also available in crates.io: https://crates.io/crates/rioterm .
- Added `navigation.mode = "Plain"`, it basically disables all platform key bindings for tabs, windows and panels creation (Ref https://github.com/raphamorim/rio/issues/213).
- Support for blinking cursor (Ref: https://github.com/raphamorim/rio/issues/137) (this option is not enabled by default).
- Migrated font-kit to a custom font loader.
- Support to MacOS tile window positioning feature (left or right).
- Added support to MacOS display native top bar items.
- Support to adaptive theme (theme selection based on user system theme variant `dark` or `light`).
- Implemented `ScrollPageUp`, `ScrollPageDown`, `ScrollHalfPageUp`, `ScrollHalfPageDown`, `ScrollToTop`, `ScrollToBottom`, `ScrollLineUp`, `ScrollLineDown` (Ref: https://github.com/raphamorim/rio/issues/206).
- Support to `fonts.family` (it overwrites regular, bold, bold-italic and italic font families).
- Added a welcome screen UI.
- Added a settings UI.
- Exposes `RIO_CONFIG` environment variable that contains the path of the configuration.
- Rio creates a configuration file with all defaults if does not exist.
- Added `OpenConfigEditor` key binding for all platforms.
- Configuration property `editor` was removed.
- Created Assistant, Rio terminal UI for display error (Ref: https://github.com/raphamorim/rio/issues/168).
- Fix 'Backspace' keypress triggers Ctrl+h keybinding in Zellij instead of deleting character. (Ref: https://github.com/raphamorim/rio/issues/197).
- Implemented `TERM_PROGRAM` and `TERM_PROGRAM_VERSION` (Ref: https://github.com/raphamorim/rio/issues/200).
- Whenever native tabs is on disable macos deadzone logic.

## 0.0.18 (2023-08-29)

- Upgraded to Rust 1.72.0.
- Fix delete key inputs square character.
- Fix Breadcrumb navigation crash.

## 0.0.17 (2023-08-29)

#### Breaking changes

- Configuration `font` does not work anymore, a new configuration API of font selection has been introduced.

```toml
[fonts]
size = 18

[fonts.regular]
family = "cascadiamono"
style = "normal"
weight = 400

[fonts.bold]
family = "cascadiamono"
style = "normal"
weight = 800

[fonts.italic]
family = "cascadiamono"
style = "italic"
weight = 400

[fonts.bold-italic]
family = "cascadiamono"
style = "italic"
weight = 800
```

- Action `TabSwitchNext` and `TabSwitchPrev` has been renamed to `SelectNextTab` and `SelectPrevTab`.

#### Rest of 0.0.17 changelog

- Support to `NativeTab` (MacOS only).
- Support for kitty's keyboard protocol (`CSI u`). Ref: https://sw.kovidgoyal.net/kitty/keyboard-protocol/
- Added new actions for tab selection: `SelectTab1`, `SelectTab2`, `SelectTab3`, `SelectTab4`, `SelectTab5`, `SelectTab6`, `SelectTab7`, `SelectTab8`, `SelectTab9`, `SelectLastTab`.
- Support lowercased action and fix overwrite for actions in custom key bindings.
- Added action `Minimize` for minimize Rio terminal window.
- Added action `ClearHistory` for clear terminal saved history.
- Added action `ReceiveChar` for custom key bindings.
- New default key bindings for Linux and Windows so that conflicts with readline key bindings are removed.
- Winit Version 0.29.1-beta.
- Allow paste with the middle mouse of the button (fixes https://github.com/raphamorim/rio/issues/123).
- Support startup notify protocol to raise initial window on Wayland/X11.
- Fix Double-tap by touchpad on the titlebar doesn't maximize/unmaximize the window in GNOME 44, Wayland.

## 0.0.16 (2023-08-03)

- Fix tab/breadcrumb bug introduced in 0.0.15
- Introduce new configuration property: `navigation.macos-hide-window-button`.

## 0.0.15 (2023-08-01)

- Introduce configurable navigation with the following options: `CollapsedTab` (default), `Breadcrumb`, `TopTab` and `BottomTab`.

An example of configuration:

```toml
[navigation]
mode = "BottomTab"
use-current-path = true
clickable = false
```

- Performance improvements with Sugarloaf de-duplication of input data.
  - Before: `~253.5µs`.
  - Now: `~51.5µs`.
- Introduce `navigation.use-current-path` which sets if a tab/breacrumb should be open from the current context path.
- Fix rendering unicode with 1 width glyphs (fix [#160](https://github.com/raphamorim/rio/issues/160)).
- Increased max tabs from 9 to 20.
- Default colors `selection-foreground` and `selection-background` has changed.
- Default colors `tab` and `tab-active` has changed.

## 0.0.14 (2023-07-23)

- Implementation of custom key bindings ([#117](https://github.com/raphamorim/rio/issues/117)).
- Fix .deb packing in GH Actions.
- Fix key binding for switch tab next (MacOS only).
- Fix scroll when copying text outside of offset.
- Fix copy key bindings.

## 0.0.13 (2023-07-21)

- Fix Fuzzy Finder issue ([#132](https://github.com/raphamorim/rio/issues/132)).
- Introduce Copa (Alacritty's VTE forked version to introduce new sequences/instructions in next versions).
- Upgraded Winit to 0.29.0-beta.0.
- Support for keybindings with dead keys.
- `Back`/`Forward` mouse buttons support in bindings.
- Fix unconditional query of xdg-portal settings on Wayland.
- Fix `Maximized` startup mode not filling the screen properly on GNOME Wayland.
- Fix Default Vi key bindings for `Last`/`First` actions not working on X11/Wayland.
- Set `padding-x` to 0 for non-macos.
- Set `app_id`/`WM_CLASS` property on Wayland/X11.

## 0.0.12 (2023-07-20)

- Strip binary is on for release builds.
- Each paste or key binding that has writing leads to clear selection and scroll bottom.
- Fixed over-rendering when scrolling.
- Fix selection.
- Support to copy using VIM.
- Fix for MacOS deadzone changing cursor to draggable on window buttons.
- Fix for scroll using tmux.

## 0.0.11 (2023-07-19)

- Fix for font styles using CachedSugar.

## 0.0.10 (2023-07-19)

- Major refactor of Sugarloaf.
  - Performance improvements around 80-110%.
  - Introduced CachedSugar.
  - Usage of PixelScale.
  - Line-height support.
- Open new tab using the current tab directory.
- Fix some symbols break the horizontal and vertical alignment of lines (ref [#148](https://github.com/raphamorim/rio/issues/148)).
- Fix font size configuration is confusing (ref [#139](https://github.com/raphamorim/rio/issues/139)).
- Fix Glyph not rendered in prompt (ref: [#135](https://github.com/raphamorim/rio/issues/135)).
- Use fork by default in context tests.
- Updated terminfo.
- Increased default font size to 18.
- Move to next and prev tab using keybindings.
- Setting editor by keybindings and new property called `editor` in configuration file.
- Rio creates `.deb` packages (canary and release).
- Binary size optimization (ref: [#152](https://github.com/raphamorim/rio/pull/152)) by [@OlshaMB]

## 0.0.9 (2023-07-14)

- Created "rio" terminfo.
- Breaking changes for configuration file regarding `Advanced`. The configuration `Advanced` has moved to root level and `disable-render-when-unfocused` renamed to `disable-unfocused-render`.

**before**

```toml
theme = "dracula"

[advanced]
disable-render-when-unfocused = true
```

**now**

```toml
theme = "dracula"
disable-unfocused-render = true
```

- Support to **spawn and fork processes**, spawn has became default. Spawn increases Rio compatibility in a broad range, like old MacOS versions (older or equal to Big Sur). However, If you want to use Rio terminal to fork processes instead of spawning processes, enable `use-fork` in the configuration file:

```toml
use-fork = true
```

- Introduced `RIO_LOG_LEVEL` variable usage. (`e.g: RIO_LOG_LEVEL=debug rio -e "echo 1"`)
- Increased max tabs from 6 to 9.
- Fix Incorrect cursor position when using multi-byte characters (Ref: [#127](https://github.com/raphamorim/rio/issues/127))
- Fix bug ["black screen with nearly zero interactivity"](https://github.com/raphamorim/rio/issues/112) and new tab hanging.
- Fix cursor disappearing after resize.
- Introduction of `shell` and `working_dir` in configuration file.
- Multi window support [#97](https://github.com/raphamorim/rio/issues/97).
- Corrections on select and scroll experience (it was using wrongly font-bound for line calculation).
- Add selection color to the theme config (closed [#125](https://github.com/raphamorim/rio/issues/125)).
- Implemented Inverse (fix [#92](https://github.com/raphamorim/rio/issues/92)).
- Proper choose formats that matches with `TextureFormat::is_srgb` (it fixed the Vulkan driver, related [#122](https://github.com/raphamorim/rio/issues/122)).
- Corcovado: Filter windows crate dependency to only Windows targets (related: [#119](https://github.com/raphamorim/rio/issues/119)).
- Teletypewriter: Fixes for musl as target_env (related: [#119](https://github.com/raphamorim/rio/issues/119)).
- FreeBSD support, implementation by [yurivict](https://github.com/yurivict) ([Commit](https://github.com/freebsd/freebsd-ports/commit/8582b8c59459a7dc5112a94a39de45f6cc124c3e), Ref: [#115](https://github.com/raphamorim/rio/issues/115))

## 0.0.8 (2023-07-03)

- Added generation of `.msi` and `.exe` files to the release pipeline (stable and canary).
- Support to Microsoft Windows.
- Ability to in|decrease font size using keyboard shortcut during session (ref: [#109](https://github.com/raphamorim/rio/issues/109))
- Inverted Canary and Stable icons.
- ANSI mouse reports (e.g: scroll and click working on VIM).
- Scroll and apply selection.
- Semantic and line selection.
- Rio is available in Homebrew casks (ref [github.com/Homebrew/homebrew-cask/pull/149824](https://github.com/Homebrew/homebrew-cask/pull/149824)).
- Rio stable versions are notarized now.
- Migration of mio, mio-extras, mio-signal-hook to Corcovado.
- Changed default black color to `#4c4345`.
- Fix mouse position for when selecting text.

## 0.0.7 (2023-06-20)

- Breaking changes for configuration file regarding `Style` property.

before:

```toml
performance = "High"
[style]
font-size = 18
theme = "lucario"
```

now:

```toml
performance = "High"
theme = "lucario"
font-size = 18
```

- Fix Background color not entirely set on vim [#88](https://github.com/raphamorim/rio/issues/88)
- Scroll now works for x11 and wayland.
- No longer renders to macos and x11 windows that are fully occluded / not directly visible.
- Introduced `window-opacity` config property for WebAssembly and Wayland builds.
- Add permissions instructions to Rio macos builds (Fix [#99](https://github.com/raphamorim/rio/issues/99)).
- Fixes for x11 and wayland rendering (Related: [#98](https://github.com/raphamorim/rio/issues/98) and [#100](https://github.com/raphamorim/rio/issues/100)).
- Performance fixes (Related: [#101](https://github.com/raphamorim/rio/issues/101)).
- Sugarloaf WebAssembly support.
- Fixed resize for all contexts: removed the glitch when resizing and switching between tabs.
- Fixed cursor inconsistencies [#95](https://github.com/raphamorim/rio/issues/95).
- Added command line interface support (`--help`, `--version`, `-e` and `--command`).
- Added a fallback for WPGU request device operation: downlevel limits, which will allow the code to run on all possible hardware.
- Added `padding-x` to configuration.
- Reload automatically when the configuration file is changed ([#69](https://github.com/raphamorim/rio/issues/69)).
- Fix `Ctrl+D`.
- Fix `exit` command not closing the app ([#87](https://github.com/raphamorim/rio/issues/87)).
- Changed default `light-black` color.

## 0.0.6 (2023-06-06)

- Fix: support to clipboard in linux by [@joseemds](https://github.com/joseemds).
- Font style for custom fonts by [@OlshaMB](https://github.com/OlshaMB) (closed [#80](https://github.com/raphamorim/rio/issues/80) and [#81](https://github.com/raphamorim/rio/issues/81))
- Text styles Underline and Strikethrough (closed [#79](https://github.com/raphamorim/rio/issues/79)).
- Update default colors for tabs/tabs-active.
- Tabs support.
- Fix rendering tab and hidden chars by replacing to space by [@niuez](https://github.com/niuez), (closed [#56](https://github.com/raphamorim/rio/issues/56)).
- Block cursor hover a character and still allow it to be visible.
- Support to caret Beam and Underline cursor [#67](https://github.com/raphamorim/rio/issues/67) by [@niuez](https://github.com/niuez).
- Fix panics if custom font is not found [#68](https://github.com/raphamorim/rio/issues/68).
- MacOs ignore alt key in cntrlseq (same behavior as Terminal.app, Hyper, iTerm and etecetera).

## 0.0.5 (2023-05-30)

- Fix ctlseqs modifiers for bindings.
- Add RioEvent::ColorRequest events to write color updates on pty.
- Fix to render specific 24bit colors (#66) by [@niuez](https://github.com/niuez).
- Cross build for arm64 and x86
- Bold and Italic support (https://github.com/raphamorim/rio/issues/33).
- Theme support (eae39bc81b5b561882b7a37b2c03896633276c27)
- Fix font-size dependency for serialization (f278102)
- Fix cursor visibility on VI mode and scroll (https://github.com/raphamorim/rio/issues/51)
- Performance fixes for rendering from teletypewriter updates.
- Fix scale issues for 1.0 scale factor or using monitor with different scale factor. (https://github.com/raphamorim/rio/issues/50)
- Improve `make pack-osx-arm` and `make pack-osx-x86` to only contain Rio.app file. (https://github.com/raphamorim/rio/issues/54)

## 0.0.4 (2023-05-24)

- Fix CPU large usage when scrolling.
- Task scheduler.
- Copy feature.
- Selection feature (selection doesn't work when scrolling yet).
- Change default cursor icon for Text (`winit::window::CursorIcon`).
- Scroll bottom when display offset is different than zero.
- Fix for user interaction "close Rio terminal" using UI interface (`ExitWithCode(0)`).
- Hide cursor when typing and make it visible again with scroll and cursor interactions.
- Implementation of paste files to string path.

## 0.0.3 (2023-05-19)

- Added Input Method Engine (IME) support. Note: only works for preedit with single character now, which means that still need to fix for other keyboards as Japanese, Chinese [...].
- Common Keybindings and keybindings for MacOS.
- Allow to configure `option-as-alt` for Winit on MacOs. Issue originally bought by Alacritty on Winit (https://github.com/rust-windowing/winit/issues/768).
- Allow to configure environment variables through config file.
- Stabilization of Sugarloaf render on emojis, symbols and unicode.

## 0.0.2

- `log-level` as configurable (`DEBUG`, `INFO`, `TRACE`, `ERROR`, `WARN` and `OFF`). `OFF` by default.
- Introduction of rendering engine called Sugarloaf.
- System font loader (tested and implemented for MacOs).
- Font loader with not native emoji font (emojis aren't stable yet).
- Rect renderer based on provided color (text background), stabilized for monospaced fonts.

## 0.0.1

- Basic move/goto functionalities.
- Initial definition of Rio default colors.
- Set and reset color by ANSI parser.
- Clear/Tabs functionalities.
- Grid introduction.
- Desktop delta scroll (up and down, without scrollbar UI component).
- `Teletypewriter` 2.0.0 usage for macos and linux.
- Resize support.
- $SHELL login on macos, by default: `/bin/zsh --login` (if $SHELL is settled as other could as run `/bin/bash --login`, `/bin/fish --login` ...).
- Cursor initial support (without VI mode).
