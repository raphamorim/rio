use super::{gl, Buffer, BufferKind, Texture, Shader};
use crate::comp::*;
use std::collections::HashMap;

pub struct Device {
    base_shader: BatchShader,
    subpx_shader: BatchShader,
    vertices: Buffer,
    indices: Buffer,
    textures: HashMap<TextureId, Texture>,
}

impl Device {
    pub fn new() -> Self {
        Self {
            base_shader: BatchShader::new(shaders::gl::BASE_VS, shaders::gl::BASE_FS),
            subpx_shader: BatchShader::new(shaders::gl::BASE_VS, shaders::gl::SUBPIXEL_FS),
            vertices: Buffer::new(BufferKind::Array),
            indices: Buffer::new(BufferKind::Element),
            textures: HashMap::default(),
        }
    }

    fn handle_texture_event(&mut self, event: &TextureEvent) {
        match event {
            TextureEvent::CreateTexture { id, format, width, height, data } => {
                let tex = Texture::new(*width as u32, *height as u32);
                if let Some(data) = data {
                    tex.update(data);
                }
                self.textures.insert(*id, tex);
            }
            TextureEvent::UpdateTexture { id, x, y, width, height, data } => {
                if let Some(tex) = self.textures.get(&id) {
                    tex.update(data);
                }

            }
            TextureEvent::DestroyTexture(id) => {
                self.textures.remove(id);
            }
        }
    }

    pub fn finish_composition(&mut self, compositor: &mut Compositor, display_list: &mut DisplayList) {
        compositor.finish(display_list, |e| self.handle_texture_event(&e));
    }

    pub fn render(&mut self, width: u32, height: u32, list: &DisplayList) {
        self.vertices.update(list.vertices());
        self.indices.update(list.indices());
        let view_proj = create_view_proj(width, height);
        self.vertices.bind();
        self.indices.bind();
        for command in list.commands() {
            match command {
                Command::BindPipeline(pipeline) => {
                    match pipeline {
                        Pipeline::Opaque => {
                            unsafe {
                                gl::DepthMask(1);
                                gl::Disable(gl::BLEND);
                                gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
                                gl::BlendEquation(gl::FUNC_ADD);
                            }
                            self.base_shader.activate();
                            self.base_shader.bind_attribs();
                            self.base_shader.set_view_proj(&view_proj);
                        }
                        Pipeline::Transparent => {
                            unsafe {
                                gl::DepthMask(0);
                                gl::Enable(gl::BLEND);
                                gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
                            }
                            self.base_shader.activate();
                            self.base_shader.bind_attribs();   
                            self.base_shader.set_view_proj(&view_proj);                         
                        }
                        Pipeline::Subpixel => {
                            unsafe {
                                gl::DepthMask(0);
                                gl::Enable(gl::BLEND);
                                gl::BlendFunc(gl::SRC1_COLOR, gl::ONE_MINUS_SRC1_COLOR);
                            }
                            self.subpx_shader.activate();
                            self.subpx_shader.bind_attribs();   
                            self.subpx_shader.set_view_proj(&view_proj);                         
                        }                        

                    }
                }
                Command::BindTexture(unit, id) => {
                    if let Some(tex) = self.textures.get(&id) {
                        tex.bind(*unit);
                    }
                }
                Command::Draw { start, count } => {
                    unsafe {
                        gl::DrawElements(
                            gl::TRIANGLES,
                            *count as _,
                            gl::UNSIGNED_INT,
                            (*start as usize * 4) as *const _,
                        );                          
                    }
                }
            }
        }
        unsafe {
            gl::DepthMask(1);
            gl::Disable(gl::BLEND);
        }        
    }
}

struct BatchShader {
    shader: Shader,
    v_pos: i32,
    v_color: i32,
    v_uv: i32,
    view_proj: i32,
    tex: i32,
    mask: i32,
}

impl BatchShader {
    pub fn new(vertex: &str, fragment: &str) -> Self {
        let shader = Shader::new(vertex, fragment);
        let v_pos = shader.attrib_location("v_pos");
        let v_color = shader.attrib_location("v_color");
        let v_uv = shader.attrib_location("v_uv");
        let view_proj = shader.uniform_location("view_proj");
        let tex = shader.uniform_location("tex");
        let mask = shader.uniform_location("mask");
        shader.activate();
        unsafe {
            gl::Uniform1i(tex, 1);
            gl::Uniform1i(mask, 0);
        }
        Self {
            shader,
            v_pos,
            v_color,
            v_uv,
            view_proj,
            tex,
            mask,
        }
    }

    pub fn activate(&self) {
        self.shader.activate();
    }

    pub fn set_view_proj(&self, view_proj: &[f32]) {
        unsafe {
            gl::UniformMatrix4fv(self.view_proj as _, 1, 0, view_proj.as_ptr());
        }
    }

    pub fn bind_attribs(&self) {
        unsafe {
            gl::VertexAttribPointer(
                self.v_pos as _,
                4,
                gl::FLOAT,
                0,
                28,
                core::ptr::null(),
            );
            gl::VertexAttribPointer(
                self.v_color as _,
                4,
                gl::UNSIGNED_BYTE,
                1,
                28,
                16usize as *const _,
            );
            gl::VertexAttribPointer(
                self.v_uv as _,
                2,
                gl::FLOAT,
                0,
                28,
                20usize as *const _,
            );
            gl::EnableVertexAttribArray(self.v_pos as _);
            gl::EnableVertexAttribArray(self.v_color as _);
            gl::EnableVertexAttribArray(self.v_uv as _);
        }
    }
}

fn create_view_proj(width: u32, height: u32) -> [f32; 16] {
    let r = width as f32;
    let l = 0.;
    let t = 0.;
    let b = height as f32;
    let n = 0.1;
    let f = 1024.;
    [
        2. / (r - l),
        0.,
        0.,
        (l + r) / (l - r),
        //
        0.,
        2. / (t - b),
        0.,
        (t + b) / (b - t),
        //
        0.,
        0.,
        2. / (f - n),
        -(f + n) / (f - n),
        //
        0.,
        0.,
        0.,
        1.,
    ]
}
