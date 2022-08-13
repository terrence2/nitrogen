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

#[cfg(target_os = "linux")]
mod linux {
    use anyhow::Result;
    use evdev::{AbsoluteAxisType, Device};
    use std::collections::HashMap;

    struct AxisInfo {
        _min: f32,
        _max: f32,
    }

    struct Joystick {
        device: Device,
        _axes: HashMap<AbsoluteAxisType, AxisInfo>,
    }

    pub struct EvdevDriver {
        joysticks: Vec<Joystick>,
    }

    impl EvdevDriver {
        fn build_joystick(device: Device) -> Result<Option<Joystick>> {
            // let mut axes: HashMap<AbsoluteAxisType, AxisInfo> = HashMap::new();

            let axis_states = device.get_abs_state()?;
            if let Some(axes) = device.supported_absolute_axes() {
                for axis in axes.iter() {
                    let state = axis_states[axis.0 as usize];
                    println!("state: {:?}", state);
                    // if axis == evdev::AbsoluteAxisType::ABS_X {
                    //     has_x = true;
                    // } else if axis == evdev::AbsoluteAxisType::ABS_Y {
                    //     has_y = true;
                    // }
                }
            } else {
                return Ok(None);
            }
            Ok(Some(Joystick {
                device,
                _axes: HashMap::new(),
            }))
        }

        pub fn new() -> Result<Self> {
            let mut joysticks = Vec::new();
            for device in evdev::enumerate() {
                if let Some(joystick) = Self::build_joystick(device)? {
                    joysticks.push(joystick);
                }
            }
            Ok(Self { joysticks })
        }

        pub fn next_event(&mut self) -> Result<Option<i32>> {
            for joy in &mut self.joysticks {
                for evt in joy.device.fetch_events()? {
                    println!("EVENT: {:?}", evt);
                }
            }
            Ok(None)
        }
    }
}

#[cfg(target_os = "linux")]
pub use linux::EvdevDriver as Jilrs;

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use anyhow::Result;
//
//     #[test]
//     fn it_works() -> Result<()> {
//         let mut jil = Jilrs::new()?;
//         for _ in 0..1_000 {
//             while let Ok(Some(event)) = jil.next_event() {
//                 println!("EVENT: {:?}", event);
//             }
//         }
//         Ok(())
//     }
// }
