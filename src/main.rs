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
use animate::Timeline;
use anyhow::{anyhow, Result};
use atmosphere::AtmosphereBuffer;
use camera::{ArcBallCamera, Camera};
use catalog::{Catalog, DirectoryDrawer};
use chrono::{Duration as ChronoDuration, TimeZone, Utc};
use composite::CompositeRenderPass;
use fullscreen::FullscreenBuffer;
use global_data::GlobalParametersBuffer;
use gpu::{make_frame_graph, CpuDetailLevel, DetailLevelOpts, Gpu, GpuDetailLevel};
use input::{InputController, InputSystem};
use legion::world::World;
use nitrous::{Interpreter, StartupOpts, Value};
use nitrous_injector::{inject_nitrous_module, method, NitrousModule};
use orrery::Orrery;
use parking_lot::RwLock;
use platform_dirs::AppDirs;
use stars::StarsBuffer;
use std::{
    fs::create_dir_all,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use structopt::StructOpt;
use terminal_size::{terminal_size, Width};
use terrain::TerrainBuffer;
use tokio::runtime::Runtime;
use ui::UiRenderPass;
use widget::{
    Border, Color, EventMapper, Expander, Label, Labeled, PositionH, PositionV, VerticalBox,
    WidgetBuffer,
};
use window::{
    size::{LeftBound, Size},
    DisplayConfig, DisplayConfigChangeReceiver, DisplayOpts, OsWindow, Window,
};
use world::WorldRenderPass;

/// Demonstrate the capabilities of the Nitrogen engine
#[derive(Debug, StructOpt)]
#[structopt(set_term_width = if let Some((Width(w), _)) = terminal_size() { w as usize } else { 80 })]
struct Opt {
    /// Extra directories to treat as libraries
    #[structopt(short, long)]
    libdir: Vec<PathBuf>,

    // #[structopt(flatten)]
    // catalog_opts: CatalogOpts,
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
                bindings.bind("l", "widget.dump_glyphs(pressed)");
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
            .set_text(format!("FoV: {}", degrees!(arcball.camera().fov_y()),));
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
    mapper: Arc<RwLock<EventMapper>>,
    window: &mut Window,
    interpreter: &mut Interpreter,
) -> Result<(Arc<RwLock<Gpu>>, FrameGraph)> {
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
    let widgets = WidgetBuffer::new(mapper, &mut gpu.write(), interpreter, &app_dirs.state_dir)?;
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

    Ok((gpu, frame_graph))
}

fn main() -> Result<()> {
    env_logger::init();
    InputSystem::run_forever(simulation_main)
}

fn simulation_main(os_window: OsWindow, input_controller: &mut InputController) -> Result<()> {
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

    input_controller.wait_for_window_configuration()?;

    let mut interpreter = Interpreter::default();
    let mapper = EventMapper::new(&mut interpreter);
    let display_config = DisplayConfig::discover(&opt.display_opts, &os_window);
    let window = Window::new(
        os_window,
        input_controller,
        display_config,
        &mut interpreter,
    )?;
    let async_rt = Arc::new(Runtime::new()?);
    let _legion = World::default();

    ///////////////////////////////////////////////////////////
    let (_gpu, mut frame_graph) = build_frame_graph(
        cpu_detail,
        gpu_detail,
        &app_dirs,
        &catalog.read(),
        mapper,
        &mut window.write(),
        &mut interpreter,
    )?;
    let _async_rt = async_rt.clone();
    let _window = window.clone();
    let _frame_graph = frame_graph.clone();
    let render_handle = std::thread::spawn(move || {
        render_main(_async_rt, _window, _gpu, _frame_graph).unwrap();
    });

    let orrery = Orrery::new(Utc.ymd(1964, 2, 24).and_hms(12, 0, 0), &mut interpreter)?;
    let arcball = ArcBallCamera::new(meters!(0.5), &mut window.write(), &mut interpreter)?;
    let timeline = Timeline::new(&mut interpreter);
    let system = System::new(&frame_graph.widgets(), &mut interpreter)?;

    opt.startup_opts.on_startup(&mut interpreter)?;

    const STEP: Duration = Duration::from_micros(16_666);
    let mut now = Instant::now();
    while !system.read().exit {
        // Catch up to system time.
        let next_now = Instant::now();
        while now + STEP < next_now {
            orrery.write().step_time(ChronoDuration::from_std(STEP)?);
            timeline.write().step_time(&now)?;
            now += STEP;
        }
        now = next_now;

        {
            arcball.write().track_state_changes();
            frame_graph.widgets_mut().track_state_changes(
                now,
                &input_controller.poll_events()?,
                &window.read(),
                interpreter.clone(),
            )?;
            frame_graph.globals_mut().track_state_changes(
                arcball.read().camera(),
                &orrery.read(),
                &window.read(),
            );
            let mut sys_lock = system.write();
            let vis_camera = sys_lock.current_camera(arcball.read_recursive().camera());
            frame_graph.terrain_mut().track_state_changes(
                arcball.read_recursive().camera(),
                vis_camera,
                catalog.clone(),
            )?;
        }

        system
            .write()
            .track_visible_state(now, &orrery.read(), &arcball.read())?;
    }

    window.write().closing = true;
    render_handle.join().ok();

    Ok(())
}

fn render_main(
    async_rt: Arc<Runtime>,
    window: Arc<RwLock<Window>>,
    gpu: Arc<RwLock<Gpu>>,
    mut frame_graph: FrameGraph,
) -> Result<()> {
    while !window.read().closing {
        let now = Instant::now();
        let mut tracker = Default::default();
        frame_graph
            .globals_mut()
            .ensure_uploaded(&gpu.read(), &mut tracker)?;
        frame_graph
            .terrain_mut()
            .ensure_uploaded(&async_rt, &mut gpu.write(), &mut tracker)?;
        frame_graph.widgets_mut().ensure_uploaded(
            now,
            &async_rt,
            &mut gpu.write(),
            &window.read(),
            &mut tracker,
        )?;
        if !frame_graph.run(gpu.clone(), tracker)? {
            gpu.write()
                .on_display_config_changed(window.read().config())?;
        }
    }

    Ok(())
}
