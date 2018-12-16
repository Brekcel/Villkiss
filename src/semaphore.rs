use std::mem::MaybeUninit;

use gfx_hal::Device;

use crate::{
	gfx_back::Backend,
	util::TakeExt,
	HALData,
};

pub struct Semaphore<'a> {
	data: &'a HALData,
	semaphore: MaybeUninit<<Backend as gfx_hal::Backend>::Semaphore>,
}

impl<'a> Semaphore<'a> {
	pub(crate) fn create(data: &'a HALData) -> Semaphore<'a> {
		println!("Creating Semaphore");
		let semaphore = data.device.create_semaphore().unwrap();
		Semaphore {
			data,
			semaphore: MaybeUninit::new(semaphore),
		}
	}

	pub(crate) fn create_n(data: &'a HALData, num: usize) -> Box<[Semaphore<'a>]> {
		(0..num)
			.map(|_| Self::create(data))
			.collect::<Vec<_>>()
			.into_boxed_slice()
	}

	pub fn semaphore(&self) -> &<Backend as gfx_hal::Backend>::Semaphore {
		unsafe { self.semaphore.get_ref() }
	}

	pub fn mut_semaphore(&mut self) -> &mut <Backend as gfx_hal::Backend>::Semaphore {
		unsafe { self.semaphore.get_mut() }
	}
}

impl<'a> Drop for Semaphore<'a> {
	fn drop(&mut self) {
		let device = &self.data.device;
		device.destroy_semaphore(MaybeUninit::take(&mut self.semaphore));
		println!("Dropped Semaphore")
	}
}
