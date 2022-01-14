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
use input::{InputController, InputEvent, InputSystem, SystemEvent, VirtualKeyCode};
use parking_lot::Mutex;
use std::sync::Arc;
use winit::window::{Window, WindowBuilder};

fn main() -> Result<()> {
    InputSystem::run_forever(
        WindowBuilder::new().with_title("Input Example"),
        window_main,
    )
}

fn window_main(window: Window, input_controller: Arc<Mutex<InputController>>) -> Result<()> {
    loop {
        for event in input_controller.lock().poll_input_events()? {
            println!("EVENT: {:?} <- {:?}", window, event);
            if let InputEvent::KeyboardKey {
                virtual_keycode, ..
            } = event
            {
                if virtual_keycode == VirtualKeyCode::Escape || virtual_keycode == VirtualKeyCode::Q
                {
                    input_controller.lock().quit()?;
                    return Ok(());
                }
            }
        }
        for event in input_controller.lock().poll_system_events()? {
            println!("EVENT: {:?} <- {:?}", window, event);
            if matches!(event, SystemEvent::Quit) {
                input_controller.lock().quit()?;
                return Ok(());
            }
        }
    }
}
