use std::{
	marker::PhantomData,
	mem::MaybeUninit,
};

use gfx_hal::{
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
		StencilTest,
	},
	Device,
	Primitive,
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
	Mesh,
	RenderPass,
};

pub struct Pipeline<
	'a,
	Vertex: VertexInfo<Vertex>,
	Uniforms: UniformInfo,
	Index: IndexType,
	Constants: PushConstantInfo,
> {
	pass: &'a RenderPass<'a>,
	pipe: MaybeUninit<<Backend as gfx_hal::Backend>::GraphicsPipeline>,
	vert_phantom: PhantomData<Vertex>,
	uniforms_phantom: PhantomData<Uniforms>,
	index_phantom: PhantomData<Index>,
	constants_phantom: PhantomData<Constants>,
}

pub struct BoundPipe<
	'a,
	'b: 'a,
	Vertex: VertexInfo<Vertex>,
	Uniforms: UniformInfo,
	Index: IndexType,
	Constants: PushConstantInfo,
> {
	encoder: &'a mut RenderSubpassCommon<'b, Backend>,
	vert_phantom: PhantomData<Vertex>,
	uniforms_phantom: PhantomData<Uniforms>,
	index_phantom: PhantomData<Index>,
	constants_phantom: PhantomData<Constants>,
}

impl<
		'a,
		Vertex: VertexInfo<Vertex>,
		Uniforms: UniformInfo,
		Index: IndexType,
		Constants: PushConstantInfo,
	> Pipeline<'a, Vertex, Uniforms, Index, Constants>
{
	pub(crate) fn create(
		pass: &'a RenderPass<'a>,
		shader: &'a Shader<'a, Vertex, Uniforms, Index, Constants>,
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
		let device = &pass.swapchain.data.device;
		let shad_set = shader.make_set();
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

		let pipe = device
			.create_graphics_pipeline(&pipeline_desc, None)
			.unwrap();

		Pipeline {
			pass,
			pipe: MaybeUninit::new(pipe),
			vert_phantom: PhantomData,
			uniforms_phantom: PhantomData,
			index_phantom: PhantomData,
			constants_phantom: PhantomData,
		}
	}

	pub fn bind_pipe<F: FnOnce(&mut BoundPipe<Vertex, Uniforms, Index, Constants>)>(
		&self,
		encoder: &mut RenderSubpassCommon<Backend>,
		draws: F,
	) {
		encoder.bind_graphics_pipeline(unsafe { self.pipe.get_ref() });
		let mut bp = BoundPipe {
			encoder,
			vert_phantom: PhantomData,
			uniforms_phantom: PhantomData,
			index_phantom: PhantomData,
			constants_phantom: PhantomData,
		};
		draws(&mut bp);
	}
}

impl<
		'a,
		'b: 'a,
		Vertex: VertexInfo<Vertex>,
		Uniforms: UniformInfo,
		Index: IndexType,
		Constants: PushConstantInfo,
	> BoundPipe<'a, 'b, Vertex, Uniforms, Index, Constants>
{
	pub fn draw_mesh(
		&mut self,
		mesh: &Mesh<Vertex, Uniforms, Index, Constants>,
		descriptor_idx: usize,
		constants: Constants,
	) {
		mesh.draw(self.encoder, descriptor_idx, constants);
	}
}

impl<
		'a,
		Vertex: VertexInfo<Vertex>,
		Uniforms: UniformInfo,
		Index: IndexType,
		Constants: PushConstantInfo,
	> Drop for Pipeline<'a, Vertex, Uniforms, Index, Constants>
{
	fn drop(&mut self) {
		let device = &self.pass.swapchain.data.device;
		device.destroy_graphics_pipeline(MaybeUninit::take(&mut self.pipe));
		println!("Dropped Pipeline");
	}
}
