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
use crate::{Extension, Runtime};
use anyhow::Result;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Clone, Debug, StructOpt)]
pub struct StartupOpts {
    /// Run a command after startup
    #[structopt(short = "C", long)]
    command: Option<String>,

    /// Run given file after startup
    #[structopt(short = "x", long)]
    execute: Option<PathBuf>,
}

impl Extension for StartupOpts {
    fn init(runtime: &mut Runtime) -> Result<()> {
        if let Ok(code) = std::fs::read_to_string("autoexec.n2o") {
            runtime.run_string(&code)?;
        }
        if let Some(opts) = runtime.maybe_resource::<StartupOpts>() {
            let opts = opts.to_owned();
            if let Some(command) = opts.command.as_ref() {
                runtime.run_string(command)?;
            }
            if let Some(exec_file) = opts.execute.as_ref() {
                match std::fs::read_to_string(exec_file) {
                    Ok(code) => {
                        runtime.run_string(&code)?;
                    }
                    Err(e) => {
                        println!("Read file for {:?}: {}", exec_file, e);
                    }
                }
            }
        }
        Ok(())
    }
}
