use std::mem::{size_of, MaybeUninit};

use gfx_hal::{
    buffer::{self, Usage},
    command::BufferCopy,
    memory::Properties,
    Device,
};
use gfx_memory::{Block, MemoryAllocator, SmartAllocator, Type};

use crate::gfx_back::Backend;
use crate::util::TakeExt;
use crate::BufferPool;

pub struct Buffer<'a> {
    parent: &'a BufferPool<'a>,
    block: MaybeUninit<<SmartAllocator<Backend> as MemoryAllocator<Backend>>::Block>,
    buffer: MaybeUninit<<Backend as gfx_hal::Backend>::Buffer>,
    usage: Usage,
    props: Properties,
}

//pub struct BufferView<'a> {
//    data: &'a HALData,
//    view: MaybeUninit<<Backend as gfx_hal::Backend>::BufferView>,
//}

impl<'a> Buffer<'a> {
    pub fn create(pool: &'a BufferPool, size: usize, usage: Usage, uses_staging: bool) -> Buffer<'a> {
        let device = &pool.data.device;
        let size = size as buffer::Offset;
        let (usage, props) = if uses_staging {
            (usage | Usage::TRANSFER_DST, Properties::DEVICE_LOCAL)
        } else {
            (usage, Properties::CPU_VISIBLE | Properties::COHERENT)
        };
        let unbound_buf = device.create_buffer(size, usage).unwrap();
        let reqs = device.get_buffer_requirements(&unbound_buf);
        let block = unsafe { pool.allocator.get_ref() }
            .borrow_mut()
            .alloc(device, (Type::General, props), reqs)
            .unwrap();
        let buffer = device
            .bind_buffer_memory(block.memory(), block.range().start, unbound_buf)
            .unwrap();
        Buffer {
            parent: pool,
            block: MaybeUninit::new(block),
            buffer: MaybeUninit::new(buffer),
            usage,
            props,
        }
    }

    pub fn upload<T>(&self, data: &[T], offset: usize) {
        let device = &self.parent.data.device;
        if self.props.contains(Properties::CPU_VISIBLE) {
            let block = unsafe { self.block.get_ref() };
            let offset = offset + block.range().start as usize;
            let memory = block.memory();
            Self::do_upload(data, offset, device, memory)
        } else {
            self.staged_upload(data, offset)
        }
    }

    pub(crate) fn do_upload<T>(
        data: &[T],
        offset: usize,
        device: &<Backend as gfx_hal::Backend>::Device,
        memory: &<Backend as gfx_hal::Backend>::Memory,
    ) {
        let t_size = size_of::<T>() as buffer::Offset;
        let len = t_size * data.len() as buffer::Offset;
        let offset = offset as buffer::Offset;

        let range = offset..offset + len;

        let map = device.map_memory(memory, range.clone()).unwrap();

        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr(), map as *mut T, data.len());
        }

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
        pool.single_submit(true, &[], &[], None, |buffer| {
            buffer.copy_buffer(&staged.buffer, self.buffer(), &[range]);
        });
    }

    pub fn buffer(&self) -> &<Backend as gfx_hal::Backend>::Buffer {
        unsafe { self.buffer.get_ref() }
    }

    //    pub fn buffer_view(&self, format: Option<Format>, range: Range<usize>) -> BufferView {
    //        let data = &self.parent.data;
    //        let device = &data.device;
    //        let range = range.start as u64..range.end as u64;
    //        let view = device
    //            .create_buffer_view(self.buffer(), format, range)
    //            .unwrap();
    //        BufferView {
    //            data,
    //            view: MaybeUninit::new(view),
    //        }
    //    }
}

impl<'a> Drop for Buffer<'a> {
    fn drop(&mut self) {
        let device = &self.parent.data.device;
        device.destroy_buffer(MaybeUninit::take(&mut self.buffer));
        unsafe { self.parent.allocator.get_ref() }
            .borrow_mut()
            .free(device, MaybeUninit::take(&mut self.block));
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
