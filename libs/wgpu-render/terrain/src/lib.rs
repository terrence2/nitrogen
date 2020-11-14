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
use atmosphere::AtmosphereBuffer;
use command::Command;
use commandable::{command, commandable, Commandable};
use failure::Fallible;
use fullscreen::{FullscreenBuffer, FullscreenVertex};
use global_data::GlobalParametersBuffer;
use gpu::GPU;
use log::trace;
use shader_shared::Group;
use terrain_geo::{TerrainGeoBuffer, TerrainVertex};

enum DebugMode {
    None,
    Deferred,
    Depth,
    Color,
    Normal,
}

#[derive(Commandable)]
pub struct TerrainRenderPass {
    composite_pipeline: wgpu::RenderPipeline,
    dbg_deferred_pipeline: wgpu::RenderPipeline,
    dbg_depth_pipeline: wgpu::RenderPipeline,
    dbg_color_pipeline: wgpu::RenderPipeline,
    dbg_normal_pipeline: wgpu::RenderPipeline,
    wireframe_pipeline: wgpu::RenderPipeline,
    show_wireframe: bool,
    debug_mode: DebugMode,
}

// 1) Render tris to an offscreen buffer, collecting (a) grat, (b) norm, (c) depth per pixel
// 2) Clear diffuse color and normal accumulation buffers
// 3) For each layer, for each pixel of the offscreen buffer, accumulate the color and normal
// 4) For each pixel of the accumulator and depth, compute lighting, skybox, stars, etc.

#[commandable]
impl TerrainRenderPass {
    pub fn new(
        gpu: &mut GPU,
        globals_buffer: &GlobalParametersBuffer,
        atmosphere_buffer: &AtmosphereBuffer,
        terrain_geo_buffer: &TerrainGeoBuffer,
    ) -> Fallible<Self> {
        trace!("TerrainRenderPass::new");

        let fullscreen_shared_vert =
            gpu.create_shader_module(include_bytes!("../target/dbg-fullscreen-shared.vert.spirv"))?;
        let fullscreen_layout =
            gpu.device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("terrain-dbg-deferred-pipeline-layout"),
                    push_constant_ranges: &[],
                    bind_group_layouts: &[
                        globals_buffer.bind_group_layout(),
                        atmosphere_buffer.bind_group_layout(),
                        terrain_geo_buffer.composite_bind_group_layout(),
                    ],
                });

        let composite_pipeline = Self::make_fullscreen_pipeline(
            gpu.device(),
            &fullscreen_layout,
            &fullscreen_shared_vert,
            &gpu.create_shader_module(include_bytes!(
                "../target/terrain-composite-buffer.frag.spirv"
            ))?,
        );
        let dbg_deferred_pipeline = Self::make_fullscreen_pipeline(
            gpu.device(),
            &fullscreen_layout,
            &fullscreen_shared_vert,
            &gpu.create_shader_module(include_bytes!(
                "../target/terrain-deferred-buffer.frag.spirv"
            ))?,
        );
        let dbg_depth_pipeline = Self::make_fullscreen_pipeline(
            gpu.device(),
            &fullscreen_layout,
            &fullscreen_shared_vert,
            &gpu.create_shader_module(include_bytes!("../target/terrain-depth-buffer.frag.spirv"))?,
        );
        let dbg_color_pipeline = Self::make_fullscreen_pipeline(
            gpu.device(),
            &fullscreen_layout,
            &fullscreen_shared_vert,
            &gpu.create_shader_module(include_bytes!(
                "../target/terrain-color_acc-buffer.frag.spirv"
            ))?,
        );
        let dbg_normal_pipeline = Self::make_fullscreen_pipeline(
            gpu.device(),
            &fullscreen_layout,
            &fullscreen_shared_vert,
            &gpu.create_shader_module(include_bytes!(
                "../target/terrain-normal_acc-buffer.frag.spirv"
            ))?,
        );

        let wireframe_pipeline =
            gpu.device()
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("terrain-wireframe-pipeline"),
                    layout: Some(&gpu.device().create_pipeline_layout(
                        &wgpu::PipelineLayoutDescriptor {
                            label: Some("terrain-wireframe-pipeline-layout"),
                            push_constant_ranges: &[],
                            bind_group_layouts: &[globals_buffer.bind_group_layout()],
                        },
                    )),
                    vertex_stage: wgpu::ProgrammableStageDescriptor {
                        module: &gpu.create_shader_module(include_bytes!(
                            "../target/terrain-wireframe.vert.spirv"
                        ))?,
                        entry_point: "main",
                    },
                    fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
                        module: &gpu.create_shader_module(include_bytes!(
                            "../target/terrain-wireframe.frag.spirv"
                        ))?,
                        entry_point: "main",
                    }),
                    rasterization_state: Some(wgpu::RasterizationStateDescriptor {
                        front_face: wgpu::FrontFace::Cw,
                        cull_mode: wgpu::CullMode::None,
                        depth_bias: 0,
                        depth_bias_slope_scale: 0.0,
                        depth_bias_clamp: 0.0,
                        clamp_depth: false,
                    }),
                    primitive_topology: wgpu::PrimitiveTopology::LineList,
                    color_states: &[wgpu::ColorStateDescriptor {
                        format: GPU::SCREEN_FORMAT,
                        color_blend: wgpu::BlendDescriptor::REPLACE,
                        alpha_blend: wgpu::BlendDescriptor::REPLACE,
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
                        vertex_buffers: &[TerrainVertex::descriptor()],
                    },
                    sample_count: 1,
                    sample_mask: !0,
                    alpha_to_coverage_enabled: false,
                });

        Ok(Self {
            composite_pipeline,
            dbg_deferred_pipeline,
            dbg_depth_pipeline,
            dbg_color_pipeline,
            dbg_normal_pipeline,
            wireframe_pipeline,

            show_wireframe: false,
            debug_mode: DebugMode::None,
        })
    }

    pub fn make_fullscreen_pipeline(
        device: &wgpu::Device,
        layout: &wgpu::PipelineLayout,
        vert_shader: &wgpu::ShaderModule,
        frag_shader: &wgpu::ShaderModule,
    ) -> wgpu::RenderPipeline {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("terrain-dbg-deferred-pipeline"),
            layout: Some(layout),
            vertex_stage: wgpu::ProgrammableStageDescriptor {
                module: &vert_shader,
                entry_point: "main",
            },
            fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
                module: &frag_shader,
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
                color_blend: wgpu::BlendDescriptor::REPLACE,
                alpha_blend: wgpu::BlendDescriptor::REPLACE,
                write_mask: wgpu::ColorWrite::ALL,
            }],
            depth_stencil_state: Some(wgpu::DepthStencilStateDescriptor {
                format: GPU::DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Greater,
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
        })
    }

    #[command]
    pub fn toggle_wireframe(&mut self, _command: &Command) {
        self.show_wireframe = !self.show_wireframe;
    }

    #[command]
    pub fn toggle_debug_mode(&mut self, _command: &Command) {
        self.debug_mode = match self.debug_mode {
            DebugMode::None => DebugMode::Deferred,
            DebugMode::Deferred => DebugMode::Depth,
            DebugMode::Depth => DebugMode::Color,
            DebugMode::Color => DebugMode::Normal,
            DebugMode::Normal => DebugMode::None,
        };
    }

    pub fn draw<'a>(
        &'a self,
        mut rpass: wgpu::RenderPass<'a>,
        globals_buffer: &'a GlobalParametersBuffer,
        fullscreen_buffer: &'a FullscreenBuffer,
        atmosphere_buffer: &'a AtmosphereBuffer,
        terrain_geo_buffer: &'a TerrainGeoBuffer,
    ) -> Fallible<wgpu::RenderPass<'a>> {
        match self.debug_mode {
            DebugMode::None => rpass.set_pipeline(&self.composite_pipeline),
            DebugMode::Deferred => rpass.set_pipeline(&self.dbg_deferred_pipeline),
            DebugMode::Depth => rpass.set_pipeline(&self.dbg_depth_pipeline),
            DebugMode::Color => rpass.set_pipeline(&self.dbg_color_pipeline),
            DebugMode::Normal => rpass.set_pipeline(&self.dbg_normal_pipeline),
        }
        rpass.set_bind_group(Group::Globals.index(), &globals_buffer.bind_group(), &[]);
        rpass.set_bind_group(
            Group::Atmosphere.index(),
            &atmosphere_buffer.bind_group(),
            &[],
        );
        rpass.set_bind_group(2, terrain_geo_buffer.composite_bind_group(), &[]);
        // rpass.set_bind_group(Group::Stars.index(), &stars_buffer.bind_group(), &[]);
        rpass.set_vertex_buffer(0, fullscreen_buffer.vertex_buffer());
        rpass.draw(0..4, 0..1);

        if self.show_wireframe {
            rpass.set_pipeline(&self.wireframe_pipeline);
            rpass.set_bind_group(Group::Globals.index(), &globals_buffer.bind_group(), &[]);
            rpass.set_vertex_buffer(0, terrain_geo_buffer.vertex_buffer());
            for i in 0..terrain_geo_buffer.num_patches() {
                let winding = terrain_geo_buffer.patch_winding(i);
                let base_vertex = terrain_geo_buffer.patch_vertex_buffer_offset(i);
                rpass.set_index_buffer(terrain_geo_buffer.wireframe_index_buffer(winding));
                rpass.draw_indexed(
                    terrain_geo_buffer.wireframe_index_range(winding),
                    base_vertex,
                    0..1,
                );
            }
        }

        Ok(rpass)
    }
}

#[cfg(test)]
mod tests {
    use command::{Command, CommandHandler};
    use commandable::{command, commandable, Commandable};
    use failure::Fallible;

    #[derive(Commandable)]
    struct Buffer {
        value: u32,
    }

    #[commandable]
    impl Buffer {
        fn new() -> Self {
            Self { value: 0 }
        }

        #[command]
        fn make_good(&mut self, _command: &Command) {
            self.value = 42;
        }

        #[command]
        fn make_bad(&mut self, _command: &Command) {
            self.value = 13;
        }
    }

    #[test]
    fn test_create() -> Fallible<()> {
        let mut buf = Buffer::new();
        assert_eq!(buf.value, 0);
        buf.handle_command(&Command::parse("test.make_good")?);
        assert_eq!(buf.value, 42);
        buf.handle_command(&Command::parse("test.make_bad")?);
        assert_eq!(buf.value, 13);
        Ok(())
    }
}
