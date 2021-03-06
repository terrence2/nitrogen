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
use atmosphere::AtmosphereBuffer;
use camera::ArcBallCamera;
use catalog::{Catalog, DirectoryDrawer};
use chrono::{TimeZone, Utc};
use command::Bindings as LegacyBindings;
use composite::CompositeRenderPass;
use failure::Fallible;
use fullscreen::FullscreenBuffer;
use geodesy::{GeoSurface, Graticule, Target};
use global_data::GlobalParametersBuffer;
use gpu::{make_frame_graph, GPU};
use input::{InputController, InputSystem};
use legion::world::World;
use log::trace;
use nalgebra::convert;
use nitrous::{Interpreter, Value};
use nitrous_injector::{inject_nitrous_module, method, NitrousModule};
use orrery::Orrery;
use parking_lot::RwLock;
use stars::StarsBuffer;
use std::{path::PathBuf, sync::Arc, time::Instant};
use structopt::StructOpt;
use terrain_geo::{CpuDetailLevel, GpuDetailLevel, TerrainGeoBuffer};
use tokio::{runtime::Runtime, sync::RwLock as AsyncRwLock};
use ui::UiRenderPass;
use widget::{Color, EventMapper, Label, PositionH, PositionV, Terminal, WidgetBuffer};
use winit::window::Window;
use world::WorldRenderPass;

/// Show the contents of an MM file
#[derive(Debug, StructOpt)]
struct Opt {
    /// Extra directories to treat as libraries
    #[structopt(short, long)]
    libdir: Vec<PathBuf>,

    /// Regenerate instead of loading cached items on startup
    #[structopt(long = "no-cache")]
    no_cache: bool,
}

#[derive(Debug, Default, NitrousModule)]
struct Demo {
    exit: bool,
}

#[inject_nitrous_module]
impl Demo {
    pub fn new(interpreter: &mut Interpreter) -> Arc<RwLock<Self>> {
        let demo = Arc::new(RwLock::new(Self::default()));
        interpreter.put_global("demo", Value::Module(demo.clone()));
        demo
    }

    pub fn add_default_bindings(&mut self, interpreter: &mut Interpreter) -> Fallible<()> {
        interpreter.interpret_once(
            r#"
                let bindings := mapper.create_bindings("demo");
                bindings.bind("quit", "demo.exit()");
                bindings.bind("Escape", "demo.exit()");
                bindings.bind("q", "demo.exit()");
            "#,
        )?;
        Ok(())
    }

    #[method]
    pub fn exit(&mut self) {
        self.exit = true;
    }
}

make_frame_graph!(
    FrameGraph {
        buffers: {
            atmosphere: AtmosphereBuffer,
            fullscreen: FullscreenBuffer,
            globals: GlobalParametersBuffer,
            stars: StarsBuffer,
            terrain_geo: TerrainGeoBuffer,
            widgets: WidgetBuffer,
            world: WorldRenderPass,
            ui: UiRenderPass,
            composite: CompositeRenderPass
        };
        passes: [
            // terrain_geo
            // Update the indices so we have correct height data to tessellate with and normal
            // and color data to accumulate.
            paint_atlas_indices: Any() { terrain_geo() },
            // Apply heights to the terrain mesh.
            tessellate: Compute() { terrain_geo() },
            // Render the terrain mesh's texcoords to an offscreen buffer.
            deferred_texture: Render(terrain_geo, deferred_texture_target) {
                terrain_geo( globals )
            },
            // Accumulate normal and color data.
            accumulate_normal_and_color: Compute() { terrain_geo( globals ) },

            // world: Flatten terrain g-buffer into the final image and mix in stars.
            render_world: Render(world, offscreen_target) {
                world( globals, fullscreen, atmosphere, stars, terrain_geo )
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

fn main() -> Fallible<()> {
    env_logger::init();

    let system_bindings = LegacyBindings::new("map")
        .bind("demo.pin_view", "p")?
        .bind("demo.toggle_terminal", "Shift+Grave")?;
    // .bind("demo.exit", "Escape")?
    // .bind("demo.exit", "q")?;
    InputSystem::run_forever(vec![system_bindings], window_main)
}

fn window_main(window: Window, input_controller: &InputController) -> Fallible<()> {
    let opt = Opt::from_args();
    let (cpu_detail, gpu_detail) = if cfg!(debug_assertions) {
        (CpuDetailLevel::Low, GpuDetailLevel::Low)
    } else {
        (CpuDetailLevel::Medium, GpuDetailLevel::High)
    };

    let mut async_rt = Runtime::new()?;
    let mut legion = World::default();

    let mut catalog = Catalog::empty();
    for (i, d) in opt.libdir.iter().enumerate() {
        catalog.add_drawer(DirectoryDrawer::from_directory(100 + i as i64, d)?)?;
    }

    let interpreter = Interpreter::new();
    let mapper = EventMapper::new(&mut interpreter.write());

    let gpu = GPU::new(&window, Default::default(), &mut interpreter.write())?;

    let orrery = Orrery::new(
        Utc.ymd(1964, 2, 24).and_hms(12, 0, 0),
        &mut interpreter.write(),
    );
    let arcball = ArcBallCamera::new(meters!(0.5), &mut gpu.write(), &mut interpreter.write());

    ///////////////////////////////////////////////////////////
    let atmosphere_buffer = Arc::new(RwLock::new(AtmosphereBuffer::new(
        opt.no_cache,
        &mut gpu.write(),
    )?));
    let fullscreen_buffer = FullscreenBuffer::new(&gpu.read());
    let globals = GlobalParametersBuffer::new(gpu.read().device(), &mut interpreter.write());
    let stars_buffer = Arc::new(RwLock::new(StarsBuffer::new(&gpu.read())?));
    let terrain_geo_buffer = TerrainGeoBuffer::new(
        &catalog,
        cpu_detail,
        gpu_detail,
        &globals.read(),
        &mut gpu.write(),
        &mut interpreter.write(),
    )?;
    let widgets = WidgetBuffer::new(&mut gpu.write())?;
    let catalog = Arc::new(AsyncRwLock::new(catalog));
    let world = WorldRenderPass::new(
        &mut gpu.write(),
        &mut interpreter.write(),
        &globals.read(),
        &atmosphere_buffer.read(),
        &stars_buffer.read(),
        &terrain_geo_buffer.read(),
    )?;
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
        &mut legion,
        &mut gpu.write(),
        atmosphere_buffer,
        fullscreen_buffer,
        globals.clone(),
        stars_buffer,
        terrain_geo_buffer,
        widgets.clone(),
        world.clone(),
        ui,
        composite,
    )?;
    ///////////////////////////////////////////////////////////

    widgets.read().root().write().add_child("mapper", mapper);

    let version_label = Label::new("Nitrogen v0.1")
        .with_color(Color::Green)
        .with_size(8.0)
        .with_pre_blended_text()
        .wrapped();
    widgets
        .read()
        .root()
        .write()
        .add_child("version", version_label)
        .set_float(PositionH::End, PositionV::Top);

    let fps_label = Label::new("fps")
        .with_color(Color::Red)
        .with_size(13.0)
        .with_pre_blended_text()
        .wrapped();
    widgets
        .read()
        .root()
        .write()
        .add_child("fps", fps_label.clone())
        .set_float(PositionH::Start, PositionV::Bottom);

    let terminal = Terminal::new(frame_graph.widgets.read().font_context())
        .with_visible(false)
        .wrapped();
    widgets
        .read()
        .root()
        .write()
        .add_child("terminal", terminal.clone())
        .set_float(PositionH::Start, PositionV::Top);

    /*
    let mut camera = UfoCamera::new(gpu.read().aspect_ratio(), 0.1f64, 3.4e+38f64);
    camera.set_position(6_378.0, 0.0, 0.0);
    camera.set_rotation(&Vector3::new(0.0, 0.0, 1.0), PI / 2.0);
    camera.apply_rotation(&Vector3::new(0.0, 1.0, 0.0), PI);
    */

    // everest: 27.9880704,86.9245623
    arcball.write().set_target(Graticule::<GeoSurface>::new(
        degrees!(27.9880704),
        degrees!(-86.9245623), // FIXME: wat?
        meters!(8000.),
    ));
    arcball.write().set_eye_relative(Graticule::<Target>::new(
        degrees!(11.5),
        degrees!(869.5),
        meters!(67668.5053),
    ))?;
    // ISS: 408km up
    // arcball.write().set_target(Graticule::<GeoSurface>::new(
    //     degrees!(27.9880704),
    //     degrees!(-86.9245623), // FIXME: wat?
    //     meters!(408_000.),
    // ));
    // arcball.write().set_eye_relative(Graticule::<Target>::new(
    //     degrees!(58),
    //     degrees!(668.0),
    //     meters!(1308.7262),
    // ))?;

    let demo = Demo::new(&mut interpreter.write());

    // let system_bindings = Bindings::new("map")
    //     .bind("demo.pin_view", "p")?
    //     .bind("demo.toggle_terminal", "Shift+Grave")?
    {
        let interp = &mut interpreter.write();
        orrery.write().add_default_bindings(interp)?;
        arcball.write().add_default_bindings(interp)?;
        globals.write().add_default_bindings(interp)?;
        world.write().add_default_bindings(interp)?;
        demo.write().add_default_bindings(interp)?;
    }

    let mut is_camera_pinned = false;
    let mut camera_double = arcball.read().camera().to_owned();
    let mut show_terminal = false;
    while !demo.read().exit {
        let loop_start = Instant::now();

        widgets
            .write()
            .handle_events(&input_controller.poll_events()?, &mut interpreter.write())?;

        for command in input_controller.poll_commands()? {
            match command.full() {
                "demo.pin_view" => {
                    println!("eye_rel: {}", arcball.read().get_eye_relative());
                    println!("target:  {}", arcball.read().get_target());
                    is_camera_pinned = !is_camera_pinned
                }
                "demo.toggle_terminal" => {
                    show_terminal = !show_terminal;
                    terminal.write().set_visible(show_terminal);
                }
                _ => trace!("unhandled command: {}", command.full(),),
            }
        }

        arcball.write().think();
        if !is_camera_pinned {
            camera_double = arcball.read().camera().to_owned();
        }

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
        frame_graph.terrain_geo_mut().make_upload_buffer(
            arcball.read().camera(),
            &camera_double,
            catalog.clone(),
            &mut async_rt,
            &mut gpu.write(),
            &mut tracker,
        )?;
        frame_graph
            .widgets
            .write()
            .make_upload_buffer(&gpu.read(), &mut tracker)?;
        if !frame_graph.run(&mut gpu.write(), tracker)? {
            let sz = gpu.read().physical_size();
            gpu.write().on_resize(sz.width as i64, sz.height as i64)?;
        }

        let frame_time = loop_start.elapsed();
        let ts = format!(
            "eye_rel: {} | tgt: {} | asl: {}, fov: {} || Date: {:?} || frame: {}.{}ms",
            arcball.read().get_eye_relative(),
            arcball.read().get_target(),
            arcball.read().get_target().distance,
            degrees!(arcball.read().camera().fov_y()),
            orrery.read().get_time(),
            frame_time.as_secs() * 1000 + u64::from(frame_time.subsec_millis()),
            frame_time.subsec_micros(),
        );
        fps_label.write().set_text(ts);
    }

    Ok(())
}
