use std::{
	iter::once,
	marker::PhantomData,
	mem::MaybeUninit,
};

use gfx_hal::{
	format::Format,
	pso::{
		AttributeDesc,
		Descriptor,
		DescriptorArrayIndex,
		DescriptorBinding,
		DescriptorSetLayoutBinding,
		DescriptorType,
		Element,
		EntryPoint,
		GraphicsShaderSet,
		ShaderStageFlags,
		VertexBufferDesc,
	},
	Device,
	IndexType as HALIndexType,
};

use crate::{
	gfx_back::Backend,
	util::TakeExt,
	BufferPool,
	DescriptorPool,
	HALData,
	Mesh,
};

pub struct Shader<
	'a,
	Vertex: VertexInfo<Vertex>,
	Uniforms: UniformInfo,
	Index: IndexType,
	Constants: PushConstantInfo<Constants>,
> {
	pub(crate) data: &'a HALData,
	pub(crate) vert_mod: MaybeUninit<<Backend as gfx_hal::Backend>::ShaderModule>,
	pub(crate) frag_mod: MaybeUninit<<Backend as gfx_hal::Backend>::ShaderModule>,
	pub(crate) vertex_desc: VertexBufferDesc,
	pub(crate) attribute_descs: Vec<AttributeDesc>,
	pub(crate) layout_bindings: Vec<DescriptorSetLayoutBinding>,
	pub(crate) descriptor_layout: MaybeUninit<<Backend as gfx_hal::Backend>::DescriptorSetLayout>,
	pub(crate) pipeline_layout: MaybeUninit<<Backend as gfx_hal::Backend>::PipelineLayout>,
	pub(crate) push_constant_stages: ShaderStageFlags,
	vert_phantom: PhantomData<Vertex>,
	uniforms_phantom: PhantomData<Uniforms>,
	index_phantom: PhantomData<Index>,
	constants_phantom: PhantomData<Constants>,
}

pub trait IndexType {
	const HAL: HALIndexType;
}

impl IndexType for u16 {
	const HAL: HALIndexType = HALIndexType::U16;
}

impl IndexType for u32 {
	const HAL: HALIndexType = HALIndexType::U32;
}

pub trait VertexInfo<T> {
	const ATTRIBUTES: &'static [Format];
	const STRIDE: u32 = std::mem::size_of::<T>() as u32;
}

pub trait UniformInfo {
	const UNIFORMS: &'static [UniformInfoData];
}

pub struct UniformInfoData {
	pub stage: ShaderStageFlags,
	pub uniform_type: DescriptorType,
	pub count: DescriptorArrayIndex,
	pub mutable: bool,
}

pub trait PushConstantInfo<T> {
	const STAGES: &'static [ShaderStageFlags];
	const SIZE: u32 = std::mem::size_of::<T>() as u32;
}

impl PushConstantInfo<()> for () {
	const SIZE: u32 = 0;
	const STAGES: &'static [ShaderStageFlags] = &[];
}

impl<
		'a,
		Vertex: VertexInfo<Vertex>,
		Uniforms: UniformInfo,
		Index: IndexType,
		Constants: PushConstantInfo<Constants>,
	> Shader<'a, Vertex, Uniforms, Index, Constants>
{
	pub(crate) fn create<'b>(
		data: &'a HALData,
		vert: &'b [u8],
		frag: &'b [u8],
	) -> Shader<'a, Vertex, Uniforms, Index, Constants> {
		println!("Creating Shader");
		let device = &data.device;

		let vert_mod = unsafe { device.create_shader_module(vert).unwrap() };
		let frag_mod = unsafe { device.create_shader_module(frag).unwrap() };

		let push_constant_stages = Constants::STAGES
			.iter()
			.fold(ShaderStageFlags::empty(), |acc, flag| acc | *flag);

		let (desc_layout, layout_bindings, pipe_layout) = {
			let layout_bindings = Uniforms::UNIFORMS
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

			let pc_layout = if Constants::SIZE == 0 {
				vec![]
			} else {
				vec![(push_constant_stages, 0..Constants::SIZE)]
			};
			unsafe {
				let desc_layout = device
					.create_descriptor_set_layout(&layout_bindings, &[])
					.unwrap();
				let pipe_layout = device
					.create_pipeline_layout(once(&desc_layout), pc_layout)
					.unwrap();
				(desc_layout, layout_bindings, pipe_layout)
			}
		};

		let vertex_desc = VertexBufferDesc {
			binding: 0,
			stride: Vertex::STRIDE,
			rate: 0,
		};

		let attribute_descs = {
			let mut offset = 0;
			Vertex::ATTRIBUTES
				.iter()
				.enumerate()
				.map(|(location, format)| {
					let location = location as u32;
					let attr = AttributeDesc {
						location,
						binding: 0,
						element: Element {
							format: *format,
							offset,
						},
					};
					offset += (format.surface_desc().bits / 8) as u32;
					attr
				})
				.collect::<Vec<_>>()
		};

		Shader {
			data,
			vert_mod: MaybeUninit::new(vert_mod),
			frag_mod: MaybeUninit::new(frag_mod),
			vertex_desc,
			attribute_descs,
			layout_bindings,
			descriptor_layout: MaybeUninit::new(desc_layout),
			pipeline_layout: MaybeUninit::new(pipe_layout),
			push_constant_stages,
			vert_phantom: PhantomData,
			uniforms_phantom: PhantomData,
			index_phantom: PhantomData,
			constants_phantom: PhantomData,
		}
	}

	pub fn create_mesh<'b>(
		&'a self,
		pool: &'a BufferPool,
		vertices: Vec<Vertex>,
		indices: Vec<Index>,
		descriptors: &'b [Vec<Descriptor<Backend>>],
	) -> Mesh<'a, Vertex, Uniforms, Index, Constants> {
		Mesh::create(self, pool, vertices, indices, descriptors)
	}

	pub(crate) fn layout_bindings(&self) -> &[DescriptorSetLayoutBinding] { &self.layout_bindings }

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
		verts.push(self.vertex_desc);
		*attrs = self.attribute_descs.clone();
	}

	pub fn create_descriptors(
		&'a self,
		pool_count: usize,
	) -> DescriptorPool<'a, Vertex, Uniforms, Index, Constants> {
		DescriptorPool::create(self, pool_count)
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

impl<
		'a,
		Vertex: VertexInfo<Vertex>,
		Uniforms: UniformInfo,
		Index: IndexType,
		Constants: PushConstantInfo<Constants>,
	> Drop for Shader<'a, Vertex, Uniforms, Index, Constants>
{
	fn drop(&mut self) {
		let device = &self.data.device;
		unsafe {
			device.destroy_shader_module(MaybeUninit::take(&mut self.vert_mod));
			device.destroy_shader_module(MaybeUninit::take(&mut self.frag_mod));

			device.destroy_descriptor_set_layout(MaybeUninit::take(&mut self.descriptor_layout));
			device.destroy_pipeline_layout(MaybeUninit::take(&mut self.pipeline_layout));
		}
		println!("Dropped Shader");
	}
}
