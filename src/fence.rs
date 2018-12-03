use std::mem::MaybeUninit;

use gfx_hal::Device;

use crate::gfx_back::Backend;
use crate::HALData;
use crate::util::TakeExt;

pub struct Fence<'a> {
	data: &'a HALData,
	fence: MaybeUninit<<Backend as gfx_hal::Backend>::Fence>
}

impl<'a> Fence<'a> {
	pub(crate) fn create(data: &'a HALData) -> Fence<'a> {
		println!("Creating Fence");
		let fence = data.device.create_fence(true).unwrap();
		Fence {
			data,
			fence: MaybeUninit::new(fence)
		}
	}

	pub fn create_n(data: &'a HALData, num: usize) -> Box<[Fence<'a>]> {
		(0..num).map(|_| Self::create(data)).collect::<Vec<_>>().into_boxed_slice()
	}

	pub fn reset(&self) {
		let fence = self.fence();
		self.data.device.reset_fence(fence).unwrap();
	}

	pub fn wait(&self) {
		let fence = self.fence();
		self.data.device.wait_for_fence(fence, !0).unwrap();
	}

	pub fn fence(&self) -> &<Backend as gfx_hal::Backend>::Fence {
		unsafe { self.fence.get_ref() }
	}

	pub fn mut_fence(&mut self) -> &mut <Backend as gfx_hal::Backend>::Fence {
		unsafe { self.fence.get_mut() }
	}
}

impl<'a> Drop for Fence<'a> {
	fn drop(&mut self) {
		let device = &self.data.device;
		device.destroy_fence(MaybeUninit::take(&mut self.fence));
		println!("Dropped Fence")
	}
}