use std::mem::MaybeUninit;

use gfx_hal::Device;

use crate::{
	gfx_back::Backend,
	util::TakeExt,
	HALData,
};

pub struct Fence<'a> {
	data: &'a HALData,
	fence: MaybeUninit<<Backend as gfx_hal::Backend>::Fence>,
}

impl<'a> Fence<'a> {
	pub(crate) fn create(data: &HALData) -> Fence {
		println!("Creating Fence");
		let fence = data.device().create_fence(true).unwrap();
		Fence {
			data,
			fence: MaybeUninit::new(fence),
		}
	}

	pub fn reset(&self) {
		let fence = self.fence();
		unsafe {
			self.data.device().reset_fence(fence).unwrap();
		}
	}

	pub fn wait(&self) {
		let fence = self.fence();
		unsafe {
			self.data.device().wait_for_fence(fence, !0).unwrap();
		}
	}

	pub fn wait_n_reset(&self) {
		self.wait();
		self.reset();
	}

	pub fn fence(&self) -> &<Backend as gfx_hal::Backend>::Fence { unsafe { self.fence.get_ref() } }

	pub fn mut_fence(&mut self) -> &mut <Backend as gfx_hal::Backend>::Fence {
		unsafe { self.fence.get_mut() }
	}
}

impl<'a> Drop for Fence<'a> {
	fn drop(&mut self) {
		let device = self.data.device();
		unsafe {
			device.destroy_fence(MaybeUninit::take(&mut self.fence));
		}
		println!("Dropped Fence")
	}
}
