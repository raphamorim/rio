use std::ffi::CString;

pub mod gl;

mod device;

pub use device::Device;

use gl::types::*;

#[derive(Copy, Clone)]
pub enum BufferKind {
    Array,
    Element,
    Uniform,
}

impl BufferKind {
    fn to_gl(self) -> GLenum {
        match self {
            Self::Array => gl::ARRAY_BUFFER,
            Self::Element => gl::ELEMENT_ARRAY_BUFFER,
            Self::Uniform => gl::UNIFORM_BUFFER,            
        }
    }
}

pub struct Buffer {
    id: GLuint,
    kind: BufferKind,
    capacity: usize,
}

impl Buffer {
    pub fn new(kind: BufferKind) -> Self {
        unsafe {
            let mut id = 0;
            gl::GenBuffers(1, &mut id);
            Self {
                id,
                kind,
                capacity: 0,
            }
        }
    }

    pub fn update<T: Copy>(&mut self, data: &[T]) {
        let size = data.len() * core::mem::size_of::<T>();
        if size > isize::MAX as usize {
            return;
        }
        let ty = self.kind.to_gl();
        unsafe {
            gl::BindBuffer(ty, self.id);
            if size > self.capacity {
                gl::BufferData(ty, size as isize, data.as_ptr() as *const _, gl::DYNAMIC_DRAW);
                self.capacity = size;
            } else {
                gl::BufferSubData(ty, 0, size as isize, data.as_ptr() as *const _);
            }    
        }
    }

    pub fn update_slice<T: Copy>(&mut self, offset: usize, data: &[T]) {
        let size = data.len() * core::mem::size_of::<T>();
        let ty = self.kind.to_gl();
        if let Some(end) = offset.checked_add(size) {
            if end <= self.capacity && end < isize::MAX as usize {
                unsafe {
                    gl::BindBuffer(ty, self.id);
                    gl::BufferSubData(ty, offset as _, size as isize, data.as_ptr() as *const _);
                }
            }
        }
    }

    pub fn bind(&self) {
        unsafe { 
            gl::BindBuffer(self.kind.to_gl(), self.id);
        }
    }    
}

impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteBuffers(1, &self.id);
        }
    }
}

pub struct Texture {
    pub id: GLuint,
    pub width: u32,
    pub height: u32,
}

impl Texture {
    pub fn new(width: u32, height: u32) -> Self {
        let mut id = 0;
        unsafe {
            gl::GenTextures(1, &mut id);
            gl::BindTexture(gl::TEXTURE_2D, id);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as _);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as _);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as _);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as _);
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RGBA as _,
                width as i32,
                height as i32,
                0,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                core::ptr::null(),
            );
        }
        Self { id, width, height }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn update<T: Copy>(&self, data: &[T]) {
        unsafe {
            gl::BindTexture(gl::TEXTURE_2D, self.id);
            gl::TexSubImage2D(gl::TEXTURE_2D, 0, 0, 0, self.width as i32, self.height as i32, gl::RGBA, gl::UNSIGNED_BYTE, data.as_ptr() as *const _);
            // gl::TexImage2D(
            //     gl::TEXTURE_2D,
            //     0,
            //     gl::RGBA as _,
            //     self.width as i32,
            //     self.height as i32,
            //     0,
            //     gl::RGBA,
            //     gl::UNSIGNED_BYTE,
            //     data.as_ptr() as *const _,
            // );
        }
    }

    pub fn bind(&self, unit: u32) {
        let val = gl::TEXTURE0 + unit;
        unsafe {
            gl::ActiveTexture(val);
            gl::BindTexture(gl::TEXTURE_2D, self.id);
            gl::ActiveTexture(gl::TEXTURE0);
        }
    }
}

impl Drop for Texture {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteTextures(1, &self.id);
        }
    }
}

pub struct Shader {
    pub id: GLuint,
}

unsafe fn compile_shader(kind: GLenum, source: &str) -> GLuint {
    let src = CString::new(source).unwrap();
    let s = gl::CreateShader(kind);
    gl::ShaderSource(
        s,
        1,
        [src.as_ptr() as *const _].as_ptr(),
        std::ptr::null(),
    );
    gl::CompileShader(s);
    let mut status = 0;
    gl::GetShaderiv(s, gl::COMPILE_STATUS, &mut status);
    if status == 0 {
        let mut info: Vec<u8> = Vec::new();
        let mut size = 0;
        gl::GetShaderiv(s, gl::INFO_LOG_LENGTH, &mut size);
        info.resize(size as usize, 0);
        let mut ignored = 0;
        gl::GetShaderInfoLog(s, size as _, &mut ignored, info.as_mut_ptr() as *mut _);
        if let Ok(s) = core::str::from_utf8(&info) {
            println!("{}", s);
        }
    }
    s
}

impl Shader {
    pub fn new(vertex: &str, fragment: &str) -> Self {
        let x = gl::ONE_MINUS_SRC1_COLOR;
        unsafe {
            let id = gl::CreateProgram();
            let vs = compile_shader(gl::VERTEX_SHADER, vertex);
            let fs = compile_shader(gl::FRAGMENT_SHADER, fragment);
            gl::AttachShader(id, vs);
            gl::AttachShader(id, fs);
            gl::LinkProgram(id);
            let mut status = 0;
            gl::GetProgramiv(id, gl::LINK_STATUS, &mut status);
            if status == 0 {
                let mut info: Vec<u8> = Vec::new();
                let mut size = 0;
                gl::GetProgramiv(id, gl::INFO_LOG_LENGTH, &mut size);
                info.resize(size as usize, 0);
                let mut ignored = 0;
                gl::GetProgramInfoLog(id, size as _, &mut ignored, info.as_mut_ptr() as *mut _);
                if let Ok(s) = core::str::from_utf8(&info) {
                    println!("{}", s);
                }
            }
            gl::DeleteShader(vs);
            gl::DeleteShader(fs);
            Self { id }
        }
    }

    pub fn attrib_location(&self, name: &str) -> i32 {
        let name = CString::new(name).unwrap();
        unsafe { gl::GetAttribLocation(self.id, name.as_ptr() as *const _) }
    }

    pub fn uniform_location(&self, name: &str) -> i32 {
        let name = CString::new(name).unwrap();
        unsafe { gl::GetUniformLocation(self.id, name.as_ptr() as *const _) }
    }

    pub fn activate(&self) {
        unsafe {
            gl::UseProgram(self.id);
        }
    }
}

impl Drop for Shader {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteProgram(self.id);
        }
    }
}

