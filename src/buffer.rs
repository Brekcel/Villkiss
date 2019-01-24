use gfx_hal::{
	adapter::PhysicalDevice,
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
	any::TypeId,
	marker::PhantomData,
	mem::{
		size_of,
		MaybeUninit,
	},
	ops::Range,
	sync::Arc,
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
	use gfx_memory::{
		MemoryAllocator,
		SmartAllocator,
	};
	pub trait InnerBuffer {
		fn data(&self) -> &HALData;
		fn hal_buffer(&self) -> &<Backend as gfx_hal::Backend>::Buffer;
		fn block(&self) -> &<SmartAllocator<Backend> as MemoryAllocator<Backend>>::Block;
	}
}

pub trait Buffer<'a>: Sized + InnerBuffer {
	fn create<'b>(data: &'a HALData, descs: &'b [BufferViewDesc]) -> Vec<BufferView<'a, Self>>;
}

pub(crate) struct BaseBuffer<'a> {
	data: &'a HALData,
	block: MaybeUninit<<SmartAllocator<Backend> as MemoryAllocator<Backend>>::Block>,
	buffer: MaybeUninit<<Backend as gfx_hal::Backend>::Buffer>,
	size_in_bytes: buffer::Offset,
}

impl InnerBuffer for BaseBuffer<'_> {
	fn data(&self) -> &HALData { &self.data }

	fn hal_buffer(&self) -> &<Backend as gfx_hal::Backend>::Buffer {
		unsafe { &self.buffer.get_ref() }
	}

	fn block(&self) -> &<SmartAllocator<Backend> as MemoryAllocator<Backend>>::Block {
		unsafe { &self.block.get_ref() }
	}
}

impl<'a> BaseBuffer<'a> {
	fn create_descs<'b>(
		data: &'a HALData,
		descs: &'b [BufferViewDesc],
		extra_usage: Usage,
		props: Properties,
	) -> (Vec<buffer::Offset>, Self) {
		let usage = extra_usage |
			descs
				.iter()
				.fold(Usage::empty(), |usage, desc| usage | desc.usage);
		let align = if usage.contains(Usage::UNIFORM) {
			let phys_device = &data.adapter.physical_device;
			phys_device.limits().min_uniform_buffer_offset_alignment
		} else {
			1
		};
		let sizes = descs
			.iter()
			.map(|desc| {
				let size = desc.len * desc.type_size;
				let padding = (align - (size % align)) % align;
				size + padding
			})
			.collect::<Vec<_>>();

		let size_in_bytes = sizes.iter().fold(0, |len, add_len| len + add_len);
		(sizes, Self::create(data, usage, props, size_in_bytes))
	}

	fn create(
		data: &'a HALData,
		usage: Usage,
		props: Properties,
		size_in_bytes: buffer::Offset,
	) -> Self {
		println!("Creating Buffer");
		unsafe {
			let device = &data.device;
			let mut allocator = data.allocator.get_ref().borrow_mut();

			let mut buffer = device.create_buffer(size_in_bytes, usage).unwrap();
			let reqs = device.get_buffer_requirements(&buffer);
			let block = allocator
				.alloc(device, (Type::General, props), reqs)
				.unwrap();
			device
				.bind_buffer_memory(block.memory(), block.range().start, &mut buffer)
				.unwrap();
			BaseBuffer {
				data,
				block: MaybeUninit::new(block),
				buffer: MaybeUninit::new(buffer),
				size_in_bytes,
			}
		}
	}
}

impl Drop for BaseBuffer<'_> {
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
		println!("Dropped Buffer");
	}
}

pub struct CPUBuffer<'a>(BaseBuffer<'a>);
pub struct GPUBuffer<'a>(BaseBuffer<'a>);

pub struct StagingBuffer<'a> {
	base: BaseBuffer<'a>,
	pub(crate) command_pool: &'a CommandPool<'a>,
	pub(crate) fence: Fence<'a>,
}

#[derive(Debug, Copy, Clone)]
pub struct BufferViewDesc {
	type_id: TypeId,
	type_size: buffer::Offset,
	usage: Usage,
	len: buffer::Offset,
	offset: buffer::Offset,
}

impl BufferViewDesc {
	pub fn create_desc<T: 'static>(usage: Usage, len: buffer::Offset) -> BufferViewDesc {
		BufferViewDesc {
			type_id: TypeId::of::<T>(),
			type_size: size_of::<T>() as buffer::Offset,
			usage,
			len,
			offset: 0,
		}
	}
}

pub struct BufferView<'a, T: Buffer<'a>> {
	buffer: Arc<T>,
	desc: BufferViewDesc,
	phantom: PhantomData<&'a T>,
}

impl<'a, T: Buffer<'a>> BufferView<'a, T> {
	fn fold_descs(
		buffer: Arc<T>,
		descs: &[BufferViewDesc],
		sizes: Vec<buffer::Offset>,
	) -> Vec<BufferView<'a, T>> {
		let mut base_offset = 0;
		descs
			.iter()
			.zip(sizes.iter())
			.map(|(desc, size)| BufferView {
				buffer: buffer.clone(),
				desc: {
					let mut desc = *desc;
					desc.offset = {
						let tmp = base_offset;
						base_offset += size;
						tmp
					};
					desc
				},
				phantom: PhantomData,
			})
			.collect()
	}

	pub fn descriptor(&self) -> Descriptor<Backend> { self.descriptor_to_end(0) }

	pub fn descriptor_to_end(&self, start: usize) -> Descriptor<Backend> {
		self.descriptor_range(0..self.desc.len as usize)
	}

	pub fn descriptor_range(&self, range: Range<usize>) -> Descriptor<Backend> {
		let range: Range<buffer::Offset> = range.start as _..range.end as _;
		assert!(range.start <= range.end);
		assert!(range.end <= self.desc().len);
		let abs_beg = self.offset() + (range.start * self.desc().type_size);
		let abs_end = self.offset() + (range.end * self.desc().type_size);
		Descriptor::Buffer(self.hal_buffer(), Some(abs_beg)..Some(abs_end))
	}

	pub(crate) fn size(&self) -> buffer::Offset { self.desc.type_size * self.desc.len }

	pub(crate) fn hal_buffer(&self) -> &<Backend as gfx_hal::Backend>::Buffer {
		self.buffer().hal_buffer()
	}

	pub(crate) fn buffer(&self) -> &T { self.buffer.as_ref() }

	pub(crate) fn offset(&self) -> buffer::Offset { self.desc.offset }

	pub(crate) fn desc(&self) -> &BufferViewDesc { &self.desc }
}

impl<'a> Buffer<'a> for CPUBuffer<'a> {
	fn create<'b>(data: &'a HALData, descs: &'b [BufferViewDesc]) -> Vec<BufferView<'a, Self>> {
		let (sizes, base) = BaseBuffer::create_descs(
			data,
			descs,
			Usage::empty(),
			Properties::COHERENT | Properties::CPU_VISIBLE,
		);
		BufferView::fold_descs(Arc::new(CPUBuffer(base)), descs, sizes)
	}
}

impl<'a> BufferView<'a, CPUBuffer<'a>> {
	pub fn upload<T: 'static>(&self, mut offset: buffer::Offset, data: &[T]) {
		assert!(self.desc.len >= data.len() as buffer::Offset);
		assert_eq!(self.desc.type_id, TypeId::of::<T>());
		let device = &self.buffer.0.data.device;
		let size_in_bytes = self.desc.type_size * (data.len() as buffer::Offset);
		offset += self.offset();
		offset += self.buffer.block().range().start;
		let range = offset..offset + size_in_bytes;
		unsafe {
			let memory = self.buffer.0.block.get_ref().memory();

			let map = device.map_memory(memory, range.clone()).unwrap();

			std::ptr::copy_nonoverlapping(data.as_ptr(), map as *mut T, data.len());

			device.unmap_memory(memory);
		}
	}
}

impl<'a> Buffer<'a> for GPUBuffer<'a> {
	fn create<'b>(data: &'a HALData, descs: &'b [BufferViewDesc]) -> Vec<BufferView<'a, Self>> {
		let (sizes, base) =
			BaseBuffer::create_descs(data, descs, Usage::TRANSFER_DST, Properties::DEVICE_LOCAL);
		BufferView::fold_descs(Arc::new(GPUBuffer(base)), descs, sizes)
	}
}

impl<'a> BufferView<'a, GPUBuffer<'a>> {
	pub fn staged_upload<'b, T: 'static + Copy + Clone>(
		&self,
		mut offset: buffer::Offset,
		data: &'b [T],
		staging_buf: &'b StagingBuffer,
	) {
		assert!(self.desc.len >= data.len() as buffer::Offset);
		assert_eq!(self.desc.type_id, TypeId::of::<T>());
		let device = &self.buffer.0.data.device;
		let command_pool = &staging_buf.command_pool;

		offset += self.offset();
		let range = BufferCopy {
			src: 0,
			dst: offset,
			size: (data.len() * std::mem::size_of::<T>()) as buffer::Offset,
		};
		staging_buf.upload(data);
		command_pool.single_submit(&[], &[], &staging_buf.fence, |buffer| unsafe {
			buffer.copy_buffer(
				staging_buf.base.buffer.get_ref(),
				self.hal_buffer(),
				&[range],
			);
		})
	}
}

impl<'a> StagingBuffer<'a> {
	pub fn create(
		data: &'a HALData,
		command_pool: &'a CommandPool<'a>,
		size: buffer::Offset,
	) -> StagingBuffer<'a> {
		let fence = data.create_fence();
		fence.reset();
		StagingBuffer {
			base: BaseBuffer::create(
				data,
				Usage::TRANSFER_SRC,
				Properties::COHERENT | Properties::CPU_VISIBLE,
				size,
			),
			command_pool,
			fence,
		}
	}

	pub(crate) fn upload<T: Copy + Clone>(&self, data: &[T]) {
		let size_in_bytes = (size_of::<T>() * data.len()) as buffer::Offset;
		assert!(
			self.base.size_in_bytes >= size_in_bytes,
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

	pub fn wait_on_upload(&self) { self.fence.wait() }
}

macro_rules! impl_inner {
	($name: ident) => {
		impl InnerBuffer for $name<'_> {
			fn data(&self) -> &HALData { &self.0.data() }

			fn hal_buffer(&self) -> &<Backend as gfx_hal::Backend>::Buffer { self.0.hal_buffer() }

			fn block(&self) -> &<SmartAllocator<Backend> as MemoryAllocator<Backend>>::Block {
				self.0.block()
			}
		}
	};
}

impl_inner!(GPUBuffer);
impl_inner!(CPUBuffer);

impl InnerBuffer for StagingBuffer<'_> {
	fn data(&self) -> &HALData { &self.base.data() }

	fn hal_buffer(&self) -> &<Backend as gfx_hal::Backend>::Buffer { self.base.hal_buffer() }

	fn block(&self) -> &<SmartAllocator<Backend> as MemoryAllocator<Backend>>::Block {
		self.base.block()
	}
}
