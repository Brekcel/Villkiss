use std::cell::RefCell;

#[cfg(not(feature = "gl"))]
use gfx_hal::adapter::DeviceType;
use gfx_hal::{
    adapter::Adapter, Device, Graphics, Instance, QueueFamily, QueueGroup, Submission, Surface,
    Swapchain as HAL_Swapchain,
};

#[cfg(not(feature = "gl"))]
use crate::gfx_back;
use crate::gfx_back::Backend;
use crate::{*, shader::*};

pub struct HALData {
    pub(crate) device: <Backend as gfx_hal::Backend>::Device,
    pub(crate) queue_group: RefCell<QueueGroup<Backend, Graphics>>,
    pub(crate) surface: RefCell<<Backend as gfx_hal::Backend>::Surface>,
    pub(crate) adapter: Adapter<Backend>,
    #[cfg(not(feature = "gl"))]
    pub(crate) instance: gfx_back::Instance,
}

impl<'a> HALData {
    pub fn new_hal(name: &str, window: &mut Window) -> HALData {
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

        HALData {
            device,
            queue_group: RefCell::new(queue_group),
            surface: RefCell::new(surface),
            adapter,
            #[cfg(not(feature = "gl"))]
            instance,
        }
    }

    pub fn create_shader<'b>(
        &'a self,
        vert: &'b [u8],
        frag: &'b [u8],
        vertices: &'b [VertexInfo],
        uniforms: &'b [UniformInfo],
    ) -> Shader<'a> {
        Shader::create(self, vert, frag, vertices, uniforms)
    }

    pub fn create_command_pool(&self) -> CommandPool {
        CommandPool::create(self)
    }

    pub fn create_swapchain(&'a self, pool: &'a BufferPool<'a>) -> Swapchain<'a> {
        Swapchain::create(self, pool)
    }

    pub fn create_fence(&self) -> Fence {
        Fence::create(self)
    }

    pub fn create_n_fences(&self, num: usize) -> Box<[Fence]> {
        Fence::create_n(self, num)
    }

    pub fn create_semaphore(&self) -> Semaphore {
        Semaphore::create(self)
    }

    pub fn create_n_semaphore(&self, num: usize) -> Box<[Semaphore]> {
        Semaphore::create_n(self, num)
    }

    pub fn submit(&self, sub: Submission<Backend, Graphics>, fence: Option<&Fence>) {
        let fence = fence.map(|f| f.fence());
        self.queue_group.borrow_mut().queues[0].submit(sub, fence);
        if fence.is_none() {
            self.queue_group.borrow_mut().queues[0].wait_idle().unwrap();
        }
    }

    pub fn present(
        &self,
        swap: &Swapchain,
        frame_idx: u32,
        present_sems: Vec<&Semaphore>,
    ) -> Result<(), ()> {
        let queue = &mut self.queue_group.borrow_mut().queues[0];
        let swap = unsafe { swap.swapchain.get_ref() }.borrow();
        let present_sems = present_sems
            .iter()
            .map(|s| s.semaphore())
            .collect::<Vec<_>>();
        swap.present(queue, frame_idx, present_sems)
    }

    pub fn wait_idle(&self) {
        self.device.wait_idle().unwrap();
    }
}
