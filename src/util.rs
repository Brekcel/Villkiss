pub trait TakeExt<T> {
	fn take(slot: &mut Self) -> T;
}

impl<T> TakeExt<T> for std::mem::MaybeUninit<T> {
	fn take(slot: &mut Self) -> T {
		let value = std::mem::replace(slot, Self::uninitialized());
		unsafe { value.into_inner() }
	}
}

//pub trait GFXRes: Drop {
//	fn data(&self) -> &HALData;
//	fn device(&self) -> &<Backend as gfx_hal::Backend>::Device {
// &self.data().device() }
//}
//
//#[macro_export]
//macro_rules! impl_res {
//	($name: ty, $($x:tt)*) => {
//		impl GFXRes for $name {
//			fn data(&self) -> &HALData {
//				&self.$($x)*
//			}
//		}
//	};
//}
