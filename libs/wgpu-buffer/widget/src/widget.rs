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
use crate::{
    font_context::{FontContext, FontId},
    paint_context::PaintContext,
    region::{Extent, Position},
};
use anyhow::Result;
use bevy_ecs::prelude::*;
use csscolorparser::Color;
use gpu::Gpu;
use input::InputEvent;
use nitrous::{inject_nitrous_component, NitrousComponent};
use runtime::ScriptHerder;
use std::{fmt::Debug, sync::Arc, time::Instant};
use window::{
    size::{AbsSize, Size},
    Window,
};

// Note: need intersection testing before this is useful.
// pub enum HoverState {
//     None(Instant),
//     Hover(Instant),
//     Press(Instant),
// }

pub trait Labeled: Debug + Sized + Send + Sync + 'static {
    fn set_text<S: AsRef<str> + Into<String>>(&mut self, content: S);
    fn set_size(&mut self, size: Size);
    fn set_color(&mut self, color: &Color);
    fn set_font(&mut self, font_id: FontId);

    fn with_text<S: AsRef<str> + Into<String>>(mut self, content: S) -> Self {
        self.set_text(content);
        self
    }

    fn with_size(mut self, size: Size) -> Self {
        self.set_size(size);
        self
    }

    fn with_font(mut self, font_id: FontId) -> Self {
        self.set_font(font_id);
        self
    }

    fn with_color(mut self, color: &Color) -> Self {
        self.set_color(color);
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WidgetFocus {
    Terminal,
    Game,
}

#[derive(Component, NitrousComponent)]
#[Name = "widget"]
pub struct WidgetComponent {
    inner: Arc<dyn Widget>,
}

#[inject_nitrous_component]
impl WidgetComponent {
    pub fn new<T: Widget>(inner: Arc<T>) -> Self {
        Self { inner }
    }

    pub fn inner(&self) -> &dyn Widget {
        self.inner.as_ref()
    }
}

pub trait Widget: Debug + Send + Sync + 'static {
    /// Return the minimum required size for displaying this widget.
    fn measure(&self, win: &Window, font_context: &FontContext) -> Result<Extent<Size>>;

    /// Apply the layout algorithm to size everything for the current displayed set.
    // fn layout(
    //     &mut self,
    //     now: Instant,
    //     region: Region<RelSize>,
    //     win: &Window,
    //     font_context: &mut FontContext,
    // ) -> Result<()>;

    /// Mutate paint context to reflect the presence of this widget.
    fn upload(
        &self,
        now: Instant,
        win: &Window,
        gpu: &Gpu,
        context: &mut PaintContext,
    ) -> Result<()>;

    /// Low level event handler. The default implementation is generally suitable
    /// such that leaf nodes can implement one of the fine-grained handle_ methods
    /// for keyboard or mouse. Container widgets should pass through to their
    /// children and not handle events directly, except in some rare cases.
    fn handle_event(
        &mut self,
        _event: &InputEvent,
        _focus: WidgetFocus,
        _cursor_position: Position<AbsSize>,
        _herder: &mut ScriptHerder,
    ) -> Result<()> {
        Ok(())
    }
}
