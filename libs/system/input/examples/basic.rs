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
use command::Bindings;
use failure::Fallible;
use input::{InputController, InputSystem};
use winit::window::Window;

fn main() -> Fallible<()> {
    let system_bindings = Bindings::new("map")
        .bind("demo.exit", "Escape")?
        .bind("demo.exit", "q")?;
    InputSystem::run_forever(vec![system_bindings], game_main)
}

fn game_main(_window: Window, input_controller: &InputController) -> Fallible<()> {
    loop {
        for command in input_controller.poll()? {
            if command.command() == "exit" {
                return Ok(());
            }
            println!("COMMAND: {:?}", command);
        }
    }
}
