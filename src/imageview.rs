use std::mem::MaybeUninit;

use gfx_hal::{Device, format::{Aspects, Format, Swizzle}, image::{SubresourceRange, ViewKind}};

use crate::{HALData};
use crate::gfx_back::Backend;
use crate::util::TakeExt;

pub struct ImageView<'a> {
	data: &'a HALData,
	view: MaybeUninit<<Backend as gfx_hal::Backend>::ImageView>,
}

impl<'a> ImageView<'a> {
	pub(crate) fn create<'b>(data: &'a HALData,
							 image: &'b <Backend as gfx_hal::Backend>::Image,
							 format: Format,
							 kind: ViewKind
	) -> ImageView<'a> {
		println!("Creating ImageView");
		let device = &data.device;
		let sub_range = SubresourceRange {
			aspects: Aspects::COLOR,
			levels: 0..1,
			layers: 0..1
		};
		let view = device.create_image_view(image, kind, format, Swizzle::NO, sub_range).unwrap();
		ImageView {
			data,
			view: MaybeUninit::new(view)
		}
	}

	pub(crate) fn view(&self) -> &<Backend as gfx_hal::Backend>::ImageView {
		unsafe { self.view.get_ref() }
	}
}

impl<'a> Drop for ImageView<'a> {
	fn drop(&mut self) {
		let device = &self.data.device;
		device.destroy_image_view(MaybeUninit::take(&mut self.view));
		println!("Dropped ImageView");
	}
}