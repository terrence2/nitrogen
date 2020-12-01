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
use failure::{bail, Fallible};
use fullscreen::FullscreenBuffer;
use geodesy::{GeoSurface, Graticule, Target};
use global_data::GlobalParametersBuffer;
use gpu::GPU;
use input::{InputController, InputSystem};
use legion::prelude::*;
// use tokio::{runtime::Runtime, sync::RwLock as AsyncRwLock};
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
    gpu: GPU,
    //async_rt: Runtime,
    legion: World,
    arcball: ArcBallCamera,

    globals_buffer: GlobalParametersBuffer,
    fullscreen_buffer: FullscreenBuffer,
}

async fn async_main() -> Fallible<()> {
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

    let gpu = GPU::new_async(&window, Default::default()).await?;
    //let mut async_rt = Runtime::new()?;
    let legion = World::default();

    let globals_buffer = GlobalParametersBuffer::new(gpu.device())?;
    let fullscreen_buffer = FullscreenBuffer::new(&gpu)?;

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

    let _ctx = AppContext {
        gpu,
        //async_rt,
        legion,
        arcball,
        globals_buffer,
        fullscreen_buffer,
    };

    let _system_bindings = Bindings::new("map")
        .bind("demo.bail", "b")?
        .bind("demo.panic", "p")?;

    #[cfg(target_arch = "wasm32")]
    InputSystem::run_forever(
        vec![_system_bindings],
        event_loop,
        window,
        window_loop,
        _ctx,
    )
    .await?;
    Ok(())
}

#[allow(unused)]
fn window_loop(
    window: &Window,
    input_controller: &InputController,
    app: &mut AppContext,
) -> Fallible<()> {
    for command in input_controller.poll()? {
        console::log_1(&format!("COMMAND: {:?}", command).into());
        app.arcball.handle_command(&command)?;
        match command.command() {
            "bail" => bail!("soft crash"),
            "panic" => bail!("hard panic"),
            _ => {}
        }
    }
    app.arcball.think();

    // Sim
    let mut tracker = Default::default();
    app.globals_buffer
        .make_upload_buffer(app.arcball.camera(), &app.gpu, &mut tracker)?;

    // Render
    let framebuffer = app.gpu.get_next_framebuffer()?.unwrap();
    let mut encoder = app
        .gpu
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
    app.gpu.queue_mut().submit(vec![encoder.finish()]);

    window.request_redraw();
    Ok(())
}
