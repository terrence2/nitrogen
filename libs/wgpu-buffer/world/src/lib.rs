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
use atmosphere::AtmosphereBuffer;
use bevy_ecs::prelude::*;
use fullscreen::{FullscreenBuffer, FullscreenVertex};
use global_data::GlobalParametersBuffer;
use gpu::{DisplayConfig, Gpu};
use log::trace;
use nitrous::{inject_nitrous_resource, method, NitrousResource};
use runtime::{Extension, FrameStage, Runtime};
use shader_shared::Group;
use stars::StarsBuffer;
use terrain::{TerrainBuffer, TerrainVertex};

#[derive(Debug)]
enum DebugMode {
    None,
    Deferred,
    Depth,
    Color,
    NormalLocal,
    NormalGlobal,
}

impl DebugMode {
    pub fn from_str(s: &str) -> Self {
        match s {
            "deferred" => Self::Deferred,
            "depth" => Self::Depth,
            "color" => Self::Color,
            "normal_local" => Self::NormalLocal,
            "normal_global" | "normal" => Self::NormalGlobal,
            _ => Self::None,
        }
    }
}

#[derive(Debug, NitrousResource)]
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

impl Extension for WorldRenderPass {
    fn init(runtime: &mut Runtime) -> Result<()> {
        let world = WorldRenderPass::new(
            &runtime.resource::<TerrainBuffer>(),
            runtime.resource::<AtmosphereBuffer>(),
            runtime.resource::<StarsBuffer>(),
            runtime.resource::<GlobalParametersBuffer>(),
            runtime.resource::<Gpu>(),
        )?;
        runtime.insert_named_resource("world", world);
        runtime
            .frame_stage_mut(FrameStage::HandleDisplayChange)
            .add_system(Self::sys_handle_display_config_change);
        runtime.frame_stage_mut(FrameStage::Render).add_system(
            Self::sys_render_world
                .before("CompositeRenderPass")
                .label("WorldRenderPass"),
        );

        // TODO: figure out debug bindings
        runtime.run_string(
            r#"
                bindings.bind("w", "world.toggle_wireframe_mode(pressed)");
                bindings.bind("r", "world.change_debug_mode(pressed)");
            "#,
        )?;

        Ok(())
    }
}

#[inject_nitrous_resource]
impl WorldRenderPass {
    pub fn new(
        terrain_buffer: &TerrainBuffer,
        atmosphere_buffer: &AtmosphereBuffer,
        stars_buffer: &StarsBuffer,
        globals_buffer: &GlobalParametersBuffer,
        gpu: &Gpu,
    ) -> Result<Self> {
        trace!("WorldRenderPass::new");

        // Render target reader for compositing.
        let deferred_bind_group_layout =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("world-deferred-bind-group-layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                view_dimension: wgpu::TextureViewDimension::D2,
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
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
            border_color: None,
        });

        let deferred_texture = Self::_make_deferred_texture_targets(gpu);
        let deferred_depth = Self::_make_deferred_depth_targets(gpu);
        let deferred_bind_group = Self::_make_deferred_bind_group(
            gpu,
            &deferred_bind_group_layout,
            &deferred_texture.1,
            &deferred_sampler,
        );

        let fullscreen_shared_vert = gpu.create_shader_module(
            "fullscreen-shared.vert",
            include_bytes!("../target/fullscreen-shared.vert.spirv"),
        )?;
        let fullscreen_layout =
            gpu.device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("world-deferred-pipeline-layout"),
                    push_constant_ranges: &[],
                    bind_group_layouts: &[
                        globals_buffer.bind_group_layout(),
                        atmosphere_buffer.bind_group_layout(),
                        stars_buffer.bind_group_layout(),
                        terrain_buffer.composite_bind_group_layout(),
                    ],
                });

        let composite_pipeline = Self::make_fullscreen_pipeline(
            gpu.device(),
            &fullscreen_layout,
            &fullscreen_shared_vert,
            &gpu.create_shader_module(
                "world-composite-buffer.frag",
                include_bytes!("../target/world-composite-buffer.frag.spirv"),
            )?,
        );
        let dbg_deferred_pipeline = Self::make_fullscreen_pipeline(
            gpu.device(),
            &fullscreen_layout,
            &fullscreen_shared_vert,
            &gpu.create_shader_module(
                "dbg-deferred-buffer.frag",
                include_bytes!("../target/dbg-deferred-buffer.frag.spirv"),
            )?,
        );
        let dbg_depth_pipeline = Self::make_fullscreen_pipeline(
            gpu.device(),
            &fullscreen_layout,
            &fullscreen_shared_vert,
            &gpu.create_shader_module(
                "dbg-depth-buffer.frag",
                include_bytes!("../target/dbg-depth-buffer.frag.spirv"),
            )?,
        );
        let dbg_color_pipeline = Self::make_fullscreen_pipeline(
            gpu.device(),
            &fullscreen_layout,
            &fullscreen_shared_vert,
            &gpu.create_shader_module(
                "dbg-color_acc-buffer.frag",
                include_bytes!("../target/dbg-color_acc-buffer.frag.spirv"),
            )?,
        );
        let dbg_normal_local_pipeline = Self::make_fullscreen_pipeline(
            gpu.device(),
            &fullscreen_layout,
            &fullscreen_shared_vert,
            &gpu.create_shader_module(
                "dbg-normal_acc-buffer-local.frag",
                include_bytes!("../target/dbg-normal_acc-buffer-local.frag.spirv"),
            )?,
        );
        let dbg_normal_global_pipeline = Self::make_fullscreen_pipeline(
            gpu.device(),
            &fullscreen_layout,
            &fullscreen_shared_vert,
            &gpu.create_shader_module(
                "dbg-normal_acc-buffer-global.frag",
                include_bytes!("../target/dbg-normal_acc-buffer-global.frag.spirv"),
            )?,
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
                    vertex: wgpu::VertexState {
                        module: &gpu.create_shader_module(
                            "dbg-wireframe.vert",
                            include_bytes!("../target/dbg-wireframe.vert.spirv"),
                        )?,
                        entry_point: "main",
                        buffers: &[TerrainVertex::descriptor()],
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &gpu.create_shader_module(
                            "dbg-wireframe.frag",
                            include_bytes!("../target/dbg-wireframe.frag.spirv"),
                        )?,
                        entry_point: "main",
                        targets: &[wgpu::ColorTargetState {
                            format: Gpu::SCREEN_FORMAT,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        }],
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::LineList,
                        strip_index_format: None,
                        front_face: wgpu::FrontFace::Cw,
                        cull_mode: None,
                        unclipped_depth: false,
                        polygon_mode: wgpu::PolygonMode::Line,
                        conservative: false,
                    },
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: Gpu::DEPTH_FORMAT,
                        depth_write_enabled: false,
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
                    }),
                    multisample: wgpu::MultisampleState {
                        count: 1,
                        mask: !0,
                        alpha_to_coverage_enabled: false,
                    },
                    multiview: None,
                });

        Ok(Self {
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
        })
    }

    fn _make_deferred_texture_targets(gpu: &Gpu) -> (wgpu::Texture, wgpu::TextureView) {
        let size = gpu.render_extent();
        let target = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("world-offscreen-texture-target"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Gpu::SCREEN_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor {
            label: Some("world-offscreen-texture-target-view"),
            format: None,
            dimension: None,
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: None,
            base_array_layer: 0,
            array_layer_count: None,
        });
        (target, view)
    }

    fn _make_deferred_depth_targets(gpu: &Gpu) -> (wgpu::Texture, wgpu::TextureView) {
        let size = gpu.render_extent();
        let depth_texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("world-offscreen-depth-texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Gpu::DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
        });
        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("world-offscreen-depth-texture-view"),
            format: None,
            dimension: None,
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: None,
            base_array_layer: 0,
            array_layer_count: None,
        });
        (depth_texture, depth_view)
    }

    fn _make_deferred_bind_group(
        gpu: &Gpu,
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
            vertex: wgpu::VertexState {
                module: vert_shader,
                entry_point: "main",
                buffers: &[FullscreenVertex::descriptor()],
            },
            fragment: Some(wgpu::FragmentState {
                module: frag_shader,
                entry_point: "main",
                targets: &[wgpu::ColorTargetState {
                    format: Gpu::SCREEN_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                }],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: Some(wgpu::IndexFormat::Uint32),
                front_face: wgpu::FrontFace::Cw,
                cull_mode: Some(wgpu::Face::Back),
                unclipped_depth: true,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: Gpu::DEPTH_FORMAT,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Greater,
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
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        })
    }

    pub fn sys_handle_display_config_change(
        updated_config: Res<Option<DisplayConfig>>,
        gpu: Res<Gpu>,
        mut world: ResMut<WorldRenderPass>,
    ) {
        if updated_config.is_some() {
            world
                .handle_render_extent_changed(&gpu)
                .expect("World::handle_render_extent_changed")
        }
    }

    fn handle_render_extent_changed(&mut self, gpu: &Gpu) -> Result<()> {
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
            println!("Debug Mode is now: {:?}", self.debug_mode);
        }
    }

    #[method]
    pub fn set_debug_mode(&mut self, value: &str) {
        self.debug_mode = DebugMode::from_str(value);
        println!("Debug Mode is now: {:?}", self.debug_mode);
    }

    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.deferred_bind_group_layout
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.deferred_bind_group
    }

    pub fn offscreen_target_cleared(
        &self,
    ) -> (
        [wgpu::RenderPassColorAttachment; 1],
        Option<wgpu::RenderPassDepthStencilAttachment>,
    ) {
        self.offscreen_target_maybe_clear(
            wgpu::LoadOp::Clear(wgpu::Color::RED),
            wgpu::LoadOp::Clear(-1f32),
        )
    }

    pub fn offscreen_target_preserved(
        &self,
    ) -> (
        [wgpu::RenderPassColorAttachment; 1],
        Option<wgpu::RenderPassDepthStencilAttachment>,
    ) {
        self.offscreen_target_maybe_clear(wgpu::LoadOp::Load, wgpu::LoadOp::Load)
    }

    fn offscreen_target_maybe_clear(
        &self,
        color_load: wgpu::LoadOp<wgpu::Color>,
        depth_load: wgpu::LoadOp<f32>,
    ) -> (
        [wgpu::RenderPassColorAttachment; 1],
        Option<wgpu::RenderPassDepthStencilAttachment>,
    ) {
        (
            [wgpu::RenderPassColorAttachment {
                view: &self.deferred_texture.1,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: color_load,
                    store: true,
                },
            }],
            Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.deferred_depth.1,
                depth_ops: Some(wgpu::Operations {
                    load: depth_load,
                    store: true,
                }),
                stencil_ops: None,
            }),
        )
    }

    fn sys_render_world(
        world: Res<WorldRenderPass>,
        globals: Res<GlobalParametersBuffer>,
        fullscreen: Res<FullscreenBuffer>,
        atmosphere: Res<AtmosphereBuffer>,
        stars: Res<StarsBuffer>,
        terrain: Res<TerrainBuffer>,
        maybe_encoder: ResMut<Option<wgpu::CommandEncoder>>,
    ) {
        if let Some(encoder) = maybe_encoder.into_inner() {
            let (color_attachments, depth_stencil_attachment) = world.offscreen_target_cleared();
            let render_pass_desc_ref = wgpu::RenderPassDescriptor {
                label: Some("offscreen-draw-world"),
                color_attachments: &color_attachments,
                depth_stencil_attachment,
            };
            let rpass = encoder.begin_render_pass(&render_pass_desc_ref);
            let _rpass =
                world.render_world(rpass, &globals, &fullscreen, &atmosphere, &stars, &terrain);
        }
    }

    pub fn render_world<'a>(
        &'a self,
        mut rpass: wgpu::RenderPass<'a>,
        globals_buffer: &'a GlobalParametersBuffer,
        fullscreen_buffer: &'a FullscreenBuffer,
        atmosphere_buffer: &'a AtmosphereBuffer,
        stars_buffer: &'a StarsBuffer,
        terrain_buffer: &'a TerrainBuffer,
    ) -> wgpu::RenderPass<'a> {
        match self.debug_mode {
            DebugMode::None => rpass.set_pipeline(&self.composite_pipeline),
            DebugMode::Deferred => rpass.set_pipeline(&self.dbg_deferred_pipeline),
            DebugMode::Depth => rpass.set_pipeline(&self.dbg_depth_pipeline),
            DebugMode::Color => rpass.set_pipeline(&self.dbg_color_pipeline),
            DebugMode::NormalLocal => rpass.set_pipeline(&self.dbg_normal_local_pipeline),
            DebugMode::NormalGlobal => rpass.set_pipeline(&self.dbg_normal_global_pipeline),
        }
        rpass.set_bind_group(Group::Globals.index(), globals_buffer.bind_group(), &[]);
        rpass.set_bind_group(
            Group::Atmosphere.index(),
            atmosphere_buffer.bind_group(),
            &[],
        );
        rpass.set_bind_group(
            Group::TerrainComposite.index(),
            terrain_buffer.composite_bind_group(),
            &[],
        );
        rpass.set_bind_group(Group::Stars.index(), stars_buffer.bind_group(), &[]);
        rpass.set_vertex_buffer(0, fullscreen_buffer.vertex_buffer());
        rpass.draw(0..4, 0..1);

        if self.show_wireframe {
            rpass.set_pipeline(&self.wireframe_pipeline);
            rpass.set_bind_group(Group::Globals.index(), globals_buffer.bind_group(), &[]);
            rpass.set_vertex_buffer(0, terrain_buffer.vertex_buffer());
            for i in 0..terrain_buffer.num_patches() {
                let winding = terrain_buffer.patch_winding(i);
                let base_vertex = terrain_buffer.patch_vertex_buffer_offset(i);
                rpass.set_index_buffer(
                    terrain_buffer.wireframe_index_buffer(winding),
                    wgpu::IndexFormat::Uint32,
                );
                rpass.draw_indexed(
                    terrain_buffer.wireframe_index_range(winding),
                    base_vertex,
                    0..1,
                );
            }
        }

        rpass
    }
}
