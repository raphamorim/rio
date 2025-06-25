// Single Point of Definition (SPD) system for WGSL shaders
// Core shader system where all rendering uses the same vertex structure

/// Core vertex structure used by all rendering components
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct UnifiedVertex {
    /// Position (x, y, z, render_mode)
    /// render_mode: 0=quad, 1=text, 2=image
    pub position: [f32; 4],
    /// Color (r, g, b, a)
    pub color: [f32; 4],
    /// UV coordinates and layer info (u, v, layer, mask_layer)
    pub uv_layer: [f32; 4],
    /// Size and border info (width, height, border_width, border_radius)
    pub size_border: [f32; 4],
    /// Extended parameters (shadow_blur, shadow_offset_x, shadow_offset_y, extra)
    pub extended: [f32; 4],
}

/// Get the core shader source for the current context
pub fn get_core_shader(supports_f16: bool) -> &'static str {
    if supports_f16 {
        include_str!("core_f16.wgsl")
    } else {
        include_str!("core.wgsl")
    }
}

/// Filter shaders (these are still separate as they have different purposes)
pub mod filters {
    pub const BLIT_SHADER: &str = include_str!("blit.wgsl");
}

/// Test utilities
pub mod test_utils {
    pub fn copy_texture_to_buffer_shader(type_name: &str) -> String {
        let template = include_str!("copy_texture_to_buffer.wgsl");
        template.replace("{{type}}", type_name)
    }
}
