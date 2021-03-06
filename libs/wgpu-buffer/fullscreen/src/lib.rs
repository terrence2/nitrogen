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
use gpu::GPU;
use parking_lot::RwLock;
use std::{mem, ops::Range, sync::Arc};
use zerocopy::{AsBytes, FromBytes};

#[repr(C)]
#[derive(AsBytes, FromBytes, Clone, Copy, Debug)]
pub struct FullscreenVertex {
    _pos: [f32; 2],
}

impl FullscreenVertex {
    pub fn new(pos: [i8; 2]) -> Self {
        Self {
            _pos: [f32::from(pos[0]), f32::from(pos[1])],
        }
    }

    pub fn buffer(gpu: &GPU) -> wgpu::Buffer {
        let vertices = vec![
            Self::new([-1, -1]),
            Self::new([-1, 1]),
            Self::new([1, -1]),
            Self::new([1, 1]),
        ];
        gpu.push_slice(
            "fullscreen-corner-vertices",
            &vertices,
            wgpu::BufferUsage::VERTEX,
        )
    }

    pub fn descriptor() -> wgpu::VertexBufferDescriptor<'static> {
        wgpu::VertexBufferDescriptor {
            stride: mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::InputStepMode::Vertex,
            attributes: &[wgpu::VertexAttributeDescriptor {
                format: wgpu::VertexFormat::Float2,
                offset: 0,
                shader_location: 0,
            }],
        }
    }
}

#[derive(Debug)]
pub struct FullscreenBuffer {
    vertex_buffer: wgpu::Buffer,
}

impl FullscreenBuffer {
    pub fn new(gpu: &GPU) -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(Self {
            vertex_buffer: FullscreenVertex::buffer(gpu),
        }))
    }

    pub fn vertex_buffer(&self) -> wgpu::BufferSlice {
        self.vertex_buffer.slice(..)
    }

    pub fn vertex_buffer_range(&self) -> Range<u32> {
        0..4
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use failure::Fallible;
    use gpu::GPU;
    use nitrous::Interpreter;
    use winit::{event_loop::EventLoop, window::Window};

    #[cfg(unix)]
    #[test]
    fn it_can_create_a_buffer() -> Fallible<()> {
        use winit::platform::unix::EventLoopExtUnix;
        let event_loop = EventLoop::<()>::new_any_thread();
        let window = Window::new(&event_loop)?;
        let interpreter = Interpreter::new();
        let gpu = GPU::new(&window, Default::default(), &mut interpreter.write())?;
        let _fullscreen_buffer = FullscreenBuffer::new(&gpu.read());
        Ok(())
    }
}
