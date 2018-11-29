use std::ops::Index;

use gfx_hal::{Device, image::Extent};

use crate::{HALData, ImageView, RenderPass, Swapchain};
use crate::gfx_back::Backend;

pub struct FrameBuffer<'a> {
	data: &'a HALData,
	frames: Vec<<Backend as gfx_hal::Backend>::Framebuffer>,
}

impl<'a> FrameBuffer<'a> {
	pub fn create<'b>(data: &'a HALData, pass: &'b RenderPass,
					  views: &'b [ImageView], sizes: &'b [Extent]) -> FrameBuffer<'a> {
		println!("Creating Framebuffer");
		let frames = views.iter().enumerate().map(|(i, iv)|
			data.device.create_framebuffer(
				unsafe { pass.pass.get_ref() }, vec![iv.view()], sizes[i]
			).unwrap()).collect::<Vec<_>>();
		FrameBuffer {
			data,
			frames
		}
	}

	pub fn create_same_size<'b>(data: &'a HALData, pass: &'b RenderPass,
								views: &'b [ImageView], size: &'b Extent) -> FrameBuffer<'a> {
		Self::create(data, pass, views, &vec![*size; views.len()])
	}

	pub fn from_swapchain<'b>(data: &'a HALData, swap: &'b Swapchain, pass: &'b RenderPass) -> FrameBuffer<'a> {
		#[cfg(not(feature = "gl"))]
		let fb = Self::create_same_size(data, pass, &swap.image_views, &swap.dims);
		#[cfg(feature = "gl")]
		let fb = FrameBuffer {
			data,
			frames: vec![swap.fbo]
		};
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
		self.frames.drain(..).for_each(|frame| device.destroy_framebuffer(frame));
		println!("Dropped Framebuffer");
	}
}