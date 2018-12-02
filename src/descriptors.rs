use std::mem::MaybeUninit;

use gfx_hal::{
    pso::{Descriptor, DescriptorPool, DescriptorRangeDesc, DescriptorSetWrite},
    Device,
};

use crate::gfx_back::Backend;
use crate::util::TakeExt;
use crate::{shader::Shader};

pub struct Descriptors<'a> {
    shader: &'a Shader<'a>,
    descriptor_pool: MaybeUninit<<Backend as gfx_hal::Backend>::DescriptorPool>,
    descriptor_sets: Vec<<Backend as gfx_hal::Backend>::DescriptorSet>,
}

impl<'a> Descriptors<'a> {
    pub(crate) fn create(shader: &'a Shader<'a>, pool_count: usize) -> Descriptors<'a> {
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
                })
                .collect::<Vec<_>>();
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

        Descriptors {
            shader,
            descriptor_pool: MaybeUninit::new(descriptor_pool),
            descriptor_sets,
        }
    }

    pub fn write(&self, set: usize, binding: usize, descriptor: Descriptor<Backend>) {
        let device = &self.shader.data.device;
        let writer = DescriptorSetWrite {
            set: &self.descriptor_sets[set],
            binding: binding as u32,
            array_offset: 0,
            descriptors: vec![descriptor],
        };
        device.write_descriptor_sets(vec![writer])
    }

    pub fn descriptor_set(&self, idx: usize) -> &<Backend as gfx_hal::Backend>::DescriptorSet {
        &self.descriptor_sets[idx]
    }
}

impl<'a> Drop for Descriptors<'a> {
    fn drop(&mut self) {
        let device = &self.shader.data.device;
        let mut pool = MaybeUninit::take(&mut self.descriptor_pool);
        pool.free_sets(self.descriptor_sets.drain(..));
        device.destroy_descriptor_pool(pool);
        println!("Dropped Descriptors");
    }
}
