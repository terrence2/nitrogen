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
use global_data::GlobalParametersBuffer;
use gpu::{GpuResource, RenderStep, GPU};
use log::trace;
use shader_shared::Group;
use std::sync::{Arc, RwLock};
use text_layout::{LayoutVertex, TextLayoutBuffer};

pub struct ScreenTextRenderStep {
    global_data: Arc<RwLock<GlobalParametersBuffer>>,
    layout_buffer: Arc<RwLock<TextLayoutBuffer>>,
    pipeline: wgpu::RenderPipeline,
}

impl ScreenTextRenderStep {
    pub fn new(
        gpu: &mut GPU,
        global_data: Arc<RwLock<GlobalParametersBuffer>>,
        layout_buffer: Arc<RwLock<TextLayoutBuffer>>,
    ) -> Fallible<Arc<RwLock<Self>>> {
        trace!("ScreenTextRenderPass::new");

        let vert_shader =
            gpu.create_shader_module(include_bytes!("../target/screen_text.vert.spirv"))?;
        let frag_shader =
            gpu.create_shader_module(include_bytes!("../target/screen_text.frag.spirv"))?;

        let pipeline_layout =
            gpu.device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    bind_group_layouts: &[
                        global_data.read().unwrap().bind_group_layout(),
                        layout_buffer.read().unwrap().glyph_bind_group_layout(),
                        layout_buffer.read().unwrap().layout_bind_group_layout(),
                    ],
                });

        let pipeline = gpu
            .device()
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                layout: &pipeline_layout,
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
                }),
                primitive_topology: wgpu::PrimitiveTopology::TriangleList,
                color_states: &[wgpu::ColorStateDescriptor {
                    format: GPU::texture_format(),
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
                    depth_compare: wgpu::CompareFunction::Less,
                    stencil_front: wgpu::StencilStateFaceDescriptor::IGNORE,
                    stencil_back: wgpu::StencilStateFaceDescriptor::IGNORE,
                    stencil_read_mask: 0,
                    stencil_write_mask: 0,
                }),
                vertex_state: wgpu::VertexStateDescriptor {
                    index_format: wgpu::IndexFormat::Uint32,
                    vertex_buffers: &[LayoutVertex::descriptor()],
                },
                sample_count: 1,
                sample_mask: !0,
                alpha_to_coverage_enabled: false,
            });

        Ok(Arc::new(RwLock::new(Self {
            global_data,
            layout_buffer,
            pipeline,
        })))
    }

    pub fn draw<'a>(
        &'a self,
        mut rpass: wgpu::RenderPass<'a>,
        global_data: &'a GlobalParametersBuffer,
        layout_buffer: &'a TextLayoutBuffer,
    ) -> wgpu::RenderPass<'a> {
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(Group::Globals.index(), &global_data.bind_group(), &[]);
        for (font_name, layout_handles) in layout_buffer.layouts_by_font() {
            let glyph_cache = layout_buffer.glyph_cache(font_name);
            rpass.set_bind_group(Group::GlyphCache.index(), &glyph_cache.bind_group(), &[]);
            for &layout_handle in layout_handles {
                let layout = layout_buffer.layout(layout_handle);
                rpass.set_bind_group(Group::TextLayout.index(), &layout.bind_group(), &[]);

                rpass.set_index_buffer(&layout.index_buffer(), 0, 0);
                rpass.set_vertex_buffer(0, &layout.vertex_buffer(), 0, 0);
                rpass.draw_indexed(layout.index_range(), 0, 0..1);
            }
        }
        rpass
    }
}

use std::{any::Any, collections::HashMap, sync::RwLockReadGuard};

impl RenderStep for ScreenTextRenderStep {
    fn name(&self) -> &str {
        "screen-text-render-step"
    }

    fn render<'a>(
        &'a self,
        mut rpass: wgpu::RenderPass<'a>,
        resources: &'a HashMap<&str, RwLockReadGuard<'a, dyn RenderStep>>,
    ) -> wgpu::RenderPass<'a> {
        println!("RENDER");
        // (&resources["globals-buffer"] as &(dyn Any + 'static))
        //     .downcast_ref::<&GlobalParametersBuffer>()
        //     .expect("globals")
        //     .bind_group();
        let a = (&resources["globals-buffer"])
            .downcast_ref::<&GlobalParametersBuffer>()
            .expect("globals")
            .bind_group();
        // let a = &resources["globals-buffer"];
        // let b = **a;
        let layout_buffer = &resources[self.layout_buffer.read().unwrap().name()];

        rpass.set_pipeline(&self.pipeline);
        //rpass.set_bind_group(Group::Globals.index(), &global_data.bind_group(), &[]);
        /*
        for (font_name, layout_handles) in layout_buffer.layouts_by_font() {
            let glyph_cache = layout_buffer.glyph_cache(font_name);
            rpass.set_bind_group(Group::GlyphCache.index(), &glyph_cache.bind_group(), &[]);
            for &layout_handle in layout_handles {
                let layout = layout_buffer.layout(layout_handle);
                rpass.set_bind_group(Group::TextLayout.index(), &layout.bind_group(), &[]);

                rpass.set_index_buffer(&layout.index_buffer(), 0, 0);
                rpass.set_vertex_buffer(0, &layout.vertex_buffer(), 0, 0);
                rpass.draw_indexed(layout.index_range(), 0, 0..1);
            }
        }
         */
        rpass
    }
}
