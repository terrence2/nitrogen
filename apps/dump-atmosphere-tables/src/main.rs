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

// All code in this module is heavily inspired by -- and all too
// frequently directly copied from -- the most excellent:
//     https://ebruneton.github.io/precomputed_atmospheric_scattering/
// Which is:
//     Copyright (c) 2017 Eric Bruneton
// All errors and omissions below were introduced in transcription
// to Rust/Vulkan/wgpu and are not reflective of the high quality of the
// original work in any way.
use anyhow::Result;
use atmosphere::{Precompute, TableHelpers};
use futures::executor::block_on;
use gpu::Gpu;
use input::{InputController, InputSystem};
use nitrous::Interpreter;
use parking_lot::Mutex;
use std::{fs, path::PathBuf, sync::Arc, time::Instant};
use structopt::StructOpt;
use window::{DisplayConfig, DisplayOpts, Window};
use winit::window::Window as OsWindow;

/// Demonstrate the capabilities of the Nitrogen engine
#[derive(Debug, StructOpt)]
struct Opt {
    /// Extra directories to treat as libraries
    #[structopt(short, long)]
    output: PathBuf,
}

fn main() -> Result<()> {
    env_logger::init();
    InputSystem::run_forever(window_main)
}

fn window_main(os_window: OsWindow, _input_controller: Arc<Mutex<InputController>>) -> Result<()> {
    let opt = Opt::from_args();
    let mut interpreter = Interpreter::default();

    let display_config = DisplayConfig::discover(&DisplayOpts::default(), &os_window);
    let window = Window::new(os_window, display_config, &mut interpreter)?;
    let gpu = Gpu::new(&mut window.write(), Default::default(), &mut interpreter)?;

    let precompute_start = Instant::now();
    let pcp = Precompute::new(&gpu.read())?;
    let _ = pcp.build_textures(&mut gpu.write())?;
    println!("Precompute time: {:?}", precompute_start.elapsed());

    let write_start = Instant::now();
    let _ = fs::create_dir(&opt.output);
    let mut transmittance_path = opt.output.clone();
    transmittance_path.push("solar_transmittance.wgpu.bin");
    let mut irradiance_path = opt.output.clone();
    irradiance_path.push("solar_irradiance.wgpu.bin");
    let mut scattering_path = opt.output.clone();
    scattering_path.push("solar_scattering.wgpu.bin");
    let mut single_mie_scattering_path = opt.output;
    single_mie_scattering_path.push("solar_single_mie_scattering.wgpu.bin");
    block_on(TableHelpers::write_textures(
        pcp.transmittance_texture(),
        &transmittance_path,
        pcp.irradiance_texture(),
        &irradiance_path,
        pcp.scattering_texture(),
        &scattering_path,
        pcp.single_mie_scattering_texture(),
        &single_mie_scattering_path,
        &mut gpu.write(),
    ))?;
    println!("Write time: {:?}", write_start.elapsed());

    Ok(())
}
