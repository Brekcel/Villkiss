use std::{
	iter::once,
	mem::MaybeUninit,
	ops::Range,
};

use gfx_hal::{
	self,
	command::{
		BufferImageCopy,
		ImageBlit,
	},
	format::{
		Aspects,
		Format,
	},
	image::{
		Access,
		Anisotropic,
		Filter,
		Kind,
		Layout,
		Offset,
		PackedColor,
		SamplerInfo,
		SubresourceLayers,
		SubresourceRange,
		Tiling,
		Usage,
		ViewCapabilities,
		ViewKind,
		WrapMode,
	},
	memory::{
		Barrier,
		Dependencies,
		Properties,
	},
	pso::{
		Descriptor,
		PipelineStage,
	},
	queue::Graphics,
	Device,
};

use gfx_memory::{
	Block,
	MemoryAllocator,
	SmartAllocator,
	Type,
};

use crate::{
	buffer::{inner::InnerBuffer, StagingBuffer},
	gfx_back::Backend,
	util::TakeExt,
	CommandPool,
	Fence,
	HALData,
	ImageView,
	Sampler,
};

pub struct Texture<'a> {
	pub(crate) data: &'a HALData,
	pub(crate) kind: ViewKind,
	pub(crate) format: Format,
	pub(crate) image: MaybeUninit<<Backend as gfx_hal::Backend>::Image>,
	pub(crate) block: MaybeUninit<<SmartAllocator<Backend> as MemoryAllocator<Backend>>::Block>,
	pub(crate) view: ImageView<'a>,
	pub(crate) sampler: Option<Sampler<'a>>,
}

#[derive(Copy, Clone)]
pub enum MipMaps {
	PreExisting(u8),
	Generate,
	None,
}

impl MipMaps {
	fn levels(&self, info: TextureInfo) -> u8 {
		match self {
			MipMaps::PreExisting(i) => *i,
			MipMaps::None => 1,
			MipMaps::Generate =>
				f32::log(
					u32::max(info.kind.extent().width, info.kind.extent().height) as f32,
					2f32,
				)
				.floor() as u8 + 1,
		}
	}
}

#[derive(Copy, Clone)]
pub struct TextureInfo<'a> {
	pub kind: Kind,
	pub format: Format,
	pub mipmaps: MipMaps,
	pub pixels: Option<&'a [u8]>,
	pub wrap_mode: (WrapMode, WrapMode, WrapMode),
}

impl<'a> Texture<'a> {
	pub(crate) fn create<'b>(
		data: &'a HALData,
		info: TextureInfo<'b>,
		staging_buf: &'b StagingBuffer,
	) -> Texture<'a> {
		println!("Creating Texture");
		let device = &data.device;
		let extent = info.kind.extent();
		let command_pool = &staging_buf.command_pool;
		let mip_levels = info.mipmaps.levels(info);
		let (usage, aspects, sampler) = if info.pixels.is_some() {
			let mut usage = Usage::TRANSFER_DST | Usage::SAMPLED;
			match info.mipmaps {
				MipMaps::Generate => usage |= Usage::TRANSFER_SRC,
				_ => (),
			}
			let aspects = Aspects::COLOR;
			let sampler = Some(Sampler::create(
				data,
				SamplerInfo {
					min_filter: Filter::Linear,
					mag_filter: Filter::Linear,
					mip_filter: Filter::Linear,
					wrap_mode: info.wrap_mode,
					lod_bias: 0f32.into(),
					lod_range: 0f32.into()..((mip_levels as f32) / 8f32).into(),
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
		let (image, block) = Texture::image_block(data, &info, usage);
		let fence = &staging_buf.fence;
		info.pixels.map_or_else(
			|| {
				command_pool.single_submit(&[], &[], &fence, |cmd_buf| {
					Self::transition_image_layout(
						cmd_buf,
						&image,
						0,
						Layout::Undefined..Layout::DepthStencilAttachmentOptimal,
					);
				})
			},
			|pixels| {
				staging_buf.upload(pixels);
				command_pool.single_submit(&[], &[], &fence, |cmd_buf| {
					let range = match info.mipmaps {
						MipMaps::PreExisting(i) => 0..i,
						_ => 0..1,
					};
					for level in range {
						let copy = BufferImageCopy {
							buffer_offset: 0,
							buffer_width: 0,
							buffer_height: 0,
							image_layers: SubresourceLayers {
								aspects: Aspects::COLOR,
								level,
								layers: 0..1,
							},
							image_offset: Offset::ZERO,
							image_extent: extent,
						};
						Self::transition_image_layout(
							cmd_buf,
							&image,
							level,
							Layout::Undefined..Layout::TransferDstOptimal,
						);
						unsafe {
							cmd_buf.copy_buffer_to_image(
								&staging_buf.hal_buffer(),
								&image,
								Layout::TransferDstOptimal,
								once(copy),
							);
						}
						Self::transition_image_layout(
							cmd_buf,
							&image,
							level,
							Layout::TransferDstOptimal..Layout::ShaderReadOnlyOptimal,
						);
					}
				})
			},
		);
		match info.mipmaps {
			MipMaps::Generate => Self::gen_mipmaps(&image, command_pool, info, &fence),
			_ => (),
		}

		let kind = match info.kind {
			Kind::D1(_, _) => ViewKind::D1,
			Kind::D2(_, _, _, _) => ViewKind::D2,
			Kind::D3(_, _, _) => ViewKind::D3,
		};

		let view = ImageView::create(data, &image, info.format, kind, aspects, mip_levels);

		fence.wait();
		Texture {
			data,
			kind,
			format: info.format,
			image: MaybeUninit::new(image),
			block: MaybeUninit::new(block),
			view,
			sampler,
		}
	}

	fn image_block<'b>(
		data: &'a HALData,
		info: &'b TextureInfo<'b>,
		usage: Usage,
	) -> (
		<Backend as gfx_hal::Backend>::Image,
		<SmartAllocator<Backend> as MemoryAllocator<Backend>>::Block,
	) {
		let device = &data.device;
		let mips = info.mipmaps.levels(*info);
		unsafe {
			let mut image = device
				.create_image(
					info.kind,
					mips,
					info.format,
					Tiling::Optimal,
					usage,
					ViewCapabilities::empty(),
				)
				.unwrap();
			let reqs = device.get_image_requirements(&image);
			let block = data
				.allocator
				.get_ref()
				.borrow_mut()
				.alloc(device, (Type::General, Properties::DEVICE_LOCAL), reqs)
				.unwrap();
			device
				.bind_image_memory(block.memory(), block.range().start, &mut image)
				.unwrap();
			(image, block)
		}
	}

	fn gen_mipmaps(
		image: &<Backend as gfx_hal::Backend>::Image,
		command_pool: &CommandPool,
		info: TextureInfo,
		fence: &Fence,
	) {
		fence.wait_n_reset();
		command_pool.single_submit(&[], &[], fence, |buffer| {
			let (mut width, mut height) = {
				let extent = info.kind.extent();
				(extent.width, extent.height)
			};
			let levels = info.mipmaps.levels(info);
			for i in 1..levels {
				let level = i - 1;
				let range = SubresourceRange {
					aspects: Aspects::COLOR,
					levels: level..(level + 1),
					layers: 0..1,
				};
				let init_barrier = Barrier::Image {
					states: (Access::TRANSFER_WRITE, Layout::TransferDstOptimal)..
						(Access::TRANSFER_READ, Layout::TransferSrcOptimal),
					target: image,
					families: None,
					range: range.clone(),
				};

				unsafe {
					buffer.pipeline_barrier(
						PipelineStage::TRANSFER..PipelineStage::TRANSFER,
						Dependencies::empty(),
						once(init_barrier),
					);

					let blit = ImageBlit {
						src_subresource: SubresourceLayers {
							aspects: Aspects::COLOR,
							level: i - 1,
							layers: 0..1,
						},
						src_bounds: Offset { x: 0, y: 0, z: 0 }..Offset {
							x: width as i32,
							y: height as i32,
							z: 1,
						},
						dst_subresource: SubresourceLayers {
							aspects: Aspects::COLOR,
							level: i,
							layers: 0..1,
						},
						dst_bounds: Offset { x: 0, y: 0, z: 0 }..Offset {
							x: if width > 1 { width / 2 } else { 1 } as i32,
							y: if height > 1 { height / 2 } else { 1 } as i32,
							z: 1,
						},
					};
					buffer.blit_image(
						image,
						Layout::TransferSrcOptimal,
						image,
						Layout::TransferDstOptimal,
						Filter::Linear,
						once(blit),
					);
					let fin_barrier = Barrier::Image {
						states: (Access::TRANSFER_READ, Layout::TransferSrcOptimal)..
							(Access::SHADER_READ, Layout::ShaderReadOnlyOptimal),
						target: image,
						families: None,
						range: range.clone(),
					};
					buffer.pipeline_barrier(
						PipelineStage::TRANSFER..PipelineStage::FRAGMENT_SHADER,
						Dependencies::empty(),
						once(fin_barrier),
					);

					if width > 1 {
						width /= 2;
					}
					if height > 1 {
						height /= 2;
					}

					let fin_barrier = Barrier::Image {
						states: (Access::TRANSFER_READ, Layout::TransferSrcOptimal)..
							(Access::SHADER_READ, Layout::ShaderReadOnlyOptimal),
						target: image,
						families: None,
						range: SubresourceRange {
							aspects: Aspects::COLOR,
							levels: levels - 1..levels,
							layers: 0..1,
						},
					};
					buffer.pipeline_barrier(
						PipelineStage::TRANSFER..PipelineStage::FRAGMENT_SHADER,
						Dependencies::empty(),
						once(fin_barrier),
					);
				}
			}
		});
	}

	fn transition_image_layout(
		cmd_buf: &mut gfx_hal::command::CommandBuffer<Backend, Graphics>,
		image: &<Backend as gfx_hal::Backend>::Image,
		levels: u8,
		layout: Range<Layout>,
	) {
		let (aspects, access, stage) =
			if layout.start == Layout::Undefined && layout.end == Layout::TransferDstOptimal {
				(
					Aspects::COLOR,
					Access::empty()..Access::TRANSFER_WRITE,
					PipelineStage::TOP_OF_PIPE..PipelineStage::TRANSFER,
				)
			} else if layout.start == Layout::TransferDstOptimal &&
				layout.end == Layout::ShaderReadOnlyOptimal
			{
				(
					Aspects::COLOR,
					Access::TRANSFER_WRITE..Access::SHADER_READ,
					PipelineStage::TRANSFER..PipelineStage::FRAGMENT_SHADER,
				)
			} else if layout.start == Layout::Undefined &&
				layout.end == Layout::DepthStencilAttachmentOptimal
			{
				(
					Aspects::DEPTH | Aspects::STENCIL,
					Access::empty()..
						(Access::DEPTH_STENCIL_ATTACHMENT_READ |
							Access::DEPTH_STENCIL_ATTACHMENT_WRITE),
					PipelineStage::TOP_OF_PIPE..PipelineStage::EARLY_FRAGMENT_TESTS,
				)
			} else {
				panic!("Unsupported layout change");
			};
		let mem_barrier = Barrier::Image {
			states: (access.start, layout.start)..(access.end, layout.end),
			target: image,
			families: None,
			range: SubresourceRange {
				aspects,
				levels: levels..levels + 1,
				layers: 0..1,
			},
		};

		unsafe {
			cmd_buf.pipeline_barrier(stage, Dependencies::empty(), once(&mem_barrier));
		}
	}

	pub(crate) fn image(&self) -> &<Backend as gfx_hal::Backend>::Image {
		unsafe { self.image.get_ref() }
	}

	pub fn view(&self) -> &ImageView { &self.view }

	pub fn sampler(&self) -> &Option<Sampler> { &self.sampler }

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
		let device = &self.data.device;
		unsafe {
			self.data
				.allocator
				.get_ref()
				.borrow_mut()
				.free(device, MaybeUninit::take(&mut self.block));

			device.destroy_image(img);
		}
		println!("Dropped Texture");
	}
}
