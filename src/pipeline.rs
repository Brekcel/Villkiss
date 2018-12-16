use std::mem::MaybeUninit;

use gfx_hal::{
    pso::{
        BlendState, ColorBlendDesc, ColorMask, Comparison, DepthStencilDesc, DepthTest, Face,
        FrontFace, GraphicsPipelineDesc, PolygonMode, Rasterizer, StencilTest,
    },
    Device, Primitive,
};

use crate::gfx_back::Backend;
use crate::util::TakeExt;
use crate::{
    shader::{IndexType, PushConstantInfo, Shader, UniformInfo, VertexInfo},
    RenderPass,
};

pub struct Pipeline<'a> {
    pass: &'a RenderPass<'a>,
    pipe: MaybeUninit<<Backend as gfx_hal::Backend>::GraphicsPipeline>,
}

impl<'a> Pipeline<'a> {
    pub(crate) fn create<
        Vertex: VertexInfo<Vertex>,
        Uniforms: UniformInfo,
        Index: IndexType,
        Constants: PushConstantInfo,
    >(
        pass: &'a RenderPass<'a>,
        shader: &'a Shader<'a, Vertex, Uniforms, Index, Constants>,
    ) -> Pipeline<'a> {
        pub const RASTERIZER: Rasterizer = Rasterizer {
            polygon_mode: PolygonMode::Fill,
            cull_face: Face::BACK,
            front_face: FrontFace::CounterClockwise,
            depth_clamping: false,
            depth_bias: None,
            conservative: false,
        };

        println!("Creating Pipeline");
        let device = &pass.swapchain.data.device;
        let shad_set = shader.make_set();
        let pipe_layout = shader.pipe_layout();
        let desc_layout = shader.desc_layout();
        let subpass = pass.make_subpass();

        let mut pipeline_desc = GraphicsPipelineDesc::new(
            shad_set,
            Primitive::TriangleList,
            RASTERIZER,
            pipe_layout,
            subpass,
        );
        pipeline_desc.depth_stencil = DepthStencilDesc {
            depth: DepthTest::On {
                fun: Comparison::Less,
                write: true,
            },
            depth_bounds: false,
            stencil: StencilTest::Off,
        };
        pipeline_desc
            .blender
            .targets
            .push(ColorBlendDesc(ColorMask::ALL, BlendState::ALPHA));

        shader.describe_vertices(
            &mut pipeline_desc.vertex_buffers,
            &mut pipeline_desc.attributes,
        );

        let pipe = device
            .create_graphics_pipeline(&pipeline_desc, None)
            .unwrap();

        Pipeline {
            pass,
            pipe: MaybeUninit::new(pipe),
        }
    }

    pub fn pipe(&self) -> &<Backend as gfx_hal::Backend>::GraphicsPipeline {
        unsafe { self.pipe.get_ref() }
    }
}

impl<'a> Drop for Pipeline<'a> {
    fn drop(&mut self) {
        let device = &self.pass.swapchain.data.device;
        device.destroy_graphics_pipeline(MaybeUninit::take(&mut self.pipe));
        println!("Dropped Pipeline");
    }
}
