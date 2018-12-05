use crate::{
    shader::{PushConstantInfo, Shader, Uniform, VertexInfo},
    Buffer, BufferPool,
};
use std::{
    iter::once,
    mem::{align_of, size_of},
    slice,
};

use crate::gfx_back::Backend;
use gfx_hal::{
    buffer::{IndexBufferView, Usage},
    command::{Primary, RenderPassInlineEncoder},
    IndexType,
};

pub struct Mesh<
    'a,
    Vertex: VertexInfo<Vertex>,
    Uniforms: Uniform,
    Index,
    Constants: PushConstantInfo,
    F: Fn() -> Constants,
> {
    shader: &'a Shader<'a, Vertex, Uniforms, Index, Constants>,
    vert_offset: u64,
    vertices: Vec<Vertex>,
    index_offset: u64,
    indices: Vec<Index>,
    buffer: Buffer<'a>,
    push_constant_fn: F,
}

impl<
        'a,
        Vertex: VertexInfo<Vertex>,
        Uniforms: Uniform,
        Index,
        Constants: PushConstantInfo,
        F: Fn() -> Constants,
    > Mesh<'a, Vertex, Uniforms, Index, Constants, F>
{
    pub(crate) fn create(
        shader: &'a Shader<'a, Vertex, Uniforms, Index, Constants>,
        pool: &'a BufferPool,
        vertices: Vec<Vertex>,
        indices: Vec<Index>,
        push_constant_fn: F,
    ) -> Mesh<'a, Vertex, Uniforms, Index, Constants, F> {
        let (vert_offset, index_offset) = {
            let vert_offset = 0;
            let mut index_offset = size_of::<Vertex>() * vertices.len();
            let padding = index_offset % align_of::<Index>();
            index_offset += padding;
            (vert_offset as u64, index_offset as u64)
        };
        let size = index_offset + ((size_of::<Index>() * indices.len()) as u64);
        let buffer = pool.create_buffer(size, Usage::VERTEX | Usage::INDEX, true);
        buffer.upload(&vertices, vert_offset);
        buffer.upload(&indices, index_offset);
        Mesh {
            shader,
            vert_offset,
            vertices,
            index_offset,
            indices,
            buffer,
            push_constant_fn,
        }
    }

    pub fn draw(&self, encoder: &mut RenderPassInlineEncoder<Backend, Primary>) {
        encoder.bind_vertex_buffers(0, once((self.buffer.buffer(), self.vert_offset)));
        encoder.bind_index_buffer(IndexBufferView {
            buffer: self.buffer.buffer(),
            offset: self.index_offset,
            index_type: IndexType::U16,
        });
        {
            let pc = (self.push_constant_fn)();
            let pc_ptr = &pc as *const Constants as *const u32;
            let slice =
                unsafe { slice::from_raw_parts(pc_ptr, size_of::<Constants>() / size_of::<u32>()) };
            encoder.push_graphics_constants(
                self.shader.pipe_layout(),
                self.shader.push_constant_stages,
                0,
                slice,
            );
        }
        encoder.draw_indexed(0..self.indices.len() as u32, 0, 0..1 as u32);
    }
}
