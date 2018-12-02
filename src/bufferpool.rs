use std::{
	cell::RefCell,
	mem::{size_of, MaybeUninit},
};

use gfx_hal::{
	buffer::{self, Usage},
	memory::Properties,
	Device, PhysicalDevice,
};
use gfx_memory::{Block, MemoryAllocator, SmartAllocator, Type};

use crate::gfx_back::Backend;
use crate::texture::{Texture, TextureInfo};
use crate::util::TakeExt;
use crate::{CommandPool, Buffer, HALData};

pub struct BufferPool<'a> {
	pub(crate) data: &'a HALData,
	pub(crate) command_pool: &'a CommandPool<'a>,
	pub(crate) allocator: MaybeUninit<RefCell<SmartAllocator<Backend>>>,
	pub(crate) staging_buf: MaybeUninit<StagingBuffer>,
}

//DUMB Storage struct. HAS to be manually dropped.
pub(crate) struct StagingBuffer {
	pub(crate) block: <SmartAllocator<Backend> as MemoryAllocator<Backend>>::Block,
	pub(crate) buffer: <Backend as gfx_hal::Backend>::Buffer,
}


impl<'a> BufferPool<'a> {
	pub(crate) fn create(command_pool: &'a CommandPool) -> BufferPool<'a> {
		println!("Creating BufferPool");
		let data = &command_pool.data;
		let device = &data.device;
		let phys_device = &data.adapter.physical_device;
		let mut allocator =
			SmartAllocator::new(phys_device.memory_properties(), 4096, 8, 64, 134217728);
		let staging_buf = StagingBuffer::create(data, &mut allocator);
		BufferPool {
			data,
			command_pool,
			allocator: MaybeUninit::new(RefCell::new(allocator)),
			staging_buf: MaybeUninit::new(staging_buf),
		}
	}

	pub fn create_buffer(&self, size: usize, usage: Usage, uses_staging: bool) -> Buffer {
		Buffer::create(self, size, usage, uses_staging)
	}

	pub fn create_buffer_data<'b, T>(
		&self,
		data: &'b [T],
		usage: Usage,
		uses_staging: bool,
	) -> Buffer {
		let buf = self.create_buffer(size_of::<T>() * data.len(), usage, uses_staging);
		buf.upload(data, 0);
		buf
	}

	pub fn create_texture<'b>(&self, info: TextureInfo<'b>) -> Texture {
		Texture::create(self, info)
	}
}

impl<'a> Drop for BufferPool<'a> {
	fn drop(&mut self) {
		let device = &self.data.device;
		MaybeUninit::take(&mut self.staging_buf).manual_drop(
			&self.data,
			&mut unsafe { self.allocator.get_ref() }.borrow_mut(),
		);
		RefCell::into_inner(MaybeUninit::take(&mut self.allocator))
			.dispose(&device)
			.unwrap();
		println!("Dropped BufferPool");
	}
}

// 16 Megabytes
//2usize.pow(26)
const STAGING_BUF_SIZE: usize = 67108864;

impl StagingBuffer {
	fn create(data: &HALData, allocator: &mut SmartAllocator<Backend>) -> StagingBuffer {
		println!("Creating StagingBuffer");
		let device = &data.device;
		let unbound_buf = device
			.create_buffer(STAGING_BUF_SIZE as buffer::Offset, Usage::TRANSFER_SRC)
			.unwrap();
		let reqs = device.get_buffer_requirements(&unbound_buf);
		let block = allocator
			.alloc(
				device,
				(
					Type::General,
					Properties::CPU_VISIBLE | Properties::COHERENT,
				),
				reqs,
			)
			.unwrap();
		let buffer = device
			.bind_buffer_memory(block.memory(), block.range().start, unbound_buf)
			.unwrap();

		StagingBuffer { block, buffer }
	}

	pub(crate) fn upload<T>(&self, data: &[T], device: &<Backend as gfx_hal::Backend>::Device) {
		Buffer::do_upload(
			data,
			self.block.range().start as usize,
			device,
			self.block.memory(),
		)
	}

	fn manual_drop(self, data: &HALData, alloc: &mut SmartAllocator<Backend>) {
		let device = &data.device;
		device.destroy_buffer(self.buffer);
		alloc.free(device, self.block);
		println!("Dropped StagingBuffer");
	}
}