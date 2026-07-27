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

typedef struct {
  uint32_t tag;
  uint32_t codepoint;
  uint8_t function_key;
  uint8_t mods;
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
bool rio_surface_key(rio_surface_t *surface, rio_key_event_s event);
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
rio_selection_s rio_render_state_selection(const rio_render_state_t *state);

#ifdef __cplusplus
}
#endif

#endif
