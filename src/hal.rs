use std::{
	borrow::Borrow,
	cell::RefCell,
	mem::MaybeUninit,
};

#[cfg(not(feature = "gl"))]
use gfx_hal::adapter::DeviceType;
use gfx_hal::{
	adapter::{
		Adapter,
		PhysicalDevice,
	},
	command::{
		Primary,
		Submittable,
	},
	pso::PipelineStage,
	Device,
	Graphics,
	Instance,
	QueueFamily,
	QueueGroup,
	Submission,
	Surface,
	Swapchain as HAL_Swapchain,
};
use gfx_memory::SmartAllocator;

#[cfg(not(feature = "gl"))]
use crate::gfx_back;
use crate::{
	buffer::StagingBuffer,
	gfx_back::Backend,
	shader::*,
	texture::TextureInfo,
	*,
};

pub struct HALData {
	pub(crate) device: <Backend as gfx_hal::Backend>::Device,
	pub(crate) queue_group: RefCell<QueueGroup<Backend, Graphics>>,
	pub(crate) surface: RefCell<<Backend as gfx_hal::Backend>::Surface>,
	pub(crate) adapter: Adapter<Backend>,
	pub(crate) allocator: MaybeUninit<RefCell<SmartAllocator<Backend>>>,
	#[cfg(not(feature = "gl"))]
	pub(crate) instance: gfx_back::Instance,
}

impl<'a> HALData {
	pub fn new_hal(name: &str, window: &mut Window) -> HALData {
		println!("Creating new HAL");
		#[cfg(not(feature = "gl"))]
		let instance = gfx_back::Instance::create("Villkiss Engine", 1);

		#[cfg(not(feature = "gl"))]
		let surface = instance.create_surface(window.window());
		#[cfg(feature = "gl")]
		let surface = window.surface.take().unwrap();

		#[cfg(not(feature = "gl"))]
		let adapter = {
			let mut adapters = instance.enumerate_adapters();
			let mut adapter = None;
			for a in adapters.drain(..) {
				println!("Possible Adapter: {:?}", &a.info.name);
				if a.info.device_type == DeviceType::DiscreteGpu {
					adapter = Some(a);
					break;
				}
			}
			adapter.unwrap()
		};
		#[cfg(feature = "gl")]
		let adapter = surface.enumerate_adapters().remove(0);

		println!("Chosen adapter: {:?}", &adapter.info.name);

		let (device, queue_group) = adapter
			.open_with::<_, Graphics>(1, |family| {
				surface.supports_queue_family(family) && family.supports_graphics()
			})
			.expect("Unable to open adapter");
		let allocator = SmartAllocator::new(
			adapter.physical_device.memory_properties(),
			4096,
			8,
			64,
			134217728,
		);
		HALData {
			device,
			queue_group: RefCell::new(queue_group),
			surface: RefCell::new(surface),
			adapter,
			allocator: MaybeUninit::new(RefCell::new(allocator)),
			#[cfg(not(feature = "gl"))]
			instance,
		}
	}

	pub fn create_shader<
		'b,
		Vertex: VertexInfo,
		Uniforms: UniformInfo,
		Index: IndexType,
		Constants: PushConstantInfo,
	>(
		&'a self,
		shaders: ShaderModData<'b>,
	) -> Shader<'a, Vertex, Uniforms, Index, Constants> {
		Shader::create(self, shaders)
	}

	pub fn create_command_pool(&self) -> CommandPool { CommandPool::create(self) }

	pub fn create_swapchain<'b>(&'a self, staging_buf: &'b StagingBuffer) -> Swapchain<'a> {
		Swapchain::create(self, staging_buf)
	}

	pub fn create_fence(&self) -> Fence { Fence::create(self) }

	pub fn create_semaphore(&self) -> Semaphore { Semaphore::create(self) }

	pub(crate) fn submit<'b, T, Ic, S, Iw, Is>(&self, sub: Submission<Ic, Iw, Is>, fence: &Fence)
	where
		T: 'b + Submittable<Backend, Graphics, Primary>,
		Ic: IntoIterator<Item = &'b T>,
		S: 'b + Borrow<<Backend as gfx_hal::Backend>::Semaphore>,
		Iw: IntoIterator<Item = (&'b S, PipelineStage)>,
		Is: IntoIterator<Item = &'b S>,
	{
		unsafe {
			self.queue_group.borrow_mut().queues[0].submit(sub, Some(fence.fence()));
		}
	}

	pub fn present(
		&self,
		swap: &Swapchain,
		frame_idx: u32,
		present_sems: &[&Semaphore],
	) -> Result<(), ()> {
		let queue = &mut self.queue_group.borrow_mut().queues[0];
		let swap = unsafe { swap.swapchain.get_ref() }.borrow();
		let present_sems = present_sems.iter().map(|s| s.semaphore());
		unsafe { swap.present(queue, frame_idx, present_sems) }
	}

	pub fn wait_idle(&self) {
		self.device.wait_idle().unwrap();
		self.queue_group.borrow_mut().queues[0].wait_idle().unwrap();
	}

	pub fn create_texture<'b>(
		&self,
		info: TextureInfo<'b>,
		staging_buf: &'b StagingBuffer,
	) -> Texture {
		Texture::create(self, info, staging_buf)
	}
}
