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
	BufferPool,
};

use self::inner::InnerBuffer;

pub(crate) mod inner {
	use crate::{
		gfx_back::Backend,
		BufferPool,
	};
	use gfx_hal::buffer;
	use gfx_memory::{
		MemoryAllocator,
		SmartAllocator,
	};
	pub trait InnerBuffer {
		fn pool(&self) -> &BufferPool;
		fn rusage(&self) -> buffer::Usage;
		fn rlen(&self) -> buffer::Offset;
		fn buffer(&self) -> &<Backend as gfx_hal::Backend>::Buffer;
		fn block(&self) -> &<SmartAllocator<Backend> as MemoryAllocator<Backend>>::Block;

		fn own_buffer(&mut self) -> <Backend as gfx_hal::Backend>::Buffer;
		fn own_block(&mut self) -> <SmartAllocator<Backend> as MemoryAllocator<Backend>>::Block;
	}
}

pub trait Buffer<'a, T: Sized + Copy + Clone>: InnerBuffer + Sized {
	fn create(pool: &'a BufferPool, usage: Usage, size: buffer::Offset) -> Self;

	fn create_slice(pool: &'a BufferPool, usage: Usage, slice: &[T]) -> Self {
		let buf = Self::create(pool, usage, slice.len() as buffer::Offset);
		buf.upload(0, slice);
		buf
	}

	fn upload(&self, offset: buffer::Offset, data: &[T]);

	fn usage(&self) -> Usage { self.rusage() }

	fn len(&self) -> buffer::Offset { self.rlen() }

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

struct BaseBuffer<'a, T: Copy + Clone> {
	pool: &'a BufferPool<'a>,
	block: MaybeUninit<<SmartAllocator<Backend> as MemoryAllocator<Backend>>::Block>,
	buffer: MaybeUninit<<Backend as gfx_hal::Backend>::Buffer>,
	usage: Usage,
	len: buffer::Offset,
	phantom: PhantomData<T>,
}

impl<T: Copy + Clone> Drop for BaseBuffer<'_, T> {
	fn drop(&mut self) {
		let pool = &self.pool;
		let device = &pool.data.device;
		unsafe {
			device.destroy_buffer(MaybeUninit::take(&mut self.buffer));

			pool.allocator
				.get_ref()
				.borrow_mut()
				.free(device, MaybeUninit::take(&mut self.block));
		}
		println!("Dropped buffer");
	}
}

impl<T: Copy + Clone> InnerBuffer for BaseBuffer<'_, T> {
	fn pool(&self) -> &BufferPool { &self.pool }

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

pub struct CPUBuffer<'a, T: Copy + Clone>(BaseBuffer<'a, T>);
pub struct GPUBuffer<'a, T: Copy + Clone>(BaseBuffer<'a, T>);

impl<'a, T: Copy + Clone> Buffer<'a, T> for CPUBuffer<'a, T> {
	fn create(pool: &'a BufferPool, usage: Usage, size: buffer::Offset) -> Self {
		unsafe {
			let device = &pool.data.device;

			let mut allocator = pool.allocator.get_ref().borrow_mut();
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
				pool,
				block: MaybeUninit::new(block),
				buffer: MaybeUninit::new(buffer),
				usage,
				len: size,
				phantom: PhantomData,
			})
		}
	}

	fn upload(&self, mut offset: buffer::Offset, data: &[T]) {
		assert!(
			Buffer::len(self) >= data.len() as buffer::Offset,
			"Attempted to upload more data than the buffer could handle!"
		);
		let device = &self.pool().data.device;
		let size_in_bytes = (size_of::<T>() * data.len()) as buffer::Offset;
		offset += self.block().range().start;
		let range = offset..offset + size_in_bytes;
		unsafe {
			let map = device.map_memory(self.block().memory(), range).unwrap();

			std::ptr::copy_nonoverlapping(data.as_ptr(), map as *mut T, data.len());

			device.unmap_memory(self.block().memory());
		}
	}
}

impl<'a, T: Copy + Clone> Buffer<'a, T> for GPUBuffer<'a, T> {
	fn create(pool: &'a BufferPool, usage: Usage, size: buffer::Offset) -> Self {
		unsafe {
			let device = &pool.data.device;

			let mut allocator = pool.allocator.get_ref().borrow_mut();
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
				pool,
				block: MaybeUninit::new(block),
				buffer: MaybeUninit::new(buffer),
				usage,
				len: size,
				phantom: PhantomData,
			})
		}
	}

	fn upload(&self, offset: u64, data: &[T]) {
		let device = &self.pool().data.device;
		let command_pool = &self.pool().command_pool;
		let staged = unsafe { &self.pool().staging_buf.get_ref() };
		let fence = &staged.fence;
		let range = BufferCopy {
			src: 0,
			dst: offset as buffer::Offset,
			size: (data.len() * std::mem::size_of::<T>()) as buffer::Offset,
		};
		staged.upload(data, device);
		fence.reset();
		staged.buf_uses.update(|mut i| {
			i += 1;
			if i >= 16 {
				command_pool.reset();
				i = 0;
			}
			i
		});
		command_pool.single_submit(&[], &[], fence, |buffer| unsafe {
			buffer.copy_buffer(&staged.buffer, self.buffer(), &[range]);
		});
	}
}

macro_rules! impl_inner {
	($name: ident) => {
		impl<T: Copy + Clone> InnerBuffer for $name<'_, T> {
			fn pool(&self) -> &BufferPool { self.0.pool() }

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
