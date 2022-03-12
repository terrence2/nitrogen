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
use absolute_unit::degrees;
use animate::{TimeStep, Timeline};
use anyhow::{anyhow, Result};
use atmosphere::AtmosphereBuffer;
use bevy_ecs::prelude::*;
use camera::{
    ArcBallController, ArcBallSystem, CameraSystem, ScreenCamera, ScreenCameraController,
};
use catalog::{Catalog, CatalogOpts};
use composite::CompositeRenderPass;
use event_mapper::EventMapper;
use fullscreen::FullscreenBuffer;
use global_data::GlobalParametersBuffer;
use gpu::{DetailLevelOpts, Gpu};
use input::{DemoFocus, InputSystem};
use measure::WorldSpaceFrame;
use nitrous::{inject_nitrous_resource, NitrousResource};
use orrery::Orrery;
use parking_lot::RwLock;
use platform_dirs::AppDirs;
use runtime::{ExitRequest, Extension, FrameStage, Runtime, StartupOpts};
use stars::StarsBuffer;
use std::{fs::create_dir_all, sync::Arc, time::Instant};
use structopt::StructOpt;
use terminal_size::{terminal_size, Width};
use terrain::TerrainBuffer;
use ui::UiRenderPass;
use widget::{
    Border, Color, Expander, Label, Labeled, PositionH, PositionV, VerticalBox, WidgetBuffer,
};
use window::{
    size::{LeftBound, Size},
    DisplayOpts, Window, WindowBuilder,
};
use world_render::WorldRenderPass;

/// Demonstrate the capabilities of the Nitrogen engine
#[derive(Clone, Debug, StructOpt)]
#[structopt(set_term_width = if let Some((Width(w), _)) = terminal_size() { w as usize } else { 80 })]
struct Opt {
    #[structopt(flatten)]
    catalog_opts: CatalogOpts,

    #[structopt(flatten)]
    detail_opts: DetailLevelOpts,

    #[structopt(flatten)]
    display_opts: DisplayOpts,

    #[structopt(flatten)]
    startup_opts: StartupOpts,
}

#[derive(Debug)]
struct VisibleWidgets {
    sim_time: Arc<RwLock<Label>>,
    camera_direction: Arc<RwLock<Label>>,
    camera_position: Arc<RwLock<Label>>,
    camera_fov: Arc<RwLock<Label>>,
    fps_label: Arc<RwLock<Label>>,
}

#[derive(Debug, NitrousResource)]
struct DemoUx {
    visible_widgets: VisibleWidgets,
}

impl Extension for DemoUx {
    fn init(runtime: &mut Runtime) -> Result<()> {
        let widgets = runtime.resource::<WidgetBuffer<DemoFocus>>();
        let system = DemoUx::new(widgets)?;
        runtime.insert_named_resource("system", system);
        runtime
            .frame_stage_mut(FrameStage::FrameEnd)
            .add_system(Self::sys_track_visible_state);
        runtime.run_string(
            r#"
                bindings.bind("Escape", "exit()");
                bindings.bind("q", "exit()");
            "#,
        )?;
        Ok(())
    }
}

#[inject_nitrous_resource]
impl DemoUx {
    pub fn new(widgets: &WidgetBuffer<DemoFocus>) -> Result<Self> {
        let visible_widgets = Self::build_gui(widgets)?;
        Ok(Self { visible_widgets })
    }

    pub fn build_gui(widgets: &WidgetBuffer<DemoFocus>) -> Result<VisibleWidgets> {
        let sim_time = Label::new("").with_color(Color::White).wrapped();
        let camera_direction = Label::new("").with_color(Color::White).wrapped();
        let camera_position = Label::new("").with_color(Color::White).wrapped();
        let camera_fov = Label::new("").with_color(Color::White).wrapped();
        let controls_box = VerticalBox::new_with_children(&[
            sim_time.clone(),
            camera_direction.clone(),
            camera_position.clone(),
            camera_fov.clone(),
        ])
        .with_background_color(Color::Gray.darken(3.).opacity(0.8))
        .with_glass_background()
        .with_padding(Border::new(
            Size::zero(),
            Size::from_px(8.),
            Size::from_px(24.),
            Size::from_px(8.),
        ))
        .wrapped();
        let expander = Expander::new_with_child("☰ Nitrogen v0.1", controls_box)
            .with_color(Color::White)
            .with_background_color(Color::Gray.darken(3.).opacity(0.8))
            .with_glass_background()
            .with_border(
                Color::Black,
                Border::new(
                    Size::zero(),
                    Size::from_px(2.),
                    Size::from_px(2.),
                    Size::zero(),
                ),
            )
            .with_padding(Border::new(
                Size::from_px(2.),
                Size::from_px(3.),
                Size::from_px(3.),
                Size::from_px(2.),
            ))
            .wrapped();
        widgets
            .root_container()
            .write()
            .add_child("controls", expander)
            .set_float(PositionH::End, PositionV::Top);

        let fps_label = Label::new("")
            .with_font(widgets.font_context().font_id_for_name("sans"))
            .with_color(Color::Red)
            .with_size(Size::from_pts(13.0))
            .with_pre_blended_text()
            .wrapped();
        widgets
            .root_container()
            .write()
            .add_child("fps", fps_label.clone())
            .set_float(PositionH::Start, PositionV::Bottom);
        Ok(VisibleWidgets {
            sim_time,
            camera_direction,
            camera_position,
            camera_fov,
            fps_label,
        })
    }

    fn sys_track_visible_state(
        query: Query<(&ArcBallController, &ScreenCameraController)>,
        camera: Res<ScreenCamera>,
        timestep: Res<TimeStep>,
        orrery: Res<Orrery>,
        system: ResMut<DemoUx>,
    ) {
        for (arcball, _) in query.iter() {
            system
                .track_visible_state(*timestep.now(), &orrery, arcball, &camera)
                .ok();
        }
    }

    pub fn track_visible_state(
        &self,
        now: Instant,
        orrery: &Orrery,
        arcball: &ArcBallController,
        camera: &ScreenCamera,
    ) -> Result<()> {
        self.visible_widgets
            .sim_time
            .write()
            .set_text(format!("Date: {}", orrery.get_time()));
        self.visible_widgets
            .camera_direction
            .write()
            .set_text(format!("Eye: {}", arcball.eye()));
        self.visible_widgets
            .camera_position
            .write()
            .set_text(format!("Position: {}", arcball.target(),));
        self.visible_widgets
            .camera_fov
            .write()
            .set_text(format!("FoV: {}", degrees!(camera.fov_y()),));
        let frame_time = now.elapsed();
        let ts = format!(
            "frame: {}.{}ms",
            frame_time.as_secs() * 1000 + u64::from(frame_time.subsec_millis()),
            frame_time.subsec_micros(),
        );
        self.visible_widgets.fps_label.write().set_text(ts);
        Ok(())
    }
}

fn main() -> Result<()> {
    // Note: process help before opening a window.
    let opt = Opt::from_args();
    env_logger::init();
    InputSystem::run_forever(
        opt,
        WindowBuilder::new().with_title("Nitrogen Demo"),
        simulation_main,
    )
}

fn simulation_main(mut runtime: Runtime) -> Result<()> {
    // Make sure various config locations exist
    let app_dirs = AppDirs::new(Some("nitrogen"), true)
        .ok_or_else(|| anyhow!("unable to find app directories"))?;
    create_dir_all(&app_dirs.config_dir)?;
    create_dir_all(&app_dirs.state_dir)?;

    let opt = runtime.resource::<Opt>().to_owned();
    runtime
        .insert_resource(opt.catalog_opts)
        .insert_resource(opt.display_opts)
        .insert_resource(opt.startup_opts)
        .insert_resource(opt.detail_opts.cpu_detail())
        .insert_resource(opt.detail_opts.gpu_detail())
        .insert_resource(app_dirs)
        .insert_resource(DemoFocus::Demo)
        .load_extension::<StartupOpts>()?
        .load_extension::<Catalog>()?
        .load_extension::<EventMapper<DemoFocus>>()?
        .load_extension::<Window>()?
        .load_extension::<Gpu>()?
        .load_extension::<AtmosphereBuffer>()?
        .load_extension::<FullscreenBuffer>()?
        .load_extension::<GlobalParametersBuffer>()?
        .load_extension::<StarsBuffer>()?
        .load_extension::<TerrainBuffer>()?
        .load_extension::<WorldRenderPass>()?
        .load_extension::<WidgetBuffer<DemoFocus>>()?
        .load_extension::<UiRenderPass<DemoFocus>>()?
        .load_extension::<CompositeRenderPass<DemoFocus>>()?
        .load_extension::<DemoUx>()?
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
    while runtime.resource::<ExitRequest>().still_running() {
        // Catch monotonic sim time up to system time. Nitrous uses a monotonic time-step game
        // loop. Ideally the sim steps should be a multiple of the frame time.
        let frame_start = Instant::now();
        while runtime.resource::<TimeStep>().next_now() < frame_start {
            runtime.run_sim_once();
        }

        // Display a frame
        runtime.run_frame_once();
    }
    runtime.run_shutdown();

    Ok(())
}
