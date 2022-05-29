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
use csscolorparser::Color;
use event_mapper::EventMapper;
use fullscreen::FullscreenBuffer;
use global_data::GlobalParametersBuffer;
use gpu::{DetailLevelOpts, Gpu, GpuStep};
use input::{InputSystem, InputTarget};
use measure::WorldSpaceFrame;
use nitrous::{inject_nitrous_resource, HeapMut, NitrousResource};
use orrery::Orrery;
use platform_dirs::AppDirs;
use runtime::{ExitRequest, Extension, Runtime, StartupOpts};
use stars::StarsBuffer;
use std::{fs::create_dir_all, time::Instant};
use structopt::StructOpt;
use terminal_size::{terminal_size, Width};
use terrain::TerrainBuffer;
use tracelog::{TraceLog, TraceLogOpts};
use ui::UiRenderPass;
use widget::{Label, Labeled, LayoutNode, LayoutPacking, PaintContext, Terminal, WidgetBuffer};
use window::{size::Size, DisplayOpts, Window, WindowBuilder};
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

    #[structopt(flatten)]
    tracelog_opts: TraceLogOpts,
}

#[derive(Debug)]
struct VisibleWidgets {
    sim_time: Entity,
    camera_direction: Entity,
    camera_position: Entity,
    camera_fov: Entity,
    fps_label: Entity,
}

#[derive(Debug, NitrousResource)]
struct DemoUx {
    visible_widgets: VisibleWidgets,
}

impl Extension for DemoUx {
    fn init(runtime: &mut Runtime) -> Result<()> {
        // let widgets = runtime.resource::<WidgetBuffer<DemoFocus>>();
        let demo = DemoUx::new(runtime.heap_mut())?;
        runtime.insert_named_resource("demo", demo);
        runtime
            .add_frame_system(Self::sys_track_visible_state.after(GpuStep::PresentTargetSurface));
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
    pub fn new(heap: HeapMut) -> Result<Self> {
        let visible_widgets = Self::build_gui(heap)?;
        Ok(Self { visible_widgets })
    }

    pub fn build_gui(
        // widgets: &mut WidgetBuffer<DemoFocus>,
        mut heap: HeapMut,
    ) -> Result<VisibleWidgets> {
        let sim_time = Label::new("")
            .with_color(&Color::from([255, 255, 255]))
            .wrapped("sim_time", heap.as_mut())?;
        let camera_direction = Label::new("")
            .with_color(&Color::from([255, 255, 255]))
            .wrapped("camera_direction", heap.as_mut())?;
        let camera_position = Label::new("")
            .with_color(&Color::from([255, 255, 255]))
            .wrapped("camera_position", heap.as_mut())?;
        let camera_fov = Label::new("")
            .with_color(&Color::from([255, 255, 255]))
            .wrapped("camera_fov", heap.as_mut())?;
        let mut controls_box = LayoutNode::new_vbox("controls_box", heap.as_mut())?;
        let controls_id = controls_box.id();
        controls_box.push_widget(sim_time)?;
        controls_box.push_widget(camera_direction)?;
        controls_box.push_widget(camera_position)?;
        controls_box.push_widget(camera_fov)?;
        heap.resource_mut::<WidgetBuffer>()
            .root_mut()
            .push_layout(controls_box)?;
        let controls_packing = LayoutPacking::default()
            .float_end()
            .float_top()
            .set_background("#555a")?
            .set_padding_left("10px", heap.as_mut())?
            .set_padding_bottom("6px", heap.as_mut())?
            .set_padding_top("4px", heap.as_mut())?
            .set_padding_right("4px", heap.as_mut())?
            .set_border_color("#000")?
            .set_border_left("2px", heap.as_mut())?
            .set_border_bottom("2px", heap.as_mut())?
            .to_owned();
        *heap.get_mut::<LayoutPacking>(controls_id) = controls_packing;

        let fps_label = Label::new("")
            .with_font(
                heap.resource::<PaintContext>()
                    .font_context
                    .font_id_for_name("sans"),
            )
            .with_color(&Color::from([255, 0, 0]))
            .with_size(Size::from_pts(13.0))
            .with_pre_blended_text()
            .wrapped("fps_label", heap.as_mut())?;
        heap.resource_mut::<WidgetBuffer>()
            .root_mut()
            .push_widget(fps_label)?;
        heap.get_mut::<LayoutPacking>(fps_label).float_bottom();
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
        mut labels: Query<&mut Label>,
        camera: Res<ScreenCamera>,
        timestep: Res<TimeStep>,
        orrery: Res<Orrery>,
        system: ResMut<DemoUx>,
    ) {
        for (arcball, _) in query.iter() {
            system
                .track_visible_state(&mut labels, *timestep.now(), &orrery, arcball, &camera)
                .ok();
        }
    }

    pub fn track_visible_state(
        &self,
        labels: &mut Query<&mut Label>,
        now: Instant,
        orrery: &Orrery,
        arcball: &ArcBallController,
        camera: &ScreenCamera,
    ) -> Result<()> {
        labels
            .get_mut(self.visible_widgets.sim_time)?
            .set_text(format!("Date: {}", orrery.get_time()));
        labels
            .get_mut(self.visible_widgets.camera_direction)?
            .set_text(format!("Eye: {}", arcball.eye()));
        labels
            .get_mut(self.visible_widgets.camera_position)?
            .set_text(format!("Position: {}", arcball.target()));
        labels
            .get_mut(self.visible_widgets.camera_fov)?
            .set_text(format!("FoV: {}", degrees!(camera.fov_y())));
        let frame_time = now.elapsed();
        let ts = format!(
            "frame: {}.{}ms",
            frame_time.as_secs() * 1000 + u64::from(frame_time.subsec_millis()),
            frame_time.subsec_micros(),
        );
        labels.get_mut(self.visible_widgets.fps_label)?.set_text(ts);
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
        .insert_resource(opt.tracelog_opts)
        .insert_resource(opt.detail_opts.cpu_detail())
        .insert_resource(opt.detail_opts.gpu_detail())
        .insert_resource(app_dirs)
        .load_extension::<TraceLog>()?
        .load_extension::<StartupOpts>()?
        .load_extension::<Catalog>()?
        .load_extension::<EventMapper>()?
        .load_extension::<Window>()?
        .load_extension::<Gpu>()?
        .load_extension::<InputTarget>()?
        .load_extension::<AtmosphereBuffer>()?
        .load_extension::<FullscreenBuffer>()?
        .load_extension::<GlobalParametersBuffer>()?
        .load_extension::<StarsBuffer>()?
        .load_extension::<TerrainBuffer>()?
        .load_extension::<WorldRenderPass>()?
        .load_extension::<WidgetBuffer>()?
        .load_extension::<UiRenderPass>()?
        .load_extension::<CompositeRenderPass>()?
        .load_extension::<DemoUx>()?
        .load_extension::<Label>()?
        .load_extension::<Terminal>()?
        .load_extension::<Orrery>()?
        .load_extension::<Timeline>()?
        .load_extension::<TimeStep>()?
        .load_extension::<CameraSystem>()?
        .load_extension::<ArcBallSystem>()?;

    // We need at least one entity with a camera controller for the screen camera
    // before the sim is fully ready to run.
    let _player_ent = runtime
        .spawn_named("camera")?
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
