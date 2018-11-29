use std::ops::Index;

use gfx_hal::{image::Extent, Device};

use crate::gfx_back::Backend;
use crate::{HALData, ImageView, RenderPass, Swapchain};

pub struct FrameBuffer<'a> {
	data: &'a HALData,
	frames: Vec<<Backend as gfx_hal::Backend>::Framebuffer>,
}

impl<'a> FrameBuffer<'a> {
	pub fn create<'b>(
		data: &'a HALData,
		pass: &'b RenderPass,
		views: &'b [Vec<&ImageView>],
		sizes: &'b [Extent],
	) -> FrameBuffer<'a> {
		println!("Creating Framebuffer");
		let device = &data.device;
		let frames = views
			.iter()
			.enumerate()
			.map(|(i, iv)| {
				device
					.create_framebuffer(pass.pass(), iv.iter().map(|i| i.view()), sizes[i])
					.unwrap()
			})
			.collect::<Vec<_>>();
		FrameBuffer { data, frames }
	}

	pub fn create_same_size<'b>(
		data: &'a HALData,
		pass: &'b RenderPass,
		views: &'b [Vec<&ImageView>],
		size: &'b Extent,
	) -> FrameBuffer<'a> {
		Self::create(data, pass, views, &vec![*size; views.len()])
	}

	pub fn from_swapchain<'b>(
		data: &'a HALData,
		swap: &'b Swapchain,
		pass: &'b RenderPass,
	) -> FrameBuffer<'a> {
		//		#[cfg(not(feature = "gl"))]
		let fb = {
			let depth = &swap.depth_tex;
			let views = swap
				.image_views
				.iter()
				.map(|iv| vec![iv, depth.view()])
				.collect::<Vec<_>>();
			Self::create_same_size(data, pass, views.as_slice(), &swap.dims)
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

	fn index(&self, index: usize) -> &<Self as Index<usize>>::Output {
		&self.frames[index]
	}
}

impl<'a> Drop for FrameBuffer<'a> {
	fn drop(&mut self) {
		let device = &self.data.device;
		self.frames
			.drain(..)
			.for_each(|frame| device.destroy_framebuffer(frame));
		println!("Dropped Framebuffer");
	}
}
