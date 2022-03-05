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

use anyhow::Result;
use atmosphere::{Precompute, TableHelpers};
use futures::executor::block_on;
use gpu::Gpu;
use input::InputSystem;
use runtime::Runtime;
use std::{fs, path::PathBuf, time::Instant};
use structopt::StructOpt;
use window::{Window, WindowBuilder};

/// Pre-compute atmosphere tables for embedding in code
#[derive(Clone, Debug, StructOpt)]
struct Opt {
    /// Write tables here
    #[structopt(short, long)]
    output: PathBuf,
}

fn main() -> Result<()> {
    let opt = Opt::from_args();
    env_logger::init();
    InputSystem::run_forever(
        opt,
        WindowBuilder::new().with_title("Build Atmosphere Tables"),
        window_main,
    )
}

fn window_main(mut runtime: Runtime) -> Result<()> {
    let opt = runtime.resource::<Opt>().to_owned();

    runtime
        .load_extension::<Window>()?
        .load_extension::<Gpu>()?;

    let precompute_start = Instant::now();
    let pcp = Precompute::new(runtime.resource::<Gpu>())?;
    let _ = pcp.build_textures(&mut runtime.resource_mut::<Gpu>())?;
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
        &mut runtime.resource_mut::<Gpu>(),
    ))?;
    println!("Write time: {:?}", write_start.elapsed());

    Ok(())
}
