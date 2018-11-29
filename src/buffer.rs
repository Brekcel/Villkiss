use core::ops::Range;
use std::{cell::RefCell, mem::{MaybeUninit, size_of}};

use gfx_hal::{buffer::{self, Usage}, command::BufferCopy, Device, PhysicalDevice,
	format::Format, memory::Properties};
use gfx_memory::{Block, MemoryAllocator, SmartAllocator, Type};

use crate::{CommandPool, HALData};
use crate::gfx_back::Backend;
use crate::texture::{Texture, TextureInfo};
use crate::util::TakeExt;

pub struct BufferPool<'a> {
	pub(crate) data: &'a HALData,
	pub(crate) command_pool: &'a CommandPool<'a>,
	pub(crate) allocator: MaybeUninit<RefCell<SmartAllocator<Backend>>>,
	pub(crate) staging_buf: MaybeUninit<StagingBuffer>,
}

pub struct Buffer<'a> {
	parent: &'a BufferPool<'a>,
	block: MaybeUninit<<SmartAllocator<Backend> as MemoryAllocator<Backend>>::Block>,
	buffer: MaybeUninit<<Backend as gfx_hal::Backend>::Buffer>,
	usage: Usage,
	props: Properties,
}

pub struct BufferView<'a> {
	data: &'a HALData,
	view: MaybeUninit<<Backend as gfx_hal::Backend>::BufferView>,
}

//DUMB Storage struct. HAS to be manually dropped.
pub(crate) struct StagingBuffer {
	pub(crate) block: <SmartAllocator<Backend> as MemoryAllocator<Backend>>::Block,
	pub(crate) buffer: <Backend as gfx_hal::Backend>::Buffer,
}

impl<'a> BufferPool<'a> {
	pub fn create(data: &'a HALData, command_pool: &'a CommandPool) -> BufferPool<'a> {
		println!("Creating BufferPool");
		let device = &data.device;
		let phys_device = &data.adapter.physical_device;
		let mut allocator =
			SmartAllocator::new(phys_device.memory_properties(), 4096,
				8, 64, 134217728);
		let staging_buf = StagingBuffer::create(data, &mut allocator);
		BufferPool {
			data,
			command_pool,
			allocator: MaybeUninit::new(RefCell::new(allocator)),
			staging_buf: MaybeUninit::new(staging_buf),
		}
	}

	pub fn create_buffer(&self, size: usize, usage: Usage, uses_staging: bool) -> Buffer {
		let device = &self.data.device;
		let size = size as buffer::Offset;
		let (usage, props) = if uses_staging {
			(usage | Usage::TRANSFER_DST, Properties::DEVICE_LOCAL)
		} else {
			(usage, Properties::CPU_VISIBLE)
		};
		let unbound_buf = device.create_buffer(size, usage).unwrap();
		let reqs = device.get_buffer_requirements(&unbound_buf);
		let block = unsafe { self.allocator.get_ref() }.borrow_mut().alloc(
			device, (Type::General, props), reqs
		).unwrap();
		let buffer = device.bind_buffer_memory(
			block.memory(), block.range().start, unbound_buf
		).unwrap();

		Buffer {
			parent: self,
			block: MaybeUninit::new(block),
			buffer: MaybeUninit::new(buffer),
			usage,
			props
		}
	}

	pub fn create_texture<'b>(&self, info: &'b TextureInfo<'b>) -> Texture {
		Texture::create(self, info)
	}
}

impl<'a> Drop for BufferPool<'a> {
	fn drop(&mut self) {
		let device = &self.data.device;
		MaybeUninit::take(&mut self.staging_buf).manual_drop(&self.data, &mut unsafe { self.allocator.get_ref() }.borrow_mut());
		RefCell::into_inner(MaybeUninit::take(&mut self.allocator)).dispose(&device).unwrap();
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
		let unbound_buf = device.create_buffer(STAGING_BUF_SIZE as buffer::Offset, Usage::TRANSFER_SRC).unwrap();
		let reqs = device.get_buffer_requirements(&unbound_buf);
		let block = allocator.alloc(
			device, (Type::General, Properties::CPU_VISIBLE | Properties::COHERENT), reqs
		).unwrap();
		let buffer = device.bind_buffer_memory(
			block.memory(), block.range().start, unbound_buf
		).unwrap();

		StagingBuffer {
			block,
			buffer
		}
	}

	pub(crate) fn upload<T>(&self, data: &[T], device: &<Backend as gfx_hal::Backend>::Device) {
		Buffer::do_upload(data, self.block.range().start as usize, device, self.block.memory())
	}

	fn manual_drop(self, data: &HALData, alloc: &mut SmartAllocator<Backend>) {
		let device = &data.device;
		device.destroy_buffer(self.buffer);
		alloc.free(device, self.block);
		println!("Dropped StagingBuffer");
	}
}

impl<'a> Buffer<'a> {
	pub fn upload<T>(&self, data: &[T], offset: usize) {
		let device = &self.parent.data.device;
		println!("Uploading buffer");
		if self.props.contains(Properties::CPU_VISIBLE) {
			let memory = unsafe { self.block.get_ref() }.memory();
			Self::do_upload(data, offset, device, memory)
		} else {
			self.staged_upload(data, offset)
		}
	}

	fn do_upload<T>(data: &[T],
					offset: usize,
					device: &<Backend as gfx_hal::Backend>::Device,
					memory: &<Backend as gfx_hal::Backend>::Memory) {
		let t_size = size_of::<T>() as buffer::Offset;
		let len = t_size * data.len() as buffer::Offset;
		let offset = offset as buffer::Offset;

		let range = offset..offset + len;
		let map = device.map_memory(memory, range.clone()).unwrap();

		unsafe {
			std::ptr::copy_nonoverlapping(data.as_ptr(), map as *mut T, data.len());
		}
		device.flush_mapped_memory_ranges(vec![(memory, range)]).unwrap();
		device.unmap_memory(memory);
	}


	fn staged_upload<T>(&self, data: &[T], offset: usize) {
		let device = &self.parent.data.device;
		let pool = &self.parent.command_pool;
		let staged = unsafe { self.parent.staging_buf.get_ref() };
		let range = BufferCopy {
			src: 0,
			dst: offset as buffer::Offset,
			size: (data.len() * std::mem::size_of::<T>()) as buffer::Offset,
		};
		staged.upload(data, device);
		pool.buffer(true, &[], &[], None, |buffer| {
			buffer.copy_buffer(&staged.buffer, self.buffer(), &[range]);
		});
	}

	pub fn buffer(&self) -> &<Backend as gfx_hal::Backend>::Buffer {
		unsafe { self.buffer.get_ref() }
	}

	pub fn buffer_view(&self, format: Option<Format>, range: Range<usize>) -> BufferView {
		let data = &self.parent.data;
		let device = &data.device;
		let range = range.start as u64..range.end as u64;
		let view = device.create_buffer_view(self.buffer(), format, range).unwrap();
		BufferView {
			data,
			view: MaybeUninit::new(view),
		}
	}
}

impl<'a> Drop for Buffer<'a> {
	fn drop(&mut self) {
		let device = &self.parent.data.device;
		device.destroy_buffer(MaybeUninit::take(&mut self.buffer));
		unsafe { self.parent.allocator.get_ref() }.borrow_mut().free(device, MaybeUninit::take(&mut self.block));
		println!("Dropped Buffer")
	}
}

impl<'a> BufferView<'a> {
	pub fn buffer_view(&self) -> &<Backend as gfx_hal::Backend>::BufferView {
		unsafe { self.view.get_ref() }
	}
}

impl<'a> Drop for BufferView<'a> {
	fn drop(&mut self) {
		let device = &self.data.device;
		device.destroy_buffer_view(MaybeUninit::take(&mut self.view));
	}
}