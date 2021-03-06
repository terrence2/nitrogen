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
use failure::Fallible;
use fullscreen::{FullscreenBuffer, FullscreenVertex};
use global_data::GlobalParametersBuffer;
use gpu::{texture_format_component_type, ResizeHint, GPU};
use log::trace;
use nitrous::{Interpreter, Value};
use nitrous_injector::{inject_nitrous_module, method, NitrousModule};
use parking_lot::RwLock;
use shader_shared::Group;
use stars::StarsBuffer;
use std::sync::Arc;
use terrain_geo::{TerrainGeoBuffer, TerrainVertex};

#[derive(Debug)]
enum DebugMode {
    None,
    Deferred,
    Depth,
    Color,
    NormalLocal,
    NormalGlobal,
}

#[derive(Debug, NitrousModule)]
pub struct WorldRenderPass {
    // Offscreen render targets
    deferred_texture: (wgpu::Texture, wgpu::TextureView),
    deferred_depth: (wgpu::Texture, wgpu::TextureView),
    deferred_sampler: wgpu::Sampler,
    deferred_bind_group_layout: wgpu::BindGroupLayout,
    deferred_bind_group: wgpu::BindGroup,

    // Debug and normal pipelines
    composite_pipeline: wgpu::RenderPipeline,
    dbg_deferred_pipeline: wgpu::RenderPipeline,
    dbg_depth_pipeline: wgpu::RenderPipeline,
    dbg_color_pipeline: wgpu::RenderPipeline,
    dbg_normal_local_pipeline: wgpu::RenderPipeline,
    dbg_normal_global_pipeline: wgpu::RenderPipeline,
    wireframe_pipeline: wgpu::RenderPipeline,

    // Render Mode
    show_wireframe: bool,
    debug_mode: DebugMode,
}

// 1) Render tris to an offscreen buffer, collecting (a) grat, (b) norm, (c) depth per pixel
// 2) Clear diffuse color and normal accumulation buffers
// 3) For each layer, for each pixel of the offscreen buffer, accumulate the color and normal
// 4) For each pixel of the accumulator and depth, compute lighting, skybox, stars, etc.

#[inject_nitrous_module]
impl WorldRenderPass {
    pub fn new(
        gpu: &mut GPU,
        interpreter: &mut Interpreter,
        globals_buffer: &GlobalParametersBuffer,
        atmosphere_buffer: &AtmosphereBuffer,
        stars_buffer: &StarsBuffer,
        terrain_geo_buffer: &TerrainGeoBuffer,
    ) -> Fallible<Arc<RwLock<Self>>> {
        trace!("WorldRenderPass::new");

        let deferred_bind_group_layout =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("world-deferred-bind-group-layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::SampledTexture {
                                dimension: wgpu::TextureViewDimension::D2,
                                component_type: texture_format_component_type(GPU::SCREEN_FORMAT),
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStage::FRAGMENT,
                            ty: wgpu::BindingType::Sampler { comparison: false },
                            count: None,
                        },
                    ],
                });

        let deferred_sampler = gpu.device().create_sampler(&wgpu::SamplerDescriptor {
            label: Some("world-deferred-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            lod_min_clamp: 0f32,
            lod_max_clamp: 0f32,
            compare: None,
            anisotropy_clamp: None,
        });

        let deferred_texture = Self::_make_deferred_texture_targets(gpu);
        let deferred_depth = Self::_make_deferred_depth_targets(gpu);
        let deferred_bind_group = Self::_make_deferred_bind_group(
            gpu,
            &deferred_bind_group_layout,
            &deferred_texture.1,
            &deferred_sampler,
        );

        let fullscreen_shared_vert =
            gpu.create_shader_module(include_bytes!("../target/dbg-fullscreen-shared.vert.spirv"))?;
        let fullscreen_layout =
            gpu.device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("world-dbg-deferred-pipeline-layout"),
                    push_constant_ranges: &[],
                    bind_group_layouts: &[
                        globals_buffer.bind_group_layout(),
                        atmosphere_buffer.bind_group_layout(),
                        stars_buffer.bind_group_layout(),
                        terrain_geo_buffer.composite_bind_group_layout(),
                    ],
                });

        let composite_pipeline = Self::make_fullscreen_pipeline(
            gpu.device(),
            &fullscreen_layout,
            &fullscreen_shared_vert,
            &gpu.create_shader_module(include_bytes!(
                "../target/world-composite-buffer.frag.spirv"
            ))?,
        );
        let dbg_deferred_pipeline = Self::make_fullscreen_pipeline(
            gpu.device(),
            &fullscreen_layout,
            &fullscreen_shared_vert,
            &gpu.create_shader_module(include_bytes!("../target/dbg-deferred-buffer.frag.spirv"))?,
        );
        let dbg_depth_pipeline = Self::make_fullscreen_pipeline(
            gpu.device(),
            &fullscreen_layout,
            &fullscreen_shared_vert,
            &gpu.create_shader_module(include_bytes!("../target/dbg-depth-buffer.frag.spirv"))?,
        );
        let dbg_color_pipeline = Self::make_fullscreen_pipeline(
            gpu.device(),
            &fullscreen_layout,
            &fullscreen_shared_vert,
            &gpu.create_shader_module(include_bytes!("../target/dbg-color_acc-buffer.frag.spirv"))?,
        );
        let dbg_normal_local_pipeline = Self::make_fullscreen_pipeline(
            gpu.device(),
            &fullscreen_layout,
            &fullscreen_shared_vert,
            &gpu.create_shader_module(include_bytes!(
                "../target/dbg-normal_acc-buffer-local.frag.spirv"
            ))?,
        );
        let dbg_normal_global_pipeline = Self::make_fullscreen_pipeline(
            gpu.device(),
            &fullscreen_layout,
            &fullscreen_shared_vert,
            &gpu.create_shader_module(include_bytes!(
                "../target/dbg-normal_acc-buffer-global.frag.spirv"
            ))?,
        );

        let wireframe_pipeline =
            gpu.device()
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("world-wireframe-pipeline"),
                    layout: Some(&gpu.device().create_pipeline_layout(
                        &wgpu::PipelineLayoutDescriptor {
                            label: Some("world-wireframe-pipeline-layout"),
                            push_constant_ranges: &[],
                            bind_group_layouts: &[globals_buffer.bind_group_layout()],
                        },
                    )),
                    vertex_stage: wgpu::ProgrammableStageDescriptor {
                        module: &gpu.create_shader_module(include_bytes!(
                            "../target/dbg-wireframe.vert.spirv"
                        ))?,
                        entry_point: "main",
                    },
                    fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
                        module: &gpu.create_shader_module(include_bytes!(
                            "../target/dbg-wireframe.frag.spirv"
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

        let world = Arc::new(RwLock::new(Self {
            deferred_texture,
            deferred_depth,
            deferred_sampler,
            deferred_bind_group_layout,
            deferred_bind_group,

            composite_pipeline,
            dbg_deferred_pipeline,
            dbg_depth_pipeline,
            dbg_color_pipeline,
            dbg_normal_local_pipeline,
            dbg_normal_global_pipeline,
            wireframe_pipeline,

            show_wireframe: false,
            debug_mode: DebugMode::None,
        }));

        gpu.add_resize_observer(world.clone());

        interpreter.put_global("world", Value::Module(world.clone()));

        Ok(world)
    }

    fn _make_deferred_texture_targets(gpu: &GPU) -> (wgpu::Texture, wgpu::TextureView) {
        let sz = gpu.physical_size();
        let target = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("world-offscreen-texture-target"),
            size: wgpu::Extent3d {
                width: sz.width as u32,
                height: sz.height as u32,
                depth: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: GPU::SCREEN_FORMAT,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT
                | wgpu::TextureUsage::COPY_SRC
                | wgpu::TextureUsage::SAMPLED,
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor {
            label: Some("world-offscreen-texture-target-view"),
            format: None,
            dimension: None,
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            level_count: None,
            base_array_layer: 0,
            array_layer_count: None,
        });
        (target, view)
    }

    fn _make_deferred_depth_targets(gpu: &GPU) -> (wgpu::Texture, wgpu::TextureView) {
        let sz = gpu.physical_size();
        let depth_texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("world-offscreen-depth-texture"),
            size: wgpu::Extent3d {
                width: sz.width as u32,
                height: sz.height as u32,
                depth: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: GPU::DEPTH_FORMAT,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT
                | wgpu::TextureUsage::COPY_SRC
                | wgpu::TextureUsage::SAMPLED,
        });
        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("world-offscreen-depth-texture-view"),
            format: None,
            dimension: None,
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            level_count: None,
            base_array_layer: 0,
            array_layer_count: None,
        });
        (depth_texture, depth_view)
    }

    fn _make_deferred_bind_group(
        gpu: &GPU,
        deferred_bind_group_layout: &wgpu::BindGroupLayout,
        deferred_texture_view: &wgpu::TextureView,
        deferred_sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("world-deferred-bind-group"),
            layout: deferred_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(deferred_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(deferred_sampler),
                },
            ],
        })
    }

    pub fn make_fullscreen_pipeline(
        device: &wgpu::Device,
        layout: &wgpu::PipelineLayout,
        vert_shader: &wgpu::ShaderModule,
        frag_shader: &wgpu::ShaderModule,
    ) -> wgpu::RenderPipeline {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("world-dbg-deferred-pipeline"),
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

    pub fn add_default_bindings(&mut self, interpreter: &mut Interpreter) -> Fallible<()> {
        interpreter.interpret_once(
            r#"
                let bindings := mapper.create_bindings("world");
                bindings.bind("w", "world.toggle_wireframe_mode(pressed)");
                bindings.bind("r", "world.change_debug_mode(pressed)");
            "#,
        )?;
        Ok(())
    }

    #[method]
    pub fn toggle_wireframe_mode(&mut self, pressed: bool) {
        if pressed {
            self.show_wireframe = !self.show_wireframe;
        }
    }

    #[method]
    pub fn change_debug_mode(&mut self, pressed: bool) {
        if pressed {
            self.debug_mode = match self.debug_mode {
                DebugMode::None => DebugMode::Deferred,
                DebugMode::Deferred => DebugMode::Depth,
                DebugMode::Depth => DebugMode::Color,
                DebugMode::Color => DebugMode::NormalLocal,
                DebugMode::NormalLocal => DebugMode::NormalGlobal,
                DebugMode::NormalGlobal => DebugMode::None,
            };
        }
    }

    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.deferred_bind_group_layout
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.deferred_bind_group
    }

    pub fn offscreen_target(
        &self,
    ) -> (
        [wgpu::RenderPassColorAttachmentDescriptor; 1],
        Option<wgpu::RenderPassDepthStencilAttachmentDescriptor>,
    ) {
        (
            [wgpu::RenderPassColorAttachmentDescriptor {
                attachment: &self.deferred_texture.1,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::RED),
                    store: true,
                },
            }],
            Some(wgpu::RenderPassDepthStencilAttachmentDescriptor {
                attachment: &self.deferred_depth.1,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(-1f32),
                    store: true,
                }),
                stencil_ops: None,
            }),
        )
    }

    pub fn render_world<'a>(
        &'a self,
        mut rpass: wgpu::RenderPass<'a>,
        globals_buffer: &'a GlobalParametersBuffer,
        fullscreen_buffer: &'a FullscreenBuffer,
        atmosphere_buffer: &'a AtmosphereBuffer,
        stars_buffer: &'a StarsBuffer,
        terrain_geo_buffer: &'a TerrainGeoBuffer,
    ) -> Fallible<wgpu::RenderPass<'a>> {
        match self.debug_mode {
            DebugMode::None => rpass.set_pipeline(&self.composite_pipeline),
            DebugMode::Deferred => rpass.set_pipeline(&self.dbg_deferred_pipeline),
            DebugMode::Depth => rpass.set_pipeline(&self.dbg_depth_pipeline),
            DebugMode::Color => rpass.set_pipeline(&self.dbg_color_pipeline),
            DebugMode::NormalLocal => rpass.set_pipeline(&self.dbg_normal_local_pipeline),
            DebugMode::NormalGlobal => rpass.set_pipeline(&self.dbg_normal_global_pipeline),
        }
        rpass.set_bind_group(Group::Globals.index(), &globals_buffer.bind_group(), &[]);
        rpass.set_bind_group(
            Group::Atmosphere.index(),
            &atmosphere_buffer.bind_group(),
            &[],
        );
        rpass.set_bind_group(
            Group::TerrainComposite.index(),
            terrain_geo_buffer.composite_bind_group(),
            &[],
        );
        rpass.set_bind_group(Group::Stars.index(), &stars_buffer.bind_group(), &[]);
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

impl ResizeHint for WorldRenderPass {
    fn note_resize(&mut self, gpu: &GPU) -> Fallible<()> {
        self.deferred_texture = Self::_make_deferred_texture_targets(gpu);
        self.deferred_depth = Self::_make_deferred_depth_targets(gpu);
        self.deferred_bind_group = Self::_make_deferred_bind_group(
            gpu,
            &self.deferred_bind_group_layout,
            &self.deferred_texture.1,
            &self.deferred_sampler,
        );
        Ok(())
    }
}
