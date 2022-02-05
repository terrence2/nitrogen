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

/// Prelude for the nitrous language when running under nitrogen.
/// This resource gets inserted into the runtime as "prelude", but
/// all of the names are also inserted automatically as values into
/// locals in every script execution, so be fastidious.
use crate::{Extension, Runtime};
use nitrous::{inject_nitrous_resource, method, NitrousResource};

#[derive(Debug, Default, NitrousResource)]
pub struct Prelude;

impl Extension for Prelude {
    fn init(runtime: &mut Runtime) -> anyhow::Result<()> {
        Ok(())
    }
}

#[inject_nitrous_resource]
impl Prelude {
    #[method]
    fn help(&self) -> String {
        "hello, world!".to_owned()
    }
}
