use std::mem::MaybeUninit;

use gfx_hal::{
    format::Format,
    pso::{
        AttributeDesc, DescriptorArrayIndex, DescriptorBinding, DescriptorSetLayoutBinding,
        DescriptorType, Element, EntryPoint, GraphicsShaderSet, ShaderStageFlags, VertexBufferDesc,
    },
    Device,
};

use crate::gfx_back::Backend;
use crate::util::{GFXRes, TakeExt};
use crate::{Descriptors, HALData};

pub struct Shader<'a> {
    pub(crate) data: &'a HALData,
    pub(crate) vert_mod: MaybeUninit<<Backend as gfx_hal::Backend>::ShaderModule>,
    pub(crate) frag_mod: MaybeUninit<<Backend as gfx_hal::Backend>::ShaderModule>,
    pub(crate) vertex_descs: Vec<VertexBufferDesc>,
    pub(crate) attribute_descs: Vec<AttributeDesc>,
    pub(crate) layout_bindings: Vec<DescriptorSetLayoutBinding>,
    pub(crate) descriptor_layout: MaybeUninit<<Backend as gfx_hal::Backend>::DescriptorSetLayout>,
    pub(crate) pipeline_layout: MaybeUninit<<Backend as gfx_hal::Backend>::PipelineLayout>,
}

pub struct VertexInfo<'a> {
    stride: u32,
    attributes: &'a [Format],
}

impl<'a> VertexInfo<'a> {
    pub fn create<T>(attributes: &'a [Format]) -> VertexInfo<'a> {
        VertexInfo {
            stride: std::mem::size_of::<T>() as u32,
            attributes,
        }
    }
}

pub struct UniformInfo {
    pub stage: ShaderStageFlags,
    pub uniform_type: DescriptorType,
    pub count: DescriptorArrayIndex,
    pub mutable: bool,
}

impl<'a> Shader<'a> {
    pub(crate) fn create<'b>(
        data: &'a HALData,
        vert: &'b [u8],
        frag: &'b [u8],
        vertices: &'b [VertexInfo],
        uniforms: &'b [UniformInfo],
    ) -> Shader<'a> {
        println!("Creating Shader");
        let device = &data.device;
        let vert_mod = device.create_shader_module(vert).unwrap();
        let frag_mod = device.create_shader_module(frag).unwrap();

        let (desc_layout, layout_bindings, pipe_layout) = {
            let layout_bindings = uniforms
                .iter()
                .enumerate()
                .map(|(binding, info)| {
                    let binding = binding as DescriptorBinding;
                    DescriptorSetLayoutBinding {
                        binding,
                        ty: info.uniform_type,
                        count: info.count,
                        stage_flags: info.stage,
                        immutable_samplers: info.mutable,
                    }
                })
                .collect::<Vec<DescriptorSetLayoutBinding>>();
            let desc_layout = device
                .create_descriptor_set_layout(&layout_bindings, &[])
                .unwrap();
            let pipe_layout = device
                .create_pipeline_layout(vec![&desc_layout], &[])
                .unwrap();
            (desc_layout, layout_bindings, pipe_layout)
        };
        let (vertex_descs, attribute_descs) = {
            let mut vertex_descs = Vec::with_capacity(vertices.len());
            let mut attribute_descs = Vec::with_capacity(vertices.len() * 3);
            vertices.iter().enumerate().for_each(|(binding, info)| {
                let binding = binding as u32;
                vertex_descs.push(VertexBufferDesc {
                    binding,
                    stride: info.stride,
                    rate: 0,
                });
                let mut offset = 0;
                info.attributes
                    .iter()
                    .enumerate()
                    .for_each(|(location, format)| {
                        let location = location as u32;
                        attribute_descs.push(AttributeDesc {
                            location,
                            binding,
                            element: Element {
                                format: *format,
                                offset,
                            },
                        });
                        offset += (format.surface_desc().bits / 8) as u32;
                    });
            });
            vertex_descs.shrink_to_fit();
            attribute_descs.shrink_to_fit();
            (vertex_descs, attribute_descs)
        };

        Shader {
            data,
            vert_mod: MaybeUninit::new(vert_mod),
            frag_mod: MaybeUninit::new(frag_mod),
            vertex_descs,
            attribute_descs,
            layout_bindings,
            descriptor_layout: MaybeUninit::new(desc_layout),
            pipeline_layout: MaybeUninit::new(pipe_layout),
        }
    }

    pub(crate) fn layout_bindings(&self) -> &[DescriptorSetLayoutBinding] {
        &self.layout_bindings
    }

    pub fn pipe_layout(&self) -> &<Backend as gfx_hal::Backend>::PipelineLayout {
        unsafe { self.pipeline_layout.get_ref() }
    }

    pub(crate) fn desc_layout(&self) -> &<Backend as gfx_hal::Backend>::DescriptorSetLayout {
        unsafe { self.descriptor_layout.get_ref() }
    }

    pub(crate) fn describe_vertices(
        &self,
        verts: &mut Vec<VertexBufferDesc>,
        attrs: &mut Vec<AttributeDesc>,
    ) {
        *verts = self.vertex_descs.clone();
        *attrs = self.attribute_descs.clone();
    }

	pub fn create_descriptors(&self, pool_count: usize) -> Descriptors {
		Descriptors::create(self, pool_count)
	}

    pub(crate) fn make_set<'b>(&'a self) -> GraphicsShaderSet<'b, Backend>
    where
        'a: 'b,
    {
        let vert_entry = EntryPoint::<Backend> {
            entry: "main",
            module: unsafe { self.vert_mod.get_ref() },
            specialization: Default::default(),
        };

        let frag_entry = EntryPoint::<Backend> {
            entry: "main",
            module: unsafe { self.frag_mod.get_ref() },
            specialization: Default::default(),
        };

        GraphicsShaderSet::<'b, Backend> {
            vertex: vert_entry,
            hull: None,
            domain: None,
            geometry: None,
            fragment: Some(frag_entry),
        }
    }
}

impl<'a> GFXRes for Shader<'a> {}

impl<'a> Drop for Shader<'a> {
    fn drop(&mut self) {
        let device = &self.data.device;
        device.destroy_shader_module(MaybeUninit::take(&mut self.vert_mod));
        device.destroy_shader_module(MaybeUninit::take(&mut self.frag_mod));

        device.destroy_descriptor_set_layout(MaybeUninit::take(&mut self.descriptor_layout));
        device.destroy_pipeline_layout(MaybeUninit::take(&mut self.pipeline_layout));
        println!("Dropped Shader");
    }
}
