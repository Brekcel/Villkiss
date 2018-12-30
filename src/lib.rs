#![allow(unused_variables)]
#![allow(dead_code)]
#![feature(cell_update)]
#![feature(maybe_uninit)]
#![feature(nll)]

#[cfg(feature = "dx12")]
pub use gfx_backend_dx12 as gfx_back;
#[cfg(feature = "dx11")]
pub use gfx_backend_dx11 as gfx_back;
#[cfg(feature = "gl")]
pub use gfx_backend_gl as gfx_back;
#[cfg(feature = "metal")]
pub use gfx_backend_metal as gfx_back;
#[cfg(feature = "vulkan")]
pub use gfx_backend_vulkan as gfx_back;

pub use cgmath;
pub use gfx_hal;
pub use winit;

pub use crate::{
	buffer::Buffer,
	bufferpool::BufferPool,
	commandpool::CommandPool,
	descriptorpool::DescriptorPool,
	fence::Fence,
	framebuffer::FrameBuffer,
	hal::HALData,
	imageview::ImageView,
	mesh::Mesh,
	pipeline::{BoundPipe, Pipeline},
	renderpass::RenderPass,
	sampler::Sampler,
	semaphore::Semaphore,
	shader::{ShaderModData, Shader},
	swapchain::Swapchain,
	texture::Texture,
	window::Window,
};

pub mod buffer;
pub mod bufferpool;
pub mod commandpool;
pub mod descriptorpool;
pub mod fence;
pub mod framebuffer;
pub mod hal;
pub mod imageview;
pub mod mesh;
pub mod pipeline;
pub mod renderpass;
pub mod sampler;
pub mod semaphore;
pub mod shader;
pub mod swapchain;
pub mod texture;
mod util;
pub mod window;
