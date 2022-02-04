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
use absolute_unit::{degrees, meters, radians};
use animate::{TimeStep, Timeline};
use anyhow::{anyhow, bail, Result};
use atmosphere::AtmosphereBuffer;
use bevy_ecs::prelude::*;
use camera::{ArcBallController, ArcBallSystem, Camera, CameraSystem};
use catalog::{Catalog, CatalogOpts};
use chrono::{TimeZone, Utc};
use composite::CompositeRenderPass;
use event_mapper::EventMapper;
use fullscreen::FullscreenBuffer;
use global_data::GlobalParametersBuffer;
use gpu::{DetailLevelOpts, Gpu, UploadTracker};
use input::{InputFocus, InputSystem};
use measure::WorldSpaceFrame;
use nitrous::{inject_nitrous_resource, method, NitrousResource, Value};
use orrery::Orrery;
use parking_lot::RwLock;
use platform_dirs::AppDirs;
use runtime::{Extension, FrameStage, Runtime, ScriptHerder, StartupOpts};
use stars::StarsBuffer;
use std::{f32::consts::PI, fs::create_dir_all, str::FromStr, sync::Arc, time::Instant};
use structopt::StructOpt;
use terminal_size::{terminal_size, Width};
use terrain::TerrainBuffer;
use ui::UiRenderPass;
use widget::{
    Border, Color, Expander, Label, Labeled, PositionH, PositionV, VerticalBox, WidgetBuffer,
};
use window::{
    size::{LeftBound, Size},
    DisplayOpts, Window, WindowBuilder,
};
use world_render::WorldRenderPass;

/// Demonstrate the capabilities of the Nitrogen engine
#[derive(Debug, StructOpt)]
#[structopt(set_term_width = if let Some((Width(w), _)) = terminal_size() { w as usize } else { 80 })]
struct Opt {
    #[structopt(flatten)]
    catalog_opts: CatalogOpts,

    #[structopt(flatten)]
    detail_opts: DetailLevelOpts,

    #[structopt(flatten)]
    display_opts: DisplayOpts,

    #[structopt(flatten)]
    startup_opts: StartupOpts,
}

#[derive(Debug)]
struct VisibleWidgets {
    sim_time: Arc<RwLock<Label>>,
    camera_direction: Arc<RwLock<Label>>,
    camera_position: Arc<RwLock<Label>>,
    camera_fov: Arc<RwLock<Label>>,
    fps_label: Arc<RwLock<Label>>,
}

#[derive(Debug, NitrousResource)]
struct System {
    exit: bool,
    pin_camera: bool,
    camera: Option<Camera>,
    visible_widgets: VisibleWidgets,
}

impl Extension for System {
    fn init(runtime: &mut Runtime) -> Result<()> {
        let widgets = runtime.resource::<WidgetBuffer<SimState>>();
        let system = System::new(widgets)?;
        runtime.insert_named_resource("system", system);
        runtime
            .frame_stage_mut(FrameStage::FrameEnd)
            .add_system(Self::sys_track_visible_state);
        runtime.resource_mut::<ScriptHerder>().run_string(
            r#"
                bindings.bind("Escape", "system.exit()");
                bindings.bind("q", "system.exit()");
                bindings.bind("p", "system.toggle_pin_camera(pressed)");
                bindings.bind("g", "widget.dump_glyphs(pressed)");
            "#,
        )?;
        Ok(())
    }
}

#[inject_nitrous_resource]
impl System {
    pub fn new(widgets: &WidgetBuffer<SimState>) -> Result<Self> {
        let visible_widgets = Self::build_gui(widgets)?;
        Ok(Self {
            exit: false,
            pin_camera: false,
            camera: None,
            visible_widgets,
        })
    }

    pub fn build_gui(widgets: &WidgetBuffer<SimState>) -> Result<VisibleWidgets> {
        let sim_time = Label::new("").with_color(Color::White).wrapped();
        let camera_direction = Label::new("").with_color(Color::White).wrapped();
        let camera_position = Label::new("").with_color(Color::White).wrapped();
        let camera_fov = Label::new("").with_color(Color::White).wrapped();
        let controls_box = VerticalBox::new_with_children(&[
            sim_time.clone(),
            camera_direction.clone(),
            camera_position.clone(),
            camera_fov.clone(),
        ])
        .with_background_color(Color::Gray.darken(3.).opacity(0.8))
        .with_glass_background()
        .with_padding(Border::new(
            Size::zero(),
            Size::from_px(8.),
            Size::from_px(24.),
            Size::from_px(8.),
        ))
        .wrapped();
        let expander = Expander::new_with_child("☰ Nitrogen v0.1", controls_box)
            .with_color(Color::White)
            .with_background_color(Color::Gray.darken(3.).opacity(0.8))
            .with_glass_background()
            .with_border(
                Color::Black,
                Border::new(
                    Size::zero(),
                    Size::from_px(2.),
                    Size::from_px(2.),
                    Size::zero(),
                ),
            )
            .with_padding(Border::new(
                Size::from_px(2.),
                Size::from_px(3.),
                Size::from_px(3.),
                Size::from_px(2.),
            ))
            .wrapped();
        widgets
            .root_container()
            .write()
            .add_child("controls", expander)
            .set_float(PositionH::End, PositionV::Top);

        let fps_label = Label::new("")
            .with_font(widgets.font_context().font_id_for_name("sans"))
            .with_color(Color::Red)
            .with_size(Size::from_pts(13.0))
            .with_pre_blended_text()
            .wrapped();
        widgets
            .root_container()
            .write()
            .add_child("fps", fps_label.clone())
            .set_float(PositionH::Start, PositionV::Bottom);
        Ok(VisibleWidgets {
            sim_time,
            camera_direction,
            camera_position,
            camera_fov,
            fps_label,
        })
    }

    fn sys_track_visible_state(
        query: Query<(&ArcBallController, &Camera)>,
        timestep: Res<TimeStep>,
        orrery: Res<Orrery>,
        system: ResMut<System>,
    ) {
        for (arcball, camera) in query.iter() {
            system
                .track_visible_state(*timestep.now(), &orrery, &arcball, &camera)
                .ok();
            break;
        }
    }

    pub fn track_visible_state(
        &self,
        now: Instant,
        orrery: &Orrery,
        arcball: &ArcBallController,
        camera: &Camera,
    ) -> Result<()> {
        self.visible_widgets
            .sim_time
            .write()
            .set_text(format!("Date: {}", orrery.get_time()));
        self.visible_widgets
            .camera_direction
            .write()
            .set_text(format!("Eye: {}", arcball.eye()));
        self.visible_widgets
            .camera_position
            .write()
            .set_text(format!("Position: {}", arcball.target(),));
        self.visible_widgets
            .camera_fov
            .write()
            .set_text(format!("FoV: {}", degrees!(camera.fov_y()),));
        let frame_time = now.elapsed();
        let ts = format!(
            "frame: {}.{}ms",
            frame_time.as_secs() * 1000 + u64::from(frame_time.subsec_millis()),
            frame_time.subsec_micros(),
        );
        self.visible_widgets.fps_label.write().set_text(ts);
        Ok(())
    }

    #[method]
    pub fn println(&self, message: Value) {
        println!("{}", message);
    }

    #[method]
    pub fn exit(&mut self) {
        self.exit = true;
    }

    #[method]
    pub fn toggle_pin_camera(&mut self, pressed: bool) {
        if pressed {
            self.pin_camera = !self.pin_camera;
        }
    }

    // pub fn current_camera(&mut self, camera: &Camera) -> &Camera {
    //     if !self.pin_camera {
    //         self.camera = camera.to_owned();
    //     }
    //     &self.camera
    // }
}

/*
make_frame_graph!(
    FrameGraph {
        buffers: {
            // Note: order must be lock order
            // system
            composite: CompositeRenderPass,
            ui: UiRenderPass,
            widgets: WidgetBuffer,
            world: WorldRenderPass,
            terrain: TerrainBuffer,
            atmosphere: AtmosphereBuffer,
            stars: StarsBuffer,
            fullscreen: FullscreenBuffer,
            globals: GlobalParametersBuffer
            // gpu
            // window
            // arcball
            // camera
            // orrery
        };
        passes: [
            // widget
            maintain_font_atlas: Any() { widgets() },

            // terrain
            // Update the indices so we have correct height data to tessellate with and normal
            // and color data to accumulate.
            paint_atlas_indices: Any() { terrain() },
            // Apply heights to the terrain mesh.
            tessellate: Compute() { terrain() },
            // Render the terrain mesh's texcoords to an offscreen buffer.
            deferred_texture: Render(terrain, deferred_texture_target) {
                terrain( globals )
            },
            // Accumulate normal and color data.
            accumulate_normal_and_color: Compute() { terrain( globals ) },

            // world: Flatten terrain g-buffer into the final image and mix in stars.
            render_world: Render(world, offscreen_target_cleared) {
                world( globals, fullscreen, atmosphere, stars, terrain )
            },

            // ui: Draw our widgets onto a buffer with resolution independent of the world.
            render_ui: Render(ui, offscreen_target) {
                ui( globals, widgets, world )
            },

            // composite: Accumulate offscreen buffers into a final image.
            composite_scene: Render(Screen) {
                composite( fullscreen, globals, world, ui )
            }
        ];
    }
);
*/

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum SimState {
    Demo,
    Terminal,
}

impl Default for SimState {
    fn default() -> Self {
        Self::Demo
    }
}

impl FromStr for SimState {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::prelude::rust_2015::Result<Self, Self::Err> {
        Ok(match s {
            "demo" => Self::Demo,
            "terminal" => Self::Terminal,
            _ => bail!(
                "unknown focus to bind in {}; expected \"demo\" or \"terminal\"",
                s
            ),
        })
    }
}

impl InputFocus for SimState {
    fn name(&self) -> &'static str {
        match self {
            Self::Demo => "demo",
            Self::Terminal => "terminal",
        }
    }

    fn is_terminal_focused(&self) -> bool {
        *self == Self::Terminal
    }

    fn toggle_terminal(&mut self) {
        *self = match self {
            Self::Terminal => Self::Demo,
            Self::Demo => Self::Terminal,
        };
    }
}

fn main() -> Result<()> {
    env_logger::init();
    InputSystem::run_forever(
        WindowBuilder::new().with_title("Nitrogen Demo"),
        simulation_main,
    )
}

fn simulation_main(mut runtime: Runtime) -> Result<()> {
    let opt = Opt::from_args();

    // Make sure various config locations exist
    let app_dirs = AppDirs::new(Some("nitrogen"), true)
        .ok_or_else(|| anyhow!("unable to find app directories"))?;
    create_dir_all(&app_dirs.config_dir)?;
    create_dir_all(&app_dirs.state_dir)?;

    runtime
        .insert_resource(opt.catalog_opts)
        .insert_resource(opt.display_opts)
        .insert_resource(opt.detail_opts.cpu_detail())
        .insert_resource(opt.detail_opts.gpu_detail())
        .insert_resource(app_dirs)
        .insert_resource(SimState::Demo)
        .load_extension::<Catalog>()?
        .load_extension::<EventMapper<SimState>>()?
        .load_extension::<Window>()?
        .load_extension::<Gpu>()?
        .load_extension::<AtmosphereBuffer>()?
        .load_extension::<FullscreenBuffer>()?
        .load_extension::<GlobalParametersBuffer>()?
        .load_extension::<StarsBuffer>()?
        .load_extension::<TerrainBuffer>()?
        .load_extension::<WorldRenderPass>()?
        .load_extension::<WidgetBuffer<SimState>>()?
        .load_extension::<UiRenderPass<SimState>>()?
        .load_extension::<CompositeRenderPass<SimState>>()?
        .load_extension::<System>()?
        .load_extension::<Orrery>()?
        .load_extension::<Timeline>()?
        .load_extension::<TimeStep>()?
        .load_extension::<CameraSystem>()?
        .load_extension::<ArcBallSystem>()?;

    // But we need at least a camera and controller before the sim is ready to run.
    let camera = Camera::new(
        radians!(PI / 2.0),
        runtime.resource::<Window>().render_aspect_ratio(),
        meters!(0.5),
    );
    let _player_ent = runtime
        .spawn_named("player")
        .insert(WorldSpaceFrame::default())
        .insert_scriptable(ArcBallController::new())
        .insert_scriptable(camera)
        .id();

    // We are now finished and can safely run the startup scripts / configuration.
    opt.startup_opts
        .on_startup(&mut runtime.resource_mut::<ScriptHerder>())?;

    while !runtime.resource::<System>().exit {
        // Catch monotonic sim time up to system time.
        let frame_start = Instant::now();
        while runtime.resource::<TimeStep>().next_now() < frame_start {
            runtime.run_sim_once();
        }
        runtime.run_frame_once();

        {
            let config = runtime.resource::<Window>().config().to_owned();

            let mut encoder = runtime
                .resource_mut::<Gpu>()
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("frame-encoder"),
                });

            let surface_texture = if let Some(surface_texture) =
                runtime.resource_mut::<Gpu>().get_next_framebuffer()?
            {
                surface_texture
            } else {
                runtime
                    .resource_mut::<Gpu>()
                    .on_display_config_changed(&config)?;
                continue;
            };

            {
                runtime
                    .resource_mut::<UploadTracker>()
                    .dispatch_uploads_until_empty(&mut encoder);

                encoder = runtime
                    .resource::<WidgetBuffer<SimState>>()
                    .maintain_font_atlas(encoder)?;
                encoder = runtime
                    .resource::<TerrainBuffer>()
                    .paint_atlas_indices(encoder)?;

                // terrain
                {
                    let cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("compute-pass"),
                    });
                    let _cpass = runtime.resource::<TerrainBuffer>().tessellate(cpass)?;
                }
                {
                    let (color_attachments, depth_stencil_attachment) = runtime
                        .resource::<TerrainBuffer>()
                        .deferred_texture_target();
                    let render_pass_desc_ref = wgpu::RenderPassDescriptor {
                        label: Some(concat!("non-screen-render-pass-terrain-deferred",)),
                        color_attachments: &color_attachments,
                        depth_stencil_attachment,
                    };
                    let rpass = encoder.begin_render_pass(&render_pass_desc_ref);
                    let _rpass = runtime
                        .resource::<TerrainBuffer>()
                        .deferred_texture(rpass, runtime.resource::<GlobalParametersBuffer>())?;
                }
                {
                    let cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("compute-pass"),
                    });
                    let _cpass = runtime
                        .resource::<TerrainBuffer>()
                        .accumulate_normal_and_color(
                            cpass,
                            runtime.resource::<GlobalParametersBuffer>(),
                        )?;
                }

                // world: Flatten terrain g-buffer into the final image and mix in stars.
                {
                    let (color_attachments, depth_stencil_attachment) = runtime
                        .resource::<WorldRenderPass>()
                        .offscreen_target_cleared();
                    let render_pass_desc_ref = wgpu::RenderPassDescriptor {
                        label: Some("offscreen-draw-world"),
                        color_attachments: &color_attachments,
                        depth_stencil_attachment,
                    };
                    let rpass = encoder.begin_render_pass(&render_pass_desc_ref);
                    let _rpass = runtime.resource::<WorldRenderPass>().render_world(
                        rpass,
                        runtime.resource::<GlobalParametersBuffer>(),
                        runtime.resource::<FullscreenBuffer>(),
                        runtime.resource::<AtmosphereBuffer>(),
                        runtime.resource::<StarsBuffer>(),
                        &runtime.resource::<TerrainBuffer>(),
                    )?;
                }

                // ui: Draw our widgets onto a buffer with resolution independent of the world.
                {
                    let (color_attachments, depth_stencil_attachment) = runtime
                        .resource::<UiRenderPass<SimState>>()
                        .offscreen_target();
                    let render_pass_desc_ref = wgpu::RenderPassDescriptor {
                        label: Some(concat!("non-screen-render-pass-ui-draw-offscreen",)),
                        color_attachments: &color_attachments,
                        depth_stencil_attachment,
                    };
                    let rpass = encoder.begin_render_pass(&render_pass_desc_ref);
                    let _rpass = runtime.resource::<UiRenderPass<SimState>>().render_ui(
                        rpass,
                        runtime.resource::<GlobalParametersBuffer>(),
                        runtime.resource::<WidgetBuffer<SimState>>(),
                        runtime.resource::<WorldRenderPass>(),
                    )?;
                }

                // composite: Accumulate offscreen buffers into a final image.
                {
                    let gpu = runtime.resource::<Gpu>();
                    let view = surface_texture
                        .texture
                        .create_view(&::wgpu::TextureViewDescriptor::default());
                    let render_pass_desc_ref = wgpu::RenderPassDescriptor {
                        label: Some("screen-composite-render-pass"),
                        color_attachments: &[Gpu::color_attachment(&view)],
                        depth_stencil_attachment: Some(gpu.depth_stencil_attachment()),
                    };
                    let rpass = encoder.begin_render_pass(&render_pass_desc_ref);
                    let _rpass = runtime
                        .resource::<CompositeRenderPass<SimState>>()
                        .composite_scene(
                            rpass,
                            runtime.resource::<FullscreenBuffer>(),
                            runtime.resource::<GlobalParametersBuffer>(),
                            runtime.resource::<WorldRenderPass>(),
                            runtime.resource::<UiRenderPass<SimState>>(),
                        )?;
                }
            };

            runtime
                .resource_mut::<Gpu>()
                .queue_mut()
                .submit(vec![encoder.finish()]);
            surface_texture.present();
        }
    }

    Ok(())
}
