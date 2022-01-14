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
use crate::TimeStep;
use absolute_unit::{meters, radians};
use anyhow::{ensure, Result};
use bevy_ecs::prelude::*;
use futures::future::{ready, FutureExt};
use geodesy::Graticule;
use log::error;
use lyon_geom::{cubic_bezier::CubicBezierSegment, Point};
use nitrous::{Interpreter, Value};
use nitrous_injector::{inject_nitrous_module, method, NitrousModule};
use parking_lot::RwLock;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use triggered::{trigger, Trigger};

#[derive(Debug)]
pub struct CubicBezierCurve {
    bezier: CubicBezierSegment<f64>,
}

impl CubicBezierCurve {
    pub const fn new((x1, y1): (f64, f64), (x2, y2): (f64, f64)) -> Self {
        Self {
            bezier: CubicBezierSegment {
                from: Point::new(0., 0.),
                ctrl1: Point::new(x1, y1),
                ctrl2: Point::new(x2, y2),
                to: Point::new(1., 1.),
            },
        }
    }

    pub fn interpolate(&self, x: f64) -> f64 {
        let ts = self.bezier.solve_t_for_x(x);
        if let Some(&t) = ts.get(0) {
            self.bezier.y(t)
        } else {
            1.
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnimationState {
    Starting,
    Running,
    Finished,
}

#[derive(Debug)]
struct ScriptableAnimation {
    trigger: Trigger,
    callable: Option<Value>,
    start: Value,
    end: Value,
    bezier: CubicBezierCurve,
    duration: Duration,
    duration_f64: f64,
    start_time: Option<Instant>,
    state: AnimationState,
}

impl ScriptableAnimation {
    pub fn new(
        trigger: Trigger,
        callable: Value,
        start: Value,
        end: Value,
        bezier: CubicBezierCurve,
        duration: Duration,
    ) -> Self {
        Self {
            trigger,
            callable: Some(callable),
            start,
            end,
            bezier,
            duration,
            duration_f64: duration.as_secs_f64(),
            start_time: None,
            state: AnimationState::Starting,
        }
    }

    pub fn empty(trigger: Trigger, duration: Duration) -> Self {
        Self {
            trigger,
            callable: None,
            start: 0.0.into(),
            end: 0.0.into(),
            bezier: Timeline::LINEAR_BEZIER,
            duration,
            duration_f64: duration.as_secs_f64(),
            start_time: None,
            state: AnimationState::Starting,
        }
    }

    pub fn apply_fract(&self, f: f64) -> Result<Value> {
        Ok(if self.start.is_numeric() {
            let t0 = self.start.to_numeric()?;
            let t1 = self.end.to_numeric()?;
            (t0 + (t1 - t0) * f).into()
        } else {
            assert!(self.start.is_graticule());
            let lat0 = self.start.to_grat_surface()?.latitude.f64();
            let lat1 = self.end.to_grat_surface()?.latitude.f64();
            let lon0 = self.start.to_grat_surface()?.longitude.f64();
            let lon1 = self.end.to_grat_surface()?.longitude.f64();
            let dist0 = self.start.to_grat_surface()?.distance.f64();
            let dist1 = self.end.to_grat_surface()?.distance.f64();
            Value::Graticule(Graticule::new(
                radians!(lat0 + (lat1 - lat0) * f),
                radians!(lon0 + (lon1 - lon0) * f),
                meters!(dist0 + (dist1 - dist0) * f),
            ))
        })
    }

    pub fn step_time(&mut self, now: &Instant) -> Result<()> {
        assert_ne!(self.state, AnimationState::Finished);
        let (current, ended) = if let Some(start_time) = self.start_time {
            let f0 = (*now - start_time).as_secs_f64() / self.duration_f64;
            let f = self.bezier.interpolate(f0);
            let current = self.apply_fract(f)?;
            if (*now - start_time) >= self.duration {
                (self.end.clone(), true)
            } else {
                (current, false)
            }
        } else {
            self.start_time = Some(*now);
            self.state = AnimationState::Running;
            (self.start.clone(), false)
        };
        if let Some(callable) = &self.callable {
            callable.spawn_method(&[current]);
        }
        if ended {
            self.state = AnimationState::Finished;
            self.trigger.trigger();
        }
        Ok(())
    }

    pub fn is_finished(&self) -> bool {
        self.state == AnimationState::Finished
    }
}

/// Drive scriptable animations.
#[derive(Debug, NitrousModule)]
pub struct Timeline {
    animations: Vec<ScriptableAnimation>,
}

#[inject_nitrous_module]
impl Timeline {
    pub const LINEAR_BEZIER: CubicBezierCurve = CubicBezierCurve::new((0., 0.), (1., 1.));
    pub const EASE_BEZIER: CubicBezierCurve = CubicBezierCurve::new((0.25, 0.1), (0.25, 1.));
    pub const EASE_IN_BEZIER: CubicBezierCurve = CubicBezierCurve::new((0.42, 0.), (1., 1.));
    pub const EASE_OUT_BEZIER: CubicBezierCurve = CubicBezierCurve::new((0., 0.), (0.58, 1.));
    pub const EASE_IN_OUT_BEZIER: CubicBezierCurve = CubicBezierCurve::new((0.42, 0.), (0.58, 1.));

    pub fn new(interpreter: &mut Interpreter) -> Arc<RwLock<Self>> {
        let timeline = Arc::new(RwLock::new(Self { animations: vec![] }));
        interpreter.put_global("timeline", Value::Module(timeline.clone()));
        timeline
    }

    pub fn sys_animate(step: Res<TimeStep>, timeline: Res<Arc<RwLock<Timeline>>>) {
        timeline.write().step_time(step.now());
    }

    pub fn step_time(&mut self, now: &Instant) {
        for animation in &mut self.animations {
            // One animation failing should not propagate to others.
            if let Err(e) = animation.step_time(now) {
                error!("step_time failed with: {}", e);
            }
        }
        self.animations.retain(|animation| !animation.is_finished());
    }

    pub fn with_curve(
        &mut self,
        callable: Value,
        start: Value,
        end: Value,
        duration_sec: f64,
        bezier: CubicBezierCurve,
    ) -> Result<Value> {
        ensure!(
            start.is_numeric() && end.is_numeric() || start.is_graticule() && end.is_graticule()
        );
        let (trigger, listener) = trigger();
        self.animations.push(ScriptableAnimation::new(
            trigger,
            callable,
            start,
            end,
            bezier,
            Duration::from_secs_f64(duration_sec),
        ));
        Ok(Value::Future(Arc::new(RwLock::new(Box::pin(
            listener.then(|_| ready(Value::True())),
        )))))
    }

    #[method]
    pub fn lerp(
        &mut self,
        callable: Value,
        start: f64,
        offset: f64,
        duration_sec: f64,
    ) -> Result<Value> {
        self.with_curve(
            callable,
            start.into(),
            (start + offset).into(),
            duration_sec,
            Self::LINEAR_BEZIER,
        )
    }

    #[method]
    pub fn ease(
        &mut self,
        callable: Value,
        start: f64,
        offset: f64,
        duration_sec: f64,
    ) -> Result<Value> {
        self.with_curve(
            callable,
            start.into(),
            (start + offset).into(),
            duration_sec,
            Self::EASE_BEZIER,
        )
    }

    #[method]
    pub fn ease_in(
        &mut self,
        callable: Value,
        start: f64,
        offset: f64,
        duration_sec: f64,
    ) -> Result<Value> {
        self.with_curve(
            callable,
            start.into(),
            (start + offset).into(),
            duration_sec,
            Self::EASE_IN_BEZIER,
        )
    }

    #[method]
    pub fn ease_out(
        &mut self,
        callable: Value,
        start: f64,
        offset: f64,
        duration_sec: f64,
    ) -> Result<Value> {
        self.with_curve(
            callable,
            start.into(),
            (start + offset).into(),
            duration_sec,
            Self::EASE_OUT_BEZIER,
        )
    }

    #[method]
    pub fn ease_in_out(
        &mut self,
        callable: Value,
        start: f64,
        offset: f64,
        duration_sec: f64,
    ) -> Result<Value> {
        self.with_curve(
            callable,
            start.into(),
            (start + offset).into(),
            duration_sec,
            Self::EASE_IN_OUT_BEZIER,
        )
    }

    #[method]
    pub fn ease_to(
        &mut self,
        callable: Value,
        start: Value,
        end: Value,
        duration_sec: f64,
    ) -> Result<Value> {
        self.with_curve(callable, start, end, duration_sec, Self::EASE_BEZIER)
    }

    #[method]
    pub fn ease_in_to(
        &mut self,
        callable: Value,
        start: Value,
        end: Value,
        duration_sec: f64,
    ) -> Result<Value> {
        self.with_curve(callable, start, end, duration_sec, Self::EASE_IN_BEZIER)
    }

    #[method]
    pub fn ease_out_to(
        &mut self,
        callable: Value,
        start: Value,
        end: Value,
        duration_sec: f64,
    ) -> Result<Value> {
        self.with_curve(callable, start, end, duration_sec, Self::EASE_OUT_BEZIER)
    }

    #[method]
    pub fn ease_in_out_to(
        &mut self,
        callable: Value,
        start: Value,
        end: Value,
        duration_sec: f64,
    ) -> Result<Value> {
        self.with_curve(callable, start, end, duration_sec, Self::EASE_IN_OUT_BEZIER)
    }

    #[allow(clippy::too_many_arguments)]
    #[method]
    pub fn ease_bezier_to(
        &mut self,
        callable: Value,
        start: Value,
        end: Value,
        duration_sec: f64,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
    ) -> Result<Value> {
        self.with_curve(
            callable,
            start,
            end,
            duration_sec,
            CubicBezierCurve::new((x1, y1), (x2, y2)),
        )
    }

    #[method]
    pub fn sleep(&mut self, duration_sec: f64) -> Result<Value> {
        let (trigger, listener) = trigger();
        self.animations.push(ScriptableAnimation::empty(
            trigger,
            Duration::from_secs_f64(duration_sec),
        ));
        Ok(Value::Future(Arc::new(RwLock::new(Box::pin(
            listener.then(|_| ready(Value::True())),
        )))))
    }
}
