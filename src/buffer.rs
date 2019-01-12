use gfx_hal::{
	buffer::{
		self,
		Usage,
	},
	command::BufferCopy,
	memory::Properties,
	pso::Descriptor,
	Device,
};
use std::{
	marker::PhantomData,
	mem::{
		size_of,
		MaybeUninit,
	},
	ops::Range,
};

use gfx_memory::{
	Block,
	MemoryAllocator,
	SmartAllocator,
	Type,
};

use crate::{
	gfx_back::Backend,
	util::TakeExt,
	CommandPool,
	Fence,
	HALData,
};

use self::inner::InnerBuffer;

pub(crate) mod inner {
	use crate::{
		gfx_back::Backend,
		HALData,
	};
	use gfx_hal::buffer;
	use gfx_memory::{
		MemoryAllocator,
		SmartAllocator,
	};
	pub trait InnerBuffer {
		fn data(&self) -> &HALData;
		fn rusage(&self) -> buffer::Usage;
		fn rlen(&self) -> buffer::Offset;
		fn buffer(&self) -> &<Backend as gfx_hal::Backend>::Buffer;
		fn block(&self) -> &<SmartAllocator<Backend> as MemoryAllocator<Backend>>::Block;

		fn own_buffer(&mut self) -> <Backend as gfx_hal::Backend>::Buffer;
		fn own_block(&mut self) -> <SmartAllocator<Backend> as MemoryAllocator<Backend>>::Block;
	}
}

pub trait Buffer<'a, T: Sized + Copy + Clone>: InnerBuffer + Sized {
	fn create(data: &'a HALData, usage: Usage, size: buffer::Offset) -> Self;

	fn descriptor(&self) -> Descriptor<Backend> { Descriptor::Buffer(self.buffer(), None..None) }

	fn descriptor_to_end(&self, start: usize) -> Descriptor<Backend> {
		Descriptor::Buffer(self.buffer(), Some(start as u64)..None)
	}

	fn descriptor_range(&self, range: Range<usize>) -> Descriptor<Backend> {
		Descriptor::Buffer(
			self.buffer(),
			Some(range.start as u64)..Some(range.end as u64),
		)
	}
}

pub(crate) struct BaseBuffer<'a, T: Copy + Clone> {
	data: &'a HALData,
	block: MaybeUninit<<SmartAllocator<Backend> as MemoryAllocator<Backend>>::Block>,
	pub(crate) buffer: MaybeUninit<<Backend as gfx_hal::Backend>::Buffer>,
	usage: Usage,
	len: buffer::Offset,
	phantom: PhantomData<T>,
}

impl<T: Copy + Clone> Drop for BaseBuffer<'_, T> {
	fn drop(&mut self) {
		let data = &self.data;
		let device = &data.device;
		unsafe {
			device.destroy_buffer(MaybeUninit::take(&mut self.buffer));

			data.allocator
				.get_ref()
				.borrow_mut()
				.free(device, MaybeUninit::take(&mut self.block));
		}
		println!("Dropped buffer");
	}
}

impl<T: Copy + Clone> InnerBuffer for BaseBuffer<'_, T> {
	fn data(&self) -> &HALData { &self.data }

	fn buffer(&self) -> &<Backend as gfx_hal::Backend>::Buffer { unsafe { &self.buffer.get_ref() } }

	fn block(&self) -> &<SmartAllocator<Backend> as MemoryAllocator<Backend>>::Block {
		unsafe { &self.block.get_ref() }
	}

	fn own_buffer(&mut self) -> <Backend as gfx_hal::Backend>::Buffer {
		MaybeUninit::take(&mut self.buffer)
	}

	fn own_block(&mut self) -> <SmartAllocator<Backend> as MemoryAllocator<Backend>>::Block {
		MaybeUninit::take(&mut self.block)
	}

	fn rusage(&self) -> Usage { self.usage }

	fn rlen(&self) -> buffer::Offset { self.len }
}

pub struct CPUBuffer<'a, T: Copy + Clone>(pub(crate) BaseBuffer<'a, T>);
pub struct GPUBuffer<'a, T: Copy + Clone>(pub(crate) BaseBuffer<'a, T>);

pub struct StagingBuffer<'a> {
	pub(crate) base: BaseBuffer<'a, u8>,
	pub(crate) command_pool: &'a CommandPool<'a>,
	pub(crate) fence: Fence<'a>,
}

impl<'a, T: Copy + Clone> Buffer<'a, T> for CPUBuffer<'a, T> {
	fn create(data: &'a HALData, usage: Usage, size: buffer::Offset) -> Self {
		unsafe {
			let device = &data.device;
			let mut allocator = data.allocator.get_ref().borrow_mut();
			let size_in_bytes = (size_of::<T>() as buffer::Offset) * size;
			let mut buffer = device.create_buffer(size_in_bytes, usage).unwrap();
			let reqs = device.get_buffer_requirements(&buffer);
			let block = allocator
				.alloc(
					device,
					(
						Type::General,
						Properties::COHERENT | Properties::CPU_VISIBLE,
					),
					reqs,
				)
				.unwrap();
			device
				.bind_buffer_memory(block.memory(), block.range().start, &mut buffer)
				.unwrap();
			CPUBuffer(BaseBuffer {
				data,
				block: MaybeUninit::new(block),
				buffer: MaybeUninit::new(buffer),
				usage,
				len: size,
				phantom: PhantomData,
			})
		}
	}
}
impl<'a, T: Copy + Clone> CPUBuffer<'a, T> {
	pub fn upload(&self, mut offset: buffer::Offset, data: &[T]) {
		assert!(
			self.0.len >= data.len() as buffer::Offset,
			"Attempted to upload more data than the buffer could handle!"
		);
		let device = &self.data().device;
		let size_in_bytes = (size_of::<T>() * data.len()) as buffer::Offset;
		offset += self.block().range().start;
		let range = offset..offset + size_in_bytes;
		let memory = self.block().memory();
		unsafe {
			let map = device.map_memory(memory, range.clone()).unwrap();

			std::ptr::copy_nonoverlapping(data.as_ptr(), map as *mut T, data.len());

			device.unmap_memory(memory);
		}
	}
}

impl<'a, T: Copy + Clone> Buffer<'a, T> for GPUBuffer<'a, T> {
	fn create(data: &'a HALData, usage: Usage, size: buffer::Offset) -> Self {
		unsafe {
			let device = &data.device;

			let mut allocator = data.allocator.get_ref().borrow_mut();
			let size_in_bytes = (size_of::<T>() as buffer::Offset) * size;
			let mut buffer = device
				.create_buffer(size_in_bytes, usage | Usage::TRANSFER_DST)
				.unwrap();
			let reqs = device.get_buffer_requirements(&buffer);
			let block = allocator
				.alloc(device, (Type::General, Properties::DEVICE_LOCAL), reqs)
				.unwrap();
			device
				.bind_buffer_memory(block.memory(), block.range().start, &mut buffer)
				.unwrap();
			GPUBuffer(BaseBuffer {
				data,
				block: MaybeUninit::new(block),
				buffer: MaybeUninit::new(buffer),
				usage,
				len: size,
				phantom: PhantomData,
			})
		}
	}
}

impl<T: Copy + Clone> GPUBuffer<'_, T> {
	pub fn upload<'b>(&self, offset: u64, data: &'b [T], staging_buf: &'b StagingBuffer) {
		let device = &self.data().device;
		let command_pool = &staging_buf.command_pool;
		let range = BufferCopy {
			src: 0,
			dst: offset as buffer::Offset,
			size: (data.len() * std::mem::size_of::<T>()) as buffer::Offset,
		};
		staging_buf.upload(data);
		command_pool.single_submit(&[], &[], &staging_buf.fence, |buffer| unsafe {
			buffer.copy_buffer(staging_buf.base.buffer.get_ref(), self.buffer(), &[range]);
		});
	}
}

impl<'a> StagingBuffer<'a> {
	pub fn create(
		data: &'a HALData,
		command_pool: &'a CommandPool<'a>,
		size: buffer::Offset,
	) -> StagingBuffer<'a> {
		unsafe {
			const USAGE: Usage = Usage::TRANSFER_SRC;
			let device = &data.device;

			let mut allocator = data.allocator.get_ref().borrow_mut();
			let mut buffer = device.create_buffer(size, USAGE).unwrap();
			let reqs = device.get_buffer_requirements(&buffer);
			let block = allocator
				.alloc(
					device,
					(
						Type::General,
						Properties::COHERENT | Properties::CPU_VISIBLE,
					),
					reqs,
				)
				.unwrap();
			device
				.bind_buffer_memory(block.memory(), block.range().start, &mut buffer)
				.unwrap();
			let fence = data.create_fence();
			StagingBuffer {
				base: BaseBuffer {
					data,
					block: MaybeUninit::new(block),
					buffer: MaybeUninit::new(buffer),
					usage: USAGE,
					len: size,
					phantom: PhantomData,
				},
				command_pool,
				fence,
			}
		}
	}

	pub(crate) fn upload<T: Copy + Clone>(&self, data: &[T]) {
		let size_in_bytes = (size_of::<T>() * data.len()) as buffer::Offset;
		assert!(
			self.base.len >= size_in_bytes,
			"Attempted to upload more data than the buffer could handle!"
		);
		let device = &self.base.data.device;
		let offset = self.base.block().range().start;
		let range = offset..offset + size_in_bytes;
		let memory = self.base.block().memory();
		self.fence.wait_n_reset();
		unsafe {
			let map = device.map_memory(memory, range.clone()).unwrap();

			std::ptr::copy_nonoverlapping(data.as_ptr(), map as *mut T, data.len());

			device.unmap_memory(memory);
		}
	}

	pub(crate) fn wait_on_upload(&self) { self.fence.wait() }
}

macro_rules! impl_inner {
	($name: ident) => {
		impl<T: Copy + Clone> InnerBuffer for $name<'_, T> {
			fn data(&self) -> &HALData { &self.0.data() }

			fn rusage(&self) -> Usage { self.0.rusage() }

			fn rlen(&self) -> buffer::Offset { self.0.rlen() }

			fn buffer(&self) -> &<Backend as gfx_hal::Backend>::Buffer { self.0.buffer() }

			fn block(&self) -> &<SmartAllocator<Backend> as MemoryAllocator<Backend>>::Block {
				self.0.block()
			}

			fn own_buffer(&mut self) -> <Backend as gfx_hal::Backend>::Buffer {
				self.0.own_buffer()
			}

			fn own_block(
				&mut self,
			) -> <SmartAllocator<Backend> as MemoryAllocator<Backend>>::Block {
				self.0.own_block()
			}
		}
	};
}

impl_inner!(GPUBuffer);
impl_inner!(CPUBuffer);
