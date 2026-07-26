# canario session persistence (Tier A)

Browser-tab-style restore for canario **only** (rio/rioterm is untouched):
on relaunch, reopen the same sidebar tree, tabs, split layout, titles, and
per-pane working directory, and replay the old scrollback as inert text.
Shells are restarted (Tier A) — no live process survival (that would be
Tier B, a librio-mux daemon).

## What we persist

The root is `AppModel.items: [SidebarItem]` (folders + `TerminalItem`
tabs; each tab = `columns` of `Panel`s). We serialize the *structure* plus
per-pane state; runtime UUIDs are NOT preserved (new ones on restore).

```
Session
└─ items: [ItemDTO]
   ├─ .folder(FolderDTO { name, isExpanded, children: [ItemDTO] })
   └─ .terminal(TerminalDTO {
         name, isExpanded,
         focused: {col,row}?,               // which pane had focus
         columns: [[PaneDTO]],              // the split grid
      })
PaneDTO { cwd: String?, title: String?, weight: Double, scrollbackFile: String? }
```

Scrollback text is large, so it goes to a **sidecar file** per pane
(`scrollback/<uuid>.txt`), and `PaneDTO.scrollbackFile` references it. The
JSON stays small.

## librio APIs used (already added, C ABI)

- `rio_surface_working_dir(surface) -> char*` — OSC 7 cwd, or NULL. Feeds
  `PaneDTO.cwd` at save time.
- `rio_surface_dump(surface) -> char*` — whole buffer as text. Feeds the
  scrollback sidecar.
- `rio_surface_new(..., working_dir)` — already exists; used at restore to
  start each shell in its saved directory.
- Titles already arrive via `RioEngine.onTitle` → `panelTitles`.

## Model additions (canario Swift)

- `Panel` / `TerminalItem` gain restore inputs, mirroring the existing
  UUID-keyed dicts (`panelTitles`, `panelWeights`):
  - `TerminalItem.panelWorkingDirs: [UUID: String]` — seed cwd per pane.
  - `TerminalItem.panelScrollback: [UUID: String]` — inert text to replay.
  - A restore initializer `TerminalItem(restoring: TerminalDTO)` that builds
    `columns` of fresh `Panel`s and fills the per-pane dicts by the new ids.

## Plumbing (two touch points in RioEngine)

1. **Restore → shell in the right dir.** In `PanelSession.startIfNeeded`,
   set `config.working_dir` from `terminal.panelWorkingDirs[panelID]` before
   `rio_surface_new`. After the surface starts, if
   `terminal.panelScrollback[panelID]` exists, `rio_surface_text` it back in
   (as inert bytes) then clear it so it replays once.
2. **Save → read live state.** `PanelSession.snapshot()` returns
   `(cwd: rio_surface_working_dir, scrollback: rio_surface_dump, cols, rows)`
   for the SessionStore to write.

## SessionStore (new file)

- Location: `~/Library/Application Support/canario/session.json` +
  `.../canario/scrollback/`.
- **Save triggers:** (a) `applicationWillTerminate` / scene → `.background`
  (authoritative), and (b) a **debounced** save (~2 s) on model changes, so a
  crash still leaves a recent session — same as browsers' "continuous"
  session write.
- **Restore trigger:** `AppModel.init()` — if `session.json` exists and
  decodes, build `items` from it instead of `createTerminal()`. On any
  decode error, fall back to a fresh terminal (never block startup).
- **Encoding:** `SidebarItem` isn't `Codable` (it wraps `@Observable`
  classes), so encode via the DTOs above, not the model types directly.

## Flow

```
launch ─► AppModel.init ─► SessionStore.load()
             ├─ ok  ─► rebuild items (folders/tabs/splits, cwd+scrollback seeded)
             └─ nil ─► createTerminal()   (first run / corrupt file)

pane appears ─► PanelSession.startIfNeeded
                  ├─ working_dir = saved cwd
                  └─ replay saved scrollback once (rio_surface_text)

model changes ─► debounce 2s ─► SessionStore.save(items, snapshots)
app quit ──────────────────────► SessionStore.save(...)   (authoritative)
```

## Edge cases / decisions

- **Fresh shells, not live processes** — a running `vim`/build does NOT
  survive; only cwd + scrollback text do. Set expectations in UI copy if
  needed. Live survival = Tier B.
- **cwd unknown** (shell without OSC 7) → `working_dir` NULL → shell starts
  in the default dir. Recommend shipping the OSC 7 hook in the default
  shell profile so restore lands in the right place.
- **Scrollback size** — cap the sidecar (e.g. last N KB) to bound disk.
- **Dead directories** — if a saved cwd no longer exists, `rio_surface_new`
  falls back to `$HOME`; don't fail the restore.
- **Never block startup** — any load/parse failure → fresh session.
- **Atomic writes** — write `session.json.tmp` then rename, so a crash
  mid-save can't corrupt the session.
