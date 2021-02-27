// This file is part of Nitrogen.
//
// Nitrogen is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Nitrogen is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Nitrogen.  If not, see <http://www.gnu.org/licenses/>.
use failure::Fallible;
use fullscreen::{FullscreenBuffer, FullscreenVertex};
use global_data::GlobalParametersBuffer;
use gpu::GPU;
use log::trace;
use shader_shared::Group;
use ui::UiRenderPass;
use world::WorldRenderPass;

#[derive(Debug)]
pub struct CompositeRenderPass {
    pipeline: wgpu::RenderPipeline,
}

impl CompositeRenderPass {
    pub fn new(
        gpu: &mut GPU,
        globals: &GlobalParametersBuffer,
        world: &WorldRenderPass,
        ui: &UiRenderPass,
    ) -> Fallible<Self> {
        trace!("CompositeRenderPass::new");

        // Layout shared by all three render passes.
        let pipeline_layout =
            gpu.device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("composite-pipeline-layout"),
                    push_constant_ranges: &[],
                    bind_group_layouts: &[
                        globals.bind_group_layout(),
                        world.bind_group_layout(),
                        ui.bind_group_layout(),
                    ],
                });

        let pipeline = gpu
            .device()
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("composite-pipeline"),
                layout: Some(&pipeline_layout),
                vertex_stage: wgpu::ProgrammableStageDescriptor {
                    module: &gpu
                        .create_shader_module(include_bytes!("../target/composite.vert.spirv"))?,
                    entry_point: "main",
                },
                fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
                    module: &gpu
                        .create_shader_module(include_bytes!("../target/composite.frag.spirv"))?,
                    entry_point: "main",
                }),
                rasterization_state: Some(wgpu::RasterizationStateDescriptor {
                    front_face: wgpu::FrontFace::Cw,
                    cull_mode: wgpu::CullMode::Back,
                    depth_bias: 0,
                    depth_bias_slope_scale: 0.0,
                    depth_bias_clamp: 0.0,
                    clamp_depth: false,
                }),
                primitive_topology: wgpu::PrimitiveTopology::TriangleStrip,
                color_states: &[wgpu::ColorStateDescriptor {
                    format: GPU::SCREEN_FORMAT,
                    alpha_blend: wgpu::BlendDescriptor::REPLACE,
                    // FIXME:
                    color_blend: wgpu::BlendDescriptor::REPLACE,
                    // color_blend: wgpu::BlendDescriptor {
                    //     src_factor: wgpu::BlendFactor::SrcAlpha,
                    //     dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    //     operation: wgpu::BlendOperation::Add,
                    // },
                    write_mask: wgpu::ColorWrite::ALL,
                }],
                depth_stencil_state: Some(wgpu::DepthStencilStateDescriptor {
                    format: GPU::DEPTH_FORMAT,
                    depth_write_enabled: false, // FIXME
                    depth_compare: wgpu::CompareFunction::Always,
                    stencil: wgpu::StencilStateDescriptor {
                        front: wgpu::StencilStateFaceDescriptor::IGNORE,
                        back: wgpu::StencilStateFaceDescriptor::IGNORE,
                        read_mask: 0,
                        write_mask: 0,
                    },
                }),
                vertex_state: wgpu::VertexStateDescriptor {
                    index_format: wgpu::IndexFormat::Uint32,
                    vertex_buffers: &[FullscreenVertex::descriptor()],
                },
                sample_count: 1,
                sample_mask: !0,
                alpha_to_coverage_enabled: false,
            });

        Ok(Self { pipeline })
    }

    pub fn composite_scene<'a>(
        &'a self,
        mut rpass: wgpu::RenderPass<'a>,
        fullscreen: &'a FullscreenBuffer,
        globals: &'a GlobalParametersBuffer,
        world: &'a WorldRenderPass,
        ui: &'a UiRenderPass,
    ) -> Fallible<wgpu::RenderPass<'a>> {
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(Group::Globals.index(), globals.bind_group(), &[]);
        rpass.set_bind_group(Group::OffScreenWorld.index(), world.bind_group(), &[]);
        rpass.set_bind_group(Group::OffScreenUi.index(), ui.bind_group(), &[]);
        rpass.set_vertex_buffer(0, fullscreen.vertex_buffer());
        rpass.draw(fullscreen.vertex_buffer_range(), 0..1);
        Ok(rpass)
    }
}
