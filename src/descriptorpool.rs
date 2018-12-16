use std::{
	iter::once,
	mem::MaybeUninit,
};

use gfx_hal::{
	pso::{
		Descriptor,
		DescriptorPool as HALDescriptorPool,
		DescriptorRangeDesc,
		DescriptorSetWrite,
	},
	Device,
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
	util::TakeExt,
};

pub struct DescriptorPool<
	'a,
	Vertex: VertexInfo<Vertex>,
	Uniforms: UniformInfo,
	Index: IndexType,
	Constants: PushConstantInfo,
> {
	shader: &'a Shader<'a, Vertex, Uniforms, Index, Constants>,
	descriptor_pool: MaybeUninit<<Backend as gfx_hal::Backend>::DescriptorPool>,
	descriptor_sets: Vec<<Backend as gfx_hal::Backend>::DescriptorSet>,
}

impl<
		'a,
		Vertex: VertexInfo<Vertex>,
		Uniforms: UniformInfo,
		Index: IndexType,
		Constants: PushConstantInfo,
	> DescriptorPool<'a, Vertex, Uniforms, Index, Constants>
{
	pub(crate) fn create(
		shader: &'a Shader<'a, Vertex, Uniforms, Index, Constants>,
		pool_count: usize,
	) -> DescriptorPool<'a, Vertex, Uniforms, Index, Constants> {
		println!("Creating Descriptors");
		let device = &shader.data.device;
		let desc_layout = shader.desc_layout();
		let mut descriptor_pool = {
			let descriptors = shader
				.layout_bindings()
				.iter()
				.map(|uniform| DescriptorRangeDesc {
					ty: uniform.ty,
					count: pool_count,
				});
			device
				.create_descriptor_pool(pool_count, descriptors)
				.unwrap()
		};

		let descriptor_sets = {
			let mut buf = Vec::with_capacity(pool_count);
			descriptor_pool
				.allocate_sets(vec![desc_layout; pool_count], &mut buf)
				.unwrap();
			buf
		};

		DescriptorPool {
			shader,
			descriptor_pool: MaybeUninit::new(descriptor_pool),
			descriptor_sets,
		}
	}

	pub fn write(&self, set: usize, descriptor: &[Descriptor<Backend>]) {
		let device = &self.shader.data.device;
		let writes = descriptor
			.iter()
			.enumerate()
			.map(|(binding, desc)| DescriptorSetWrite {
				set: self.descriptor_set(set),
				binding: binding as u32,
				array_offset: 0,
				descriptors: once(desc),
			});
		device.write_descriptor_sets(writes)
	}

	pub fn descriptor_set(&self, idx: usize) -> &<Backend as gfx_hal::Backend>::DescriptorSet {
		&self.descriptor_sets[idx]
	}
}

impl<
		'a,
		Vertex: VertexInfo<Vertex>,
		Uniforms: UniformInfo,
		Index: IndexType,
		Constants: PushConstantInfo,
	> Drop for DescriptorPool<'a, Vertex, Uniforms, Index, Constants>
{
	fn drop(&mut self) {
		let device = &self.shader.data.device;
		let pool = MaybeUninit::take(&mut self.descriptor_pool);
		//        pool.free_sets(self.descriptor_sets.drain(..));
		device.destroy_descriptor_pool(pool);
		println!("Dropped Descriptors");
	}
}
