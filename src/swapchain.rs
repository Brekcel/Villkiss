use std::{
	cell::RefCell,
	mem::MaybeUninit,
};

use gfx_hal::{
	format::{
		Aspects,
		ChannelType,
		Format,
	},
	image::{
		Extent,
		Kind,
		WrapMode,
	},
	window::Extent2D,
	AcquireError,
	Backbuffer,
	CompositeAlpha,
	Device,
	FrameSync,
	PresentMode,
	Surface,
	Swapchain as HALSwapchain,
	SwapchainConfig,
};
//#[cfg(not(feature = "gl"))]
use gfx_hal::image::ViewKind;
//#[cfg(not(feature = "gl"))]
use crate::ImageView;

use crate::{
	gfx_back::Backend,
	texture::{
		MipMaps,
		Texture,
		TextureInfo,
	},
	util::TakeExt,
	BufferPool,
	HALData,
	RenderPass,
	Semaphore,
};

pub struct Swapchain<'a> {
	pub(crate) data: &'a HALData,
	pub(crate) dims: Extent,
	pub(crate) swapchain: MaybeUninit<RefCell<<Backend as gfx_hal::Backend>::Swapchain>>,
	pub(crate) backbuffer: Backbuffer<Backend>,
	//	#[cfg(not(feature = "gl"))]
	pub(crate) image_views: Vec<ImageView<'a>>,
	pub(crate) depth_tex: Texture<'a>,
	/*	#[cfg(feature = "gl")]
	 *	pub(crate) fbo: <Backend as gfx_hal::Backend>::Framebuffer, */
}

impl<'a> Swapchain<'a> {
	pub(crate) fn create<'b>(data: &'a HALData, pool: &'a BufferPool<'a>) -> Swapchain<'a> {
		println!("Creating Swapchain");
		let device = &data.device;
		let (capabilities, formats, _, composite_alpha) = data
			.surface
			.borrow()
			.compatibility(&data.adapter.physical_device);
		let surface_color_format = match formats {
			Some(choices) => choices
				.into_iter()
				.find(|format| format.base_format().1 == ChannelType::Srgb)
				.unwrap(),
			None => Format::Rgba8Srgb,
		};
		let swap_config = {
			let mut swap_config = SwapchainConfig::from_caps(
				&capabilities,
				surface_color_format,
				Extent2D {
					width: 800,
					height: 600,
				},
			)
			.with_mode(PresentMode::Mailbox);
			swap_config.composite_alpha =
				*composite_alpha.get(0).unwrap_or(&CompositeAlpha::Inherit);
			swap_config
		};
		let dims = swap_config.extent.to_extent();
		let (swapchain, backbuffer) = unsafe {
			device
				.create_swapchain(&mut data.surface.borrow_mut(), swap_config, None)
				.unwrap()
		};
		let depth_tex = pool.create_texture(TextureInfo {
			kind: Kind::D2(dims.width, dims.height, 1, 1),
			format: Format::D32FloatS8Uint,
			mipmaps: MipMaps::None,
			pixels: None,
			wrap_mode: (WrapMode::Border, WrapMode::Border, WrapMode::Border),
		});
		//		#[cfg(not(feature = "gl"))]
		let image_views = match backbuffer {
			Backbuffer::Images(ref i) => i
				.iter()
				.map(|i| {
					ImageView::create(
						data,
						i,
						surface_color_format,
						ViewKind::D2,
						Aspects::COLOR,
						1,
					)
				})
				.collect::<Vec<_>>(),
			_ => panic!("Non-opengl backend gave framebuffers!"),
		};

		#[cfg(feature = "gl")]
		let fbo = match backbuffer {
			Backbuffer::Framebuffer(fbo) => fbo,
			_ => panic!("Opengl backend gave images!"),
		};
		Swapchain {
			data,
			dims,
			swapchain: MaybeUninit::new(RefCell::new(swapchain)),
			backbuffer,
			//			#[cfg(not(feature = "gl"))]
			image_views,
			depth_tex,
			/*			#[cfg(feature = "gl")]
			 *			fbo, */
		}
	}

	pub fn acquire_next_image<'b>(&'b self, sem: &'b mut Semaphore) -> Result<u32, AcquireError> {
		unsafe {
			self.swapchain
				.get_ref()
				.borrow_mut()
				.acquire_image(!0, FrameSync::Semaphore(sem.semaphore()))
		}
	}

	pub fn image_count(&self) -> usize {
		//		#[cfg(not(feature = "gl"))]
		let count = self.image_views.len();
		//		#[cfg(feature = "gl")]
		//		let count = 1;
		count
	}

	pub fn dims(&self) -> &Extent { &self.dims }

	pub fn create_renderpass(&self) -> RenderPass { RenderPass::create(self) }
}

impl<'a> Drop for Swapchain<'a> {
	fn drop(&mut self) {
		let device = &self.data.device;
		//		#[cfg(not(feature = "gl"))]
		unsafe {
			device.destroy_swapchain(RefCell::into_inner(MaybeUninit::take(&mut self.swapchain)));
		}
		println!("Dropped Swapchain");
	}
}
