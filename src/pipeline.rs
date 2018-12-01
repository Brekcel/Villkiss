use std::mem::MaybeUninit;

use gfx_hal::{Device, Primitive, pso::{BlendState, DepthStencilDesc, StencilTest, DepthTest, Comparison, PolygonMode, Face, FrontFace, ColorBlendDesc, ColorMask,
	GraphicsPipelineDesc, Rasterizer}};

use crate::{HALData, RenderPass, Shader, Swapchain};
use crate::gfx_back::Backend;
use crate::util::TakeExt;

pub struct Pipeline<'a> {
	data: &'a HALData,
	pipe: MaybeUninit<<Backend as gfx_hal::Backend>::GraphicsPipeline>,
}

impl<'a> Pipeline<'a> {
	pub fn create<'b>(data: &'a HALData,
					  shader: &'b Shader,
					  renderpass: &'b RenderPass,
					  swapchain: &'b Swapchain) -> Pipeline<'a> {
		pub const RASTERIZER: Rasterizer = Rasterizer {
			polygon_mode: PolygonMode::Fill,
			cull_face: Face::BACK,
			front_face: FrontFace::CounterClockwise,
			depth_clamping: false,
			depth_bias: None,
			conservative: false,
		};

		println!("Creating Pipeline");
		let device = &data.device;
		let pool_count = swapchain.image_count();
		let shad_set = shader.make_set();
		let pipe_layout = shader.pipe_layout();
		let desc_layout = shader.desc_layout();
		let subpass = renderpass.make_subpass();

		let mut pipeline_desc = GraphicsPipelineDesc::new(
			shad_set, Primitive::TriangleList, RASTERIZER,
			pipe_layout, subpass
		);
		pipeline_desc.depth_stencil = DepthStencilDesc {
			depth: DepthTest::On {
				fun: Comparison::Less,
				write: true
			},
//			depth: DepthTest::Off,
			depth_bounds: false,
			stencil: StencilTest::Off
		};
		pipeline_desc.blender.targets.push(ColorBlendDesc(ColorMask::ALL, BlendState::ALPHA));

		shader.describe_vertices(&mut pipeline_desc.vertex_buffers, &mut pipeline_desc.attributes);

		let pipe = device.create_graphics_pipeline(&pipeline_desc, None).unwrap();

		Pipeline {
			data,
			pipe: MaybeUninit::new(pipe),
		}
	}

	pub fn pipe(&self) -> &<Backend as gfx_hal::Backend>::GraphicsPipeline {
		unsafe { self.pipe.get_ref() }
	}
}

impl<'a> Drop for Pipeline<'a> {
	fn drop(&mut self) {
		let device = &self.data.device;
		device.destroy_graphics_pipeline(MaybeUninit::take(&mut self.pipe));
		println!("Dropped Pipeline");
	}
}
