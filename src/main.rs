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
use anyhow::Result;
use atmosphere::AtmosphereBuffer;
use camera::{ArcBallCamera, Camera};
use catalog::{Catalog, DirectoryDrawer};
use chrono::{Duration as ChronoDuration, TimeZone, Utc};
use composite::CompositeRenderPass;
use fullscreen::FullscreenBuffer;
use global_data::GlobalParametersBuffer;
use gpu::{
    make_frame_graph,
    size::{AbsSize, LeftBound, Size},
    Gpu,
};
use input::{InputController, InputSystem};
use legion::world::World;
use nalgebra::convert;
use nitrous::{Interpreter, Value};
use nitrous_injector::{inject_nitrous_module, method, NitrousModule};
use orrery::Orrery;
use parking_lot::RwLock;
use stars::StarsBuffer;
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use structopt::StructOpt;
use terrain::{CpuDetailLevel, GpuDetailLevel, TerrainBuffer};
use tokio::{runtime::Runtime, sync::RwLock as AsyncRwLock};
use ui::UiRenderPass;
use widget::{
    Border, Color, Expander, Extent, Label, Labeled, PositionH, PositionV, VerticalBox,
    WidgetBuffer,
};
use winit::window::Window;
use world::WorldRenderPass;

/// Demonstrate the capabilities of the Nitrogen engine
#[derive(Debug, StructOpt)]
struct Opt {
    /// Extra directories to treat as libraries
    #[structopt(short, long)]
    libdir: Vec<PathBuf>,

    /// Run a command after startup
    #[structopt(short, long)]
    command: Option<String>,

    /// Run given file after startup
    #[structopt(short, long)]
    execute: Option<PathBuf>,
}

#[derive(Debug, NitrousModule)]
struct System {
    exit: bool,
    pin_camera: bool,
    camera: Camera,
}

#[inject_nitrous_module]
impl System {
    pub fn new(interpreter: &mut Interpreter) -> Arc<RwLock<Self>> {
        let system = Arc::new(RwLock::new(Self {
            exit: false,
            pin_camera: false,
            camera: Default::default(),
        }));
        interpreter.put_global("system", Value::Module(system.clone()));
        system
    }

    pub fn add_default_bindings(&mut self, interpreter: &mut Interpreter) -> Result<()> {
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
            atmosphere: AtmosphereBuffer,
            fullscreen: FullscreenBuffer,
            globals: GlobalParametersBuffer,
            stars: StarsBuffer,
            terrain: TerrainBuffer,
            widgets: WidgetBuffer,
            world: WorldRenderPass,
            ui: UiRenderPass,
            composite: CompositeRenderPass
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

fn main() -> Result<()> {
    env_logger::init();
    InputSystem::run_forever(window_main)
}

fn window_main(window: Window, input_controller: &InputController) -> Result<()> {
    let opt = Opt::from_args();
    let (cpu_detail, gpu_detail) = if cfg!(debug_assertions) {
        (CpuDetailLevel::Low, GpuDetailLevel::Low)
    } else {
        (CpuDetailLevel::Medium, GpuDetailLevel::High)
    };

    let mut async_rt = Runtime::new()?;
    let mut _legion = World::default();

    let mut catalog = Catalog::empty("main");
    for (i, d) in opt.libdir.iter().enumerate() {
        catalog.add_drawer(DirectoryDrawer::from_directory(100 + i as i64, d)?)?;
    }

    let interpreter = Interpreter::new();
    let timeline = Timeline::new(&mut interpreter.write());
    let gpu = Gpu::new(window, Default::default(), &mut interpreter.write())?;

    let orrery = Orrery::new(
        Utc.ymd(1964, 2, 24).and_hms(12, 0, 0),
        &mut interpreter.write(),
    );
    let arcball = ArcBallCamera::new(meters!(0.5), &mut gpu.write(), &mut interpreter.write());

    ///////////////////////////////////////////////////////////
    let atmosphere_buffer = AtmosphereBuffer::new(&mut gpu.write())?;
    let fullscreen_buffer = FullscreenBuffer::new(&gpu.read());
    let globals = GlobalParametersBuffer::new(gpu.read().device(), &mut interpreter.write());
    let stars_buffer = Arc::new(RwLock::new(StarsBuffer::new(&gpu.read())?));
    let terrain_buffer = TerrainBuffer::new(
        &catalog,
        cpu_detail,
        gpu_detail,
        &globals.read(),
        &mut gpu.write(),
        &mut interpreter.write(),
    )?;
    let catalog = Arc::new(AsyncRwLock::new(catalog));
    let world = WorldRenderPass::new(
        &mut gpu.write(),
        &mut interpreter.write(),
        &globals.read(),
        &atmosphere_buffer.read(),
        &stars_buffer.read(),
        &terrain_buffer.read(),
    )?;
    let widgets = WidgetBuffer::new(&mut gpu.write(), &mut interpreter.write())?;
    let ui = UiRenderPass::new(
        &mut gpu.write(),
        &globals.read(),
        &widgets.read(),
        &world.read(),
    )?;
    let composite = Arc::new(RwLock::new(CompositeRenderPass::new(
        &mut gpu.write(),
        &globals.read(),
        &world.read(),
        &ui.read(),
    )?));

    let mut frame_graph = FrameGraph::new(
        atmosphere_buffer,
        fullscreen_buffer,
        globals.clone(),
        stars_buffer,
        terrain_buffer,
        widgets.clone(),
        world.clone(),
        ui,
        composite,
    )?;

    let system = System::new(&mut interpreter.write());

    ///////////////////////////////////////////////////////////
    // UI Setup
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
        .read()
        .root_container()
        .write()
        .add_child("controls", expander)
        .set_float(PositionH::End, PositionV::Top);

    let fps_label = Label::new("")
        .with_font(widgets.read().font_context().font_id_for_name("sans"))
        .with_color(Color::Red)
        .with_size(Size::from_pts(13.0))
        .with_pre_blended_text()
        .wrapped();
    widgets
        .read()
        .root_container()
        .write()
        .add_child("fps", fps_label.clone())
        .set_float(PositionH::Start, PositionV::Bottom);

    {
        let interp = &mut interpreter.write();
        gpu.write().add_default_bindings(interp)?;
        orrery.write().add_default_bindings(interp)?;
        arcball.write().add_default_bindings(interp)?;
        globals.write().add_default_bindings(interp)?;
        world.write().add_default_bindings(interp)?;
        system.write().add_default_bindings(interp)?;
    }

    if let Some(command) = opt.command.as_ref() {
        let rv = interpreter.write().interpret_once(command)?;
        println!("{}", rv);
    }

    if let Ok(code) = std::fs::read_to_string("autoexec.n2o") {
        let rv = interpreter.write().interpret_once(&code);
        println!("Execution Completed: {:?}", rv);
    }

    if let Some(exec_file) = opt.execute {
        match std::fs::read_to_string(&exec_file) {
            Ok(code) => {
                let rv = interpreter.write().interpret_async(code);
                println!("Execution Completed: {:?}", rv);
            }
            Err(e) => {
                println!("Read file for {:?}: {}", exec_file, e);
            }
        }
    }

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
            let logical_extent: Extent<AbsSize> = gpu.read().logical_size().into();
            let scale_factor = { gpu.read().scale_factor() };
            frame_graph.widgets.write().handle_events(
                now,
                &input_controller.poll_events()?,
                interpreter.clone(),
                scale_factor,
                logical_extent,
            )?;
            frame_graph
                .widgets
                .write()
                .layout_for_frame(now, &mut gpu.write())?;
        }

        arcball.write().think();

        let mut tracker = Default::default();
        frame_graph.globals().make_upload_buffer(
            arcball.read().camera(),
            &gpu.read(),
            &mut tracker,
        )?;
        frame_graph.atmosphere().make_upload_buffer(
            convert(orrery.read().sun_direction()),
            &gpu.read(),
            &mut tracker,
        )?;
        frame_graph.terrain_mut().make_upload_buffer(
            arcball.read().camera(),
            system.write().current_camera(arcball.read().camera()),
            catalog.clone(),
            &mut async_rt,
            &mut gpu.write(),
            &mut tracker,
        )?;
        frame_graph.widgets.write().make_upload_buffer(
            now,
            &mut gpu.write(),
            &async_rt,
            &mut tracker,
        )?;
        if !frame_graph.run(&mut gpu.write(), tracker)? {
            let sz = gpu.read().physical_size();
            gpu.write().on_resize(sz.width as i64, sz.height as i64)?;
        }

        sim_time
            .write()
            .set_text(format!("Date: {}", orrery.read().get_time()));
        camera_direction
            .write()
            .set_text(format!("Eye: {}", arcball.read().eye()));
        camera_position
            .write()
            .set_text(format!("Position: {}", arcball.read().target(),));
        camera_fov.write().set_text(format!(
            "FoV: {}",
            degrees!(arcball.read().camera().fov_y()),
        ));
        let frame_time = now.elapsed();
        let ts = format!(
            "frame: {}.{}ms",
            frame_time.as_secs() * 1000 + u64::from(frame_time.subsec_millis()),
            frame_time.subsec_micros(),
        );
        fps_label.write().set_text(ts);
    }

    Ok(())
}
