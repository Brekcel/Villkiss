#![allow(unused_variables)]
#![allow(dead_code)]
#![feature(maybe_uninit)]
#![feature(nll)]

//#[cfg(feature = "dx12")]
//pub use gfx_backend_dx12 as gfx_back;
//#[cfg(feature = "dx11")]
//pub use gfx_backend_dx11 as gfx_back;
//#[cfg(feature = "gl")]
//pub use gfx_backend_gl as gfx_back;
//#[cfg(feature = "metal")]
//pub use gfx_backend_metal as gfx_back;
//#[cfg(feature = "vulkan")]
pub use gfx_backend_vulkan as gfx_back;

pub use gfx_hal;
pub use winit;
pub use cgmath;

pub use crate::framebuffer::FrameBuffer;
pub use crate::hal::HALData;
pub use crate::pipeline::Pipeline;
pub use crate::renderpass::RenderPass;
pub use crate::shader::Shader;
pub use crate::swapchain::Swapchain;
pub use crate::window::Window;
pub use crate::commandpool::CommandPool;
pub use crate::semaphore::Semaphore;
pub use crate::fence::Fence;
pub use crate::buffer::BufferPool;
pub use crate::descriptors::Descriptors;
pub use crate::texture::Texture;
pub use crate::imageview::ImageView;
pub use crate::sampler::Sampler;

pub mod window;
pub mod hal;
pub mod shader;
pub mod swapchain;
pub mod pipeline;
pub mod renderpass;
pub mod framebuffer;
pub mod commandpool;
pub mod semaphore;
pub mod fence;
pub mod buffer;
pub mod descriptors;
pub mod texture;
pub mod imageview;
pub mod sampler;
mod util;