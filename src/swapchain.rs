use std::mem::MaybeUninit;

use gfx_hal::{AcquireError, Backbuffer, Device, format::{ChannelType, Format},
	FrameSync, image::Extent, PresentMode, Surface, SwapchainConfig};
#[cfg(not(feature = "gl"))]
use gfx_hal::{image::ViewKind};
#[cfg(not(feature = "gl"))]
use crate::ImageView;


use crate::{HALData, Semaphore};
use crate::gfx_back::Backend;
use crate::util::TakeExt;

pub struct Swapchain<'a> {
	pub(crate) data: &'a HALData,
	pub(crate) dims: Extent,
	pub(crate) swapchain: MaybeUninit<<Backend as gfx_hal::Backend>::Swapchain>,
	pub(crate) backbuffer: Backbuffer<Backend>,
	#[cfg(not(feature = "gl"))]
	pub(crate) image_views: Vec<ImageView<'a>>,
	#[cfg(feature = "gl")]
	pub(crate) fbo: <Backend as gfx_hal::Backend>::Framebuffer
}

impl<'a> Swapchain<'a> {
	pub fn create<'b>(data: &'b HALData) -> Swapchain<'a> where 'b: 'a {
		let device = &data.device;
		println!("Creating Swapchain");
		let (capabilities, formats, _) = data.surface.borrow().compatibility(&data.adapter.physical_device);
		let surface_color_format = match formats {
			Some(choices) => choices.into_iter().find(|format|
				format.base_format().1 == ChannelType::Srgb
			).unwrap(),
			None => Format::Rgba8Srgb
		};
		let swap_config =
			SwapchainConfig::from_caps(&capabilities, surface_color_format).with_mode(PresentMode::Mailbox);
		let dims = swap_config.extent.to_extent();
		let (swapchain, backbuffer) =
			device.create_swapchain(&mut data.surface.borrow_mut(), swap_config, None).unwrap();
		#[cfg(not(feature = "gl"))]
		let image_views =
			match backbuffer {
				Backbuffer::Images(ref i) => {
					i.iter().map(|i| {
						ImageView::create(data, i, surface_color_format, ViewKind::D2)
					}).collect::<Vec<_>>()
				},
				_ => panic!("Non-opengl backend gave framebuffers!")
			};


		#[cfg(feature = "gl")]
		let fbo = match backbuffer {
			Backbuffer::Framebuffer(fbo) => fbo,
			_ => panic!("Opengl backend gave images!")
		};
		Swapchain {
			data,
			dims,
			swapchain: MaybeUninit::new(swapchain),
			backbuffer,
			#[cfg(not(feature = "gl"))]
			image_views,
			#[cfg(feature = "gl")]
			fbo,
		}
	}

	pub fn acquire_next_image<'b>(&'b mut self, sem: &'b mut Semaphore) -> Result<u32, AcquireError> {
		(unsafe { self.swapchain.get_mut() } as &mut gfx_hal::Swapchain<Backend>).
			acquire_image(!0, FrameSync::Semaphore(sem.semaphore()))
	}

	pub fn image_count(&self) -> usize {
		#[cfg(not(feature = "gl"))]
		let count = self.image_views.len();
		#[cfg(feature = "gl")]
		let count = 1;
		count
	}

	pub fn dims(&self) -> &Extent {
		&self.dims
	}
}


impl<'a> Drop for Swapchain<'a> {
	fn drop(&mut self) {
		let device = &self.data.device;
//		#[cfg(not(feature = "gl"))]
		device.destroy_swapchain(MaybeUninit::take(&mut self.swapchain));
		println!("Dropped Swapchain");
	}
}