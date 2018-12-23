use std::mem::MaybeUninit;

use gfx_hal::{
	format::{
		ChannelType,
		Format,
	},
	image::{
		Access,
		Extent,
		Layout,
	},
	pass::{
		Attachment,
		AttachmentLoadOp,
		AttachmentOps,
		AttachmentStoreOp,
		Subpass,
		SubpassDependency,
		SubpassDesc,
		SubpassRef,
	},
	pso::PipelineStage,
	Device,
	Surface,
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
	FrameBuffer,
	ImageView,
	Pipeline,
	Swapchain,
};

pub struct RenderPass<'a> {
	pub(crate) swapchain: &'a Swapchain<'a>,
	pub(crate) pass: MaybeUninit<<Backend as gfx_hal::Backend>::RenderPass>,
}

impl<'a> RenderPass<'a> {
	pub(crate) fn create(swapchain: &'a Swapchain) -> RenderPass<'a> {
		println!("Creating Renderpass");
		let device = &swapchain.data.device;
		let render_pass = {
			let (capabilities, formats, _) = swapchain
				.data
				.surface
				.borrow()
				.compatibility(&swapchain.data.adapter.physical_device);
			let surface_color_format = match formats {
				Some(choices) => choices
					.into_iter()
					.find(|format| format.base_format().1 == ChannelType::Srgb)
					.unwrap(),
				None => Format::Rgba8Srgb,
			};

			let color_attachment = Attachment {
				format: Some(surface_color_format),
				samples: 1,
				ops: AttachmentOps::new(AttachmentLoadOp::Clear, AttachmentStoreOp::Store),
				stencil_ops: AttachmentOps::DONT_CARE,
				layouts: Layout::Undefined..Layout::Present,
			};

			let depth_stencil = &swapchain.depth_tex;

			let depth_attachment = Attachment {
				format: Some(depth_stencil.format),
				samples: 1,
				ops: AttachmentOps::new(AttachmentLoadOp::Clear, AttachmentStoreOp::DontCare),
				stencil_ops: AttachmentOps::DONT_CARE,
				layouts: Layout::Undefined..Layout::DepthStencilAttachmentOptimal,
			};

			let subpass = SubpassDesc {
				colors: &[(0, Layout::ColorAttachmentOptimal)],
				depth_stencil: Some(&(1, Layout::DepthStencilAttachmentOptimal)),
				inputs: &[],
				resolves: &[],
				preserves: &[],
			};

			let dependency = SubpassDependency {
				passes: SubpassRef::External..SubpassRef::Pass(0),
				stages: PipelineStage::COLOR_ATTACHMENT_OUTPUT..
					PipelineStage::COLOR_ATTACHMENT_OUTPUT,
				accesses: Access::empty()..
					(Access::COLOR_ATTACHMENT_READ | Access::COLOR_ATTACHMENT_WRITE),
			};

			device
				.create_render_pass(
					&[color_attachment, depth_attachment],
					&[subpass],
					&[dependency],
				)
				.unwrap()
		};
		RenderPass {
			swapchain,
			pass: MaybeUninit::new(render_pass),
		}
	}

	pub fn create_framebuffer_from_chain(&self) -> FrameBuffer { FrameBuffer::from_swapchain(self) }

	pub fn create_framebuffer_same_size<'b>(
		&self,
		views: &'b [Vec<&ImageView>],
		size: Extent,
	) -> FrameBuffer {
		FrameBuffer::create_same_size(self, views, size)
	}

	pub fn create_framebuffer<'b>(
		&self,
		views: &'b [Vec<&ImageView>],
		sizes: &'b [Extent],
	) -> FrameBuffer {
		FrameBuffer::create(self, views, sizes)
	}

	pub(crate) fn make_subpass<'b>(&'a self) -> Subpass<'b, Backend>
	where
		'a: 'b,
	{
		Subpass {
			index: 0,
			main_pass: unsafe { self.pass.get_ref() },
		}
	}

	pub fn pass(&self) -> &<Backend as gfx_hal::Backend>::RenderPass {
		unsafe { self.pass.get_ref() }
	}

	pub fn create_pipeline<
		Vertex: VertexInfo<Vertex>,
		Uniforms: UniformInfo,
		Index: IndexType,
		Constants: PushConstantInfo,
	>(
		&'a self,
		shader: &'a Shader<'a, Vertex, Uniforms, Index, Constants>,
	) -> Pipeline<'a, Vertex, Uniforms, Index, Constants> {
		Pipeline::create(self, shader)
	}
}

impl<'a> Drop for RenderPass<'a> {
	fn drop(&mut self) {
		let device = &self.swapchain.data.device;
		device.destroy_render_pass(MaybeUninit::take(&mut self.pass));
		println!("Dropped Renderpass");
	}
}
