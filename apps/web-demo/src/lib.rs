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
use chrono::{TimeZone, Utc};
use geodesy::{GeoSurface, Graticule, Target};
use global_data::GlobalParametersBuffer;
use gpu::Gpu;
use input::{GenericEvent, InputController, InputSystem, VirtualKeyCode};
//use legion::*;
// use tokio::{runtime::Runtime, sync::RwLock as AsyncRwLock};
use nitrous::Interpreter;
use orrery::Orrery;
use parking_lot::RwLock;
use std::sync::Arc;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use web_sys::console;
use window::{DisplayConfig, DisplayOpts, Window};
#[cfg(target_arch = "wasm32")]
use winit::platform::web::WindowExtWebSys;
use winit::window::{Window as OsWindow, WindowBuilder};

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
    interpreter: Interpreter,
    window: Arc<RwLock<Window>>,
    gpu: Arc<RwLock<Gpu>>,
    arcball: Arc<RwLock<ArcBallCamera>>,
    orrery: Arc<RwLock<Orrery>>,
    // //async_rt: Runtime,
    // //legion: World,
    globals_buffer: Arc<RwLock<GlobalParametersBuffer>>,
    // fullscreen_buffer: Arc<RwLock<FullscreenBuffer>>,
}

async fn async_main() -> Result<()> {
    let event_loop = InputSystem::make_event_loop();
    let os_window = WindowBuilder::new()
        .with_title("Nitrogen Web Demo")
        .build(&event_loop)?;

    // FIXME: we need a different mechanism for handling resize events on web
    let mut input_controller = InputController::for_web(&event_loop);

    #[cfg(target_arch = "wasm32")]
    {
        let canvas = os_window.canvas();
        let js_window = web_sys::window().expect("the browser window");
        let js_document = js_window.document().unwrap();
        let js_body = js_document.body().unwrap();
        js_body.append_child(&canvas).unwrap();
    }

    //let mut async_rt = Runtime::new()?;
    //let legion = World::default();

    let mut interpreter = Interpreter::default();
    // let mapper = EventMapper::new(&mut interpreter);

    let display_config = DisplayConfig::discover(&DisplayOpts::default(), &os_window);
    let window = Window::new(
        os_window,
        &mut input_controller,
        display_config,
        &mut interpreter,
    )?;
    let gpu = Gpu::new_async(&mut window.write(), Default::default(), &mut interpreter).await?;

    let globals_buffer = GlobalParametersBuffer::new(gpu.read().device(), &mut interpreter);
    // let fullscreen_buffer = FullscreenBuffer::new(&gpu.read());

    let arcball = ArcBallCamera::new(meters!(0.1), &mut window.write(), &mut interpreter)?;
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
    let orrery = Orrery::new(Utc.ymd(1964, 2, 24).and_hms(12, 0, 0), &mut interpreter)?;

    let _ctx = AppContext {
        interpreter,
        window,
        gpu,
        arcball,
        //async_rt,
        //legion,
        globals_buffer,
        // fullscreen_buffer,
        orrery,
    };
    #[cfg(target_arch = "wasm32")]
    InputSystem::run_forever(event_loop, window, window_loop, _ctx).await?;

    Ok(())
}

#[allow(unused)]
fn window_loop(
    os_window: &OsWindow,
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
    app.arcball.write().track_state_changes();

    // Sim
    app.globals_buffer.write().track_state_changes(
        app.arcball.read().camera(),
        &app.orrery.read(),
        &app.window.read(),
    );

    // Render
    // was crashing...
    // let framebuffer = app.gpu.write().get_next_framebuffer()?.unwrap();

    let mut tracker = Default::default();
    app.globals_buffer
        .write()
        .ensure_uploaded(&app.gpu.read(), &mut tracker)?;

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

    os_window.request_redraw();
    Ok(())
}
