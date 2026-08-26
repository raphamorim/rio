#include "../include/librio.h"

#include <stdatomic.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

static atomic_int wakeups = 0;

static void on_wakeup(void *userdata, rio_surface_id_t surface) {
  (void)userdata;
  (void)surface;
  atomic_fetch_add(&wakeups, 1);
}

static int wait_for(rio_render_state_t *state, const char *needle) {
  for (int attempt = 0; attempt < 200; attempt++) {
    rio_render_state_update(state);
    uint16_t lines = rio_render_state_lines(state);
    uint16_t cols = rio_render_state_columns(state);
    for (uint16_t line = 0; line < lines; line++) {
      char text[512] = {0};
      uint16_t limit = cols < 511 ? cols : 511;
      for (uint16_t col = 0; col < limit; col++) {
        rio_cell_s cell = rio_render_state_cell(state, line, col);
        text[col] = (cell.codepoint >= 32 && cell.codepoint < 127)
                        ? (char)cell.codepoint
                        : ' ';
      }
      if (strstr(text, needle)) {
        printf("row %u: %s\n", line, text);
        return 1;
      }
    }
    usleep(25 * 1000);
  }
  return 0;
}

int main(void) {
  rio_runtime_config_s config = {0};
  config.wakeup_cb = on_wakeup;

  rio_engine_t *engine = rio_engine_new(&config);
  if (!engine) {
    fprintf(stderr, "engine creation failed\n");
    return 1;
  }

  rio_surface_config_s surface_config = {0};
  surface_config.cols = 80;
  surface_config.rows = 24;
  surface_config.pixel_width = 720;
  surface_config.pixel_height = 432;
  surface_config.scrollback = 1000;

  rio_surface_t *surface = rio_surface_new(engine, &surface_config);
  if (!surface) {
    fprintf(stderr, "surface creation failed\n");
    return 1;
  }

  usleep(400 * 1000);
  const char *cmd = "printf '%s%s\\n' li brio-cgate\r";
  rio_surface_text(surface, cmd, strlen(cmd));

  rio_render_state_t *state = rio_render_state_new(surface);
  int found = wait_for(state, "librio-cgate");

  /* Paste into a cat child: no line editor in the way, so the check does
   * not depend on the host's shell. The tty's own echo puts it on screen. */
  rio_surface_config_s paste_config = surface_config;
  paste_config.shell = "/bin/cat";
  rio_surface_t *paste_surface = rio_surface_new(engine, &paste_config);
  rio_render_state_t *paste_state =
      paste_surface ? rio_render_state_new(paste_surface) : NULL;
  int found_paste = 0;
  if (paste_surface) {
    const char *pasted = "librio-pgate";
    rio_surface_paste(paste_surface, pasted, strlen(pasted));
    found_paste = wait_for(paste_state, "librio-pgate");
  }

  rio_cursor_s cursor = rio_render_state_cursor(state);
  printf("wakeups=%d cursor=%u,%u found=%d found_paste=%d\n",
         atomic_load(&wakeups), cursor.line, cursor.column, found,
         found_paste);

  if (paste_state) {
    rio_render_state_free(paste_state);
  }
  if (paste_surface) {
    rio_surface_free(paste_surface);
  }
  rio_render_state_free(state);
  rio_surface_free(surface);
  rio_engine_free(engine);

  if (!found || !found_paste || atomic_load(&wakeups) == 0) {
    fprintf(stderr, "gate failed\n");
    return 1;
  }
  printf("librio c gate passed\n");
  return 0;
}
