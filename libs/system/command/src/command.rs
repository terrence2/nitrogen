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
use failure::{bail, ensure, Fallible};
use smallvec::{smallvec, SmallVec};
use std::path::PathBuf;
use winit::{
    dpi::{LogicalPosition, LogicalSize},
    event::DeviceId,
};

pub trait CommandHandler {
    fn handle_command(&mut self, command: &Command);
}

#[derive(Clone, Debug)]
pub enum CommandArg {
    None,
    Boolean(bool),
    Float(f64),
    Path(PathBuf),
    Device(DeviceId),
    Displacement((f64, f64)),
}

impl From<DeviceId> for CommandArg {
    fn from(v: DeviceId) -> Self {
        CommandArg::Device(v)
    }
}

impl From<(f64, f64)> for CommandArg {
    fn from(v: (f64, f64)) -> Self {
        CommandArg::Displacement((v.0, v.1))
    }
}

impl From<(f32, f32)> for CommandArg {
    fn from(v: (f32, f32)) -> Self {
        CommandArg::Displacement((f64::from(v.0), f64::from(v.1)))
    }
}

impl From<f64> for CommandArg {
    fn from(v: f64) -> Self {
        CommandArg::Float(v)
    }
}

impl From<LogicalSize> for CommandArg {
    fn from(v: LogicalSize) -> Self {
        CommandArg::Displacement((v.width, v.height))
    }
}

impl From<LogicalPosition> for CommandArg {
    fn from(v: LogicalPosition) -> Self {
        CommandArg::Displacement((v.x, v.y))
    }
}

impl From<PathBuf> for CommandArg {
    fn from(v: PathBuf) -> Self {
        CommandArg::Path(v)
    }
}

impl From<bool> for CommandArg {
    fn from(v: bool) -> Self {
        CommandArg::Boolean(v)
    }
}

#[derive(Clone, Debug)]
pub struct Command {
    target: String,
    command: String,
    args: SmallVec<[CommandArg; 1]>,
}

impl Command {
    pub fn parse(raw: &str) -> Fallible<Self> {
        ensure!(raw.contains('.'));
        let (first, second) = raw.chars().partition(|&c| c == '.');
        Ok(Self {
            target: first,
            command: second,
            args: smallvec![],
        })
    }

    pub fn with_arg(mut self, arg: CommandArg) -> Self {
        self.args.push(arg);
        self
    }

    pub fn full(&self) -> String {
        format!("{}.{}", self.target, self.command)
    }

    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn boolean(&self, index: usize) -> Fallible<bool> {
        match self.args.get(index) {
            Some(CommandArg::Boolean(v)) => Ok(*v),
            _ => bail!("not a boolean argument"),
        }
    }

    pub fn float(&self, index: usize) -> Fallible<f64> {
        match self.args.get(index) {
            Some(CommandArg::Float(v)) => Ok(*v),
            _ => bail!("not a float argument"),
        }
    }

    pub fn path(&self, index: usize) -> Fallible<PathBuf> {
        match &self.args.get(index) {
            Some(CommandArg::Path(v)) => Ok(v.to_path_buf()),
            _ => bail!("not a path argument"),
        }
    }

    pub fn displacement(&self, index: usize) -> Fallible<(f64, f64)> {
        match self.args.get(index) {
            Some(CommandArg::Displacement(v)) => Ok(*v),
            _ => bail!("not a displacement argument"),
        }
    }

    pub fn device(&self, index: usize) -> Fallible<DeviceId> {
        match self.args.get(index) {
            Some(CommandArg::Device(v)) => Ok(*v),
            _ => bail!("not a device argument"),
        }
    }
}
