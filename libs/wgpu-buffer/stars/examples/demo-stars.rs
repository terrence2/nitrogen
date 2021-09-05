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
use anyhow::Result;
use camera::ArcBallCamera;
use fullscreen::{FullscreenBuffer, FullscreenVertex};
use geodesy::{GeoSurface, Graticule, Target};
use global_data::GlobalParametersBuffer;
use gpu::Gpu;
use input::{
    GenericEvent, GenericSystemEvent, GenericWindowEvent, InputController, InputSystem,
    VirtualKeyCode,
};
use nitrous::Interpreter;
use stars::StarsBuffer;
use winit::window::Window;

fn main() -> Result<()> {
    InputSystem::run_forever(window_main)
}

fn window_main(window: Window, input_controller: &InputController) -> Result<()> {
    let interpreter = Interpreter::new();
    let gpu = Gpu::new(window, Default::default(), &mut interpreter.write())?;

    let globals_buffer = GlobalParametersBuffer::new(gpu.read().device(), &mut interpreter.write());
    let fullscreen_buffer = FullscreenBuffer::new(&gpu.read());
    let stars_buffers = StarsBuffer::new(&gpu.read())?;

    let vert_shader = gpu.write().create_shader_module(
        "example.vert",
        include_bytes!("../target/example.vert.spirv"),
    )?;
    let frag_shader = gpu.write().create_shader_module(
        "example.frag",
        include_bytes!("../target/example.frag.spirv"),
    )?;

    let empty_layout =
        gpu.read()
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("empty-bind-group-layout"),
                entries: &[],
            });
    let empty_bind_group = gpu
        .read()
        .device()
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("empty-bind-group"),
            layout: &empty_layout,
            entries: &[],
        });

    let pipeline_layout =
        gpu.read()
            .device()
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("demo-stars-pipeline-layout"),
                push_constant_ranges: &[],
                bind_group_layouts: &[
                    globals_buffer.read().bind_group_layout(),
                    &empty_layout,
                    stars_buffers.bind_group_layout(),
                ],
            });
    let pipeline = gpu
        .read()
        .device()
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("demo-stars-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vert_shader,
                entry_point: "main",
                buffers: &[FullscreenVertex::descriptor()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &frag_shader,
                entry_point: "main",
                targets: &[wgpu::ColorTargetState {
                    format: Gpu::SCREEN_FORMAT,
                    color_blend: wgpu::BlendState::REPLACE,
                    alpha_blend: wgpu::BlendState::REPLACE,
                    write_mask: wgpu::ColorWrite::ALL,
                }],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: Some(wgpu::IndexFormat::Uint16),
                front_face: wgpu::FrontFace::Cw,
                cull_mode: wgpu::CullMode::Back,
                polygon_mode: wgpu::PolygonMode::Fill,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: Gpu::DEPTH_FORMAT,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState {
                    front: wgpu::StencilFaceState::IGNORE,
                    back: wgpu::StencilFaceState::IGNORE,
                    read_mask: 0,
                    write_mask: 0,
                },
                bias: wgpu::DepthBiasState {
                    constant: 0,
                    slope_scale: 0.0,
                    clamp: 0.0,
                },
                clamp_depth: false,
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
        });

    let arcball = ArcBallCamera::new(meters!(0.1), &mut gpu.write(), &mut interpreter.write());
    arcball.write().pan_view(true);
    arcball.write().set_eye(Graticule::<Target>::new(
        degrees!(0),
        degrees!(0),
        meters!(10),
    ))?;
    arcball.write().set_target(Graticule::<GeoSurface>::new(
        degrees!(0),
        degrees!(0),
        meters!(10),
    ));
    arcball.write().set_distance(meters!(40.0));

    loop {
        for event in input_controller.poll_events()? {
            match event {
                GenericEvent::KeyboardKey {
                    virtual_keycode, ..
                } => {
                    if virtual_keycode == VirtualKeyCode::Q
                        || virtual_keycode == VirtualKeyCode::Escape
                    {
                        return Ok(());
                    }
                }
                GenericEvent::Window(GenericWindowEvent::Resized { width, height }) => {
                    gpu.write().on_resize(width as i64, height as i64)?;
                }
                GenericEvent::Window(GenericWindowEvent::ScaleFactorChanged { scale }) => {
                    gpu.write().on_dpi_change(scale)?;
                }
                GenericEvent::System(GenericSystemEvent::Quit) => {
                    return Ok(());
                }
                _ => {}
            }
        }
        arcball.write().handle_mousemotion(-0.5f64, 0f64);
        arcball.write().think();

        // Prepare new camera parameters.
        let mut tracker = Default::default();
        globals_buffer.write().make_upload_buffer(
            arcball.read().camera(),
            &gpu.read(),
            &mut tracker,
        )?;

        let gpu = &mut gpu.write();
        let gb_borrow = &globals_buffer.read();
        let fs_borrow = &fullscreen_buffer.read();
        let sb_borrow = &stars_buffers;
        let framebuffer = gpu.get_next_framebuffer()?.unwrap();
        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame-encoder"),
            });
        tracker.dispatch_uploads(&mut encoder);
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Gpu::color_attachment(&framebuffer.output.view)],
                depth_stencil_attachment: Some(gpu.depth_stencil_attachment()),
            });
            rpass.set_pipeline(&pipeline);
            rpass.set_bind_group(0, gb_borrow.bind_group(), &[]);
            rpass.set_bind_group(1, &empty_bind_group, &[]);
            rpass.set_bind_group(2, sb_borrow.bind_group(), &[]);
            rpass.set_vertex_buffer(0, fs_borrow.vertex_buffer());
            rpass.draw(0..4, 0..1);
        }
        gpu.queue_mut().submit(vec![encoder.finish()]);
    }
}
