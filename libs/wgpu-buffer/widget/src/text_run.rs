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
    color::Color,
    font_context::{FontId, TextSpanMetrics, SANS_FONT_ID},
    paint_context::PaintContext,
    widget::UploadMetrics,
};
use failure::Fallible;
use gpu::GPU;
use input::ModifiersState;
use smallvec::{smallvec, SmallVec};
use std::{cmp::Ordering, ops::Range};

#[derive(Debug)]
pub struct TextSpan {
    text: String,
    color: Color,
    size_pts: f32,
    font_id: FontId,
}

impl TextSpan {
    pub fn new(text: &str, size_pts: f32, font_id: FontId, color: Color) -> Self {
        Self {
            text: text.to_owned(),
            color,
            size_pts,
            font_id,
        }
    }

    pub fn insert_at(&mut self, s: &str, position: usize) {
        self.text.insert_str(position, s);
    }

    pub fn set_font(&mut self, font_id: FontId) {
        self.font_id = font_id;
    }

    pub fn set_size_pts(&mut self, size_pts: f32) {
        self.size_pts = size_pts;
    }

    pub fn set_color(&mut self, color: Color) {
        self.color = color;
    }

    pub fn delete_range(&mut self, range: Range<usize>) {
        self.text.replace_range(range, "");
    }

    pub fn content(&self) -> &str {
        &self.text
    }

    pub fn size_pts(&self) -> f32 {
        self.size_pts
    }

    pub fn font(&self) -> FontId {
        self.font_id
    }

    pub fn color(&self) -> &Color {
        &self.color
    }
}

#[derive(Clone, Debug)]
pub enum SpanSelection {
    None,
    Cursor { position: usize },
    Select { range: Range<usize> },
}

#[derive(Copy, Clone, Debug, Default)]
pub struct TextSelection {
    anchor: usize,
    focus: usize,
}

impl TextSelection {
    fn is_empty(&self) -> bool {
        self.anchor == self.focus
    }

    fn anchor(&self) -> usize {
        self.anchor
    }

    fn leftmost(&self) -> usize {
        self.anchor.min(self.focus)
    }

    fn rightmost(&self) -> usize {
        self.anchor.max(self.focus)
    }

    fn as_range(&self) -> Range<usize> {
        self.leftmost()..self.rightmost()
    }

    // Find intersection between this selection and the given range. Return a span selection.
    fn intersect(&self, other: Range<usize>) -> SpanSelection {
        if self.anchor == self.focus {
            return if other.contains(&self.anchor) {
                SpanSelection::Cursor {
                    position: self.anchor - other.start,
                }
            } else {
                SpanSelection::None
            };
        }
        let rng = self.as_range();
        let start = rng.start.max(other.start);
        let end = rng.end.min(other.end);
        match start.cmp(&end) {
            Ordering::Less => SpanSelection::Select {
                range: start - other.start..end - other.start,
            },
            _ => SpanSelection::None,
        }
    }

    fn move_to(&mut self, offset: usize) {
        self.focus = offset;
        self.anchor = self.focus;
    }

    fn move_home(&mut self, pin_anchor: bool) {
        self.focus = 0;
        if !pin_anchor {
            self.anchor = self.focus;
        }
    }

    fn move_end(&mut self, pin_anchor: bool, end: usize) {
        self.focus = end;
        if !pin_anchor {
            self.anchor = self.focus;
        }
    }

    fn move_left(&mut self, pin_anchor: bool) {
        self.focus = self.focus.saturating_sub(1);
        if !pin_anchor {
            self.anchor = self.focus;
        }
    }

    fn move_right(&mut self, pin_anchor: bool, end: usize) {
        self.focus = self.focus.saturating_add(1).min(end);
        if !pin_anchor {
            self.anchor = self.focus;
        }
    }
}

#[derive(Debug)]
pub struct TextRun {
    pub spans: Vec<TextSpan>,

    selection: TextSelection,
    hide_selection: bool, // e.g. for Label
    pre_blend_text: bool,

    default_font_id: FontId,
    default_size_pts: f32,
    default_color: Color,
}

impl TextRun {
    pub fn len(&self) -> usize {
        let mut out = 0;
        for span in &self.spans {
            out += span.text.len();
        }
        out
    }

    pub fn empty() -> Self {
        Self {
            spans: vec![],
            selection: Default::default(),
            hide_selection: false,
            pre_blend_text: false,
            default_font_id: SANS_FONT_ID,
            default_size_pts: 12.0,
            default_color: Color::Magenta,
        }
    }

    pub fn with_hidden_selection(mut self) -> Self {
        self.hide_selection = true;
        self
    }

    pub fn with_pre_blended_text(mut self) -> Self {
        self.pre_blend_text = true;
        self
    }

    pub fn with_text(mut self, text: &str) -> Self {
        self.select_all();
        self.insert(text);
        self
    }

    pub fn with_default_color(mut self, color: Color) -> Self {
        self.default_color = color;
        self
    }

    pub fn set_default_color(&mut self, color: Color) {
        self.default_color = color;
    }

    pub fn with_default_font(mut self, font_id: FontId) -> Self {
        self.default_font_id = font_id;
        self
    }

    pub fn set_default_font(&mut self, font_id: FontId) {
        self.default_font_id = font_id;
    }

    pub fn with_default_size_pts(mut self, size_pts: f32) -> Self {
        self.default_size_pts = size_pts;
        self
    }

    pub fn set_default_size_pts(&mut self, size_pts: f32) {
        self.default_size_pts = size_pts;
    }

    pub fn from_text(text: &str) -> Self {
        let mut obj = TextRun::empty();
        obj.insert(text);
        obj.set_cursor(text.len());
        obj
    }

    /// Change the selected region's color.
    pub fn change_color(&mut self, color: Color) {
        self.change_properties(Some(color), None, None);
    }

    /// Change the selected region's font.
    pub fn change_font(&mut self, font_id: FontId) {
        self.change_properties(None, None, Some(font_id));
    }

    /// Change the selected region's size.
    pub fn change_size_pts(&mut self, size_pts: f32) {
        self.change_properties(None, Some(size_pts), None);
    }

    fn change_properties(
        &mut self,
        color: Option<Color>,
        size_pts: Option<f32>,
        font_id: Option<FontId>,
    ) {
        if self.selection.is_empty() {
            return;
        }
        let mut next_spans = Vec::new();
        let mut position = 0;
        for mut span in self.spans.drain(..) {
            if let SpanSelection::Select { range: span_range } = self
                .selection
                .intersect(position..position + span.content().len())
            {
                position += span.content().len();
                if span_range.start == 0 && span_range.end == span.content().len() {
                    if let Some(color) = color {
                        span.set_color(color);
                    }
                    if let Some(size_pts) = size_pts {
                        span.set_size_pts(size_pts);
                    }
                    if let Some(font_id) = font_id {
                        span.set_font(font_id);
                    }
                    next_spans.push(span);
                } else {
                    let parts = [
                        (0..span_range.start, None, None, None),
                        (span_range.clone(), color, size_pts, font_id),
                        (span_range.end..span.text.len(), None, None, None),
                    ];
                    for (part_range, color, size_pts, font_id) in &parts {
                        if !part_range.is_empty() {
                            next_spans.push(TextSpan::new(
                                &span.content()[part_range.to_owned()],
                                size_pts.unwrap_or_else(|| span.size_pts()),
                                font_id.unwrap_or_else(|| span.font()),
                                color.unwrap_or_else(|| *span.color()),
                            ));
                        }
                    }
                }
            } else {
                position += span.content().len();
                next_spans.push(span);
            }
        }
        self.spans = next_spans;
    }

    /// Selects the entire text run.
    pub fn select_all(&mut self) {
        self.selection.move_home(false);
        self.selection.move_end(true, self.len());
    }

    /// Turn the selection area into a cursor.
    pub fn select_none(&mut self) {
        self.selection = Default::default();
    }

    /// Select the given character range.
    pub fn select(&mut self, range: Range<usize>) {
        let own_len = self.len();
        self.selection = TextSelection {
            anchor: range.start.min(own_len),
            focus: range.end.min(own_len),
        };
    }

    /// Set the cursor position in the run, deselecting any previous selection.
    pub fn set_cursor(&mut self, cursor: usize) {
        self.selection.move_to(cursor.min(self.len()));
    }

    /// Set the cursor to the start of the line. If shift is held, the selection end remains fixed.
    pub fn move_home(&mut self, modifiers: &ModifiersState) {
        self.selection.move_home(modifiers.shift());
    }

    /// Set the cursor to the end of the line. If shift is held, the selection start remains fixed.
    pub fn move_end(&mut self, modifiers: &ModifiersState) {
        self.selection.move_end(modifiers.shift(), self.len());
    }

    /// Move the cursor one left. If shift is held, the selection end remains fixed.
    pub fn move_left(&mut self, modifiers: &ModifiersState) {
        self.selection.move_left(modifiers.shift());
    }

    /// Move the cursor one right. If shift is held, the selection start remains fixed.
    pub fn move_right(&mut self, modifiers: &ModifiersState) {
        self.selection.move_right(modifiers.shift(), self.len());
    }

    /// Delete any selected range and insert the given text at the new cursor.
    pub fn insert(&mut self, text: &str) {
        if !self.selection.is_empty() {
            self.delete();
        }
        self.insert_at_cursor(text);
    }

    /// Delete either the current selection or one forward of the cursor.
    pub fn delete(&mut self) {
        if self.selection.is_empty() {
            self.move_right(&ModifiersState::SHIFT);
        }
        for (span_id, span_range) in self.selected_spans() {
            self.spans[span_id].delete_range(span_range);
        }
        self.selection
            .move_to(self.selection.leftmost().min(self.len()));
    }

    /// Delete either the selected range or one left of the cursor.
    pub fn backspace(&mut self) {
        if self.selection.is_empty() {
            self.move_left(&ModifiersState::SHIFT);
        }
        for (span_id, span_range) in self.selected_spans() {
            self.spans[span_id].delete_range(span_range);
        }
        self.selection
            .move_to(self.selection.leftmost().min(self.len()));
    }

    fn insert_at_cursor(&mut self, text: &str) {
        if let Some((span, offset)) = self.find_cursor_in_span() {
            span.insert_at(text, offset);
        } else if let Some(span) = self.spans.last_mut() {
            span.insert_at(text, span.text.len());
        } else {
            self.spans.push(TextSpan::new(
                text,
                self.default_size_pts,
                self.default_font_id,
                self.default_color,
            ));
        }
        let offset = self.selection.anchor() + text.len();
        self.selection.move_to(offset.min(self.len()));
    }

    fn find_cursor_in_span(&mut self) -> Option<(&mut TextSpan, usize)> {
        let cursor = self.selection.anchor();
        let mut base = 0;
        for span in self.spans.iter_mut() {
            if cursor >= base && cursor < base + span.text.len() {
                return Some((span, cursor - base));
            }
            base += span.text.len();
        }
        None
    }

    fn selected_spans(&self) -> SmallVec<[(usize, Range<usize>); 2]> {
        let mut out = smallvec![];
        let mut span_start = 0;
        for (i, span) in self.spans.iter().enumerate() {
            if let Some(span_selected) = self.selected_span_region(span, span_start) {
                out.push((i, span_selected));
            }
            span_start += span.text.len();
        }
        out
    }

    // Find the overlap between the given span, starting at base, and the current selection.
    fn selected_span_region(&self, span: &TextSpan, base: usize) -> Option<Range<usize>> {
        let span_range = base..base + span.text.len();
        let selection_range = self.selection.as_range();
        let intersect =
            selection_range.start.max(span_range.start)..selection_range.end.min(span_range.end);
        if intersect.start <= intersect.end {
            return Some(intersect.start - base..intersect.end - base);
        }
        None
    }

    pub fn flatten(&self) -> String {
        let mut out = String::new();
        for span in &self.spans {
            out.push_str(&span.text);
        }
        out
    }

    pub fn upload(
        &self,
        height_offset: f32,
        widget_info_index: u32,
        gpu: &GPU,
        context: &mut PaintContext,
    ) -> Fallible<(UploadMetrics, TextSpanMetrics)> {
        let mut min_text_offset = usize::MAX;
        let mut min_background_offset = usize::MAX;
        let mut position = 0;
        let mut total_width = 0f32;
        let mut max_height = 0f32;
        let mut max_ascent = 0f32;
        let mut min_descent = 0f32;
        let mut max_line_gap = 0f32;
        for span in self.spans.iter() {
            let selection_area = if self.hide_selection {
                SpanSelection::None
            } else {
                self.selection
                    .intersect(position..position + span.content().len())
            };
            position += span.content().len();

            let span_metrics = context.layout_text(
                &span,
                [total_width, -height_offset],
                widget_info_index,
                selection_area,
                gpu,
            )?;
            total_width += span_metrics.width;

            max_height = max_height.max(span_metrics.height);
            max_line_gap = max_line_gap.max(span_metrics.line_gap);
            max_ascent = max_ascent.max(span_metrics.ascent);
            min_descent = min_descent.min(span_metrics.descent);
            min_text_offset = min_text_offset.min(span_metrics.initial_text_offset);
            min_background_offset =
                min_background_offset.min(span_metrics.initial_background_offset);
        }
        min_text_offset = min_text_offset.min(context.text_pool.len());
        min_background_offset = min_background_offset.min(context.background_pool.len());

        for v in &mut context.text_pool[min_text_offset..] {
            v.position[1] -= max_ascent;
        }
        for v in &mut context.background_pool[min_background_offset..] {
            v.position[1] -= max_ascent;
        }

        Ok((
            UploadMetrics {
                widget_info_indexes: vec![widget_info_index],
                width: total_width,
                height: max_height,
            },
            TextSpanMetrics {
                width: total_width,
                ascent: max_ascent,
                descent: min_descent,
                height: max_height,
                line_gap: max_line_gap,
                initial_text_offset: min_text_offset,
                initial_background_offset: min_background_offset,
            },
        ))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_text_editing() {
        let mut run = TextRun::from_text("");
        assert_eq!("", run.flatten());
        run.insert("a");
        assert_eq!("a", run.flatten());
        run.insert("bc");
        assert_eq!("abc", run.flatten());
        run.move_left(&Default::default());
        run.backspace();
        assert_eq!("ac", run.flatten());
        run.insert("def");
        assert_eq!("adefc", run.flatten());
        run.move_home(&Default::default());
        run.move_right(&Default::default());
        run.delete();
        assert_eq!("aefc", run.flatten());
        run.move_home(&Default::default());
        run.insert("yxz");
        assert_eq!("yxzaefc", run.flatten());
    }

    #[test]
    fn test_text_selection() {
        let mut run = TextRun::from_text("abcdefg");
        assert_eq!("abcdefg", run.flatten());
        run.backspace();
        assert_eq!("abcdef", run.flatten());
        run.select_all();
        run.delete();
        assert_eq!("", run.flatten());
        run.insert("fdsa");
        assert_eq!("fdsa", run.flatten());
        run.select_all();
        run.backspace();
        assert_eq!("", run.flatten());
        run.insert("12345");
        run.move_left(&ModifiersState::SHIFT);
        run.move_left(&ModifiersState::SHIFT);
        run.insert("6");
        assert_eq!("1236", run.flatten());
        run.move_left(&ModifiersState::SHIFT);
        run.move_left(&Default::default());
        run.move_home(&ModifiersState::SHIFT);
        run.delete();
        assert_eq!("36", run.flatten());
        run.insert("12");
        run.move_right(&Default::default());
        run.insert("45");
        assert_eq!("123456", run.flatten());
        run.move_right(&ModifiersState::SHIFT);
        run.insert("7");
        assert_eq!("123457", run.flatten());
        run.move_end(&ModifiersState::SHIFT);
        run.insert("8");
        assert_eq!("1234578", run.flatten());
        run.move_home(&Default::default());
        run.move_end(&ModifiersState::SHIFT);
        run.insert("A");
        assert_eq!("A", run.flatten());
    }
}
