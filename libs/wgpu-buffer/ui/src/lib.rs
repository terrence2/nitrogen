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
use bevy_ecs::prelude::*;
use global_data::GlobalParametersBuffer;
use gpu::{DisplayConfig, Gpu};
use input::InputFocus;
use log::trace;
use runtime::{Extension, FrameStage, Runtime};
use shader_shared::Group;
use std::marker::PhantomData;
use widget::{WidgetBuffer, WidgetVertex};
use world::WorldRenderPass;

#[derive(Debug)]
pub struct UiRenderPass<T>
where
    T: InputFocus,
{
    // Offscreen render targets
    deferred_texture: (wgpu::Texture, wgpu::TextureView),
    deferred_depth: (wgpu::Texture, wgpu::TextureView),
    deferred_sampler: wgpu::Sampler,
    deferred_bind_group_layout: wgpu::BindGroupLayout,
    deferred_bind_group: wgpu::BindGroup,

    background_pipeline: wgpu::RenderPipeline,
    // image_pipeline: wgpu::RenderPipeline,
    text_pipeline: wgpu::RenderPipeline,

    widget_type_holder: PhantomData<T>,
}

impl<T> Extension for UiRenderPass<T>
where
    T: InputFocus,
{
    fn init(runtime: &mut Runtime) -> Result<()> {
        let ui = UiRenderPass::new(
            runtime.resource::<WidgetBuffer<T>>(),
            runtime.resource::<WorldRenderPass>(),
            runtime.resource::<GlobalParametersBuffer>(),
            runtime.resource::<Gpu>(),
        )?;
        runtime.insert_resource(ui);
        runtime
            .frame_stage_mut(FrameStage::HandleDisplayChange)
            .add_system(Self::sys_handle_display_config_change);
        runtime.frame_stage_mut(FrameStage::Render).add_system(
            Self::sys_render_ui
                .after("maintain_font_atlas")
                .label("before_composite"),
        );
        Ok(())
    }
}

impl<T> UiRenderPass<T>
where
    T: InputFocus,
{
    pub fn new(
        widget_buffer: &WidgetBuffer<T>,
        world_render_pass: &WorldRenderPass,
        global_data: &GlobalParametersBuffer,
        gpu: &Gpu,
    ) -> Result<Self> {
        trace!("UiRenderPass::new");

        // Binding layout for composite to read our offscreen render.
        let deferred_bind_group_layout =
            gpu.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("ui-deferred-bind-group-layout"),
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
            label: Some("ui-deferred-sampler"),
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

        // Layout shared by all three render passes.
        let pipeline_layout =
            gpu.device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("screen-text-pipeline-layout"),
                    push_constant_ranges: &[],
                    bind_group_layouts: &[
                        global_data.bind_group_layout(),
                        widget_buffer.bind_group_layout(),
                        world_render_pass.bind_group_layout(),
                    ],
                });

        let background_pipeline =
            gpu.device()
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("ui-background-pipeline"),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &gpu.create_shader_module(
                            "ui-background.vert",
                            include_bytes!("../target/ui-background.vert.spirv"),
                        )?,
                        entry_point: "main",
                        buffers: &[WidgetVertex::descriptor()],
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &gpu.create_shader_module(
                            "ui-background.frag",
                            include_bytes!("../target/ui-background.frag.spirv"),
                        )?,
                        entry_point: "main",
                        targets: &[wgpu::ColorTargetState {
                            format: Gpu::SCREEN_FORMAT,
                            blend: Some(wgpu::BlendState {
                                color: wgpu::BlendComponent {
                                    src_factor: wgpu::BlendFactor::SrcAlpha,
                                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                                    operation: wgpu::BlendOperation::Add,
                                },
                                alpha: wgpu::BlendComponent::REPLACE,
                            }),
                            write_mask: wgpu::ColorWrites::ALL,
                        }],
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        strip_index_format: None,
                        front_face: wgpu::FrontFace::Ccw,
                        cull_mode: Some(wgpu::Face::Back),
                        unclipped_depth: false,
                        polygon_mode: wgpu::PolygonMode::Fill,
                        conservative: false,
                    },
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: Gpu::DEPTH_FORMAT,
                        depth_write_enabled: true,
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
                });

        let text_pipeline = gpu
            .device()
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("ui-text-pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &gpu.create_shader_module(
                        "ui-text.vert",
                        include_bytes!("../target/ui-text.vert.spirv"),
                    )?,
                    entry_point: "main",
                    buffers: &[WidgetVertex::descriptor()],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &gpu.create_shader_module(
                        "ui-text.frag",
                        include_bytes!("../target/ui-text.frag.spirv"),
                    )?,
                    entry_point: "main",
                    targets: &[wgpu::ColorTargetState {
                        format: Gpu::SCREEN_FORMAT,
                        blend: Some(wgpu::BlendState {
                            color: wgpu::BlendComponent {
                                src_factor: wgpu::BlendFactor::SrcAlpha,
                                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                                operation: wgpu::BlendOperation::Add,
                            },
                            alpha: wgpu::BlendComponent {
                                src_factor: wgpu::BlendFactor::SrcAlpha,
                                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                                operation: wgpu::BlendOperation::Max,
                            },
                        }),
                        write_mask: wgpu::ColorWrites::ALL,
                    }],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: Some(wgpu::Face::Back),
                    unclipped_depth: false,
                    polygon_mode: wgpu::PolygonMode::Fill,
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

            background_pipeline,
            text_pipeline,

            widget_type_holder: PhantomData::default(),
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

    pub fn sys_handle_display_config_change(
        updated_config: Res<Option<DisplayConfig>>,
        gpu: Res<Gpu>,
        mut ui: ResMut<UiRenderPass<T>>,
    ) {
        if updated_config.is_some() {
            ui.handle_render_extent_changed(&gpu)
                .expect("UI::handle_render_extent_changed")
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

    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.deferred_bind_group_layout
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.deferred_bind_group
    }

    pub fn offscreen_target(
        &self,
    ) -> (
        [wgpu::RenderPassColorAttachment; 1],
        Option<wgpu::RenderPassDepthStencilAttachment>,
    ) {
        (
            [wgpu::RenderPassColorAttachment {
                view: &self.deferred_texture.1,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: true,
                },
            }],
            Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.deferred_depth.1,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0f32),
                    store: true,
                }),
                stencil_ops: None,
            }),
        )
    }

    fn sys_render_ui(
        ui: Res<UiRenderPass<T>>,
        globals: Res<GlobalParametersBuffer>,
        widgets: Res<WidgetBuffer<T>>,
        world: Res<WorldRenderPass>,
        mut maybe_encoder: ResMut<Option<wgpu::CommandEncoder>>,
    ) {
        if let Some(encoder) = maybe_encoder.into_inner() {
            let (color_attachments, depth_stencil_attachment) = ui.offscreen_target();
            let render_pass_desc_ref = wgpu::RenderPassDescriptor {
                label: Some(concat!("non-screen-render-pass-ui-draw-offscreen",)),
                color_attachments: &color_attachments,
                depth_stencil_attachment,
            };
            let rpass = encoder.begin_render_pass(&render_pass_desc_ref);
            let _rpass = ui.render_ui(rpass, &globals, &widgets, &world);
        }
    }

    pub fn render_ui<'a>(
        &'a self,
        mut rpass: wgpu::RenderPass<'a>,
        global_data: &'a GlobalParametersBuffer,
        widget_buffer: &'a WidgetBuffer<T>,
        world: &'a WorldRenderPass,
    ) -> wgpu::RenderPass<'a> {
        // Background
        rpass.set_pipeline(&self.background_pipeline);
        rpass.set_bind_group(Group::Globals.index(), global_data.bind_group(), &[]);
        rpass.set_bind_group(Group::Ui.index(), widget_buffer.bind_group(), &[]);
        rpass.set_bind_group(Group::OffScreenWorld.index(), world.bind_group(), &[]);
        rpass.set_vertex_buffer(0, widget_buffer.background_vertex_buffer());
        rpass.draw(widget_buffer.background_vertex_range(), 0..1);
        // Image
        // Text
        rpass.set_pipeline(&self.text_pipeline);
        rpass.set_bind_group(Group::Globals.index(), global_data.bind_group(), &[]);
        rpass.set_bind_group(Group::Ui.index(), widget_buffer.bind_group(), &[]);
        rpass.set_vertex_buffer(0, widget_buffer.text_vertex_buffer());
        rpass.draw(widget_buffer.text_vertex_range(), 0..1);

        rpass
    }
}
