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
use anyhow::{bail, Error, Result};
use std::str::FromStr;
use structopt::StructOpt;

fn parse_level_str(s: &str) -> Result<u8> {
    Ok(match s {
        "low" | "lo" | "0" => 0,
        "medium" | "med" | "1" => 1,
        "high" | "hi" | "2" => 2,
        "ultra" | "max" | "3" => 3,
        _ => bail!("unrecognized detail level; expected low, medium, high, or ultra"),
    })
}

/// Indicates a set of features (delegated to the crate) that sets
/// the level of detail (and thus cost) for various CPU-related
/// functionality. This of course varies machine-to-machine, so
/// fine grained adjustment is also possible; this just sets some
/// baselines.
#[derive(Clone, Copy, Debug)]
pub enum CpuDetailLevel {
    Low,
    Medium,
    High,
    Ultra,
}

impl CpuDetailLevel {
    /// TODO: auto-detect based on cpuid, bogomips, etc
    pub fn detect() -> Self {
        if cfg!(debug_assertions) {
            Self::Low
        } else {
            Self::High
        }
    }
}

impl FromStr for CpuDetailLevel {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match parse_level_str(s)? {
            0 => Self::Low,
            1 => Self::Medium,
            2 => Self::High,
            3 => Self::Ultra,
            _ => panic!("invalid level parse result"),
        })
    }
}

/// As for CpuDetailLevel, but for GPU-related operations.
#[derive(Clone, Copy, Debug)]
pub enum GpuDetailLevel {
    Low,
    Medium,
    High,
    Ultra,
}

impl GpuDetailLevel {
    /// TODO: auto-detect via vendor strings, bogomips, etc
    pub fn detect() -> Self {
        if cfg!(debug_assertions) {
            Self::Low
        } else {
            Self::High
        }
    }
}

impl FromStr for GpuDetailLevel {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match parse_level_str(s)? {
            0 => Self::Low,
            1 => Self::Medium,
            2 => Self::High,
            3 => Self::Ultra,
            _ => panic!("invalid level parse result"),
        })
    }
}

#[derive(Clone, Debug, StructOpt)]
pub struct DetailLevelOpts {
    /// Set the CPU detail level (low, medium, high, or ultra)
    #[structopt(long)]
    cpu_detail: Option<CpuDetailLevel>,

    /// Set the GPU detail level (low, medium, high, or ultra)
    #[structopt(long)]
    gpu_detail: Option<GpuDetailLevel>,
}

impl DetailLevelOpts {
    pub fn cpu_detail(&self) -> CpuDetailLevel {
        self.cpu_detail.unwrap_or_else(CpuDetailLevel::detect)
    }

    pub fn gpu_detail(&self) -> GpuDetailLevel {
        self.gpu_detail.unwrap_or_else(GpuDetailLevel::detect)
    }
}
