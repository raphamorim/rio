#version 450

// Bootstrap pipeline: generates a centered rect via gl_VertexIndex with
// TRIANGLE_STRIP topology. No vertex buffers; no descriptor sets. This
// shader is temporary scaffolding to prove the Vulkan pipeline plumbing
// works end-to-end — the real renderer.vert.glsl ports `renderer.metal`.
void main() {
    // index bit 0 -> x in {0, 1}; bit 1 -> y in {0, 1}
    //   0: (-0.5, -0.5)  2: (-0.5, 0.5)
    //   1: ( 0.5, -0.5)  3: ( 0.5, 0.5)
    // Strip order (0,1,2,3) -> two triangles forming a centered rect.
    float x = float(gl_VertexIndex & 1) - 0.5;
    float y = float((gl_VertexIndex >> 1) & 1) - 0.5;
    gl_Position = vec4(x, y, 0.0, 1.0);
}
