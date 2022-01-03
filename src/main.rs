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
use animate::Timeline;
use anyhow::{anyhow, Result};
use atmosphere::AtmosphereBuffer;
use bevy_ecs::prelude::*;
use camera::{ArcBallCamera, ArcBallController, Camera, CameraComponent};
use catalog::{Catalog, DirectoryDrawer};
use chrono::{Duration as ChronoDuration, TimeZone, Utc};
use composite::CompositeRenderPass;
use event_mapper::EventMapper;
use fullscreen::FullscreenBuffer;
use global_data::GlobalParametersBuffer;
use gpu::{make_frame_graph, CpuDetailLevel, DetailLevelOpts, Gpu, GpuDetailLevel};
use input::{InputController, InputEventVec, InputFocus, InputSystem, SystemEventVec};
use measure::{SimStep, WorldSpaceFrame};
use nitrous::{Interpreter, StartupOpts, Value};
use nitrous_injector::{inject_nitrous_module, method, NitrousModule};
use orrery::Orrery;
use parking_lot::{Mutex, RwLock};
use platform_dirs::AppDirs;
use stars::StarsBuffer;
use std::{
    f32::consts::PI,
    fs::create_dir_all,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use structopt::StructOpt;
use terminal_size::{terminal_size, Width};
use terrain::TerrainBuffer;
use ui::UiRenderPass;
use widget::{
    Border, Color, Expander, Label, Labeled, PositionH, PositionV, VerticalBox, WidgetBuffer,
};
use window::{
    size::{LeftBound, Size},
    DisplayConfig, DisplayOpts, OsWindow, Window,
};
use world_render::WorldRenderPass;

/// Demonstrate the capabilities of the Nitrogen engine
#[derive(Debug, StructOpt)]
#[structopt(set_term_width = if let Some((Width(w), _)) = terminal_size() { w as usize } else { 80 })]
struct Opt {
    /// Extra directories to treat as libraries
    #[structopt(short, long)]
    libdir: Vec<PathBuf>,

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
        interpreter.interpret_once(
            r#"
                let bindings := mapper.create_bindings("system");
                bindings.bind("Escape", "system.exit()");
                bindings.bind("q", "system.exit()");
                bindings.bind("p", "system.toggle_pin_camera(pressed)");
                bindings.bind("g", "widget.dump_glyphs(pressed)");
            "#,
        )?;
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

fn build_frame_graph(
    cpu_detail: CpuDetailLevel,
    gpu_detail: GpuDetailLevel,
    app_dirs: &AppDirs,
    catalog: &Catalog,
    window: &mut Window,
    interpreter: &mut Interpreter,
    world: &mut World,
) -> Result<(Arc<RwLock<Gpu>>, FrameGraph)> {
    let gpu = Gpu::new(window, Default::default(), interpreter)?;
    world.insert_resource(gpu.clone());

    let atmosphere_buffer = AtmosphereBuffer::new(&mut gpu.write())?;
    world.insert_resource(atmosphere_buffer.clone());

    let fullscreen_buffer = FullscreenBuffer::new(&gpu.read());
    world.insert_resource(fullscreen_buffer.clone());

    let globals = GlobalParametersBuffer::new(gpu.read().device(), interpreter);
    world.insert_resource(globals.clone());
    globals.write().add_debug_bindings(interpreter)?;

    let stars_buffer = Arc::new(RwLock::new(StarsBuffer::new(&gpu.read())?));
    world.insert_resource(stars_buffer.clone());

    let terrain_buffer = TerrainBuffer::new(
        catalog,
        cpu_detail,
        gpu_detail,
        &globals.read(),
        &mut gpu.write(),
        interpreter,
    )?;
    world.insert_resource(terrain_buffer.clone());

    let world_gfx = WorldRenderPass::new(
        &terrain_buffer.read(),
        &atmosphere_buffer.read(),
        &stars_buffer.read(),
        &globals.read(),
        &mut gpu.write(),
        interpreter,
    )?;
    world.insert_resource(world_gfx.clone());
    world_gfx.write().add_debug_bindings(interpreter)?;

    let widgets = WidgetBuffer::new(&mut gpu.write(), interpreter, &app_dirs.state_dir)?;
    world.insert_resource(widgets.clone());

    // This is just rendering for widgets, so should be merged.
    let ui = UiRenderPass::new(
        &widgets.read(),
        &world_gfx.read(),
        &globals.read(),
        &mut gpu.write(),
    )?;
    world.insert_resource(ui.clone());

    let composite = Arc::new(RwLock::new(CompositeRenderPass::new(
        &ui.read(),
        &world_gfx.read(),
        &globals.read(),
        &mut gpu.write(),
    )?));
    world.insert_resource(composite.clone());

    // Compose the frame graph.
    // TODO: should this be dynamic?
    let frame_graph = FrameGraph::new(
        composite,
        ui,
        widgets,
        world_gfx,
        terrain_buffer,
        atmosphere_buffer,
        stars_buffer,
        fullscreen_buffer,
        globals,
    )?;

    Ok((gpu, frame_graph))
}

fn main() -> Result<()> {
    env_logger::init();
    InputSystem::run_forever(simulation_main)
}

const STEP: Duration = Duration::from_micros(16_666);

fn simulation_main(
    os_window: OsWindow,
    input_controller: Arc<Mutex<InputController>>,
) -> Result<()> {
    os_window.set_title("Nitrogen Demo");

    // Create the game resource system and insert globals
    let mut world = World::default();
    world.insert_resource(InputEventVec::new());
    world.insert_resource(SystemEventVec::new());
    world.insert_resource(InputFocus::Game);
    world.insert_resource(None as Option<DisplayConfig>);
    //resources.insert(SimStep::new_60fps());
    world.insert_resource(Instant::now());
    fn sim_now(world: &World) -> &Instant {
        world.get_resource::<Instant>().unwrap()
    }

    // Parse arguments
    let opt = Opt::from_args();
    let cpu_detail = opt.detail_opts.cpu_detail();
    let gpu_detail = opt.detail_opts.gpu_detail();

    // Find our files
    let mut catalog = Catalog::empty("main");
    for (i, d) in opt.libdir.iter().enumerate() {
        catalog.add_drawer(DirectoryDrawer::from_directory(100 + i as i64, d)?)?;
    }
    let catalog = Arc::new(RwLock::new(catalog));
    world.insert_resource(catalog.clone());

    // Make sure various config locations exist
    let app_dirs = AppDirs::new(Some("nitrogen"), true)
        .ok_or_else(|| anyhow!("unable to find app directories"))?;
    create_dir_all(&app_dirs.config_dir)?;
    create_dir_all(&app_dirs.state_dir)?;

    // Create the game interpreter
    let mut interpreter = Interpreter::default();
    world.insert_resource(interpreter.clone());

    // We have to create the mapper immediately so that the namespace will be available to scripts.
    let mapper = EventMapper::new(&mut interpreter);
    world.insert_resource(mapper);

    // Hack so that our window APIs work, so that we can discover the system.
    // We want to do this as late as possible so that we don't have to wait long.
    // But it needs to happen before we create the window or graphics subsystems.
    input_controller.lock().wait_for_window_configuration()?;
    world.insert_resource(input_controller);
    let display_config = DisplayConfig::discover(&opt.display_opts, &os_window);

    // So that we can create the window.
    let window = Window::new(os_window, display_config, &mut interpreter)?;
    world.insert_resource(window.clone());

    // We don't technically need the window here, just the graphics configuration, and we could
    // even potentially create blind and expect the resize later. Worth looking into as it would
    // let us initialize async. The main work to do in parallel is discovering tile trees; this
    // does not technically depend on the gpu, but does because it lives with terrain creation,
    // which does need the gpu for resource creation. Probably worth looking to parallelize this
    // as we could potentially half our startup time.
    let (gpu, mut frame_graph) = build_frame_graph(
        cpu_detail,
        gpu_detail,
        &app_dirs,
        &catalog.read(),
        &mut window.write(),
        &mut interpreter,
        &mut world,
    )?;

    // Create rest of game resources
    let initial_utc = Utc.ymd(1964, 2, 24).and_hms(12, 0, 0);
    let orrery = Orrery::new(initial_utc, &mut interpreter)?;
    world.insert_resource(orrery.clone());
    let timeline = Timeline::new(&mut interpreter);
    world.insert_resource(timeline);

    let system = System::new(
        &frame_graph.widgets(),
        //&resources.get::<Arc<RwLock<WidgetBuffer>>>().unwrap().read(),
        &mut interpreter,
    )?;
    world.insert_resource(system.clone());

    // But we need at least a camera and controller before the sim is ready to run.
    let camera = Camera::install(
        radians!(PI / 2.0),
        window.read().render_aspect_ratio(),
        meters!(0.5),
        &mut interpreter,
    )?;
    let arcball = ArcBallCamera::install(&mut interpreter)?;
    // let _player_id = world.push((
    //     WorldSpaceFrame::default(),
    //     ArcBallController::new(arcball.clone()),
    //     CameraComponent::new(camera.clone()),
    // ));
    let _player_ent = world
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
    fn sim_update_current_time(mut now: ResMut<Instant>) {
        *now += STEP;
    }

    fn sim_read_input_events(
        input_controller: Res<Arc<Mutex<InputController>>>,
        mut input_events: ResMut<InputEventVec>,
    ) {
        // Note: if we are stopping, the queue might have shut down, in which case we don't
        // really care about the output anymore.
        *input_events = if let Ok(events) = input_controller.lock().poll_input_events() {
            events
        } else {
            InputEventVec::new()
        };
    }

    fn sim_widgets_handle_input_events(
        events: Res<InputEventVec>,
        mut input_focus: ResMut<InputFocus>,
        window: Res<Arc<RwLock<Window>>>,
        mut interpreter: ResMut<Interpreter>,
        widgets: Res<Arc<RwLock<WidgetBuffer>>>,
    ) {
        widgets
            .write()
            .handle_events(&events, *input_focus, &mut interpreter, &window.read())
            .expect("Widgets::handle_events");

        let widgets = widgets.read();
        if events
            .iter()
            .any(|event| widgets.is_toggle_terminal_event(event))
        {
            input_focus.toggle_terminal();
        }
    }

    fn sim_mapper_handle_input_events(
        events: Res<InputEventVec>,
        input_focus: Res<InputFocus>,
        mut interpreter: ResMut<Interpreter>,
        mapper: Res<Arc<RwLock<EventMapper>>>,
    ) {
        mapper
            .write()
            .handle_events(&events, *input_focus, &mut interpreter)
            .expect("EventMapper::handle_events");
    }

    fn sim_apply_arcball_updates(mut query: Query<(&mut ArcBallController, &mut WorldSpaceFrame)>) {
        for (mut arcball, mut frame) in query.iter_mut() {
            arcball.apply_input_state();
            *frame = arcball.world_space_frame();
        }
    }

    fn sim_update_camera_frame(mut query: Query<(&WorldSpaceFrame, &mut CameraComponent)>) {
        for (frame, mut camera) in query.iter_mut() {
            camera.apply_input_state();
            camera.update_frame(frame);
        }
    }

    fn sim_orrery_step_time(orrery: Res<Arc<RwLock<Orrery>>>) {
        orrery
            .write()
            .step_time(ChronoDuration::from_std(STEP).expect("in range"));
    }

    fn sim_timeline_animate(now: Res<Instant>, timeline: Res<Arc<RwLock<Timeline>>>) {
        timeline.write().step_time(&now);
    }

    let mut sim_fixed_schedule = Schedule::default();
    sim_fixed_schedule.add_stage(
        "sim_serial",
        SystemStage::single_threaded()
            .with_system(sim_update_current_time.system())
            .with_system(sim_read_input_events.system())
            .with_system(sim_widgets_handle_input_events.system())
            .with_system(sim_mapper_handle_input_events.system())
            .with_system(sim_apply_arcball_updates.system())
            .with_system(sim_update_camera_frame.system())
            .with_system(sim_orrery_step_time.system())
            .with_system(sim_timeline_animate.system()),
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
    fn update_handle_system_events(
        input_controller: Res<Arc<Mutex<InputController>>>,
        window: Res<Arc<RwLock<Window>>>,
        mut updated_config: ResMut<Option<DisplayConfig>>,
    ) {
        let events = input_controller
            .lock()
            .poll_system_events()
            .expect("poll_system_events");
        *updated_config = window.write().handle_system_events(&events);
    }

    fn update_handle_camera_aspect_change(
        mut query: Query<&mut CameraComponent>,
        updated_config: Res<Option<DisplayConfig>>,
    ) {
        for mut camera in query.iter_mut() {
            if let Some(config) = updated_config.as_ref() {
                camera.on_display_config_updated(config);
            }
        }
    }

    fn update_handle_window_config_change(
        updated_config: Res<Option<DisplayConfig>>,
        gpu: Res<Arc<RwLock<Gpu>>>,
    ) {
        if let Some(config) = updated_config.as_ref() {
            gpu.write()
                .on_display_config_changed(config)
                .expect("Gpu::on_display_config_changed");
        }
    }

    fn update_widget_track_state_changes(
        now: Res<Instant>,
        window: Res<Arc<RwLock<Window>>>,
        widgets: Res<Arc<RwLock<WidgetBuffer>>>,
    ) {
        widgets
            .write()
            .track_state_changes(*now, &window.read())
            .expect("Widgets::track_state_changes");
    }

    fn update_globals_track_state_changes(
        query: Query<&CameraComponent>,
        orrery: Res<Arc<RwLock<Orrery>>>,
        window: Res<Arc<RwLock<Window>>>,
        global_data: Res<Arc<RwLock<GlobalParametersBuffer>>>,
    ) {
        // FIXME: multiple camera support
        let mut global_data = global_data.write();
        let window = window.read();
        let orrery = orrery.read();
        for (i, camera) in query.iter().enumerate() {
            assert_eq!(i, 0);
            global_data.track_state_changes(&camera.camera(), &orrery, &window);
        }
    }

    fn update_terrain_track_state_changes(
        query: Query<&CameraComponent>,
        catalog: Res<Arc<RwLock<Catalog>>>,
        system: Res<Arc<RwLock<System>>>,
        terrain: Res<Arc<RwLock<TerrainBuffer>>>,
    ) {
        // FIXME: multiple camera support
        let mut system = system.write();
        let mut terrain = terrain.write();
        for (i, camera) in query.iter().enumerate() {
            assert_eq!(i, 0);
            let vis_camera = system.current_camera(&camera.camera());
            terrain
                .track_state_changes(&camera.camera(), vis_camera, catalog.clone())
                .expect("Terrain::track_state_changes");
        }
    }

    let mut update_frame_schedule = Schedule::default();
    update_frame_schedule.add_stage(
        "update_frame",
        SystemStage::single_threaded()
            .with_system(update_handle_system_events.system())
            .with_system(update_handle_camera_aspect_change.system())
            .with_system(update_handle_window_config_change.system())
            .with_system(update_widget_track_state_changes.system())
            .with_system(update_globals_track_state_changes.system())
            .with_system(update_terrain_track_state_changes.system()),
    );

    // We are now finished and can safely run the startup scripts / configuration.
    opt.startup_opts.on_startup(&mut interpreter)?;

    while !system.read().exit {
        // Catch monotonic sim time up to system time.
        let frame_start = Instant::now();
        while *sim_now(&world) + STEP < frame_start {
            sim_fixed_schedule.run_once(&mut world);
        }

        update_frame_schedule.run_once(&mut world);

        let mut tracker = Default::default();
        frame_graph
            .globals()
            .ensure_uploaded(&gpu.read(), &mut tracker)?;
        frame_graph
            .terrain_mut()
            .ensure_uploaded(&mut gpu.write(), &mut tracker)?;
        frame_graph.widgets_mut().ensure_uploaded(
            *sim_now(&world),
            &mut gpu.write(),
            &window.read(),
            &mut tracker,
        )?;
        if !frame_graph.run(gpu.clone(), tracker)? {
            gpu.write()
                .on_display_config_changed(window.read().config())?;
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
