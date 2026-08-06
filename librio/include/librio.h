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

rio_engine_t *rio_engine_new(const rio_runtime_config_s *config);
void rio_engine_free(rio_engine_t *engine);

rio_surface_t *rio_surface_new(rio_engine_t *engine,
                               const rio_surface_config_s *config);
void rio_surface_free(rio_surface_t *surface);
rio_surface_id_t rio_surface_id(const rio_surface_t *surface);

/* Input entry points are callable from any thread. */
void rio_surface_text(rio_surface_t *surface, const char *bytes, size_t len);
/* Returns true when the event was consumed and encoded to the PTY. */
bool rio_surface_key(rio_surface_t *surface, const rio_key_event_s *event);
/* Whether alt acts as meta, prefixing with ESC, instead of letting the text
 * the platform produced through. Defaults to true, as terminals do: it is the
 * difference between alt+d deleting a word and inserting a character. */
void rio_surface_set_alt_is_meta(rio_surface_t *surface, bool enabled);
void rio_surface_resize(rio_surface_t *surface, uint16_t cols, uint16_t rows,
                        uint16_t pixel_width, uint16_t pixel_height);
void rio_surface_scroll(rio_surface_t *surface, int32_t delta_lines);

void rio_surface_selection_begin(rio_surface_t *surface, int32_t viewport_line,
                                 uint16_t col, uint8_t kind);
void rio_surface_selection_update(rio_surface_t *surface, int32_t viewport_line,
                                  uint16_t col);
void rio_surface_selection_clear(rio_surface_t *surface);
/* Returns NULL when nothing is selected. Free with rio_text_free. */
char *rio_surface_selection_text(const rio_surface_t *surface);

/* Session persistence: current working directory (OSC 7), NULL if unknown.
 * Free with rio_text_free. */
char *rio_surface_working_dir(const rio_surface_t *surface);
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
rio_cell_s rio_render_state_cell(const rio_render_state_t *state, uint16_t line,
                                 uint16_t column);
rio_cursor_s rio_render_state_cursor(const rio_render_state_t *state);
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
