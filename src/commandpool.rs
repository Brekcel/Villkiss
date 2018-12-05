use std::{
    cell::{Cell, RefCell},
    iter::once,
    mem::MaybeUninit,
};

use gfx_hal::{
    command::{CommandBuffer, OneShot, Primary, Submit},
    pool::{CommandPool as HAL_CommandPool, CommandPoolCreateFlags},
    pso::PipelineStage,
    Device, Graphics, Submission,
};

use crate::gfx_back::Backend;
use crate::{util::TakeExt, BufferPool, Fence, HALData, Semaphore};

const MAX_BUFS: usize = 16;

pub struct CommandPool<'a> {
    pub(crate) data: &'a HALData,
    pub(crate) pool: MaybeUninit<RefCell<HAL_CommandPool<Backend, Graphics>>>,
    bufs_used: Cell<usize>,
}

impl<'a> CommandPool<'a> {
    pub(crate) fn create(data: &HALData) -> CommandPool {
        println!("Creating Commandpool");
        let flags: CommandPoolCreateFlags = CommandPoolCreateFlags::empty();
        let device = &data.device;
        let pool = device
            .create_command_pool_typed(&data.queue_group.borrow(), flags, MAX_BUFS)
            .unwrap();
        CommandPool {
            data,
            pool: MaybeUninit::new(RefCell::new(pool)),
            bufs_used: Cell::new(0),
        }
    }

    pub fn single_submit<F>(
        &self,
        allow_pending_resubmit: bool,
        wait_sems: &[(&Semaphore, PipelineStage)],
        signal_sems: &[&Semaphore],
        fence: Option<&Fence>,
        mut f: F,
    ) where
        F: FnMut(&mut CommandBuffer<Backend, Graphics>),
    {
        self.incr_used();
        let mut pool = unsafe { self.pool.get_ref() }.borrow_mut();
        let mut buffer = pool.acquire_command_buffer(allow_pending_resubmit);
        f(&mut buffer);
        let finished = buffer.finish();
        let wait_sems = wait_sems
            .iter()
            .map(|(sem, stage)| (sem.semaphore(), *stage));
        let signal_sems = signal_sems.iter().map(|sem| sem.semaphore());
        let submission = Submission::new()
            .wait_on(wait_sems)
            .signal(signal_sems)
            .submit(once(finished));
        self.data.submit(submission, fence);
    }

    pub fn submit<F>(
        &self,
        allow_pending_resubmit: bool,
        mut f: F,
    ) -> Submit<Backend, Graphics, OneShot, Primary>
    where
        F: FnMut(&mut CommandBuffer<Backend, Graphics>),
    {
        self.incr_used();
        let mut pool = unsafe { self.pool.get_ref() }.borrow_mut();
        let mut buffer = pool.acquire_command_buffer(allow_pending_resubmit);
        f(&mut buffer);
        buffer.finish()
    }

    pub fn create_bufferpool(&self) -> BufferPool {
        BufferPool::create(self)
    }

    pub fn reset(&self) {
        self.bufs_used.set(0);
//        unsafe { self.pool.get_ref() }.borrow_mut().reset();
    }

    fn incr_used(&self) {
        if self.bufs_used.get() == MAX_BUFS {
            self.reset();
        }
        self.bufs_used.update(|x| x + 1);
    }
}

impl<'a> Drop for CommandPool<'a> {
    fn drop(&mut self) {
        let device = &self.data.device;
        device.destroy_command_pool(
            RefCell::into_inner(MaybeUninit::take(&mut self.pool)).into_raw(),
        );
        println!("Dropped Commandpool");
    }
}
