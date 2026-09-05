#ifndef LIBRIO_H
#define LIBRIO_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct rio_engine rio_engine_t;
typedef struct rio_surface rio_surface_t;
typedef struct rio_render_state rio_render_state_t;

typedef size_t rio_surface_id_t;

#define RIO_ACTION_SET_TITLE 0u
#define RIO_ACTION_RING_BELL 1u
#define RIO_ACTION_CURSOR_BLINKING_CHANGE 2u
/* OSC 9;4 progress: data_a is the ConEmu state (0 remove, 1 set, 2 error,
   3 indeterminate, 4 paused), data_b the 0-100 value. */
#define RIO_ACTION_PROGRESS 3u

#define RIO_COLOR_NAMED 0u
#define RIO_COLOR_INDEXED 1u
#define RIO_COLOR_RGB 2u

#define RIO_KEY_CHAR 0u
#define RIO_KEY_ENTER 1u
#define RIO_KEY_TAB 2u
#define RIO_KEY_BACKSPACE 3u
#define RIO_KEY_ESCAPE 4u
#define RIO_KEY_UP 5u
#define RIO_KEY_DOWN 6u
#define RIO_KEY_LEFT 7u
#define RIO_KEY_RIGHT 8u
#define RIO_KEY_HOME 9u
#define RIO_KEY_END 10u
#define RIO_KEY_PAGE_UP 11u
#define RIO_KEY_PAGE_DOWN 12u
#define RIO_KEY_INSERT 13u
#define RIO_KEY_DELETE 14u
#define RIO_KEY_F 15u
/* No key: an event that carries only text, such as an input method commit. */
#define RIO_KEY_NONE 16u
/* Modifier keys as keys; reported only in kitty report-all mode. */
#define RIO_KEY_CAPS_LOCK 17u
#define RIO_KEY_SHIFT_LEFT 18u
#define RIO_KEY_SHIFT_RIGHT 19u
#define RIO_KEY_CONTROL_LEFT 20u
#define RIO_KEY_CONTROL_RIGHT 21u
#define RIO_KEY_ALT_LEFT 22u
#define RIO_KEY_ALT_RIGHT 23u
#define RIO_KEY_SUPER_LEFT 24u
#define RIO_KEY_SUPER_RIGHT 25u

#define RIO_KEY_ACTION_PRESS 0u
#define RIO_KEY_ACTION_REPEAT 1u
#define RIO_KEY_ACTION_RELEASE 2u

#define RIO_SELECTION_SIMPLE 0u
#define RIO_SELECTION_WORD 1u
#define RIO_SELECTION_LINE 2u
#define RIO_SELECTION_BLOCK 3u

#define RIO_MOD_SHIFT (1u << 0)
#define RIO_MOD_CTRL (1u << 1)
#define RIO_MOD_ALT (1u << 2)
#define RIO_MOD_SUPER (1u << 3)

typedef struct {
  uint32_t tag;
  /* Valid only for the duration of the callback. */
  const char *title;
  const char *subtitle;
  /* Numeric payload; meaning depends on tag (see RIO_ACTION_*). */
  uint32_t data_a;
  uint32_t data_b;
} rio_action_s;

typedef struct {
  void *userdata;
  /* May fire on librio's PTY IO thread. Callbacks must only flag or
     schedule work; calling back into rio_* from inside a callback is
     undefined behavior. */
  void (*wakeup_cb)(void *userdata, rio_surface_id_t surface);
  void (*action_cb)(void *userdata, rio_surface_id_t surface, rio_action_s action);
  void (*clipboard_write_cb)(void *userdata, rio_surface_id_t surface, uint8_t kind,
                             const char *utf8);
  void (*close_surface_cb)(void *userdata, rio_surface_id_t surface);
} rio_runtime_config_s;

typedef struct {
  /* NULL means $SHELL, falling back to /bin/sh. */
  const char *shell;
  const char *working_dir;
  uint16_t cols;
  uint16_t rows;
  uint16_t pixel_width;
  uint16_t pixel_height;
  size_t scrollback;
  /* Arguments for the program, after argv[0]. NULL/0 means none. These reach
   * the child through execvp as separate argv entries, so a host with a
   * command to run can spawn it instead of typing it into the terminal:
   * nothing here is ever read by a shell's line editor. */
  const char *const *args;
  size_t args_len;
} rio_surface_config_s;

typedef struct {
  /* Original form: RIO_COLOR_NAMED / _INDEXED / _RGB. `value` holds the
     named-color id or palette index for the first two. */
  uint8_t kind;
  uint16_t value;
  /* Always the resolved RGB, regardless of `kind`, so a CPU renderer can
     read r/g/b directly without owning a palette. */
  uint8_t r;
  uint8_t g;
  uint8_t b;
} rio_color_s;

typedef struct {
  uint32_t codepoint;
  rio_color_s fg;
  rio_color_s bg;
  uint16_t style_flags;
} rio_cell_s;

/* Bit assignments for rio_cell_s.style_flags. Bits 6-10 are the
 * underline kind; at most one is set. At most one blink bit is set.
 * Bit 15 is RIO_CELL_HAS_CLUSTER, not a style. */
#define RIO_STYLE_INVERSE (1u << 0)
#define RIO_STYLE_BOLD (1u << 1)
#define RIO_STYLE_ITALIC (1u << 2)
#define RIO_STYLE_DIM (1u << 3)
#define RIO_STYLE_HIDDEN (1u << 4)
#define RIO_STYLE_STRIKEOUT (1u << 5)
#define RIO_STYLE_UNDERLINE (1u << 6)
#define RIO_STYLE_DOUBLE_UNDERLINE (1u << 7)
#define RIO_STYLE_UNDERCURL (1u << 8)
#define RIO_STYLE_DOTTED_UNDERLINE (1u << 9)
#define RIO_STYLE_DASHED_UNDERLINE (1u << 10)
#define RIO_STYLE_SLOW_BLINK (1u << 11)
#define RIO_STYLE_RAPID_BLINK (1u << 12)

typedef struct {
  uint8_t r;
  uint8_t g;
  uint8_t b;
} rio_rgb_s;

/* A color scheme, limited to what the engine itself resolves: the 16 ANSI
 * slots plus the named defaults. Selection/cursor rendering colors stay
 * host-side; `cursor` here covers cells that reference the cursor color. */
typedef struct {
  /* ANSI 0-7 normal, 8-15 bright. */
  rio_rgb_s ansi[16];
  rio_rgb_s foreground;
  rio_rgb_s background;
  rio_rgb_s cursor;
} rio_colors_s;

typedef struct {
  uint16_t line;
  uint16_t column;
} rio_cursor_s;

typedef struct {
  bool active;
  uint16_t start_line;
  uint16_t start_col;
  uint16_t end_line;
  uint16_t end_col;
  bool is_block;
} rio_selection_s;

/* A key event as the platform delivered it.
 *
 * The host reports what happened and librio decides what the terminal
 * receives, because that depends on state the host does not track:
 * application cursor mode, the kitty keyboard flags, and modifyOtherKeys.
 */
typedef struct {
  /* RIO_KEY_ACTION_*. Releases are dropped unless the program running in the
   * terminal asked for key event types. */
  uint32_t action;
  /* RIO_KEY_*, naming the key with no shift applied: shift+a is
   * RIO_KEY_CHAR with codepoint 'a', and the "A" belongs in text. */
  uint32_t tag;
  uint32_t codepoint;
  /* 1 to 12 when tag is RIO_KEY_F. */
  uint8_t function_key;
  /* Every modifier held, as RIO_MOD_*. */
  uint8_t mods;
  /* Modifiers the platform already spent producing text, so they are not
   * encoded twice. Zero when the platform cannot say. */
  uint8_t consumed_mods;
  /* Whether an input method owns the key right now. Nothing is encoded; the
   * text arrives when composition commits. */
  bool composing;
  /* What the platform produced for this key, UTF-8, after any dead key or
   * input method composition. May be NULL. */
  const char *text;
  size_t text_len;
} rio_key_event_s;

/* Hosts on Linux should ignore SIGPIPE: internal wake pipes written from
 * signal handlers expect EPIPE. On macOS/BSD the sockets opt out per-fd. */
rio_engine_t *rio_engine_new(const rio_runtime_config_s *config);
void rio_engine_free(rio_engine_t *engine);

/* Replace the palette used to resolve named/indexed cell colors, process-wide.
 * NULL restores Rio's default theme. Dim variants derive from the new palette.
 * The host owns redraw: repaint every surface after this (cells re-resolve on
 * the next rio_render_state_cell read; no re-snapshot needed). */
void rio_set_colors(const rio_colors_s *colors);

rio_surface_t *rio_surface_new(rio_engine_t *engine,
                               const rio_surface_config_s *config);
void rio_surface_free(rio_surface_t *surface);
rio_surface_id_t rio_surface_id(const rio_surface_t *surface);

/* Input entry points are callable from any thread. */
void rio_surface_text(rio_surface_t *surface, const char *bytes, size_t len);
/* Paste `bytes` (UTF-8), bracketed when the program has enabled mode 2004. */
void rio_surface_paste(rio_surface_t *surface, const char *bytes, size_t len);
/* Terminal modes for input decisions the host makes on its own: bit 0 mouse
 * reporting, bit 1 application cursor keys, bit 2 alternate screen, bit 3
 * bracketed paste. */
uint32_t rio_surface_mode_bits(const rio_surface_t *surface);
/* Returns true when the event was consumed and encoded to the PTY. */
bool rio_surface_key(rio_surface_t *surface, const rio_key_event_s *event);
/* Whether alt acts as meta, prefixing with ESC, instead of letting the text
 * the platform produced through. Defaults to true, as terminals do: it is the
 * difference between alt+d deleting a word and inserting a character. */
void rio_surface_set_alt_is_meta(rio_surface_t *surface, bool enabled);
/* Grapheme cluster processing (DEC private mode 2027) as the default for
 * cell layout. On by default; embedders whose renderers assume legacy
 * wcwidth layout can turn it off. A program's DECSET/DECRST still wins at
 * runtime, and RIS restores this configured default. */
void rio_surface_set_grapheme_clustering(rio_surface_t *surface, bool enabled);
void rio_surface_resize(rio_surface_t *surface, uint16_t cols, uint16_t rows,
                        uint16_t pixel_width, uint16_t pixel_height);
void rio_surface_scroll(rio_surface_t *surface, int32_t delta_lines);
/* A wheel scroll, dispatched by librio: a mouse report when the program
   asked for mouse events, cursor keys on the alternate screen with
   alternate-scroll on, otherwise the scrollback view. `lines` is positive
   for scrolling up; `col`/`row` are the cell under the pointer. Returns
   true when the program consumed it. */
bool rio_surface_scroll_wheel(rio_surface_t *surface, int32_t lines,
                              uint16_t col, uint16_t row, uint8_t mods);

/* Report a mouse button press/release to the program when it asked for mouse
   events (DEC 1000/1002/1003, SGR/UTF8/X10). `button` is 0=left, 1=middle,
   2=right; `mods` uses the same bits as rio_surface_scroll_wheel. Returns
   true when a report reached the program (the host should then skip its own
   selection); false when nothing grabs the mouse, or shift bypasses to a
   local selection. */
bool rio_surface_mouse_button(rio_surface_t *surface, uint16_t col, uint16_t row,
                              uint8_t button, bool pressed, uint8_t mods);

/* Report pointer motion for button-event (1002, a button held) / any-event
   (1003, bare motion) modes. `button` is 0/1/2 for the button held during a
   drag, or 3 for none. Returns true when a report was emitted. */
bool rio_surface_mouse_motion(rio_surface_t *surface, uint16_t col, uint16_t row,
                              uint8_t button, uint8_t mods);

/* `side_right` is true when the pointer sits in the right half of the cell.
   It decides whether that cell falls inside the selection, so a drag can
   reach the cells at both ends. */
void rio_surface_selection_begin(rio_surface_t *surface, int32_t viewport_line,
                                 uint16_t col, uint8_t kind, bool side_right);
void rio_surface_selection_update(rio_surface_t *surface, int32_t viewport_line,
                                  uint16_t col, bool side_right);
void rio_surface_selection_clear(rio_surface_t *surface);
/* Returns NULL when nothing is selected. Free with rio_text_free. */
char *rio_surface_selection_text(const rio_surface_t *surface);

/* Session persistence: current working directory (OSC 7), NULL if unknown.
 * Free with rio_text_free. */
char *rio_surface_working_dir(const rio_surface_t *surface);
/* Foreground process name (the running program). Free with rio_text_free. */
char *rio_surface_foreground_process_name(const rio_surface_t *surface);
/* Inject bytes into the terminal DISPLAY (not the PTY input), for replaying
 * persisted scrollback on restore. */
void rio_surface_inject_output(rio_surface_t *surface, const char *bytes,
                               size_t len);
/* Whole buffer (scrollback + screen) as UTF-8 text, for persist/replay.
 * Free with rio_text_free. */
char *rio_surface_dump(const rio_surface_t *surface);

void rio_text_free(char *text);

/* Render-state calls must all come from one thread. */
rio_render_state_t *rio_render_state_new(const rio_surface_t *surface);
void rio_render_state_free(rio_render_state_t *state);
void rio_render_state_update(rio_render_state_t *state);
uint16_t rio_render_state_lines(const rio_render_state_t *state);
uint16_t rio_render_state_columns(const rio_render_state_t *state);
bool rio_render_state_row_dirty(const rio_render_state_t *state, uint16_t line);
void rio_render_state_reset_dirty(rio_render_state_t *state);
/* Set in rio_cell_s.style_flags when the cell carries attached cluster
 * codepoints (combining marks, or a DEC-2027 grapheme cluster) beyond
 * `codepoint`. Fetch the full text with rio_render_state_cell_cluster
 * and draw that instead of the base char. Style flags proper occupy
 * bits 0-12 (RIO_STYLE_*); this is bit 15. */
#define RIO_CELL_HAS_CLUSTER (1u << 15)

/* Write the full text of a cell (base codepoint plus attached cluster
 * codepoints) as UTF-32 into `out` (capacity `cap` code units).
 * Returns the total codepoint count, which may exceed `cap` (call
 * again with a larger buffer); 0 for plain cells. */
size_t rio_render_state_cell_cluster(const rio_render_state_t *state,
                                     uint16_t line, uint16_t column,
                                     uint32_t *out, size_t cap);

/* Measure the first grapheme cluster in a UTF-32 buffer: returns the
 * number of codepoints the cluster spans and writes its terminal cell
 * width to `out_width` when non-null (2 for wide, 1 for narrow, 0 for
 * bare zero-width marks); 0 codepoints for a null or empty buffer. Same segmentation and width rules as printing
 * under grapheme clustering (DEC mode 2027). The buffer must contain
 * a complete first cluster or the logical end of the text. Values
 * that are not Unicode scalars measure as one single-width codepoint
 * when first and terminate the cluster when later. */
size_t rio_cluster_width(const uint32_t *codepoints, size_t len,
                         uint8_t *out_width);

rio_cell_s rio_render_state_cell(const rio_render_state_t *state, uint16_t line,
                                 uint16_t column);
rio_cursor_s rio_render_state_cursor(const rio_render_state_t *state);

/* This frame's dynamic (OSC 10/11/12) default colors. `which`: 0 =
 * foreground, 1 = background, 2 = cursor. Writes the resolved RGB into
 * `out` and returns true when the program has set that color via OSC;
 * returns false and leaves `out` untouched when unset (including after an
 * OSC 110/111/112 reset), so the host falls back to its own scheme. */
bool rio_render_state_dynamic_color(const rio_render_state_t *state,
                                    uint8_t which, rio_rgb_s *out);

size_t rio_render_state_display_offset(const rio_render_state_t *state);
bool rio_render_state_alt_screen(const rio_render_state_t *state);

const uint8_t *rio_symbols_nerd_font(size_t *len);

/* A kitty image placement resolved to pixels. x/y are relative to the
 * grid origin (add your padding); src_* is the normalized source
 * rectangle (origin + size) to crop from the image. */
typedef struct {
  uint32_t image_id;
  int32_t z_index;
  float x;
  float y;
  float width;
  float height;
  float src_x;
  float src_y;
  float src_w;
  float src_h;
} rio_kitty_placement_s;

size_t rio_render_state_kitty_count(const rio_render_state_t *state);
bool rio_render_state_kitty_placement(const rio_render_state_t *state,
                                      size_t index, float cell_width,
                                      float cell_height,
                                      rio_kitty_placement_s *out);
bool rio_render_state_kitty_image_info(const rio_render_state_t *state,
                                       uint32_t image_id, uint32_t *out_width,
                                       uint32_t *out_height,
                                       uint64_t *out_stamp);
size_t rio_render_state_kitty_image_rgba(const rio_render_state_t *state,
                                         uint32_t image_id, uint8_t *buf,
                                         size_t cap);

typedef struct {
  double x;
  double y;
  double width;
  double height;
} rio_glyph_box_s;

bool rio_nerd_constrain(uint32_t codepoint, rio_glyph_box_s glyph,
                        double cell_width, double cell_height,
                        double icon_height_single, uint8_t constraint_width,
                        rio_glyph_box_s *out);
rio_selection_s rio_render_state_selection(const rio_render_state_t *state);

#ifdef __cplusplus
}
#endif

#endif
