#version 450

// Fullscreen triangle, ported from `grid_bg_vertex` in
// `sugarloaf/src/grid/shaders/grid.metal`. One triangle that covers
// the viewport (clipped at the edges); fragment shader does the work.
//
//   gl_VertexIndex 0 → (-1, -3)
//   gl_VertexIndex 1 → (-1,  1)
//   gl_VertexIndex 2 → ( 3,  1)
//
// Drawn with `vkCmdDraw(cmd, 3, 1, 0, 0)` and TRIANGLE_LIST topology.
// No vertex buffers — `gl_VertexIndex` drives everything.

void main() {
    float x = (gl_VertexIndex == 2) ?  3.0 : -1.0;
    float y = (gl_VertexIndex == 0) ? -3.0 :  1.0;
    gl_Position = vec4(x, y, 0.0, 1.0);
}
