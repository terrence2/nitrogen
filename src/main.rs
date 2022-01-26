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
use anyhow::{anyhow, Result};
use atmosphere::AtmosphereBuffer;
use bevy_ecs::prelude::*;
use camera::{ArcBallCamera, ArcBallController, Camera, CameraComponent};
use catalog::{Catalog, CatalogOpts, DirectoryDrawer};
use chrono::{TimeZone, Utc};
use composite::CompositeRenderPass;
use event_mapper::EventMapper;
use fullscreen::FullscreenBuffer;
use global_data::GlobalParametersBuffer;
use gpu::{CpuDetailLevel, DetailLevelOpts, Gpu, GpuDetailLevel};
use input::{InputController, InputSystem};
use measure::WorldSpaceFrame;
use nitrous::{Interpreter, StartupOpts, Value};
use nitrous_injector::{inject_nitrous_module, method, NitrousModule};
use orrery::Orrery;
use parking_lot::RwLock;
use platform_dirs::AppDirs;
use runtime::{Extension, Runtime};
use stars::StarsBuffer;
use std::{f32::consts::PI, fs::create_dir_all, path::PathBuf, sync::Arc, time::Instant};
use structopt::StructOpt;
use terminal_size::{terminal_size, Width};
use terrain::TerrainBuffer;
use ui::UiRenderPass;
use widget::{
    Border, Color, Expander, Label, Labeled, PositionH, PositionV, VerticalBox, WidgetBuffer,
};
use window::{
    size::{LeftBound, Size},
    DisplayConfig, DisplayOpts, OsWindow, Window, WindowBuilder,
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

#[derive(Debug, NitrousModule)]
struct System {
    exit: bool,
    pin_camera: bool,
    camera: Camera,
    visible_widgets: VisibleWidgets,
}

#[inject_nitrous_module]
impl System {
    pub fn new(widgets: &WidgetBuffer, interpreter: &mut Interpreter) -> Result<Arc<RwLock<Self>>> {
        let visible_widgets = Self::build_gui(widgets)?;
        let system = Arc::new(RwLock::new(Self {
            exit: false,
            pin_camera: false,
            camera: Default::default(),
            visible_widgets,
        }));
        interpreter.put_global("system", Value::Module(system.clone()));
        // interpreter.interpret_once(
        //     r#"
        //         let bindings := mapper.create_bindings("system");
        //         bindings.bind("Escape", "system.exit()");
        //         bindings.bind("q", "system.exit()");
        //         bindings.bind("p", "system.toggle_pin_camera(pressed)");
        //         bindings.bind("g", "widget.dump_glyphs(pressed)");
        //     "#,
        // )?;
        Ok(system)
    }

    pub fn build_gui(widgets: &WidgetBuffer) -> Result<VisibleWidgets> {
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

    pub fn track_visible_state(
        &mut self,
        now: Instant,
        orrery: &Orrery,
        arcball: &ArcBallCamera,
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

    pub fn current_camera(&mut self, camera: &Camera) -> &Camera {
        if !self.pin_camera {
            self.camera = camera.to_owned();
        }
        &self.camera
    }
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

// fn build_frame_graph(
//     app_dirs: &AppDirs,
//     interpreter: &mut Interpreter,
//     runtime: &mut Runtime,
// ) -> Result<FrameGraph> {
//     // let atmosphere_buffer = AtmosphereBuffer::new(&mut runtime.resource_mut::<Gpu>())?;
//     // runtime.insert_resource(atmosphere_buffer.clone());
//
//
//     // Compose the frame graph.
//     // TODO: should this be dynamic?
//     let frame_graph = FrameGraph::new(
//         composite,
//         ui,
//         widgets,
//         world_gfx,
//         terrain,
//         atmosphere_buffer,
//         stars_buffer,
//         fullscreen_buffer,
//         globals,
//     )?;
//
//     Ok(frame_graph)
// }

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

    fn timestep(runtime: &Runtime) -> &TimeStep {
        runtime.resource::<TimeStep>()
    }

    // Create the game interpreter
    let mut interpreter = Interpreter::default();
    // runtime.world.insert_resource(interpreter.clone());

    runtime
        .insert_resource(opt.catalog_opts)
        .insert_resource(opt.display_opts)
        .insert_resource(opt.detail_opts.cpu_detail())
        .insert_resource(opt.detail_opts.gpu_detail())
        .load_extension::<Catalog>()?
        .load_extension::<EventMapper>()?
        .load_extension::<Window>()?
        .load_extension::<Gpu>()?
        .load_extension::<AtmosphereBuffer>()?
        .load_extension::<FullscreenBuffer>()?
        .load_extension::<GlobalParametersBuffer>()?
        .load_extension::<StarsBuffer>()?
        .load_extension::<TerrainBuffer>()?
        .load_extension::<WorldRenderPass>()?
        .load_extension::<TimeStep>()?;

    // We don't technically need the window here, just the graphics configuration, and we could
    // even potentially create blind and expect the resize later. Worth looking into as it would
    // let us initialize async. The main work to do in parallel is discovering tile trees; this
    // does not technically depend on the gpu, but does because it lives with terrain creation,
    // which does need the gpu for resource creation. Probably worth looking to parallelize this
    // as we could potentially half our startup time.
    //let mut frame_graph = build_frame_graph(&app_dirs, &mut interpreter, &mut runtime)?;

    // let fullscreen_buffer = FullscreenBuffer::new(runtime.resource::<Gpu>());
    // runtime.insert_resource(fullscreen_buffer.clone());

    // let globals = GlobalParametersBuffer::new(runtime.resource::<Gpu>().device(), &mut interpreter);
    // runtime.insert_resource(globals.clone());

    // let stars_buffer = Arc::new(RwLock::new(StarsBuffer::new(runtime.resource::<Gpu>())?));
    // runtime.insert_resource(stars_buffer.clone());

    // let terrain = {
    //     let catalog_ref = runtime.resource::<Arc<RwLock<Catalog>>>().clone();
    //     let catalog = &catalog_ref.read();
    //     let cpu_detail_level = *runtime.resource::<CpuDetailLevel>();
    //     let gpu_detail_level = *runtime.resource::<GpuDetailLevel>();
    //     let terrain_buffer = TerrainBuffer::new(
    //         catalog,
    //         cpu_detail_level,
    //         gpu_detail_level,
    //         runtime.resource::<GlobalParametersBuffer>(),
    //         &runtime.resource::<Gpu>(),
    //         &mut interpreter,
    //     )?;
    //     runtime.insert_resource(terrain_buffer.clone());
    //     terrain_buffer
    // };

    // let world_gfx = WorldRenderPass::new(
    //     &runtime.resource::<TerrainBuffer>(),
    //     runtime.resource::<AtmosphereBuffer>(),
    //     runtime.resource::<StarsBuffer>(),
    //     runtime.resource::<GlobalParametersBuffer>(),
    //     runtime.resource::<Gpu>(),
    //     &mut interpreter,
    // )?;
    // runtime.insert_resource(world_gfx.clone());
    // world_gfx.write().add_debug_bindings(interpreter)?;

    let widgets = WidgetBuffer::new(
        &mut runtime.resource_mut::<Gpu>(),
        &mut interpreter,
        &app_dirs.state_dir,
    )?;
    runtime.insert_resource(widgets.clone());

    // This is just rendering for widgets, so should be merged.
    let ui = UiRenderPass::new(
        &widgets.read(),
        runtime.resource::<WorldRenderPass>(),
        runtime.resource::<GlobalParametersBuffer>(),
        runtime.resource::<Gpu>(),
    )?;
    runtime.insert_resource(ui.clone());

    let composite = Arc::new(RwLock::new(CompositeRenderPass::new(
        &ui.read(),
        runtime.resource::<WorldRenderPass>(),
        runtime.resource::<GlobalParametersBuffer>(),
        runtime.resource::<Gpu>(),
    )?));
    runtime.insert_resource(composite.clone());
    // Create rest of game resources
    let initial_utc = Utc.ymd(1964, 2, 24).and_hms(12, 0, 0);
    let orrery = Orrery::new(initial_utc, &mut interpreter)?;
    runtime.world.insert_resource(orrery.clone());
    let timeline = Timeline::new(&mut interpreter);
    runtime.world.insert_resource(timeline);

    let system = System::new(&widgets.read(), &mut interpreter)?;
    runtime.world.insert_resource(system.clone());

    // But we need at least a camera and controller before the sim is ready to run.
    let camera = Camera::install(
        radians!(PI / 2.0),
        runtime.resource::<Window>().render_aspect_ratio(),
        meters!(0.5),
        &mut interpreter,
    )?;
    let arcball = ArcBallCamera::install(&mut interpreter)?;
    let _player_ent = runtime
        .world
        .spawn()
        .insert(WorldSpaceFrame::default())
        .insert(ArcBallController::new(arcball.clone()))
        .insert(CameraComponent::new(camera.clone()))
        .id();

    //////////////////////////////////////////////////////////////////
    // Sim Schedule
    //
    // The simulation schedule should be used for "pure" entity to entity work and update of
    // a handful of game related resources, rather than communicating with the GPU. This generally
    // splits into two phases: per-fixed-tick resource updates and entity updates from resources.
    let mut sim_schedule = Schedule::default();
    sim_schedule.add_stage(
        "time",
        SystemStage::single_threaded().with_system(Orrery::sys_step_time.system()),
    );
    sim_schedule.add_stage(
        "interpret_input_events",
        SystemStage::parallel()
            .with_system(WidgetBuffer::sys_handle_input_events.system())
            .with_system(Timeline::sys_animate.system()),
    );
    sim_schedule.add_stage(
        "propagate_changes",
        SystemStage::single_threaded()
            .with_system(ArcBallCamera::sys_apply_input.system())
            .with_system(CameraComponent::sys_apply_input.system()),
    );

    //////////////////////////////////////////////////////////////////
    // Track State Changes (from entities and game resources [into graphics systems])
    //
    // Copy from entities into buffers more suitable for upload to the GPU. Also, do heavier
    // CPU-side graphics work that can be parallelized efficiently, like updating the terrain
    // from the current cameras. Not generally for writing to the GPU.
    //
    // TODO: how much can we parallelize for writing to the GPU? Anything at all?
    //       UploadTracker is shared state. Can we do anything with frame graph?
    //
    // Note: We have to take resources as non-mutable references, so that we can run in parallel.
    //       We can take as many of these in parallel as we want, iff there is no parallel write
    //       to those same resource (e.g. window). Otherwise we might deadlock.

    fn update_widget_track_state_changes(
        step: Res<TimeStep>,
        window: Res<Window>,
        widgets: Res<Arc<RwLock<WidgetBuffer>>>,
    ) {
        widgets
            .write()
            .track_state_changes(*step.now(), &window)
            .expect("Widgets::track_state_changes");
    }

    fn update_terrain_track_state_changes(
        query: Query<&CameraComponent>,
        catalog: Res<Arc<RwLock<Catalog>>>,
        system: Res<Arc<RwLock<System>>>,
        mut terrain: ResMut<TerrainBuffer>,
    ) {
        // FIXME: multiple camera support
        let mut system = system.write();
        for (i, camera) in query.iter().enumerate() {
            assert_eq!(i, 0);
            let vis_camera = system.current_camera(&camera.camera());
            terrain
                .track_state_changes(&camera.camera(), vis_camera, catalog.clone())
                .expect("Terrain::track_state_changes");
        }
    }

    let mut frame_schedule = Schedule::default();
    frame_schedule.add_stage(
        "input",
        SystemStage::single_threaded()
            .with_system(InputController::sys_read_system_events.system()),
    );
    let mut update_frame_schedule = Schedule::default();
    update_frame_schedule.add_stage(
        "update_frame",
        SystemStage::single_threaded()
            .with_system(CameraComponent::sys_apply_display_changes)
            .with_system(UiRenderPass::sys_handle_display_config_change)
            .with_system(update_widget_track_state_changes)
            .with_system(update_terrain_track_state_changes), // .with_wystem(update_widgets_ensure_uploaded),
    );

    // We are now finished and can safely run the startup scripts / configuration.
    opt.startup_opts.on_startup(&mut interpreter)?;

    while !system.read().exit {
        // Catch monotonic sim time up to system time.
        let frame_start = Instant::now();
        while timestep(&runtime).next_now() < frame_start {
            runtime.run_sim_once();
            sim_schedule.run_once(&mut runtime.world);
        }

        runtime.run_frame_once();
        update_frame_schedule.run_once(&mut runtime.world);

        let mut tracker = Default::default();
        runtime
            .resource::<GlobalParametersBuffer>()
            .ensure_uploaded(runtime.resource::<Gpu>(), &mut tracker)?;
        runtime
            .world
            .resource_scope(|world, mut terrain: Mut<TerrainBuffer>| {
                terrain
                    .ensure_uploaded(world.get_resource::<Gpu>().unwrap(), &mut tracker)
                    .ok();
            });
        widgets.write().ensure_uploaded(
            *timestep(&runtime).now(),
            runtime.resource::<Gpu>(),
            runtime.resource::<Window>(),
            &mut tracker,
        )?;

        {
            // if !frame_graph.run(&mut runtime.resource_mut::<Gpu>(), tracker)? {
            //     let config = runtime.resource::<Window>().config().to_owned();
            //     runtime
            //         .resource_mut::<Gpu>()
            //         .on_display_config_changed(&config)?;
            // }
            let config = runtime.resource::<Window>().config().to_owned();
            //let gpu = &mut runtime.resource_mut::<Gpu>();
            let composite = composite.read();
            let ui = ui.read();
            let widgets = widgets.read();
            // let world = world_gfx.read();
            // let terrain = terrain.read();
            // let stars = stars_buffer.read();
            // let fullscreen = fullscreen_buffer.read();
            // let globals = globals.read();

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
                tracker.dispatch_uploads(&mut encoder);

                encoder = widgets.maintain_font_atlas(encoder)?;
                encoder = runtime
                    .resource::<TerrainBuffer>()
                    .paint_atlas_indices(encoder)?;

                // terrain
                {
                    let cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("compute-pass"),
                    });
                    let cpass = runtime.resource::<TerrainBuffer>().tessellate(cpass)?;
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
                    let rpass = runtime
                        .resource::<TerrainBuffer>()
                        .deferred_texture(rpass, runtime.resource::<GlobalParametersBuffer>())?;
                }
                {
                    let cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("compute-pass"),
                    });
                    let cpass = runtime
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
                    let rpass = runtime.resource::<WorldRenderPass>().render_world(
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
                    let (color_attachments, depth_stencil_attachment) = ui.offscreen_target();
                    let render_pass_desc_ref = wgpu::RenderPassDescriptor {
                        label: Some(concat!("non-screen-render-pass-ui-draw-offscreen",)),
                        color_attachments: &color_attachments,
                        depth_stencil_attachment,
                    };
                    let rpass = encoder.begin_render_pass(&render_pass_desc_ref);
                    let rpass = ui.render_ui(
                        rpass,
                        runtime.resource::<GlobalParametersBuffer>(),
                        &widgets,
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
                    let rpass = composite.composite_scene(
                        rpass,
                        runtime.resource::<FullscreenBuffer>(),
                        runtime.resource::<GlobalParametersBuffer>(),
                        runtime.resource::<WorldRenderPass>(),
                        &ui,
                    )?;
                }
            };

            runtime
                .resource_mut::<Gpu>()
                .queue_mut()
                .submit(vec![encoder.finish()]);
            surface_texture.present();
        }

        system.write().track_visible_state(
            frame_start, // compute frame times from actual elapsed time
            &orrery.read(),
            &arcball.read(),
            &camera.read(),
        )?;
    }

    Ok(())
}
