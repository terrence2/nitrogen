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
use chrono::prelude::*;
use command::{Bindings, CommandHandler};
use failure::Fallible;
use fullscreen::FullscreenBuffer;
use geodesy::{GeoSurface, Graticule, Target};
use global_data::GlobalParametersBuffer;
use gpu::{make_frame_graph, UploadTracker, GPU};
use input::{InputController, InputSystem};
use legion::prelude::*;
use log::trace;
use nalgebra::convert;
use orrery::Orrery;
use screen_text::ScreenTextRenderPass;
use skybox::SkyboxRenderPass;
use stars::StarsBuffer;
use std::{path::PathBuf, sync::Arc, time::Instant};
use structopt::StructOpt;
use terrain::TerrainRenderPass;
use terrain_geo::{CpuDetailLevel, GpuDetailLevel, TerrainGeoBuffer};
use text_layout::{TextAnchorH, TextAnchorV, TextLayoutBuffer, TextPositionH, TextPositionV};
use tokio::{runtime::Runtime, sync::RwLock as AsyncRwLock};
use winit::window::Window;

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

make_frame_graph!(
    FrameGraph {
        buffers: {
            atmosphere: AtmosphereBuffer,
            fullscreen: FullscreenBuffer,
            globals: GlobalParametersBuffer,
            stars: StarsBuffer,
            terrain_geo: TerrainGeoBuffer,
            text_layout: TextLayoutBuffer
        };
        renderers: [
            skybox: SkyboxRenderPass { globals, fullscreen, stars, atmosphere },
            terrain: TerrainRenderPass { globals, atmosphere, stars, terrain_geo },
            screen_text: ScreenTextRenderPass { globals, text_layout }
        ];
        passes: [
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

            draw: Render(Screen) {
                //skybox( globals, fullscreen, stars, atmosphere ),
                terrain( globals, fullscreen, atmosphere, stars, terrain_geo ),
                screen_text( globals, text_layout )
            }
        ];
    }
);

fn main() -> Fallible<()> {
    env_logger::init();

    let system_bindings = Bindings::new("map")
        .bind("terrain.toggle_wireframe", "w")?
        .bind("terrain.toggle_debug_mode", "r")?
        .bind("demo.+target_up_fast", "Shift+Up")?
        .bind("demo.+target_down_fast", "Shift+Down")?
        .bind("demo.+target_up", "Up")?
        .bind("demo.+target_down", "Down")?
        .bind("demo.decrease_gamma", "LBracket")?
        .bind("demo.increase_gamma", "RBracket")?
        .bind("demo.decrease_exposure", "Shift+LBracket")?
        .bind("demo.increase_exposure", "Shift+RBracket")?
        .bind("demo.pin_view", "p")?
        .bind("demo.exit", "Escape")?
        .bind("demo.exit", "q")?;
    InputSystem::run_forever(
        vec![
            Orrery::debug_bindings()?,
            ArcBallCamera::default_bindings()?,
            system_bindings,
        ],
        window_main,
    )
}

fn window_main(window: Window, input_controller: &InputController) -> Fallible<()> {
    let opt = Opt::from_args();

    let mut async_rt = Runtime::new()?;
    let mut legion = World::default();

    let mut catalog = Catalog::empty();
    for (i, d) in opt.libdir.iter().enumerate() {
        catalog.add_drawer(DirectoryDrawer::from_directory(100 + i as i64, d)?)?;
    }

    let mut gpu = GPU::new(&window, Default::default())?;

    let (cpu_detail, gpu_detail) = if cfg!(debug_assertions) {
        (CpuDetailLevel::Low, GpuDetailLevel::Low)
    } else {
        (CpuDetailLevel::Medium, GpuDetailLevel::High)
    };

    ///////////////////////////////////////////////////////////
    let atmosphere_buffer = AtmosphereBuffer::new(opt.no_cache, &mut gpu)?;
    let fullscreen_buffer = FullscreenBuffer::new(&gpu)?;
    let globals_buffer = GlobalParametersBuffer::new(gpu.device())?;
    let stars_buffer = StarsBuffer::new(&gpu)?;
    let terrain_geo_buffer =
        TerrainGeoBuffer::new(&catalog, cpu_detail, gpu_detail, &globals_buffer, &mut gpu)?;
    let text_layout_buffer = TextLayoutBuffer::new(&mut gpu)?;
    let catalog = Arc::new(AsyncRwLock::new(catalog));
    let mut frame_graph = FrameGraph::new(
        &mut legion,
        &mut gpu,
        atmosphere_buffer,
        fullscreen_buffer,
        globals_buffer,
        stars_buffer,
        terrain_geo_buffer,
        text_layout_buffer,
    )?;
    ///////////////////////////////////////////////////////////

    let fps_handle = frame_graph
        .text_layout()
        .add_screen_text("", "", &gpu)?
        .with_color(&[1f32, 0f32, 0f32, 1f32])
        .with_horizontal_position(TextPositionH::Left)
        .with_horizontal_anchor(TextAnchorH::Left)
        .with_vertical_position(TextPositionV::Top)
        .with_vertical_anchor(TextAnchorV::Top)
        .handle();

    let mut orrery = Orrery::new(Utc.ymd(1964, 2, 24).and_hms(12, 0, 0));

    /*
    let mut camera = UfoCamera::new(gpu.aspect_ratio(), 0.1f64, 3.4e+38f64);
    camera.set_position(6_378.0, 0.0, 0.0);
    camera.set_rotation(&Vector3::new(0.0, 0.0, 1.0), PI / 2.0);
    camera.apply_rotation(&Vector3::new(0.0, 1.0, 0.0), PI);
    */

    let mut arcball = ArcBallCamera::new(gpu.aspect_ratio(), meters!(0.5));

    // everest: 27.9880704,86.9245623
    arcball.set_target(Graticule::<GeoSurface>::new(
        degrees!(27.9880704),
        degrees!(-86.9245623), // FIXME: wat?
        meters!(8000.),
    ));
    arcball.set_eye_relative(Graticule::<Target>::new(
        degrees!(11.5),
        degrees!(869.5),
        meters!(67668.5053),
    ))?;
    // ISS: 408km up
    // arcball.set_target(Graticule::<GeoSurface>::new(
    //     degrees!(27.9880704),
    //     degrees!(-86.9245623), // FIXME: wat?
    //     meters!(408_000.),
    // ));
    // arcball.set_eye_relative(Graticule::<Target>::new(
    //     degrees!(58),
    //     degrees!(668.0),
    //     meters!(1308.7262),
    // ))?;

    let mut tone_gamma = 2.2f32;
    let mut tone_exposure = 10e-5f32;
    let mut is_camera_pinned = false;
    let mut camera_double = arcball.camera().to_owned();
    let mut target_vec = meters!(0f64);
    loop {
        let loop_start = Instant::now();

        for command in input_controller.poll()? {
            frame_graph.handle_command(&command);
            arcball.handle_command(&command)?;
            orrery.handle_command(&command)?;
            match command.command() {
                "+target_up" => target_vec = meters!(1),
                "-target_up" => target_vec = meters!(0),
                "+target_down" => target_vec = meters!(-1),
                "-target_down" => target_vec = meters!(0),
                "+target_up_fast" => target_vec = meters!(100),
                "-target_up_fast" => target_vec = meters!(0),
                "+target_down_fast" => target_vec = meters!(-100),
                "-target_down_fast" => target_vec = meters!(0),
                "decrease_gamma" => tone_gamma /= 1.1,
                "increase_gamma" => tone_gamma *= 1.1,
                "decrease_exposure" => tone_exposure /= 1.1,
                "increase_exposure" => tone_exposure *= 1.1,
                "pin_view" => {
                    println!("eye_rel: {}", arcball.get_eye_relative());
                    println!("target:  {}", arcball.get_target());
                    is_camera_pinned = !is_camera_pinned
                }
                // system bindings
                "window-close" | "window-destroy" | "exit" => return Ok(()),
                "resize" => {
                    gpu.note_resize(&window);
                    frame_graph.terrain_geo.note_resize(&gpu);
                    arcball.camera_mut().set_aspect_ratio(gpu.aspect_ratio());
                }
                "cursor-move" => {}
                "mouse-move" => {}
                _ => trace!("unhandled command: {}", command.full(),),
            }
        }
        let mut g = arcball.get_target();
        g.distance += target_vec;
        if g.distance < meters!(0f64) {
            g.distance = meters!(0f64);
        }
        arcball.set_target(g);

        arcball.think();
        if !is_camera_pinned {
            camera_double = arcball.camera().to_owned();
        }

        let mut tracker = Default::default();
        frame_graph.globals().make_upload_buffer(
            arcball.camera(),
            tone_gamma,
            tone_exposure,
            &gpu,
            &mut tracker,
        )?;
        frame_graph.atmosphere().make_upload_buffer(
            convert(orrery.sun_direction()),
            &gpu,
            &mut tracker,
        )?;
        frame_graph.terrain_geo().make_upload_buffer(
            arcball.camera(),
            &camera_double,
            catalog.clone(),
            &mut async_rt,
            &mut gpu,
            &mut tracker,
        )?;
        frame_graph
            .text_layout()
            .make_upload_buffer(&gpu, &mut tracker)?;
        if !frame_graph.run(&mut gpu, tracker)? {
            gpu.note_resize(&window);
            frame_graph.terrain_geo.note_resize(&gpu);
            arcball.camera_mut().set_aspect_ratio(gpu.aspect_ratio());
        }

        let frame_time = loop_start.elapsed();
        let ts = format!(
            "eye_rel: {} | tgt: {} | asl: {}, fov: {} || Date: {:?} || frame: {}.{}ms",
            arcball.get_eye_relative(),
            arcball.get_target(),
            g.distance,
            degrees!(arcball.camera().fov_y()),
            orrery.get_time(),
            frame_time.as_secs() * 1000 + u64::from(frame_time.subsec_millis()),
            frame_time.subsec_micros(),
        );
        fps_handle.grab(frame_graph.text_layout()).set_span(&ts);
    }
}
