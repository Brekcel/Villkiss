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
	Vertex: VertexInfo,
	Uniforms: UniformInfo,
	Index: IndexType,
	Constants: PushConstantInfo,
> {
	pub(crate) data: &'a HALData,
	pub(crate) mods: MaybeUninit<ShaderMods>,
	pub(crate) vertex_desc: VertexBufferDesc,
	pub(crate) attribute_descs: Vec<AttributeDesc>,
	pub(crate) layout_bindings: Vec<DescriptorSetLayoutBinding>,
	pub(crate) descriptor_layout: MaybeUninit<<Backend as gfx_hal::Backend>::DescriptorSetLayout>,
	pub(crate) pipeline_layout: MaybeUninit<<Backend as gfx_hal::Backend>::PipelineLayout>,
	pub(crate) push_constant_stages: ShaderStageFlags,
	phantom: PhantomData<(Vertex, Uniforms, Index, Constants)>,
}

#[derive(Default)]
pub struct ShaderSet<T> {
	pub vertex: T,
	pub hull: Option<T>,
	pub domain: Option<T>,
	pub geometry: Option<T>,
	pub fragment: Option<T>,
}

pub type ShaderModData<'a> = ShaderSet<&'a [u8]>;
type ShaderMods = ShaderSet<<Backend as gfx_hal::Backend>::ShaderModule>;

pub trait IndexType {
	const HAL: HALIndexType;
}

impl IndexType for u16 {
	const HAL: HALIndexType = HALIndexType::U16;
}

impl IndexType for u32 {
	const HAL: HALIndexType = HALIndexType::U32;
}

pub trait VertexInfo {
	const ATTRIBUTES: &'static [Format];
	const STRIDE: u32;
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

pub trait PushConstantInfo {
	const STAGES: &'static [ShaderStageFlags];
	const SIZE: u32;
}

impl PushConstantInfo for () {
	const SIZE: u32 = 0;
	const STAGES: &'static [ShaderStageFlags] = &[];
}

impl<
		'a,
		Vertex: VertexInfo,
		Uniforms: UniformInfo,
		Index: IndexType,
		Constants: PushConstantInfo,
	> Shader<'a, Vertex, Uniforms, Index, Constants>
{
	pub(crate) fn create<'b>(
		data: &'a HALData,
		shaders: ShaderModData<'b>,
	) -> Shader<'a, Vertex, Uniforms, Index, Constants> {
		println!("Creating Shader");
		let device = &data.device;

		let mods = shaders.make_mods(device);

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
				None
			} else {
				Some((push_constant_stages, 0..Constants::SIZE))
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
			mods: MaybeUninit::new(mods),
			vertex_desc,
			attribute_descs,
			layout_bindings,
			descriptor_layout: MaybeUninit::new(desc_layout),
			pipeline_layout: MaybeUninit::new(pipe_layout),
			push_constant_stages,
			phantom: PhantomData,
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
		unsafe { self.mods.get_ref() }.make_entry_points()
	}
}

impl<
		'a,
		Vertex: VertexInfo,
		Uniforms: UniformInfo,
		Index: IndexType,
		Constants: PushConstantInfo,
	> Drop for Shader<'a, Vertex, Uniforms, Index, Constants>
{
	fn drop(&mut self) {
		let device = &self.data.device;
		unsafe {
			MaybeUninit::take(&mut self.mods).man_drop(device);

			device.destroy_descriptor_set_layout(MaybeUninit::take(&mut self.descriptor_layout));
			device.destroy_pipeline_layout(MaybeUninit::take(&mut self.pipeline_layout));
		}
		println!("Dropped Shader");
	}
}

impl ShaderModData<'_> {
	fn make_mods(self, device: &<Backend as gfx_hal::Backend>::Device) -> ShaderMods {
		unsafe {
			ShaderMods {
				vertex: device.create_shader_module(self.vertex).unwrap(),
				hull: self.hull.map(|h| device.create_shader_module(h).unwrap()),
				domain: self.domain.map(|h| device.create_shader_module(h).unwrap()),
				geometry: self
					.geometry
					.map(|h| device.create_shader_module(h).unwrap()),
				fragment: self
					.fragment
					.map(|h| device.create_shader_module(h).unwrap()),
			}
		}
	}
}

impl ShaderMods {
	fn make_entry_points<'a, 'b>(&'a self) -> GraphicsShaderSet<'b, Backend>
	where
		'a: 'b,
	{
		fn entry_point<'a, 'b>(
			shad_mod: &'a Option<<Backend as gfx_hal::Backend>::ShaderModule>,
		) -> Option<EntryPoint<'b, Backend>>
		where
			'a: 'b,
		{
			shad_mod.as_ref().map(|m| EntryPoint::<'b, Backend> {
				entry: "main",
				module: m,
				specialization: Default::default(),
			})
		}
		GraphicsShaderSet {
			vertex: EntryPoint::<'b, Backend> {
				entry: "main",
				module: &self.vertex,
				specialization: Default::default(),
			},
			hull: entry_point(&self.hull),
			domain: entry_point(&self.domain),
			geometry: entry_point(&self.geometry),
			fragment: entry_point(&self.fragment),
		}
	}

	fn man_drop(self, device: &<Backend as gfx_hal::Backend>::Device) {
		unsafe {
			device.destroy_shader_module(self.vertex);
			self.domain.map(|v| device.destroy_shader_module(v));
			self.fragment.map(|v| device.destroy_shader_module(v));
			self.geometry.map(|v| device.destroy_shader_module(v));
			self.hull.map(|v| device.destroy_shader_module(v));
		}
	}
}

#[macro_export]
macro_rules! push_constant {
	//Actual macro
	(
		$vis:vis struct $name: ident {
			const STAGES = [$($stage:ident),*];
			$($vert_name: ident : $vert_type:ty),*,
		}
	) => {
			#[derive(Debug, Clone, Copy)]
			#[repr(C)]
			$vis struct $name {
				$(
					pub $vert_name: $vert_type,
				)*
			}

			impl ::villkiss::shader::PushConstantInfo for $name {
				const STAGES: &'static [::villkiss::gfx_hal::pso::ShaderStageFlags] = &[
					$(
						::villkiss::gfx_hal::pso::ShaderStageFlags::$stage
					,)*
				];
				const SIZE: u32 = ::std::mem::size_of::<$name>() as u32;
			}
		};
	//No trailing comma
	(
		$vis:vis struct $name: ident {
			const STAGES = [$($stage:ident),*];
			$($vert_name: ident : $vert_type:ty),*
		}
	) => (push_constant!($vis struct $name {const STAGES = [$($stage,)*];$($vert_name: $vert_type,)*}));
	//No vis
	(
		struct $name: ident {
			const STAGES = [$($stage:ident),*];
			$($vert_name: ident : $vert_type:ty),*,
		}
	) => (push_constant!(pub(self) struct $name {const STAGES = [$($stage,)*];$($vert_name: $vert_type,)*}));
	//No trailing comma or vis
	(
		struct $name: ident {
			const STAGES = [$($stage:ident),*];
			$($vert_name: ident : $vert_type:ty),*
		}
	) => (push_constant!(struct $name {const STAGES = [$($stage,)*];$($vert_name: $vert_type,)*}));
}

#[macro_export]
macro_rules! vertex {
	//Actual macro
	(
		$vis:vis struct $name: ident {
			$($vert_name: ident : $vert_type:ty as $format_type:ident),*,
		}
	) => {
		#[derive(Debug, Clone, Copy)]
		#[repr(C)]
		$vis struct $name {
			$(
				pub $vert_name: $vert_type,
			)*
		}
		impl ::villkiss::shader::VertexInfo for $name {
			const ATTRIBUTES: &'static [::villkiss::gfx_hal::format::Format] = &[
				$(
					::villkiss::gfx_hal::format::Format::$format_type,
				)*
			];
			const STRIDE: u32 = std::mem::size_of::<$name>() as u32;
		}
	};
	//No trailing comma
	(
		$vis:vis struct $name: ident {
			$($vert_name: ident : $vert_type:ty as $format_type:ident),*
		}
	) => (vertex!{$vis struct $name {$($vert_name: $vert_type as $format_type,)*}});
	//No vis
	(
		struct $name: ident {
			$($vert_name: ident : $vert_type:ty as $format_type:ident),*,
		}
	) => (vertex!{pub(self) struct $name {$($vert_name: $vert_type as $format_type,)*}});
	//No vis or trailing comma
	(
		struct $name: ident {
			$($vert_name: ident : $vert_type:ty as $format_type:ident),*
		}
	) => (vertex!{struct $name {$($vert_name: $vert_type as $format_type,)*}});
}

#[macro_export]
macro_rules! uniform {
	//Actual macro
	(
		$vis:vis $name: ident = [
			$($ui:expr),*,
		];
	) => {
		$vis struct $name;
		impl ::villkiss::shader::UniformInfo for $name {
			const UNIFORMS: &'static [shader::UniformInfoData] = &[
				$($ui,)*
			];
		}
	};
	//No visibility, Yes trailing comma
	(
		$name: ident = [
			$($ui:expr),*,
		];
	) => (uniform!{pub(self) $name = [$($ui,)*];});
	//Yes visibility, No trailing comma
	(
		$vis:vis $name: ident = [
			$($ui:expr),*
		];
	) => (uniform!{$vis $name = [$($ui,)*];});
	//No trailing comma, no visibility
	(
		$name: ident = [
			$($ui:expr),*
		];
	) => (uniform!{$name = [$($ui,)*];});

}
