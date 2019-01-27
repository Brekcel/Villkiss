use std::{
	borrow::BorrowMut,
	iter::once,
	mem::{
		size_of,
		MaybeUninit,
	},
	any::TypeId,
	ops::Range,
	slice,
};

use gfx_hal::{
	buffer::IndexBufferView,
	command::RenderSubpassCommon,
	pso::{
		BlendState,
		ColorBlendDesc,
		ColorMask,
		Comparison,
		DepthStencilDesc,
		DepthTest,
		Face,
		FrontFace,
		GraphicsPipelineDesc,
		PolygonMode,
		Rasterizer,
		Specialization as HAL_Specialization,
		SpecializationConstant,
		StencilTest,
	},
	Device,
	IndexCount,
	InstanceCount,
	Primitive,
};

use crate::{
	buffer::{
		Buffer,
		BufferView,
	},
	gfx_back::Backend,
	shader::{
		IndexType,
		PushConstantInfo,
		Shader,
		ShaderSet,
		UniformInfo,
		VertexInfo,
	},
	util::TakeExt,
	RenderPass,
};

use byteorder::{
	NativeEndian as NE,
	WriteBytesExt,
};

pub struct Pipeline<
	'a,
	Vertex: VertexInfo,
	Uniforms: UniformInfo,
	Index: IndexType,
	Constants: PushConstantInfo,
> {
	pass: &'a RenderPass<'a>,
	shader: &'a Shader<'a, Vertex, Uniforms, Index, Constants>,
	pipe: MaybeUninit<<Backend as gfx_hal::Backend>::GraphicsPipeline>,
}

pub struct BoundPipe<
	'a,
	C: BorrowMut<<Backend as gfx_hal::Backend>::CommandBuffer>,
	Vertex: VertexInfo,
	Uniforms: UniformInfo,
	Index: IndexType,
	Constants: PushConstantInfo,
> {
	pipeline: &'a Pipeline<'a, Vertex, Uniforms, Index, Constants>,
	encoder: &'a mut RenderSubpassCommon<Backend, C>,
}

pub enum SpecializationValue {
	Bool(bool),
	Int(i32),
	Uint(u32),
	Float(f32),
	Double(f64),
}

pub struct Specialization {
	pub value: SpecializationValue,
	pub constant_idx: u32,
}

pub type PipeSpecialization<'a> = ShaderSet<&'a [Specialization]>;

struct PipeToHal {
	data: Vec<u8>,
	vert_constants: Vec<SpecializationConstant>,
	geom_constants: Vec<SpecializationConstant>,
	hull_constants: Vec<SpecializationConstant>,
	doma_constants: Vec<SpecializationConstant>,
	frag_constants: Vec<SpecializationConstant>,
}

impl<
		'a,
		Vertex: VertexInfo,
		Uniforms: UniformInfo,
		Index: IndexType,
		Constants: PushConstantInfo,
	> Pipeline<'a, Vertex, Uniforms, Index, Constants>
{
	pub(crate) fn create<'b>(
		pass: &'a RenderPass<'a>,
		shader: &'a Shader<'a, Vertex, Uniforms, Index, Constants>,
		specialization: PipeSpecialization<'b>,
	) -> Pipeline<'a, Vertex, Uniforms, Index, Constants> {
		pub const RASTERIZER: Rasterizer = Rasterizer {
			polygon_mode: PolygonMode::Fill,
			cull_face: Face::BACK,
			front_face: FrontFace::CounterClockwise,
			depth_clamping: false,
			depth_bias: None,
			conservative: false,
		};

		println!("Creating Pipeline");
		let device = pass.swapchain.data.device();
		let pipe_to_hal = PipeToHal::create(specialization);
		let shad_set = shader.make_set(pipe_to_hal.make_hal());
		let pipe_layout = shader.pipe_layout();
		let desc_layout = shader.desc_layout();
		let subpass = pass.make_subpass();

		let mut pipeline_desc = GraphicsPipelineDesc::new(
			shad_set,
			Primitive::TriangleList,
			RASTERIZER,
			pipe_layout,
			subpass,
		);
		pipeline_desc.depth_stencil = DepthStencilDesc {
			depth: DepthTest::On {
				fun: Comparison::Less,
				write: true,
			},
			depth_bounds: false,
			stencil: StencilTest::Off,
		};
		pipeline_desc
			.blender
			.targets
			.push(ColorBlendDesc(ColorMask::ALL, BlendState::ALPHA));

		shader.describe_vertices(
			&mut pipeline_desc.vertex_buffers,
			&mut pipeline_desc.attributes,
		);

		let pipe = unsafe {
			device
				.create_graphics_pipeline(&pipeline_desc, None)
				.unwrap()
		};

		Pipeline {
			pass,
			shader,
			pipe: MaybeUninit::new(pipe),
		}
	}

	pub fn bind_pipe<
		C: BorrowMut<<Backend as gfx_hal::Backend>::CommandBuffer>,
		F: FnOnce(&mut BoundPipe<C, Vertex, Uniforms, Index, Constants>),
	>(
		&self,
		encoder: &mut RenderSubpassCommon<Backend, C>,
		draws: F,
	) {
		unsafe {
			encoder.bind_graphics_pipeline(self.pipe.get_ref());
		}
		let mut bp = BoundPipe {
			pipeline: self,
			encoder,
		};
		draws(&mut bp);
	}
}

impl<
		'a,
		C: BorrowMut<<Backend as gfx_hal::Backend>::CommandBuffer>,
		Vertex: VertexInfo + 'static,
		Uniforms: UniformInfo,
		Index: IndexType + 'static,
		Constants: PushConstantInfo,
	> BoundPipe<'a, C, Vertex, Uniforms, Index, Constants>
{
	pub fn bind_vertex_buffer<'b, T: Buffer<'b>>(&mut self, buffer: &BufferView<'b, T>) {
		assert_eq!(*buffer.type_id(), TypeId::of::<Vertex>());
		unsafe {
			self.encoder
				.bind_vertex_buffers(0, once((buffer.hal_buffer(), buffer.offset())));
		}
	}

	pub fn bind_index_buffer<'b, T: Buffer<'b>>(&mut self, buffer: &BufferView<'b, T>) {
		assert_eq!(*buffer.type_id(), TypeId::of::<Index>());
		unsafe {
			self.encoder.bind_index_buffer(IndexBufferView {
				buffer: buffer.hal_buffer(),
				offset: buffer.offset(),
				index_type: Index::HAL,
			});
		}
	}

	pub fn bind_descriptors(&mut self, descriptors: &<Backend as gfx_hal::Backend>::DescriptorSet) {
		unsafe {
			self.encoder.bind_graphics_descriptor_sets(
				self.pipeline.shader.pipe_layout(),
				0,
				once(descriptors),
				&[],
			);
		}
	}

	pub fn draw_indexed(&mut self, indices: Range<IndexCount>, instances: Range<InstanceCount>) {
		unsafe { self.encoder.draw_indexed(indices, 0, instances) }
	}
}

impl<
		'a,
		C: BorrowMut<<Backend as gfx_hal::Backend>::CommandBuffer>,
		Vertex: VertexInfo,
		Uniforms: UniformInfo,
		Index: IndexType,
		Constants: PushConstantInfo, //Find a way to implement !()
	> BoundPipe<'a, C, Vertex, Uniforms, Index, Constants>
{
	pub fn bind_push_constants(&mut self, constants: Constants) {
		unsafe {
			let pc_ptr = &constants as *const Constants as *const u32;
			let slice = slice::from_raw_parts(pc_ptr, size_of::<Constants>() / size_of::<u32>());
			self.encoder.push_graphics_constants(
				self.pipeline.shader.pipe_layout(),
				self.pipeline.shader.push_constant_stages,
				0,
				slice,
			);
		}
	}
}

impl<
		'a,
		Vertex: VertexInfo,
		Uniforms: UniformInfo,
		Index: IndexType,
		Constants: PushConstantInfo,
	> Drop for Pipeline<'a, Vertex, Uniforms, Index, Constants>
{
	fn drop(&mut self) {
		let device = self.pass.swapchain.data.device();
		unsafe {
			device.destroy_graphics_pipeline(MaybeUninit::take(&mut self.pipe));
		}
		println!("Dropped Pipeline");
	}
}

impl SpecializationValue {
	fn write_data(&self, data: &mut Vec<u8>) -> u16 {
		match *self {
			SpecializationValue::Bool(b) => {
				if b {
					data.write_u32::<NE>(1).unwrap()
				} else {
					data.write_u32::<NE>(0).unwrap()
				}
				4
			},
			SpecializationValue::Double(d) => {
				data.write_f64::<NE>(d).unwrap();
				8
			},
			SpecializationValue::Float(f) => {
				data.write_f32::<NE>(f).unwrap();
				4
			},
			SpecializationValue::Int(i) => {
				data.write_i32::<NE>(i).unwrap();
				4
			},
			SpecializationValue::Uint(u) => {
				data.write_u32::<NE>(u).unwrap();
				4
			},
		}
	}
}

impl<'a> PipeToHal {
	fn create(specs: PipeSpecialization) -> PipeToHal {
		let mut data = Vec::new();

		let mut cur_offset = 0;
		let mut f = |s: &[Specialization]| {
			s.iter()
				.map(|s| {
					let size = s.value.write_data(&mut data);
					let range = cur_offset..(cur_offset + size);
					cur_offset += size;
					SpecializationConstant {
						id: s.constant_idx,
						range,
					}
				})
				.collect::<Vec<SpecializationConstant>>()
		};
		let vert_constants = specs.vertex.map(|s| f(s)).unwrap_or(Vec::new());
		let hull_constants = specs.hull.map(|s| f(s)).unwrap_or(Vec::new());
		let geom_constants = specs.geometry.map(|s| f(s)).unwrap_or(Vec::new());
		let doma_constants = specs.domain.map(|s| f(s)).unwrap_or(Vec::new());
		let frag_constants = specs.fragment.map(|s| f(s)).unwrap_or(Vec::new());
		PipeToHal {
			data,
			vert_constants,
			geom_constants,
			hull_constants,
			doma_constants,
			frag_constants,
		}
	}

	fn make_hal(&'a self) -> ShaderSet<HAL_Specialization<'a>> {
		let spec = |consts: &'a Vec<SpecializationConstant>| -> Option<HAL_Specialization<'a>> {
			if consts.len() == 0 {
				None
			} else {
				Some(HAL_Specialization {
					constants: consts.as_slice(),
					data: self.data.as_slice(),
				})
			}
		};
		ShaderSet {
			vertex: spec(&self.vert_constants),
			hull: spec(&self.hull_constants),
			domain: spec(&self.doma_constants),
			geometry: spec(&self.geom_constants),
			fragment: spec(&self.frag_constants),
		}
	}
}
