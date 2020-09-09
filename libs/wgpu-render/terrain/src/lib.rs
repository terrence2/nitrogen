// This file is part of OpenFA.
//
// OpenFA is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// OpenFA is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with OpenFA.  If not, see <http://www.gnu.org/licenses/>.
use atmosphere::AtmosphereBuffer;
use command::Command;
use commandable::{command, commandable, Commandable};
use failure::Fallible;
use global_data::GlobalParametersBuffer;
use gpu::GPU;
use log::trace;
use shader_shared::Group;
use terrain_geo::{TerrainGeoBuffer, TerrainVertex};

#[derive(Commandable)]
pub struct TerrainRenderPass {
    patch_pipeline: wgpu::RenderPipeline,
    wireframe_pipeline: wgpu::RenderPipeline,
    show_wireframe: bool,
}

#[commandable]
impl TerrainRenderPass {
    pub fn new(
        gpu: &mut GPU,
        globals_buffer: &GlobalParametersBuffer,
        atmosphere_buffer: &AtmosphereBuffer,
        terrain_geo_buffer: &TerrainGeoBuffer,
    ) -> Fallible<Self> {
        trace!("TerrainRenderPass::new");

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
                        format: GPU::texture_format(),
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

        let patch_pipeline = gpu
            .device()
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("terrain-patch-pipeline"),
                layout: Some(&gpu.device().create_pipeline_layout(
                    &wgpu::PipelineLayoutDescriptor {
                        label: Some("terrain-patch-pipeline-layout"),
                        push_constant_ranges: &[],
                        bind_group_layouts: &[
                            globals_buffer.bind_group_layout(),
                            atmosphere_buffer.bind_group_layout(),
                            terrain_geo_buffer.bind_group_layout(),
                        ],
                    },
                )),
                vertex_stage: wgpu::ProgrammableStageDescriptor {
                    module: &gpu
                        .create_shader_module(include_bytes!("../target/terrain.vert.spirv"))?,
                    entry_point: "main",
                },
                fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
                    module: &gpu
                        .create_shader_module(include_bytes!("../target/terrain.frag.spirv"))?,
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
                    format: GPU::texture_format(),
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
                    vertex_buffers: &[TerrainVertex::descriptor()],
                },
                sample_count: 1,
                sample_mask: !0,
                alpha_to_coverage_enabled: false,
            });

        Ok(Self {
            patch_pipeline,
            wireframe_pipeline,

            show_wireframe: true,
        })
    }

    #[command]
    pub fn toggle_wireframe(&mut self, _command: &Command) {
        self.show_wireframe = !self.show_wireframe;
    }

    pub fn draw<'a>(
        &'a self,
        mut rpass: wgpu::RenderPass<'a>,
        globals_buffer: &'a GlobalParametersBuffer,
        atmosphere_buffer: &'a AtmosphereBuffer,
        terrain_geo_buffer: &'a TerrainGeoBuffer,
    ) -> wgpu::RenderPass<'a> {
        rpass.set_pipeline(&self.patch_pipeline);
        rpass.set_bind_group(Group::Globals.index(), &globals_buffer.bind_group(), &[]);
        rpass.set_bind_group(
            Group::Atmosphere.index(),
            &atmosphere_buffer.bind_group(),
            &[],
        );
        rpass.set_bind_group(
            Group::Terrain.index(),
            &terrain_geo_buffer.bind_group(),
            &[],
        );
        rpass.set_vertex_buffer(0, terrain_geo_buffer.vertex_buffer());
        for i in 0..terrain_geo_buffer.num_patches() {
            let winding = terrain_geo_buffer.patch_winding(i);
            let base_vertex = terrain_geo_buffer.patch_vertex_buffer_offset(i);
            rpass.set_index_buffer(terrain_geo_buffer.tristrip_index_buffer(winding));
            rpass.draw_indexed(
                terrain_geo_buffer.tristrip_index_range(winding),
                base_vertex,
                0..1,
            );
        }

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

        rpass
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
