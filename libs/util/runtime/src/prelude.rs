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

const GUIDE: &'static str = r#"
Welcome to the Nitrogen Terminal
--------------------------------
Engine "resources" are accessed with the name of the resource followed by a dot,
followed by the name of a property or method on the resource. Methods may be called
by adding a pair of parentheses after.

Example: terrain.toggle_pin_camera(true)

Named game "entities" are accessed with an @ symbol, followed by the name of the
entity, followed by a dot, followed by the name of a "component" on the entity,
followed by another dot, followed by the name of a property or method on that
component. As with resources, methods are called by appending parentheses.

Example: @player.camera.exposure()

The command `list()` may be used at the top level, or on any item, to get a list
of all items that can be accessed on that item.
"#;

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
    fn show_guide(&self) -> String {
        GUIDE.to_owned()
    }

    #[method]
    fn list(&self) {
        println!("IN LIST");
        // for name in runtime.resource_names() {
        //     let resource = runtime.resource_by_name(name);
        //     // resource.to_resource()
        // }
    }
}
