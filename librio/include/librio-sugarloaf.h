#ifndef LIBRIO_SUGARLOAF_H
#define LIBRIO_SUGARLOAF_H

#include "librio-vt.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct rio_renderer rio_renderer_t;

/* ns_view: pointer to an NSView. librio attaches and owns a CAMetalLayer
   on it. width/height are physical pixels; scale is backingScaleFactor. */
rio_renderer_t *rio_renderer_new(void *ns_view, float width, float height,
                                 float scale, float font_size);
void rio_renderer_free(rio_renderer_t *renderer);

/* Must be called from the same single thread as all rio_render_state_*
   calls, typically the host display-link tick. */
void rio_renderer_draw(rio_renderer_t *renderer, rio_render_state_t *state);
void rio_renderer_resize(rio_renderer_t *renderer, uint32_t pixel_width,
                         uint32_t pixel_height);
void rio_renderer_rescale(rio_renderer_t *renderer, float scale);
/* Logical points. Host derives cols/rows from view size. */
void rio_renderer_cell_size(const rio_renderer_t *renderer, float *out_width,
                            float *out_height);
float rio_renderer_padding(const rio_renderer_t *renderer);
void rio_renderer_set_font_size(rio_renderer_t *renderer, float size);
float rio_renderer_font_size(const rio_renderer_t *renderer);
/* NULL or empty clears the preedit overlay. */
void rio_renderer_set_preedit(rio_renderer_t *renderer, const char *utf8);

#ifdef __cplusplus
}
#endif

#endif
