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
use std::{
    fmt,
    ops::Range,
    path::{Path, PathBuf},
};
use winit::{
    dpi::{LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize},
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

impl From<LogicalSize<f64>> for CommandArg {
    fn from(v: LogicalSize<f64>) -> Self {
        CommandArg::Displacement((v.width, v.height))
    }
}

impl From<PhysicalSize<u32>> for CommandArg {
    fn from(v: PhysicalSize<u32>) -> Self {
        CommandArg::Displacement((v.width as f64, v.height as f64))
    }
}

impl From<LogicalPosition<f64>> for CommandArg {
    fn from(v: LogicalPosition<f64>) -> Self {
        CommandArg::Displacement((v.x, v.y))
    }
}

impl From<PhysicalPosition<f64>> for CommandArg {
    fn from(v: PhysicalPosition<f64>) -> Self {
        CommandArg::Displacement((v.x, v.y))
    }
}

impl From<PhysicalPosition<i32>> for CommandArg {
    fn from(v: PhysicalPosition<i32>) -> Self {
        CommandArg::Displacement((v.x as f64, v.y as f64))
    }
}

impl From<&Path> for CommandArg {
    fn from(v: &Path) -> Self {
        CommandArg::Path(v.to_owned())
    }
}

impl From<bool> for CommandArg {
    fn from(v: bool) -> Self {
        CommandArg::Boolean(v)
    }
}

#[derive(Clone, Debug)]
pub struct Command {
    content: String,
    target: Range<usize>,
    command: Range<usize>,
    is_held_command: bool,
    args: SmallVec<[CommandArg; 1]>,
}

impl Command {
    pub fn parse(raw: &str) -> Fallible<Self> {
        if let Some(position) = raw.chars().position(|c| c == '.') {
            ensure!(raw.chars().count() > position + 1);
            let is_held_command = raw[position + 1..].starts_with('+');
            Ok(Self {
                content: raw.to_owned(),
                target: 0..position,
                command: position + 1..raw.len(),
                is_held_command,
                args: smallvec![],
            })
        } else {
            bail!("invalid command string - must have both target and command");
        }
    }

    pub fn with_arg(mut self, arg: CommandArg) -> Self {
        self.args.push(arg);
        self
    }

    pub fn full(&self) -> &str {
        &self.content
    }

    pub fn target(&self) -> &str {
        &self.content[self.target.clone()]
    }

    pub fn command(&self) -> &str {
        &self.content[self.command.clone()]
    }

    pub fn is_held_command(&self) -> bool {
        self.is_held_command
    }

    pub fn full_release_command(&self) -> String {
        assert!(self.is_held_command);
        format!("{}.-{}", self.target(), &self.command()[1..])
    }

    pub fn release_command(&self) -> Fallible<Option<Command>> {
        if self.is_held_command {
            Ok(Some(Command::parse(&self.full_release_command())?))
        } else {
            Ok(None)
        }
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

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.content)
    }
}
