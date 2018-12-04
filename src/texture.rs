use std::{iter::once, mem::MaybeUninit};

use gfx_hal::{
    self,
    command::{BufferImageCopy, ImageBlit},
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
            MipMaps::Generate => {
                f32::log(
                    u32::max(info.kind.extent().width, info.kind.extent().height) as f32,
                    2f32,
                )
                .floor() as u8
                    + 1
            }
        }
    }
}

#[derive(Copy, Clone)]
pub struct TextureInfo<'a> {
    pub kind: Kind,
    pub format: Format,
    pub mipmaps: MipMaps,
    pub pixels: Option<&'a [u8]>,
}

impl<'a> Texture<'a> {
    pub(crate) fn create<'b>(pool: &'a BufferPool, info: TextureInfo<'b>) -> Texture<'a> {
        println!("Creating Texture");
        let data = &pool.data;
        let device = &data.device;
        let extent = info.kind.extent();

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
                    wrap_mode: (WrapMode::Tile, WrapMode::Tile, WrapMode::Tile),
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

        let (image, block) = Texture::image_block(pool, &info, usage);
        info.pixels.map_or_else(
            || {
                pool.command_pool
                    .single_submit(false, &[], &[], None, |cmd_buf| {
                        Self::transition_image_layout(
                            cmd_buf,
                            &image,
                            0,
                            Layout::Undefined,
                            Layout::DepthStencilAttachmentOptimal,
                        );
                    });
            },
            |pixels| {
                let staged = unsafe { pool.staging_buf.get_ref() };
                staged.upload(pixels, device);
                pool.command_pool
                    .single_submit(false, &[], &[], None, |cmd_buf| {
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
                                level,
                                Layout::TransferDstOptimal,
                                Layout::ShaderReadOnlyOptimal,
                            );
                        }
                    });
            },
        );
        match info.mipmaps {
            MipMaps::Generate => Self::gen_mipmaps(&image, &pool, info),
            _ => (),
        }

        let kind = match info.kind {
            Kind::D1(_, _) => ViewKind::D1,
            Kind::D2(_, _, _, _) => ViewKind::D2,
            Kind::D3(_, _, _) => ViewKind::D3,
        };

        let view = ImageView::create(data, &image, info.format, kind, aspects, mip_levels);

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
        let mips = info.mipmaps.levels(*info);
        let unbound_image = device
            .create_image(
                info.kind,
                mips,
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

    fn gen_mipmaps(
        image: &<Backend as gfx_hal::Backend>::Image,
        pool: &BufferPool,
        info: TextureInfo,
    ) {
        pool.command_pool
            .single_submit(false, &[], &[], None, |buffer| {
                let (mut width, mut height) = {
                    let extent = info.kind.extent();
                    (extent.width, extent.height)
                };
                let levels = info.mipmaps.levels(info);
                println!("Mipmap levels: {}", levels);
                for i in 1..levels {
                    let level = i - 1;
                    let init_barrier = Barrier::Image {
                        states: (Access::TRANSFER_WRITE, Layout::TransferDstOptimal)
                            ..(Access::TRANSFER_READ, Layout::TransferSrcOptimal),
                        target: image,
                        range: SubresourceRange {
                            aspects: Aspects::COLOR,
                            levels: level..(level + 1),
                            layers: 0..1,
                        },
                    };
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
                        states: (Access::TRANSFER_READ, Layout::TransferSrcOptimal)
                            ..(Access::SHADER_READ, Layout::ShaderReadOnlyOptimal),
                        target: image,
                        range: SubresourceRange {
                            aspects: Aspects::COLOR,
                            levels: level..(level + 1),
                            layers: 0..1,
                        },
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
                }
                let fin_barrier = Barrier::Image {
                    states: (Access::TRANSFER_READ, Layout::TransferSrcOptimal)
                        ..(Access::SHADER_READ, Layout::ShaderReadOnlyOptimal),
                    target: image,
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
            })
    }

    fn transition_image_layout(
        cmd_buf: &mut gfx_hal::command::CommandBuffer<Backend, Graphics>,
        image: &<Backend as gfx_hal::Backend>::Image,
        levels: u8,
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
                levels: levels..levels + 1,
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
            .single_submit(false, &[], &[], None, |cmd_buf| {
                Self::transition_image_layout(
                    cmd_buf,
                    &img,
                    1,
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
