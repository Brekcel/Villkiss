use std::mem::MaybeUninit;

use gfx_hal::{Device, pso::{DescriptorPool, Descriptor, DescriptorSetWrite, DescriptorRangeDesc}};

use crate::{HALData, shader::{Shader}};
use crate::gfx_back::Backend;
use crate::util::TakeExt;

pub struct Descriptors<'a> {
	data: &'a HALData,
	descriptor_pool: MaybeUninit<<Backend as gfx_hal::Backend>::DescriptorPool>,
	descriptor_sets: Vec<<Backend as gfx_hal::Backend>::DescriptorSet>,
}

impl<'a> Descriptors<'a> {
	pub fn create<'b>(data: &'a HALData,
							 pool_count: usize,
							 shader: &'b Shader
	) -> Descriptors<'a> {
		println!("Creating Descriptors");
		let device = &data.device;
		let desc_layout = shader.desc_layout();
		let mut descriptor_pool = {
			let descriptors = shader.layout_bindings().iter().map(|uniform| {
				DescriptorRangeDesc {
					ty: uniform.ty,
					count: pool_count,
				}
			}).collect::<Vec<_>>();
			device.create_descriptor_pool(pool_count, descriptors).unwrap()
		};

		let descriptor_sets = {
			let mut buf = Vec::with_capacity(pool_count);
			descriptor_pool.allocate_sets(vec![desc_layout;pool_count], &mut buf).unwrap();
			buf
		};

		println!("Sets: {}", &descriptor_sets.len());
		Descriptors {
			data: data,
			descriptor_pool: MaybeUninit::new(descriptor_pool),
			descriptor_sets,
		}
	}

	pub fn write<'b>(&self, set: usize, binding: usize, descriptor: Descriptor<Backend>) where 'a: 'b {
		let device = &self.data.device;
		let writer = DescriptorSetWrite {
			set: &self.descriptor_sets[set],
			binding: binding as u32,
			array_offset: 0,
			descriptors: vec![descriptor]
		};
		device.write_descriptor_sets(vec![writer])

	}

	pub fn descriptor_set(&self, idx: usize) -> &<Backend as gfx_hal::Backend>::DescriptorSet {
		&self.descriptor_sets[idx]
	}
}

impl<'a> Drop for Descriptors<'a> {
	fn drop(&mut self) {
		let device = &self.data.device;
		let mut pool = MaybeUninit::take(&mut self.descriptor_pool);
		pool.free_sets(self.descriptor_sets.drain(..));
		device.destroy_descriptor_pool(pool);
		println!("Dropped Descriptors");
	}
}