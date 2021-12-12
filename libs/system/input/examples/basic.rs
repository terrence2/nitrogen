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
use input::{GenericEvent, GenericSystemEvent, InputController, InputSystem, VirtualKeyCode};
use winit::window::Window;

fn main() -> Result<()> {
    InputSystem::run_forever(window_main)
}

fn window_main(window: Window, input_controller: &mut InputController) -> Result<()> {
    loop {
        for event in input_controller.poll_events()? {
            println!("EVENT: {:?} <- {:?}", window, event);
            match event {
                GenericEvent::System(GenericSystemEvent::Quit) => {
                    input_controller.quit()?;
                    return Ok(());
                }
                GenericEvent::KeyboardKey {
                    virtual_keycode, ..
                } => {
                    if virtual_keycode == VirtualKeyCode::Escape
                        || virtual_keycode == VirtualKeyCode::Q
                    {
                        input_controller.quit()?;
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
    }
}
