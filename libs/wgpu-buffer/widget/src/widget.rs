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
use crate::paint_context::PaintContext;
use anyhow::Result;
use gpu::GPU;
use input::GenericEvent;
use nitrous::Interpreter;
use std::fmt::Debug;

#[derive(Clone, Debug, Default)]
pub struct UploadMetrics {
    pub widget_info_indexes: Vec<u32>,
    pub width: f32,
    pub height: f32,
}

pub trait Widget: Debug {
    fn upload(&self, gpu: &GPU, context: &mut PaintContext) -> Result<UploadMetrics>;
    fn handle_events(
        &mut self,
        events: &[GenericEvent],
        interpreter: &mut Interpreter,
    ) -> Result<()>;
}
