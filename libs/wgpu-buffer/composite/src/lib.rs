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
use anyhow::Result;
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
    ) -> Result<Self> {
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
                vertex: wgpu::VertexState {
                    module: &gpu.create_shader_module(
                        "composite.vert",
                        include_bytes!("../target/composite.vert.spirv"),
                    )?,
                    entry_point: "main",
                    buffers: &[FullscreenVertex::descriptor()],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &gpu.create_shader_module(
                        "composite.frag",
                        include_bytes!("../target/composite.frag.spirv"),
                    )?,
                    entry_point: "main",
                    targets: &[wgpu::ColorTargetState {
                        format: GPU::SCREEN_FORMAT,
                        color_blend: wgpu::BlendState::REPLACE,
                        alpha_blend: wgpu::BlendState::REPLACE,
                        write_mask: wgpu::ColorWrite::ALL,
                    }],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleStrip,
                    strip_index_format: Some(wgpu::IndexFormat::Uint32),
                    front_face: wgpu::FrontFace::Cw,
                    cull_mode: wgpu::CullMode::Back,
                    polygon_mode: wgpu::PolygonMode::Fill,
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: GPU::DEPTH_FORMAT,
                    depth_write_enabled: false, // FIXME
                    depth_compare: wgpu::CompareFunction::Always,
                    stencil: wgpu::StencilState {
                        front: wgpu::StencilFaceState::IGNORE,
                        back: wgpu::StencilFaceState::IGNORE,
                        read_mask: 0,
                        write_mask: 0,
                    },
                    bias: wgpu::DepthBiasState {
                        constant: 0,
                        slope_scale: 0.0,
                        clamp: 0.0,
                    },
                    clamp_depth: false,
                }),
                multisample: wgpu::MultisampleState {
                    count: 1,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
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
    ) -> Result<wgpu::RenderPass<'a>> {
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(Group::Globals.index(), globals.bind_group(), &[]);
        rpass.set_bind_group(Group::OffScreenWorld.index(), world.bind_group(), &[]);
        rpass.set_bind_group(Group::OffScreenUi.index(), ui.bind_group(), &[]);
        rpass.set_vertex_buffer(0, fullscreen.vertex_buffer());
        rpass.draw(fullscreen.vertex_buffer_range(), 0..1);
        Ok(rpass)
    }
}
