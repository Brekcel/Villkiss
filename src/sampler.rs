use crate::{
	gfx_back::Backend,
	util::TakeExt,
	HALData,
};
use std::mem::MaybeUninit;

use gfx_hal::{
	image::SamplerInfo,
	Device,
};

pub struct Sampler<'a> {
	data: &'a HALData,
	pub(crate) sampler: MaybeUninit<<Backend as gfx_hal::Backend>::Sampler>,
}

impl<'a> Sampler<'a> {
	pub(crate) fn create(data: &'a HALData, sampler_info: SamplerInfo) -> Sampler {
		println!("Creating Sampler");
		let device = &data.device;
		let sampler = unsafe { device.create_sampler(sampler_info).unwrap() };
		Sampler {
			data,
			sampler: MaybeUninit::new(sampler),
		}
	}

	pub fn sampler(&self) -> &<Backend as gfx_hal::Backend>::Sampler {
		unsafe { self.sampler.get_ref() }
	}
}

impl<'a> Drop for Sampler<'a> {
	fn drop(&mut self) {
		let device = &self.data.device;
		unsafe {
			device.destroy_sampler(MaybeUninit::take(&mut self.sampler));
		}
		println!("Dropped Sampler");
	}
}
