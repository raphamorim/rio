use glutin::{self, PossiblyCurrent};
use std::ffi::CStr;

include!(concat!(env!("OUT_DIR"), "/gl_bindings.rs"));

pub fn load(gl_context: &glutin::Context<PossiblyCurrent>) {
    load_with(|ptr| gl_context.get_proc_address(ptr) as *const _);
    let version = unsafe {
        let data = CStr::from_ptr(GetString(VERSION) as *const _).to_bytes().to_vec();
        String::from_utf8(data).unwrap()
    };
    let mut vao = 0;
    unsafe {
        GenVertexArrays(1, &mut vao);
        BindVertexArray(vao);
    }
    println!("OpenGL version {}", version);
}

