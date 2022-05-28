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
use animate::{TimeStep, Timeline};
use anyhow::Result;
use camera::{ArcBallController, ArcBallSystem, CameraSystem, ScreenCameraController};
use event_mapper::EventMapper;
use global_data::GlobalParametersBuffer;
use gpu::Gpu;
use input::{InputController, InputSystem, InputTarget};
use measure::WorldSpaceFrame;
use orrery::Orrery;
use runtime::Runtime;
use std::time::Instant;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use web_sys::console;
use window::Window;
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

async fn async_main() -> Result<()> {
    let event_loop = InputSystem::make_event_loop();
    let _os_window = WindowBuilder::new()
        .with_title("Nitrogen Web Demo")
        .build(&event_loop)?;

    // FIXME: we need a different mechanism for handling resize events on web
    // let _input_controller = InputController::for_web(&event_loop);

    #[cfg(target_arch = "wasm32")]
    {
        let canvas = os_window.canvas();
        let js_window = web_sys::window().expect("the browser window");
        let js_document = js_window.document().unwrap();
        let js_body = js_document.body().unwrap();
        js_body.append_child(&canvas).unwrap();
    }

    let mut runtime = Runtime::default();
    runtime
        // .insert_resource(opt.catalog_opts)
        // .insert_resource(opt.display_opts)
        // .insert_resource(opt.startup_opts)
        // .insert_resource(opt.detail_opts.cpu_detail())
        // .insert_resource(opt.detail_opts.gpu_detail())
        // .insert_resource(app_dirs)
        .insert_resource(InputTarget::Demo)
        // .load_extension::<StartupOpts>()?
        // .load_extension::<Catalog>()?
        .load_extension::<EventMapper<InputTarget>>()?
        .load_extension::<Window>()?
        .load_extension::<Gpu>()?
        // .load_extension::<AtmosphereBuffer>()?
        // .load_extension::<FullscreenBuffer>()?
        .load_extension::<GlobalParametersBuffer>()?
        // .load_extension::<StarsBuffer>()?
        // .load_extension::<TerrainBuffer>()?
        // .load_extension::<WorldRenderPass>()?
        // .load_extension::<WidgetBuffer<DemoFocus>>()?
        // .load_extension::<UiRenderPass<DemoFocus>>()?
        // .load_extension::<CompositeRenderPass<DemoFocus>>()?
        // .load_extension::<System>()?
        .load_extension::<Orrery>()?
        .load_extension::<Timeline>()?
        .load_extension::<TimeStep>()?
        .load_extension::<CameraSystem>()?
        .load_extension::<ArcBallSystem>()?;

    // We need at least one entity with a camera controller for the screen camera
    // before the sim is fully ready to run.
    let _player_ent = runtime
        .spawn_named("player")?
        .insert(WorldSpaceFrame::default())
        .insert_named(ArcBallController::default())?
        .insert(ScreenCameraController::default())
        .id();

    runtime.run_startup();

    #[cfg(target_arch = "wasm32")]
    InputSystem::run_forever(event_loop, _os_window, window_loop, runtime).await?;

    Ok(())
}

#[allow(unused)]
fn window_loop(
    os_window: &OsWindow,
    input_controller: &InputController,
    runtime: &mut Runtime,
) -> Result<()> {
    // Catch monotonic sim time up to system time.
    let frame_start = Instant::now();
    while runtime.resource::<TimeStep>().next_now() < frame_start {
        runtime.run_sim_once();
    }

    // Display a frame
    runtime.run_frame_once();

    os_window.request_redraw();
    Ok(())

    /*
    for event in input_controller.poll_input_events()? {
        console::log_1(&format!("EVENT: {:?}", event).into());
        match event {
            InputEvent::KeyboardKey {
                virtual_keycode: VirtualKeyCode::B,
                ..
            } => {
                bail!("soft crash");
            }
            InputEvent::KeyboardKey {
                virtual_keycode: VirtualKeyCode::P,
                ..
            } => {
                panic!("hard panic");
            }
            _ => {}
        }
    }
    app.arcball.write().apply_input_state();
    let frame = app.arcball.read().world_space_frame();
    app.camera.write().update_frame(&frame);

    // Sim
    app.globals_buffer.write().track_state_changes(
        &app.camera.read(),
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
     */
}
