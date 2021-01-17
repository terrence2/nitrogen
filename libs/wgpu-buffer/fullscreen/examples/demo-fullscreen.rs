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
use absolute_unit::{degrees, meters};
use camera::ArcBallCamera;
use command::Bindings;
use failure::Fallible;
use fullscreen::{FullscreenBuffer, FullscreenVertex};
use geodesy::{GeoSurface, Graticule, Target};
use global_data::GlobalParametersBuffer;
use gpu::GPU;
use input::{InputController, InputSystem};
use winit::window::Window;

fn main() -> Fallible<()> {
    let system_bindings = Bindings::new("system")
        .bind("demo.exit", "Escape")?
        .bind("demo.exit", "q")?;
    InputSystem::run_forever(
        vec![ArcBallCamera::default_bindings()?, system_bindings],
        window_main,
    )
}

fn window_main(window: Window, input_controller: &InputController) -> Fallible<()> {
    let mut gpu = GPU::new(&window, Default::default())?;

    let globals_buffer = GlobalParametersBuffer::new(gpu.device())?;
    let fullscreen_buffer = FullscreenBuffer::new(&gpu)?;

    let vert_shader = gpu.create_shader_module(include_bytes!("../target/example.vert.spirv"))?;
    let frag_shader = gpu.create_shader_module(include_bytes!("../target/example.frag.spirv"))?;

    let pipeline_layout = gpu
        .device()
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("main-pipeline-layout"),
            bind_group_layouts: &[globals_buffer.bind_group_layout()],
            push_constant_ranges: &[],
        });
    let pipeline = gpu
        .device()
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("main-pipeline"),
            layout: Some(&pipeline_layout),
            vertex_stage: wgpu::ProgrammableStageDescriptor {
                module: &vert_shader,
                entry_point: "main",
            },
            fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
                module: &frag_shader,
                entry_point: "main",
            }),
            rasterization_state: Some(wgpu::RasterizationStateDescriptor {
                front_face: wgpu::FrontFace::Cw,
                cull_mode: wgpu::CullMode::Back,
                clamp_depth: false,
                depth_bias: 0,
                depth_bias_slope_scale: 0.0,
                depth_bias_clamp: 0.0,
            }),
            primitive_topology: wgpu::PrimitiveTopology::TriangleStrip,
            color_states: &[wgpu::ColorStateDescriptor {
                format: GPU::SCREEN_FORMAT,
                color_blend: wgpu::BlendDescriptor::REPLACE,
                alpha_blend: wgpu::BlendDescriptor::REPLACE,
                write_mask: wgpu::ColorWrite::ALL,
            }],
            depth_stencil_state: Some(wgpu::DepthStencilStateDescriptor {
                format: GPU::DEPTH_FORMAT,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilStateDescriptor {
                    front: wgpu::StencilStateFaceDescriptor::IGNORE,
                    back: wgpu::StencilStateFaceDescriptor::IGNORE,
                    read_mask: 0,
                    write_mask: 0,
                },
            }),
            vertex_state: wgpu::VertexStateDescriptor {
                index_format: wgpu::IndexFormat::Uint16,
                vertex_buffers: &[FullscreenVertex::descriptor()],
            },
            sample_count: 1,
            sample_mask: !0,
            alpha_to_coverage_enabled: false,
        });

    let mut arcball = ArcBallCamera::new(gpu.aspect_ratio(), meters!(0.1));
    arcball.set_eye_relative(Graticule::<Target>::new(
        degrees!(0),
        degrees!(0),
        meters!(10),
    ))?;
    arcball.set_target(Graticule::<GeoSurface>::new(
        degrees!(0),
        degrees!(0),
        meters!(10),
    ));
    arcball.set_distance(meters!(40.0));

    loop {
        for command in input_controller.poll_commands()? {
            arcball.handle_command(&command)?;
            match command.command() {
                "window-close" | "window-destroy" | "exit" => return Ok(()),
                "window-resize" => {
                    gpu.note_resize(None, &window);
                    arcball.camera_mut().set_aspect_ratio(gpu.aspect_ratio());
                }
                "window.dpi-change" => {
                    gpu.note_resize(Some(command.float(0)?), &window);
                    arcball.camera_mut().set_aspect_ratio(gpu.aspect_ratio());
                }
                "window-cursor-move" => {}
                _ => {}
            }
        }
        arcball.think();

        // Prepare new camera parameters.
        let mut tracker = Default::default();
        globals_buffer.make_upload_buffer(arcball.camera(), 2.2, &gpu, &mut tracker)?;

        let gb_borrow = &globals_buffer;
        let fs_borrow = &fullscreen_buffer;
        let framebuffer = gpu.get_next_framebuffer()?.unwrap();
        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame-encoder"),
            });
        tracker.dispatch_uploads(&mut encoder);
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[GPU::color_attachment(&framebuffer.output.view)],
                depth_stencil_attachment: Some(gpu.depth_stencil_attachment()),
            });
            rpass.set_pipeline(&pipeline);
            rpass.set_bind_group(0, gb_borrow.bind_group(), &[]);
            rpass.set_vertex_buffer(0, fs_borrow.vertex_buffer());
            rpass.draw(0..4, 0..1);
        }
        gpu.queue_mut().submit(vec![encoder.finish()]);
    }
}
