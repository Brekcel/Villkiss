use std::{cell::RefCell, mem::MaybeUninit};

use gfx_hal::{command::{CommandBuffer}, pso::PipelineStage,	Device, Graphics, Submission,
	pool::{CommandPool as HAL_CommandPool, CommandPoolCreateFlags}};

use crate::{HALData, Semaphore, Fence, util::TakeExt};
use crate::gfx_back::Backend;

pub struct CommandPool<'a> {
	data: &'a HALData,
	pool: MaybeUninit<RefCell<HAL_CommandPool<Backend, Graphics>>>,
}

impl<'a> CommandPool<'a> {
	pub fn create<'b: 'a>(data: &'b HALData) -> CommandPool<'a> {
		println!("Creating Commandpool");
		let flags: CommandPoolCreateFlags = CommandPoolCreateFlags::empty();
		const MAX_BUFS: usize = 16;
		let device = &data.device;
		let pool = device.create_command_pool_typed(&data.queue_group.borrow(), flags, MAX_BUFS).unwrap();
		CommandPool {
			data,
			pool: MaybeUninit::new(RefCell::new(pool)),
		}
	}

	pub fn buffer<F>(&self, allow_pending_resubmit: bool, wait_sems: &[(&Semaphore, PipelineStage)], signal_sems: &[&Semaphore], fence: Option<&Fence>, mut f: F)
		where F: FnMut(&mut CommandBuffer<Backend, Graphics>) {
		let mut pool = unsafe { self.pool.get_ref() }.borrow_mut();
		let mut buffer = pool.acquire_command_buffer(allow_pending_resubmit);
		f(&mut buffer);
		let finished = buffer.finish();
		let wait_sems = wait_sems.iter().map(|(sem, stage)| (sem.semaphore(), *stage)).collect::<Vec<_>>();
		let signal_sems = signal_sems.iter().map(|sem| sem.semaphore()).collect::<Vec<_>>();
		let submission = Submission::new()
			.wait_on(wait_sems)
			.signal(&signal_sems)
			.submit(vec![finished]);
		self.data.submit(submission, fence);
	}

	pub fn reset(&self) {
		unsafe { self.pool.get_ref() }.borrow_mut().reset();
	}
}

impl<'a> Drop for CommandPool<'a> {
	fn drop(&mut self) {
		let device = &self.data.device;
		device.destroy_command_pool(RefCell::into_inner(MaybeUninit::take(&mut self.pool)).into_raw());
		println!("Dropped Commandpool");
	}
}