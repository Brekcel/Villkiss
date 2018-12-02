use std::mem::MaybeUninit;

use gfx_hal::{
	self,
	command::BufferImageCopy,
	format::{Aspects, Format},
	image::{
		Access, Anisotropic, Filter, Kind, Layout, Offset, PackedColor, SamplerInfo,
		SubresourceLayers, SubresourceRange, Tiling, Usage, ViewCapabilities, ViewKind, WrapMode,
	},
	memory::Dependencies,
	memory::{Barrier, Properties},
	pso::Descriptor,
	pso::PipelineStage,
	queue::Graphics,
	Device,
};
use gfx_memory::{Block, MemoryAllocator, SmartAllocator, Type};

use crate::gfx_back::Backend;
use crate::util::TakeExt;
use crate::{BufferPool, ImageView, Sampler};

pub struct Texture<'a> {
	pub(crate) pool: &'a BufferPool<'a>,
	pub(crate) kind: ViewKind,
	pub(crate) format: Format,
	pub(crate) image: MaybeUninit<<Backend as gfx_hal::Backend>::Image>,
	pub(crate) block: MaybeUninit<<SmartAllocator<Backend> as MemoryAllocator<Backend>>::Block>,
	pub(crate) view: ImageView<'a>,
	pub(crate) sampler: Option<Sampler<'a>>,
}

pub struct TextureInfo<'a> {
	pub kind: Kind,
	pub format: Format,
	pub mip_levels: u8,
	pub pixels: Option<&'a [u8]>,
}

impl<'a> Texture<'a> {
	pub(crate) fn create<'b>(pool: &'a BufferPool, info: TextureInfo<'b>) -> Texture<'a> {
		println!("Creating Texture");
		let data = &pool.data;
		let device = &data.device;
		let extent = info.kind.extent();

		let (usage, aspects, sampler) = if info.pixels.is_some() {
			let usage = Usage::TRANSFER_DST | Usage::SAMPLED;
			let aspects = Aspects::COLOR;
			let sampler = Some(Sampler::create(
				data,
				SamplerInfo {
					min_filter: Filter::Linear,
					mag_filter: Filter::Linear,
					mip_filter: Filter::Linear,
					wrap_mode: (WrapMode::Tile, WrapMode::Tile, WrapMode::Tile),
					lod_bias: 0f32.into(),
					lod_range: 0f32.into()..0f32.into(),
					comparison: None,
					border: PackedColor(0x0),
					anisotropic: Anisotropic::On(16),
				},
			));
			(usage, aspects, sampler)
		} else {
			let usage = Usage::DEPTH_STENCIL_ATTACHMENT;
			let aspects = Aspects::DEPTH;
			let sampler = None;
			(usage, aspects, sampler)
		};

		let (image, block) = Texture::image_block(pool, &info, usage);

		info.pixels.map_or_else(
			|| {
				pool.command_pool.buffer(false, &[], &[], None, |cmd_buf| {
					Self::transition_image_layout(
						cmd_buf,
						&image,
						Layout::Undefined,
						Layout::DepthStencilAttachmentOptimal,
					);
				});
			},
			|pixels| {
				let staged = unsafe { pool.staging_buf.get_ref() };
				staged.upload(pixels, device);
				pool.command_pool.buffer(false, &[], &[], None, |cmd_buf| {
					let copy = BufferImageCopy {
						buffer_offset: 0,
						buffer_width: 0,
						buffer_height: 0,
						image_layers: SubresourceLayers {
							aspects: Aspects::COLOR,
							level: 0,
							layers: 0..1,
						},
						image_offset: Offset::ZERO,
						image_extent: extent,
					};
					Self::transition_image_layout(
						cmd_buf,
						&image,
						Layout::Undefined,
						Layout::TransferDstOptimal,
					);
					cmd_buf.copy_buffer_to_image(
						&staged.buffer,
						&image,
						Layout::TransferDstOptimal,
						vec![copy],
					);
					Self::transition_image_layout(
						cmd_buf,
						&image,
						Layout::TransferDstOptimal,
						Layout::ShaderReadOnlyOptimal,
					);
				});
			},
		);

		let kind = match info.kind {
			Kind::D1(_, _) => ViewKind::D1,
			Kind::D2(_, _, _, _) => ViewKind::D2,
			Kind::D3(_, _, _) => ViewKind::D3,
		};

		let view = ImageView::create(data, &image, info.format, kind, aspects);

		Texture {
			pool,
			kind,
			format: info.format,
			image: MaybeUninit::new(image),
			block: MaybeUninit::new(block),
			view,
			sampler: sampler,
		}
	}

	fn image_block<'b>(
		pool: &'a BufferPool,
		info: &'b TextureInfo<'b>,
		usage: Usage,
	) -> (
		<Backend as gfx_hal::Backend>::Image,
		<SmartAllocator<Backend> as MemoryAllocator<Backend>>::Block,
	) {
		let device = &pool.data.device;
		let unbound_image = device
			.create_image(
				info.kind,
				1,
				info.format,
				Tiling::Optimal,
				usage,
				ViewCapabilities::empty(),
			)
			.unwrap();
		let reqs = device.get_image_requirements(&unbound_image);
		let block = unsafe { pool.allocator.get_ref() }
			.borrow_mut()
			.alloc(device, (Type::General, Properties::DEVICE_LOCAL), reqs)
			.unwrap();
		let image = device
			.bind_image_memory(block.memory(), block.range().start, unbound_image)
			.unwrap();
		(image, block)
	}

	fn transition_image_layout(
		cmd_buf: &mut gfx_hal::command::CommandBuffer<Backend, Graphics>,
		image: &<Backend as gfx_hal::Backend>::Image,
		old: Layout,
		new: Layout,
	) {
		let (aspects, src_access, src_stage, dst_access, dst_stage) =
			if old == Layout::Undefined && new == Layout::TransferDstOptimal {
				(
					Aspects::COLOR,
					Access::empty(),
					PipelineStage::TOP_OF_PIPE,
					Access::TRANSFER_WRITE,
					PipelineStage::TRANSFER,
				)
			} else if old == Layout::TransferDstOptimal && new == Layout::ShaderReadOnlyOptimal {
				(
					Aspects::COLOR,
					Access::TRANSFER_WRITE,
					PipelineStage::TRANSFER,
					Access::SHADER_READ,
					PipelineStage::FRAGMENT_SHADER,
				)
			} else if old == Layout::Undefined && new == Layout::DepthStencilAttachmentOptimal {
				(
					Aspects::DEPTH | Aspects::STENCIL,
					Access::empty(),
					PipelineStage::TOP_OF_PIPE,
					Access::DEPTH_STENCIL_ATTACHMENT_READ | Access::DEPTH_STENCIL_ATTACHMENT_WRITE,
					PipelineStage::EARLY_FRAGMENT_TESTS,
				)
			} else {
				panic!("Unsupported layout change");
			};
		let mem_barrier = Barrier::Image {
			states: (src_access, old)..(dst_access, new),
			target: image,
			range: SubresourceRange {
				aspects: aspects,
				levels: 0..1,
				layers: 0..1,
			},
		};

		cmd_buf.pipeline_barrier(
			src_stage..dst_stage,
			Dependencies::empty(),
			vec![&mem_barrier],
		);
	}

	pub(crate) fn image(&self) -> &<Backend as gfx_hal::Backend>::Image {
		unsafe { self.image.get_ref() }
	}

	pub fn view(&self) -> &ImageView {
		&self.view
	}

	pub fn sampler(&self) -> &Option<Sampler> {
		&self.sampler
	}

	pub fn descriptor(&self) -> Descriptor<Backend> {
		match self.sampler() {
			Some(sampler) => Descriptor::CombinedImageSampler(
				self.view.view(),
				Layout::ShaderReadOnlyOptimal,
				sampler.sampler(),
			),
			None => Descriptor::Image(self.view.view(), Layout::TransferDstOptimal),
		}
	}
}

impl<'a> Drop for Texture<'a> {
	fn drop(&mut self) {
		let img = MaybeUninit::take(&mut self.image);
		self.pool
			.command_pool
			.buffer(false, &[], &[], None, |cmd_buf| {
				Self::transition_image_layout(
					cmd_buf,
					&img,
					Layout::TransferDstOptimal,
					Layout::ShaderReadOnlyOptimal,
				);
			});
		let device = &self.pool.data.device;
		unsafe { self.pool.allocator.get_ref() }
			.borrow_mut()
			.free(device, MaybeUninit::take(&mut self.block));
		device.destroy_image(img);
		println!("Dropped Texture");
	}
}
