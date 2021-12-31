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
use camera::{ArcBallCamera, ArcBallController, Camera, CameraComponent};
use catalog::{Catalog, DirectoryDrawer};
use chrono::{Duration as ChronoDuration, TimeZone, Utc};
use composite::CompositeRenderPass;
use event_mapper::EventMapper;
use fullscreen::FullscreenBuffer;
use global_data::GlobalParametersBuffer;
use gpu::{make_frame_graph, CpuDetailLevel, DetailLevelOpts, Gpu, GpuDetailLevel};
use input::{InputController, InputFocus, InputSystem};
use legion::*;
use measure::WorldSpaceFrame;
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
                bindings.bind("quit", "system.exit()");
                bindings.bind("Escape", "system.exit()");
                bindings.bind("q", "system.exit()");
                bindings.bind("p", "system.toggle_pin_camera(pressed)");
                // bindings.bind("l", "widget.dump_glyphs(pressed)");
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
) -> Result<(Arc<RwLock<Gpu>>, FrameGraph, Resources)> {
    let gpu = Gpu::new(window, Default::default(), interpreter)?;
    let atmosphere_buffer = AtmosphereBuffer::new(&mut gpu.write())?;
    let fullscreen_buffer = FullscreenBuffer::new(&gpu.read());
    let globals = GlobalParametersBuffer::new(gpu.read().device(), interpreter);
    let stars_buffer = Arc::new(RwLock::new(StarsBuffer::new(&gpu.read())?));
    let terrain_buffer = TerrainBuffer::new(
        catalog,
        cpu_detail,
        gpu_detail,
        &globals.read(),
        &mut gpu.write(),
        interpreter,
    )?;
    let world = WorldRenderPass::new(
        &terrain_buffer.read(),
        &atmosphere_buffer.read(),
        &stars_buffer.read(),
        &globals.read(),
        &mut gpu.write(),
        interpreter,
    )?;
    let widgets = WidgetBuffer::new(&mut gpu.write(), interpreter, &app_dirs.state_dir)?;
    let ui = UiRenderPass::new(
        &widgets.read(),
        &world.read(),
        &globals.read(),
        &mut gpu.write(),
    )?;
    let composite = Arc::new(RwLock::new(CompositeRenderPass::new(
        &ui.read(),
        &world.read(),
        &globals.read(),
        &mut gpu.write(),
    )?));

    let mut resources = Resources::default();
    resources.insert(gpu.clone());
    resources.insert(composite.clone());
    resources.insert(ui.clone());
    resources.insert(widgets.clone());
    resources.insert(world.clone());
    resources.insert(terrain_buffer.clone());
    resources.insert(atmosphere_buffer.clone());
    resources.insert(stars_buffer.clone());
    resources.insert(fullscreen_buffer.clone());
    resources.insert(globals.clone());

    globals.write().add_debug_bindings(interpreter)?;
    world.write().add_debug_bindings(interpreter)?;
    let frame_graph = FrameGraph::new(
        composite,
        ui,
        widgets,
        world,
        terrain_buffer,
        atmosphere_buffer,
        stars_buffer,
        fullscreen_buffer,
        globals,
    )?;

    Ok((gpu, frame_graph, resources))
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

    let opt = Opt::from_args();
    let cpu_detail = opt.detail_opts.cpu_detail();
    let gpu_detail = opt.detail_opts.gpu_detail();

    let app_dirs = AppDirs::new(Some("nitrogen"), true)
        .ok_or_else(|| anyhow!("unable to find app directories"))?;
    create_dir_all(&app_dirs.config_dir)?;
    create_dir_all(&app_dirs.state_dir)?;

    let mut catalog = Catalog::empty("main");
    for (i, d) in opt.libdir.iter().enumerate() {
        catalog.add_drawer(DirectoryDrawer::from_directory(100 + i as i64, d)?)?;
    }
    let catalog = Arc::new(RwLock::new(catalog));

    input_controller.lock().wait_for_window_configuration()?;

    let mut interpreter = Interpreter::default();
    let mapper = EventMapper::new(&mut interpreter);
    let display_config = DisplayConfig::discover(&opt.display_opts, &os_window);
    let window = Window::new(os_window, display_config, &mut interpreter)?;
    let mut world = World::default();

    ///////////////////////////////////////////////////////////
    let (gpu, mut frame_graph, mut resources) = build_frame_graph(
        cpu_detail,
        gpu_detail,
        &app_dirs,
        &catalog.read(),
        &mut window.write(),
        &mut interpreter,
    )?;

    let orrery = Orrery::new(Utc.ymd(1964, 2, 24).and_hms(12, 0, 0), &mut interpreter)?;
    let timeline = Timeline::new(&mut interpreter);
    let system = System::new(
        /*&frame_graph.widgets(),*/
        &resources.get::<Arc<RwLock<WidgetBuffer>>>().unwrap().read(),
        &mut interpreter,
    )?;

    resources.insert(Instant::now());
    resources.insert(InputFocus::Game);
    resources.insert(mapper);
    resources.insert(input_controller);
    resources.insert(orrery.clone());
    resources.insert(timeline);
    resources.insert(window.clone());
    resources.insert(interpreter.clone());
    resources.insert(None as Option<DisplayConfig>);

    fn sim_now(resources: &Resources) -> Instant {
        *resources.get::<Instant>().unwrap()
    }

    let camera = Camera::install(
        radians!(PI / 2.0),
        window.read().render_aspect_ratio(),
        meters!(0.5),
        &mut interpreter,
    )?;
    let arcball = ArcBallCamera::install(&mut interpreter)?;
    let _player_id = world.push((
        WorldSpaceFrame::default(),
        ArcBallController::new(arcball.clone()),
        CameraComponent::new(camera.clone()),
    ));

    opt.startup_opts.on_startup(&mut interpreter)?;

    //////////////////////////////////////////////////////////////////
    // Sim Schedule
    //
    // The simulation schedule should be used for "pure" entity to entity work and update of
    // a handful of game related resources, rather than communicating with the GPU. This generally
    // splits into two phases: per-fixed-tick resource updates and entity updates from resources.
    #[system]
    fn sim_update_current_time(#[resource] now: &mut Instant) {
        *now += STEP;
    }

    #[system]
    fn sim_handle_input_events(
        #[resource] input_controller: &Arc<Mutex<InputController>>,
        #[resource] input_focus: &InputFocus,
        #[resource] window: &Arc<RwLock<Window>>,
        #[resource] interpreter: &mut Interpreter,
        #[resource] widgets: &Arc<RwLock<WidgetBuffer>>,
        #[resource] mapper: &Arc<RwLock<EventMapper>>,
    ) {
        // Note: if we are stopping, the queue might have shut down, in which case we don't
        // really care about the output anymore.
        let events = if let Ok(events) = input_controller.lock().poll_input_events() {
            events
        } else {
            return;
        };
        mapper
            .write()
            .handle_events(&events, *input_focus, interpreter)
            .expect("EventMapper::handle_events");
        widgets
            .write()
            .handle_events(&events, *input_focus, interpreter, &window.read())
            .expect("Widgets::handle_events");
    }

    #[system(for_each)]
    fn sim_apply_arcball_updates(arcball: &mut ArcBallController, frame: &mut WorldSpaceFrame) {
        arcball.apply_input_state();
        *frame = arcball.world_space_frame();
    }

    #[system(for_each)]
    fn sim_update_camera_frame(frame: &WorldSpaceFrame, camera: &mut CameraComponent) {
        camera.apply_input_state();
        camera.update_frame(frame);
    }

    #[system]
    fn sim_orrery_step_time(#[resource] orrery: &Arc<RwLock<Orrery>>) {
        orrery
            .write()
            .step_time(ChronoDuration::from_std(STEP).expect("in range"));
    }

    #[system]
    fn sim_timeline_animate(
        #[resource] now: &Instant,
        #[resource] timeline: &Arc<RwLock<Timeline>>,
    ) {
        timeline.write().step_time(now);
    }

    let mut sim_fixed_schedule = Schedule::builder()
        .add_thread_local(sim_update_current_time_system())
        .add_thread_local(sim_handle_input_events_system())
        .add_thread_local(sim_apply_arcball_updates_system())
        .add_thread_local(sim_update_camera_frame_system())
        .add_system(sim_orrery_step_time_system())
        .add_system(sim_timeline_animate_system())
        .build();

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
    #[system]
    fn update_handle_system_events(
        #[resource] input_controller: &Arc<Mutex<InputController>>,
        #[resource] window: &Arc<RwLock<Window>>,
        #[resource] updated_config: &mut Option<DisplayConfig>,
    ) {
        let events = input_controller
            .lock()
            .poll_system_events()
            .expect("poll_system_events");
        *updated_config = window.write().handle_system_events(&events);
    }

    #[system(for_each)]
    fn update_handle_camera_aspect_change(
        camera: &mut CameraComponent,
        #[resource] updated_config: &Option<DisplayConfig>,
    ) {
        if let Some(config) = updated_config {
            camera.on_display_config_updated(config);
        }
    }

    #[system]
    fn update_handle_window_config_change(
        #[resource] updated_config: &Option<DisplayConfig>,
        #[resource] gpu: &Arc<RwLock<Gpu>>,
    ) {
        if let Some(config) = updated_config {
            gpu.write()
                .on_display_config_changed(config)
                .expect("Gpu::on_display_config_changed");
        }
    }

    #[system]
    fn update_widget_track_state_changes(
        #[resource] now: &Instant,
        #[resource] window: &Arc<RwLock<Window>>,
        #[resource] widgets: &Arc<RwLock<WidgetBuffer>>,
    ) {
        widgets
            .write()
            .track_state_changes(*now, &window.read())
            .expect("Widgets::track_state_changes");
    }

    // #[system(for_each)]
    // fn update_globals_track_state_changes(
    //     camera: &mut CameraComponent,
    //     #[resource] orrery: &Arc<RwLock<Orrery>>,
    //     #[resource] window: &Arc<RwLock<Window>>,
    //     #[resource] global_data: &Arc<RwLock<GlobalParametersBuffer>>,
    // ) {
    // }

    let mut update_frame_schedule = Schedule::builder()
        .add_thread_local(update_handle_system_events_system())
        .add_thread_local(update_handle_camera_aspect_change_system())
        .add_thread_local(update_handle_window_config_change_system())
        .add_system(update_widget_track_state_changes_system())
        .build();

    //////////////////////////////////////////////////////////////////
    // Ensure (Graphics) Updated
    //
    // Write current state to GPU

    while !system.read().exit {
        // Catch monotonic sim time up to system time.
        let system_now = Instant::now();
        while sim_now(&resources) + STEP < system_now {
            sim_fixed_schedule.execute(&mut world, &mut resources);
        }

        update_frame_schedule.execute(&mut world, &mut resources);

        {
            frame_graph.globals_mut().track_state_changes(
                &camera.read(),
                &orrery.read(),
                &window.read(),
            );
            let mut sys_lock = system.write();
            let vis_camera = sys_lock.current_camera(&camera.read());
            frame_graph.terrain_mut().track_state_changes(
                &camera.read(),
                vis_camera,
                catalog.clone(),
            )?;
        }

        let now = Instant::now();
        let mut tracker = Default::default();
        frame_graph
            .globals_mut()
            .ensure_uploaded(&gpu.read(), &mut tracker)?;
        frame_graph
            .terrain_mut()
            .ensure_uploaded(&mut gpu.write(), &mut tracker)?;
        frame_graph.widgets_mut().ensure_uploaded(
            now,
            &mut gpu.write(),
            &window.read(),
            &mut tracker,
        )?;
        if !frame_graph.run(gpu.clone(), tracker)? {
            gpu.write()
                .on_display_config_changed(window.read().config())?;
        }

        system.write().track_visible_state(
            system_now, // compute frame times from actual elapsed time
            &orrery.read(),
            &arcball.read(),
            &camera.read(),
        )?;
    }

    Ok(())
}
