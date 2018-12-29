use std::{
	cell::RefCell,
	iter::once,
	mem::MaybeUninit,
};

use gfx_hal::{
	command::{
		CommandBuffer,
		OneShot,
		Primary,
	},
	pool::{
		CommandPool as HAL_CommandPool,
		CommandPoolCreateFlags,
	},
	pso::PipelineStage,
	Device,
	Graphics,
	Submission,
};

use crate::{
	gfx_back::Backend,
	util::TakeExt,
	Fence,
	HALData,
	Semaphore,
};

pub struct CommandPool<'a> {
	pub(crate) data: &'a HALData,
	pub(crate) pool: MaybeUninit<RefCell<HAL_CommandPool<Backend, Graphics>>>,
    buffers: RefCell<Vec<CommandBuffer<Backend, Graphics, OneShot, Primary>>>
}

impl<'a> CommandPool<'a> {
	pub(crate) fn create(data: &HALData) -> CommandPool {
		println!("Creating Commandpool");
		let device = &data.device;

		let pool = unsafe {
			device
				.create_command_pool_typed(
					&data.queue_group.borrow(),
					CommandPoolCreateFlags::empty(),
				)
				.unwrap()
		};
		CommandPool {
			data,
			pool: MaybeUninit::new(RefCell::new(pool)),
            buffers: RefCell::new(Vec::with_capacity(4))
		}
	}

	pub fn reset(&self) {
		unsafe {
            let mut pool = self.pool.get_ref().borrow_mut();
            pool.free(self.buffers.borrow_mut().drain(..));
			pool.reset();
		}
	}

	pub fn single_submit(
		&self,
		wait_sems: &[(&Semaphore, PipelineStage)],
		signal_sems: &[&Semaphore],
		fence: &Fence,
		f: impl FnOnce(&mut CommandBuffer<Backend, Graphics, OneShot, Primary>),
	) {
		unsafe {
			let mut buffer = self
				.pool
				.get_ref()
				.borrow_mut()
				.acquire_command_buffer::<OneShot>();
			buffer.begin();
			f(&mut buffer);
			buffer.finish();

			let wait_sems = wait_sems
				.iter()
				.map(|(sem, stage)| (sem.semaphore(), *stage));
			let signal_sems = signal_sems.iter().map(|sem| sem.semaphore());
			let submission = Submission {
				command_buffers: once(&buffer),
				wait_semaphores: wait_sems,
				signal_semaphores: signal_sems,
			};
			self.data.submit(submission, fence);
            self.buffers.borrow_mut().push(buffer);
		}
	}
}

impl<'a> Drop for CommandPool<'a> {
	fn drop(&mut self) {
		let device = &self.data.device;
		unsafe {
			self.reset();
			device.destroy_command_pool(
				RefCell::into_inner(MaybeUninit::take(&mut self.pool)).into_raw(),
			);
		}
		println!("Dropped Commandpool");
	}
}
