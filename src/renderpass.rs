use std::mem::MaybeUninit;

use gfx_hal::{Device, format::{ChannelType, Format}, image::{Access, Layout}, pass::{Attachment, AttachmentLoadOp, AttachmentOps,
	AttachmentStoreOp, Subpass, SubpassDependency, SubpassDesc, SubpassRef},
	pso::PipelineStage, Surface};

use crate::gfx_back::Backend;
use crate::HALData;
use crate::util::TakeExt;

pub struct RenderPass<'a> {
	data: &'a HALData,
	pub(crate) pass: MaybeUninit<<Backend as gfx_hal::Backend>::RenderPass>,
}

impl<'a> RenderPass<'a> {
	pub fn create<'b: 'a>(data: &'a HALData) -> RenderPass {
		println!("Creating Renderpass");
		let render_pass = {
			let (capabilities, formats, _) = data.surface.borrow().compatibility(&data.adapter.physical_device);
			let surface_color_format = match formats {
				Some(choices) => choices.into_iter().find(|format|
					format.base_format().1 == ChannelType::Srgb
				).unwrap(),
				None => Format::Rgba8Srgb
			};

			let color_attachment = Attachment {
				format: Some(surface_color_format),
				samples: 1,
				ops: AttachmentOps::new(AttachmentLoadOp::Clear, AttachmentStoreOp::Store),
				stencil_ops: AttachmentOps::DONT_CARE,
				layouts: Layout::Undefined..Layout::Present,
			};

			let subpass = SubpassDesc {
				colors: &[(0, Layout::ColorAttachmentOptimal)],
				depth_stencil: None,
				inputs: &[],
				resolves: &[],
				preserves: &[],
			};

			let dependency = SubpassDependency {
				passes: SubpassRef::External..SubpassRef::Pass(0),
				stages: PipelineStage::COLOR_ATTACHMENT_OUTPUT..PipelineStage::COLOR_ATTACHMENT_OUTPUT,
				accesses: Access::empty()..(Access::COLOR_ATTACHMENT_READ | Access::COLOR_ATTACHMENT_WRITE),
			};

			data.device.create_render_pass(&[color_attachment], &[subpass], &[dependency]).unwrap()
		};
		RenderPass {
			data,
			pass: MaybeUninit::new(render_pass)
		}
	}

	pub(crate) fn make_subpass<'b>(&'a self) -> Subpass<'b, Backend> where 'a: 'b {
		Subpass {
			index: 0,
			main_pass: unsafe { self.pass.get_ref() }
		}
	}

	pub fn pass(&self) -> &<Backend as gfx_hal::Backend>::RenderPass {
		unsafe { self.pass.get_ref() }
	}
}

impl<'a> Drop for RenderPass<'a> {
	fn drop(&mut self) {
		let device = &self.data.device;
		device.destroy_render_pass(MaybeUninit::take(&mut self.pass));
		println!("Dropped Renderpass");
	}
}