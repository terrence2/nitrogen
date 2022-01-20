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
use camera::{ArcBallCamera, Camera};
use chrono::{TimeZone, Utc};
use event_mapper::EventMapper;
use fullscreen::{FullscreenBuffer, FullscreenVertex};
use geodesy::{GeoSurface, Graticule, Target};
use global_data::GlobalParametersBuffer;
use gpu::Gpu;
use input::{InputController, InputEvent, InputSystem, SystemEvent, VirtualKeyCode};
use nitrous::Interpreter;
use orrery::Orrery;
use parking_lot::Mutex;
use std::sync::Arc;
use window::{DisplayConfig, DisplayOpts, OsWindow, Window, WindowBuilder};

fn main() -> Result<()> {
    InputSystem::run_forever(
        WindowBuilder::new().with_title("Nitrogen Render Demo"),
        window_main,
    )
}

fn window_main(os_window: OsWindow, input_controller: Arc<Mutex<InputController>>) -> Result<()> {
    let mut interpreter = Interpreter::default();
    let _mapper = EventMapper::new(&mut interpreter);
    let display_config = DisplayConfig::discover(&DisplayOpts::default(), &os_window);
    let window = Window::new(os_window, display_config, &mut interpreter)?;
    let gpu = Gpu::new(&mut window.write(), Default::default(), &mut interpreter)?;
    let orrery = Orrery::new(Utc.ymd(1964, 2, 24).and_hms(12, 0, 0), &mut interpreter)?;

    let globals_buffer = GlobalParametersBuffer::new(gpu.read().device(), &mut interpreter);
    let fullscreen_buffer = FullscreenBuffer::new(&gpu.read());

    let vert_shader = gpu.read().create_shader_module(
        "example.vert",
        include_bytes!("../target/example.vert.spirv"),
    )?;
    let frag_shader = gpu.read().create_shader_module(
        "example.frag",
        include_bytes!("../target/example.frag.spirv"),
    )?;

    let pipeline_layout =
        gpu.read()
            .device()
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("main-pipeline-layout"),
                push_constant_ranges: &[],
                bind_group_layouts: &[globals_buffer.read().bind_group_layout()],
            });
    let pipeline = gpu
        .read()
        .device()
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("main-pipeline"),
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
                    blend: None,
                    write_mask: wgpu::ColorWrites::COLOR,
                }],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: Some(wgpu::IndexFormat::Uint16),
                front_face: wgpu::FrontFace::Cw,
                cull_mode: Some(wgpu::Face::Back),
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
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
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

    let camera = Camera::install(
        degrees!(90),
        window.read().render_aspect_ratio(),
        meters!(0.1),
        &mut interpreter,
    )?;
    let arcball = ArcBallCamera::install(&mut interpreter)?;
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
        for event in input_controller.lock().poll_input_events()? {
            if let InputEvent::KeyboardKey {
                virtual_keycode, ..
            } = event
            {
                if virtual_keycode == VirtualKeyCode::Q || virtual_keycode == VirtualKeyCode::Escape
                {
                    return Ok(());
                }
            }
        }
        let sys_events = input_controller.lock().poll_system_events()?;
        for event in &sys_events {
            if matches!(event, SystemEvent::Quit) {
                return Ok(());
            }
        }
        if let Some(config) = window.write().handle_system_events(&sys_events) {
            camera.write().on_display_config_updated(&config);
            gpu.write().on_display_config_changed(&config)?;
        }

        arcball.write().handle_mousemotion(-0.5f64, 0f64);
        arcball.write().apply_input_state();
        camera.write().apply_input_state();
        camera
            .write()
            .update_frame(&arcball.read().world_space_frame());

        // Prepare new camera parameters.
        let mut tracker = Default::default();
        globals_buffer
            .write()
            .track_state_changes(&camera.read(), &orrery.read(), &window.read());
        globals_buffer
            .write()
            .ensure_uploaded(&gpu.read(), &mut tracker)?;

        let gpu = &mut gpu.write();
        let gb_borrow = &globals_buffer.read();
        let fs_borrow = &fullscreen_buffer.read();
        let framebuffer = gpu.get_next_framebuffer()?.unwrap();
        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame-encoder"),
            });
        let view = framebuffer
            .texture
            .create_view(&wgpu::TextureViewDescriptor {
                label: None,
                format: None,
                dimension: None,
                aspect: Default::default(),
                base_mip_level: 0,
                mip_level_count: None,
                base_array_layer: 0,
                array_layer_count: None,
            });
        tracker.dispatch_uploads(&mut encoder);
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Gpu::color_attachment(&view)],
                depth_stencil_attachment: Some(gpu.depth_stencil_attachment()),
            });
            rpass.set_pipeline(&pipeline);
            rpass.set_bind_group(0, gb_borrow.bind_group(), &[]);
            rpass.set_vertex_buffer(0, fs_borrow.vertex_buffer());
            rpass.draw(0..4, 0..1);
        }
        gpu.queue_mut().submit(vec![encoder.finish()]);
        framebuffer.present();
    }
}
