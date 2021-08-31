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
use anyhow::Result;
use futures::future::{ready, FutureExt};
use nitrous::{Interpreter, Value};
use nitrous_injector::{inject_nitrous_module, method, NitrousModule};
use parking_lot::RwLock;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use triggered::{trigger, Trigger};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnimationState {
    Starting,
    Running,
    Finished,
}

#[derive(Debug)]
struct ScriptableAnimation {
    trigger: Trigger,
    callable: Value,
    start: f64,
    end: f64,
    extent: f64,
    direction: i8,
    duration: Duration,
    duration_f64: f64,
    start_time: Option<Instant>,
    state: AnimationState,
}

impl ScriptableAnimation {
    pub fn new(
        trigger: Trigger,
        callable: Value,
        start: f64,
        extent: f64,
        duration: Duration,
    ) -> Self {
        Self {
            trigger,
            callable,
            start,
            end: start + extent,
            extent,
            direction: if extent > 0. { 1 } else { -1 },
            duration,
            duration_f64: duration.as_secs_f64(),
            start_time: None,
            state: AnimationState::Starting,
        }
    }

    pub fn step_time(&mut self, now: &Instant) -> Result<()> {
        assert_ne!(self.state, AnimationState::Finished);
        let current = if let Some(start_time) = self.start_time {
            let f = (*now - start_time).as_secs_f64() / self.duration_f64;
            self.start + self.extent * f
        } else {
            self.start_time = Some(*now);
            self.state = AnimationState::Running;
            self.start
        };
        let (module, name) = self.callable.to_method()?;
        module.write().call_method(name, &[current.into()])?;
        if (self.direction > 0 && current >= self.end)
            || (self.direction < 0 && current <= self.end)
        {
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
    pub fn new(interpreter: &mut Interpreter) -> Arc<RwLock<Self>> {
        let timeline = Arc::new(RwLock::new(Self { animations: vec![] }));
        interpreter.put_global("timeline", Value::Module(timeline.clone()));
        timeline
    }

    pub fn step_time(&mut self, now: &Instant) -> Result<()> {
        for animation in &mut self.animations {
            animation.step_time(now)?;
        }
        self.animations.retain(|animation| !animation.is_finished());
        Ok(())
    }

    #[method]
    pub fn lerp(&mut self, callable: Value, start: f64, offset: f64, duration_sec: f64) -> Value {
        let (trigger, listener) = trigger();
        self.animations.push(ScriptableAnimation::new(
            trigger,
            callable,
            start,
            offset,
            Duration::from_secs_f64(duration_sec),
        ));
        Value::Future(Arc::new(RwLock::new(Box::pin(
            listener.then(|_| ready(Value::True())),
        ))))
    }

    #[method]
    pub fn lerp_to(&mut self, callable: Value, start: f64, end: f64, duration_sec: f64) -> Value {
        let (trigger, listener) = trigger();
        self.animations.push(ScriptableAnimation::new(
            trigger,
            callable,
            start,
            end - start,
            Duration::from_secs_f64(duration_sec),
        ));
        Value::Future(Arc::new(RwLock::new(Box::pin(
            listener.then(|_| ready(Value::True())),
        ))))
    }
}
