use std::ops::Index;

use gfx_hal::{
	image::Extent,
	Device,
};

use crate::{
	gfx_back::Backend,
	ImageView,
	RenderPass,
};

pub struct FrameBuffer<'a> {
	pass: &'a RenderPass<'a>,
	frames: Vec<<Backend as gfx_hal::Backend>::Framebuffer>,
}

impl<'a> FrameBuffer<'a> {
	pub(crate) fn create<'b>(
		pass: &'a RenderPass,
		views: &'b [Vec<&ImageView>],
		sizes: &'b [Extent],
	) -> FrameBuffer<'a> {
		println!("Creating Framebuffer");
		let device = &pass.swapchain.data.device;
		let frames = views
			.iter()
			.enumerate()
			.map(|(i, iv)| unsafe {
				device
					.create_framebuffer(pass.pass(), iv.iter().map(|i| i.view()), sizes[i])
					.unwrap()
			})
			.collect::<Vec<_>>();
		FrameBuffer { pass, frames }
	}

	pub(crate) fn create_same_size<'b>(
		pass: &'a RenderPass,
		views: &'b [Vec<&ImageView>],
		size: Extent,
	) -> FrameBuffer<'a> {
		Self::create(pass, views, &vec![size; views.len()])
	}

	pub(crate) fn from_swapchain(pass: &'a RenderPass) -> FrameBuffer<'a> {
		//		#[cfg(not(feature = "gl"))]
		let fb = {
			let depth = &pass.swapchain.depth_tex;
			let views = pass
				.swapchain
				.image_views
				.iter()
				.map(|iv| vec![iv, depth.view()])
				.collect::<Vec<_>>();
			Self::create_same_size(pass, views.as_slice(), pass.swapchain.dims.clone())
		};
		//		#[cfg(feature = "gl")]
		//		let fb = FrameBuffer {
		//			data,
		//			frames: vec![swap.fbo]
		//		};
		fb
	}
}

impl<'a> Index<usize> for FrameBuffer<'a> {
	type Output = <Backend as gfx_hal::Backend>::Framebuffer;

	fn index(&self, index: usize) -> &<Self as Index<usize>>::Output { &self.frames[index] }
}

impl<'a> Drop for FrameBuffer<'a> {
	fn drop(&mut self) {
		let device = &self.pass.swapchain.data.device;
		self.frames
			.drain(..)
			.for_each(|frame| unsafe { device.destroy_framebuffer(frame) });
		println!("Dropped Framebuffer");
	}
}
