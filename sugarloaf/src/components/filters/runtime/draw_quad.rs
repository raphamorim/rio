// This file was originally taken from https://github.com/SnowflakePowered/librashader
// SnowflakePowered/librashader is licensed under MPL-2.0
// https://github.com/SnowflakePowered/librashader/blob/master/LICENSE.md

use crate::concat_arrays;
use librashader_runtime::quad::{QuadType, VertexInput};
use wgpu::util::{BufferInitDescriptor, DeviceExt};
use wgpu::{Buffer, Device, RenderPass};

const OFFSCREEN_VBO_DATA: [VertexInput; 4] = [
    VertexInput {
        position: [-1.0, -1.0, 0.0, 1.0],
        texcoord: [0.0, 0.0],
    },
    VertexInput {
        position: [-1.0, 1.0, 0.0, 1.0],
        texcoord: [0.0, 1.0],
    },
    VertexInput {
        position: [1.0, -1.0, 0.0, 1.0],
        texcoord: [1.0, 0.0],
    },
    VertexInput {
        position: [1.0, 1.0, 0.0, 1.0],
        texcoord: [1.0, 1.0],
    },
];

const FINAL_VBO_DATA: [VertexInput; 4] = [
    VertexInput {
        position: [0.0, 0.0, 0.0, 1.0],
        texcoord: [0.0, 0.0],
    },
    VertexInput {
        position: [0.0, 1.0, 0.0, 1.0],
        texcoord: [0.0, 1.0],
    },
    VertexInput {
        position: [1.0, 0.0, 0.0, 1.0],
        texcoord: [1.0, 0.0],
    },
    VertexInput {
        position: [1.0, 1.0, 0.0, 1.0],
        texcoord: [1.0, 1.0],
    },
];

static VBO_DATA: &[VertexInput; 8] = &concat_arrays!(OFFSCREEN_VBO_DATA, FINAL_VBO_DATA);

pub struct DrawQuad {
    buffer: Buffer,
}

impl DrawQuad {
    pub fn new(device: &Device) -> DrawQuad {
        let buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("librashader vbo"),
            contents: bytemuck::cast_slice(VBO_DATA),
            usage: wgpu::BufferUsages::VERTEX,
        });

        DrawQuad { buffer }
    }

    pub fn draw_quad<'a, 'b: 'a>(&'b self, cmd: &mut RenderPass<'a>, vbo: QuadType) {
        cmd.set_vertex_buffer(0, self.buffer.slice(0..));

        let offset = match vbo {
            QuadType::Offscreen => 0..4,
            QuadType::Final => 4..8,
        };

        cmd.draw(offset, 0..1)
    }
}
