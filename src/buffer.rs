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

pub struct Buffer<'a> {
	parent: &'a BufferPool<'a>,
	block: MaybeUninit<<SmartAllocator<Backend> as MemoryAllocator<Backend>>::Block>,
	buffer: MaybeUninit<<Backend as gfx_hal::Backend>::Buffer>,
	usage: Usage,
	props: Properties,
}

impl<'a> Buffer<'a> {
	pub fn create(
		pool: &'a BufferPool<'a>,
		size: u64,
		usage: Usage,
		uses_staging: bool,
	) -> Buffer<'a> {
		let device = &pool.data.device;
		let size = size as buffer::Offset;
		let (usage, props) = if uses_staging {
			(usage | Usage::TRANSFER_DST, Properties::DEVICE_LOCAL)
		} else {
			(usage, Properties::CPU_VISIBLE | Properties::COHERENT)
		};
		unsafe {
			let mut buffer = device.create_buffer(size, usage).unwrap();
			let reqs = device.get_buffer_requirements(&buffer);
			let block = pool
				.allocator
				.get_ref()
				.borrow_mut()
				.alloc(device, (Type::General, props), reqs)
				.unwrap();
			device
				.bind_buffer_memory(block.memory(), block.range().start, &mut buffer)
				.unwrap();

			Buffer {
				parent: pool,
				block: MaybeUninit::new(block),
				buffer: MaybeUninit::new(buffer),
				usage,
				props,
			}
		}
	}

	pub fn upload<T>(&mut self, data: &[T], offset: u64) {
		let device = &self.parent.data.device;
		if self.props.contains(Properties::CPU_VISIBLE) {
			let block = unsafe { self.block.get_ref() };
			let offset = offset + block.range().start;
			let memory = block.memory();
			Self::do_upload(data, offset, device, memory)
		} else {
			self.staged_upload(data, offset)
		}
	}

	pub(crate) fn do_upload<T>(
		data: &[T],
		offset: u64,
		device: &<Backend as gfx_hal::Backend>::Device,
		memory: &<Backend as gfx_hal::Backend>::Memory,
	) {
		let t_size = size_of::<T>() as buffer::Offset;
		let len = t_size * data.len() as buffer::Offset;
		let offset = offset as buffer::Offset;

		let range = offset..offset + len;

		unsafe {
			let map = device.map_memory(memory, range.clone()).unwrap();

			std::ptr::copy_nonoverlapping(data.as_ptr(), map as *mut T, data.len());

			device.unmap_memory(memory);
		}
	}

	fn staged_upload<T>(&mut self, data: &[T], offset: u64) {
		let device = &self.parent.data.device;
		let pool = &self.parent.command_pool;
		let staged = unsafe { self.parent.staging_buf.get_ref() };
		let fence = &staged.fence;
		let range = BufferCopy {
			src: 0,
			dst: offset as buffer::Offset,
			size: (data.len() * std::mem::size_of::<T>()) as buffer::Offset,
		};
		fence.wait_n_reset();
		staged.buf_uses.update(|mut i| {
			i += 1;
			if i >= 16 {
				pool.reset();
				i = 0;
			}
			i
		});
		staged.upload(data, device);
		pool.single_submit(&[], &[], fence, |buffer| unsafe {
			buffer.copy_buffer(&staged.buffer, self.buffer(), &[range]);
		});
	}

	pub(crate) fn buffer(&self) -> &<Backend as gfx_hal::Backend>::Buffer {
		unsafe { self.buffer.get_ref() }
	}

	pub fn descriptor(&self) -> Descriptor<Backend> {
		Descriptor::Buffer(self.buffer(), None..None)
	}

	pub fn descriptor_to_end(&self, start: usize) -> Descriptor<Backend> {
		Descriptor::Buffer(self.buffer(), Some(start as u64)..None)
	}

	pub fn descriptor_range(&self, range: Range<usize>) -> Descriptor<Backend> {
		Descriptor::Buffer(
			self.buffer(),
			Some(range.start as u64)..Some(range.end as u64),
		)
	}
}

impl<'a> Drop for Buffer<'a> {
	fn drop(&mut self) {
		let device = &self.parent.data.device;
		unsafe {
			device.destroy_buffer(MaybeUninit::take(&mut self.buffer));
			self.parent
				.allocator
				.get_ref()
				.borrow_mut()
				.free(device, MaybeUninit::take(&mut self.block));
		}
		println!("Dropped Buffer")
	}
}

//impl<'a> BufferView<'a> {
//    pub fn buffer_view(&self) -> &<Backend as gfx_hal::Backend>::BufferView {
//        unsafe { self.view.get_ref() }
//    }
//}
//
//impl<'a> Drop for BufferView<'a> {
//    fn drop(&mut self) {
//        let device = &self.data.device;
//        device.destroy_buffer_view(MaybeUninit::take(&mut self.view));
//    }
//}
