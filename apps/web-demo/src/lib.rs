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
use anyhow::{bail, Result};
use camera::ArcBallCamera;
//use fullscreen::FullscreenBuffer;
use geodesy::{GeoSurface, Graticule, Target};
use global_data::GlobalParametersBuffer;
use gpu::Gpu;
use input::{GenericEvent, InputController, InputSystem, VirtualKeyCode};
//use legion::*;
// use tokio::{runtime::Runtime, sync::RwLock as AsyncRwLock};
use nitrous::Interpreter;
use parking_lot::RwLock;
use std::sync::Arc;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use web_sys::console;
#[cfg(target_arch = "wasm32")]
use winit::platform::web::WindowExtWebSys;
use winit::window::{Window, WindowBuilder};

#[wasm_bindgen]
pub fn wasm_main() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    spawn_local(async_trampoline());
}

async fn async_trampoline() {
    match async_main().await {
        Ok(()) => {}
        Err(e) => console::log_1(&format!("program failed with: {}", e).into()),
    }
}

#[allow(unused)]
struct AppContext {
    interpreter: Arc<RwLock<Interpreter>>,
    gpu: Arc<RwLock<Gpu>>,
    arcball: Arc<RwLock<ArcBallCamera>>,
    // //async_rt: Runtime,
    // //legion: World,
    globals_buffer: Arc<RwLock<GlobalParametersBuffer>>,
    // fullscreen_buffer: Arc<RwLock<FullscreenBuffer>>,
}

async fn async_main() -> Result<()> {
    let event_loop = InputSystem::make_event_loop();
    let window = WindowBuilder::new()
        .with_title("Nitrogen")
        .build(&event_loop)?;

    #[cfg(target_arch = "wasm32")]
    {
        let canvas = window.canvas();
        let js_window = web_sys::window().expect("the browser window");
        let js_document = js_window.document().unwrap();
        let js_body = js_document.body().unwrap();
        js_body.append_child(&canvas).unwrap();
    }

    let interpreter = Interpreter::new();
    let gpu = Gpu::new_async(window, Default::default(), &mut interpreter.write()).await?;
    //let mut async_rt = Runtime::new()?;
    //let legion = World::default();

    let globals_buffer = GlobalParametersBuffer::new(gpu.read().device(), &mut interpreter.write());
    // let fullscreen_buffer = FullscreenBuffer::new(&gpu.read());

    let arcball = ArcBallCamera::new(meters!(0.1), &mut gpu.write(), &mut interpreter.write());
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

    let _ctx = AppContext {
        interpreter,
        gpu,
        arcball,
        //async_rt,
        //legion,
        globals_buffer,
        // fullscreen_buffer,
    };
    #[cfg(target_arch = "wasm32")]
    InputSystem::run_forever(event_loop, window, window_loop, _ctx).await?;

    Ok(())
}

#[allow(unused)]
fn window_loop(
    window: &Window,
    input_controller: &InputController,
    app: &mut AppContext,
) -> Result<()> {
    for event in input_controller.poll_events()? {
        console::log_1(&format!("EVENT: {:?}", event).into());
        match event {
            GenericEvent::KeyboardKey {
                virtual_keycode: VirtualKeyCode::B,
                ..
            } => {
                bail!("soft crash");
            }
            GenericEvent::KeyboardKey {
                virtual_keycode: VirtualKeyCode::P,
                ..
            } => {
                panic!("hard panic");
            }
            _ => {}
        }
    }
    app.arcball.write().think();

    // Sim
    let mut tracker = Default::default();
    app.globals_buffer.write().make_upload_buffer(
        app.arcball.read().camera(),
        &app.gpu.read(),
        &mut tracker,
    )?;

    /*
    // Render
    let framebuffer = app.gpu.write().get_next_framebuffer()?.unwrap();
            */
    let mut encoder =
        app.gpu
            .read()
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame-encoder"),
            });
    tracker.dispatch_uploads(&mut encoder);
    {
        // let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        //     color_attachments: &[GPU::color_attachment(&framebuffer.output.view)],
        //     depth_stencil_attachment: Some(app.gpu.depth_stencil_attachment()),
        // });
        // rpass.set_pipeline(&pipeline);
        // rpass.set_bind_group(0, gb_borrow.bind_group(), &[]);
        // rpass.set_vertex_buffer(0, fs_borrow.vertex_buffer());
        // rpass.draw(0..4, 0..1);
    }
    app.gpu.write().queue_mut().submit(vec![encoder.finish()]);

    window.request_redraw();
    Ok(())
}
