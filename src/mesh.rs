use std::{
	borrow::BorrowMut,
	iter::once,
	mem::{
		align_of,
		size_of,
	},
	slice,
};

use gfx_hal::{
	buffer::{
		IndexBufferView,
		Usage,
	},
	command::RenderSubpassCommon,
	pso::Descriptor,
};

use crate::{
	gfx_back::Backend,
	shader::{
		IndexType,
		PushConstantInfo,
		Shader,
		UniformInfo,
		VertexInfo,
	},
	Buffer,
	BufferPool,
	DescriptorPool,
};

pub struct Mesh<
	'a,
	Vertex: VertexInfo,
	Uniforms: UniformInfo,
	Index: IndexType,
	Constants: PushConstantInfo,
> {
	shader: &'a Shader<'a, Vertex, Uniforms, Index, Constants>,
	vert_offset: u64,
	vertices: Vec<Vertex>,
	index_offset: u64,
	indices: Vec<Index>,
	buffer: Buffer<'a>,
	descriptor_pool: DescriptorPool<'a, Vertex, Uniforms, Index, Constants>,
}

impl<
		'a,
		Vertex: VertexInfo,
		Uniforms: UniformInfo,
		Index: IndexType,
		Constants: PushConstantInfo,
	> Mesh<'a, Vertex, Uniforms, Index, Constants>
{
	pub(crate) fn create(
		shader: &'a Shader<'a, Vertex, Uniforms, Index, Constants>,
		pool: &'a BufferPool,
		vertices: Vec<Vertex>,
		indices: Vec<Index>,
		descriptors: &[Vec<Descriptor<Backend>>],
	) -> Mesh<'a,Vertex, Uniforms, Index, Constants> {
		let (vert_offset, index_offset) = {
			let vert_offset = 0;
			let mut index_offset = size_of::<Vertex>() * vertices.len();
			let padding = index_offset % align_of::<Index>();
			index_offset += padding;
			(vert_offset as u64, index_offset as u64)
		};
		let size = index_offset + ((size_of::<Index>() * indices.len()) as u64);
		let mut buffer = pool.create_buffer(size, Usage::VERTEX | Usage::INDEX, true);
		buffer.upload(&vertices, vert_offset);
		buffer.upload(&indices, index_offset);
		let descriptor_pool = shader.create_descriptors(descriptors.len());
		descriptors
			.iter()
			.enumerate()
			.for_each(|(idx, desc)| descriptor_pool.write(idx, desc));
		Mesh {
			shader,
			vert_offset,
			vertices,
			index_offset,
			indices,
			buffer,
			descriptor_pool,
		}
	}

	pub(crate) fn draw(
		&self,
		encoder: &mut RenderSubpassCommon<
			Backend,
			impl BorrowMut<<Backend as gfx_hal::Backend>::CommandBuffer>,
		>,
		descriptor_idx: usize,
		push_constants: Constants,
	) {
		unsafe {
			encoder.bind_vertex_buffers(0, once((self.buffer.buffer(), self.vert_offset)));

			encoder.bind_index_buffer(IndexBufferView {
				buffer: self.buffer.buffer(),
				offset: self.index_offset,
				index_type: Index::HAL,
			});
			encoder.bind_graphics_descriptor_sets(
				self.shader.pipe_layout(),
				0,
				once(self.descriptor_pool.descriptor_set(descriptor_idx)),
				&[],
			);
			if Constants::SIZE != 0 {
				let pc_ptr = &push_constants as *const Constants as *const u32;
				let slice =
					slice::from_raw_parts(pc_ptr, size_of::<Constants>() / size_of::<u32>());
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
}
