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
use absolute_unit::{degrees, meters};
use animate::TimeStep;
use anyhow::Result;
use bevy_ecs::prelude::*;
use camera::{ArcBallController, ArcBallSystem, Camera, CameraSystem};
use event_mapper::EventMapper;
use fullscreen::{FullscreenBuffer, FullscreenVertex};
use geodesy::{GeoSurface, Graticule, Target};
use global_data::GlobalParametersBuffer;
use gpu::Gpu;
use input::{DemoFocus, InputSystem};
use measure::WorldSpaceFrame;
use orrery::Orrery;
use runtime::{ExitRequest, Extension, FrameStage, Runtime};
use std::time::Instant;
use window::{DisplayOpts, Window, WindowBuilder};

struct App {
    pipeline: wgpu::RenderPipeline,
}

impl Extension for App {
    fn init(runtime: &mut Runtime) -> Result<()> {
        let app = App::new(
            runtime.resource::<GlobalParametersBuffer>(),
            runtime.resource::<Gpu>(),
        )?;
        runtime.insert_resource(app);
        runtime
            .frame_stage_mut(FrameStage::Render)
            .add_system(Self::sys_render);
        Ok(())
    }
}

impl App {
    fn new(globals: &GlobalParametersBuffer, gpu: &Gpu) -> Result<Self> {
        let vert_shader = gpu.create_shader_module(
            "example.vert",
            include_bytes!("../target/example.vert.spirv"),
        )?;
        let frag_shader = gpu.create_shader_module(
            "example.frag",
            include_bytes!("../target/example.frag.spirv"),
        )?;

        let pipeline_layout =
            gpu.device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("main-pipeline-layout"),
                    push_constant_ranges: &[],
                    bind_group_layouts: &[globals.bind_group_layout()],
                });
        let pipeline = gpu
            .device()
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("main-pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &vert_shader,
                    entry_point: "main",
                    buffers: &[FullscreenVertex::descriptor()],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &frag_shader,
                    entry_point: "main",
                    targets: &[wgpu::ColorTargetState {
                        format: Gpu::SCREEN_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::COLOR,
                    }],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleStrip,
                    strip_index_format: Some(wgpu::IndexFormat::Uint16),
                    front_face: wgpu::FrontFace::Cw,
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
        Ok(Self { pipeline })
    }

    fn sys_render(
        app: Res<App>,
        globals: Res<GlobalParametersBuffer>,
        fullscreen: Res<FullscreenBuffer>,
        gpu: Res<Gpu>,
        maybe_surface: Res<Option<wgpu::SurfaceTexture>>,
        maybe_encoder: ResMut<Option<wgpu::CommandEncoder>>,
    ) {
        if let Some(surface_texture) = maybe_surface.into_inner() {
            if let Some(encoder) = maybe_encoder.into_inner() {
                let view = surface_texture
                    .texture
                    .create_view(&::wgpu::TextureViewDescriptor::default());
                let render_pass_desc_ref = wgpu::RenderPassDescriptor {
                    label: Some("screen-composite-render-pass"),
                    color_attachments: &[Gpu::color_attachment(&view)],
                    depth_stencil_attachment: Some(gpu.depth_stencil_attachment()),
                };
                let mut rpass = encoder.begin_render_pass(&render_pass_desc_ref);
                rpass.set_pipeline(&app.pipeline);
                rpass.set_bind_group(0, globals.bind_group(), &[]);
                rpass.set_vertex_buffer(0, fullscreen.vertex_buffer());
                rpass.draw(0..4, 0..1);
            }
        }
    }
}

fn main() -> Result<()> {
    InputSystem::run_forever(
        WindowBuilder::new().with_title("Nitrogen Render Demo"),
        window_main,
    )
}

fn window_main(mut runtime: Runtime) -> Result<()> {
    runtime
        .insert_resource(DisplayOpts::default())
        .insert_resource(DemoFocus::Demo)
        .load_extension::<EventMapper<DemoFocus>>()?
        .load_extension::<Window>()?
        .load_extension::<Gpu>()?
        .load_extension::<FullscreenBuffer>()?
        .load_extension::<GlobalParametersBuffer>()?
        .load_extension::<App>()?
        .load_extension::<Orrery>()?
        .load_extension::<CameraSystem>()?
        .load_extension::<ArcBallSystem>()?
        .load_extension::<TimeStep>()?
        .run_string(r#"bindings.bind("Escape", "exit()");"#)?;

    // But we need at least a camera and controller before the sim is ready to run.
    let camera = Camera::new(
        degrees!(90),
        runtime.resource::<Window>().render_aspect_ratio(),
        meters!(0.1),
    );
    let mut arcball = ArcBallController::default();
    arcball.pan_view(true);
    arcball.set_eye(Graticule::<Target>::new(
        degrees!(0),
        degrees!(0),
        meters!(10),
    ))?;
    arcball.set_target(Graticule::<GeoSurface>::new(
        degrees!(0),
        degrees!(0),
        meters!(10),
    ));
    arcball.set_distance(meters!(40.0));
    let player_ent = runtime
        .spawn_named("player")?
        .insert(WorldSpaceFrame::default())
        .insert_scriptable(arcball)?
        .insert_scriptable(camera)?
        .id();

    while runtime.resource::<ExitRequest>().still_running() {
        // Catch monotonic sim time up to system time.
        let frame_start = Instant::now();
        while runtime.resource::<TimeStep>().next_now() < frame_start {
            runtime.run_sim_once();
        }

        runtime.run_frame_once();

        runtime
            .get_mut::<ArcBallController>(player_ent)
            .handle_mousemotion(-0.5f64, 0f64);
    }

    Ok(())
}
