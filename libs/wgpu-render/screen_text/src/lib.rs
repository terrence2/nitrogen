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
use commandable::{commandable, Commandable};
use failure::Fallible;
use global_data::GlobalParametersBuffer;
use gpu::GPU;
use log::trace;
use shader_shared::Group;
use text_layout::{LayoutVertex, TextLayoutBuffer};

#[derive(Commandable)]
pub struct ScreenTextRenderPass {
    pipeline: wgpu::RenderPipeline,
}

#[commandable]
impl ScreenTextRenderPass {
    pub fn new(
        gpu: &mut GPU,
        global_data: &GlobalParametersBuffer,
        layout_buffer: &TextLayoutBuffer,
    ) -> Fallible<Self> {
        trace!("ScreenTextRenderPass::new");

        let vert_shader =
            gpu.create_shader_module(include_bytes!("../target/screen_text.vert.spirv"))?;
        let frag_shader =
            gpu.create_shader_module(include_bytes!("../target/screen_text.frag.spirv"))?;

        let pipeline_layout =
            gpu.device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("screen-text-pipeline-layout"),
                    push_constant_ranges: &[],
                    bind_group_layouts: &[
                        global_data.bind_group_layout(),
                        layout_buffer.glyph_bind_group_layout(),
                        layout_buffer.layout_bind_group_layout(),
                    ],
                });

        let pipeline = gpu
            .device()
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("screen-text-pipeline"),
                layout: Some(&pipeline_layout),
                vertex_stage: wgpu::ProgrammableStageDescriptor {
                    module: &vert_shader,
                    entry_point: "main",
                },
                fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
                    module: &frag_shader,
                    entry_point: "main",
                }),
                rasterization_state: Some(wgpu::RasterizationStateDescriptor {
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: wgpu::CullMode::Back,
                    depth_bias: 0,
                    depth_bias_slope_scale: 0.0,
                    depth_bias_clamp: 0.0,
                    clamp_depth: false,
                }),
                primitive_topology: wgpu::PrimitiveTopology::TriangleList,
                color_states: &[wgpu::ColorStateDescriptor {
                    format: GPU::SCREEN_FORMAT,
                    alpha_blend: wgpu::BlendDescriptor::REPLACE,
                    color_blend: wgpu::BlendDescriptor {
                        src_factor: wgpu::BlendFactor::SrcAlpha,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                    write_mask: wgpu::ColorWrite::ALL,
                }],
                depth_stencil_state: Some(wgpu::DepthStencilStateDescriptor {
                    format: GPU::DEPTH_FORMAT,
                    depth_write_enabled: false,
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
                    vertex_buffers: &[LayoutVertex::descriptor()],
                },
                sample_count: 1,
                sample_mask: !0,
                alpha_to_coverage_enabled: false,
            });

        Ok(Self { pipeline })
    }

    pub fn draw<'a>(
        &'a self,
        mut rpass: wgpu::RenderPass<'a>,
        global_data: &'a GlobalParametersBuffer,
        layout_buffer: &'a TextLayoutBuffer,
    ) -> Fallible<wgpu::RenderPass<'a>> {
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(Group::Globals.index(), &global_data.bind_group(), &[]);
        for (font_name, layout_handles) in layout_buffer.layouts_by_font() {
            let glyph_cache = layout_buffer.glyph_cache(font_name);
            rpass.set_bind_group(Group::GlyphCache.index(), &glyph_cache.bind_group(), &[]);
            for &layout_handle in layout_handles {
                let layout = layout_buffer.layout(layout_handle);
                rpass.set_bind_group(Group::TextLayout.index(), &layout.bind_group(), &[]);

                rpass.set_index_buffer(layout.index_buffer());
                rpass.set_vertex_buffer(0, layout.vertex_buffer());
                rpass.draw_indexed(layout.index_range(), 0, 0..1);
            }
        }
        Ok(rpass)
    }
}
