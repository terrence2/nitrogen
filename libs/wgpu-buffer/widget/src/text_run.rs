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
use crate::font_context::SANS_FONT_ID;
use crate::{
    color::Color,
    font_context::{FontId, TextSpanMetrics},
    paint_context::PaintContext,
};
use gpu::GPU;
use smallvec::{smallvec, SmallVec};
use std::ops::Range;
use winit::event::ModifiersState;

#[derive(Debug)]
pub struct TextSpan {
    text: String,
    size_pts: f32,
    font_id: FontId,
    color: Color,
}

impl TextSpan {
    pub fn new(text: &str, size_pts: f32, font_id: FontId, color: Color) -> Self {
        Self {
            text: text.to_owned(),
            size_pts,
            font_id,
            color,
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
}

#[derive(Debug)]
pub struct TextRun {
    pub spans: Vec<TextSpan>,

    // When the range is empty, this is a cursor at `start`. When selection is non-empty,
    // it represents a selection.
    selection: Range<usize>,

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
            selection: 0..0,
            default_font_id: SANS_FONT_ID,
            default_size_pts: 12.0,
            default_color: Color::Magenta,
        }
    }

    pub fn with_default_color(mut self, color: Color) -> Self {
        self.default_color = color;
        self
    }

    pub fn with_default_font_id(mut self, font_id: FontId) -> Self {
        self.default_font_id = font_id;
        self
    }

    pub fn with_default_size_pts(mut self, size_pts: f32) -> Self {
        self.default_size_pts = size_pts;
        self
    }

    pub fn from_text(text: &str) -> Self {
        let mut obj = TextRun::empty();
        obj.insert(text);
        obj.set_cursor(text.len());
        obj
    }

    /// Selects the entire text run.
    pub fn select_all(&mut self) {
        self.selection = 0..self.len();
    }

    /// Set the cursor position in the run, deselecting any previous selection.
    pub fn set_cursor(&mut self, cursor: usize) {
        let c = cursor.min(self.len());
        self.selection = c..c;
    }

    /// Set the cursor to the start of the line. If shift is held, the selection end remains fixed.
    pub fn move_home(&mut self, modifiers: &ModifiersState) {
        if modifiers.shift() {
            self.selection.start = 0;
        } else {
            self.selection = 0..0;
        }
    }

    /// Set the cursor to the end of the line. If shift is held, the selection start remains fixed.
    pub fn move_end(&mut self, modifiers: &ModifiersState) {
        let own_len = self.len();
        if modifiers.shift() {
            self.selection.end = own_len;
        } else {
            self.selection = own_len..own_len;
        }
    }

    /// Move the cursor one left. If shift is held, the selection end remains fixed.
    pub fn move_left(&mut self, modifiers: &ModifiersState) {
        if modifiers.shift() {
            if self.selection.start > 0 {
                self.selection.start -= 1;
            }
        } else {
            if self.selection.start > 0 {
                self.selection = self.selection.start - 1..self.selection.start - 1;
            }
        }
    }

    /// Move the cursor one right. If shift is held, the selection start remains fixed.
    pub fn move_right(&mut self, modifiers: &ModifiersState) {
        let own_len = self.len();
        if modifiers.shift() {
            if self.selection.end < own_len {
                self.selection.end += 1;
            }
        } else {
            if self.selection.end < own_len {
                self.selection = self.selection.end + 1..self.selection.end + 1;
            }
        }
    }

    /// Delete any selected range and insert the given text at the new cursor.
    pub fn insert(&mut self, text: &str) {
        if !self.selection.is_empty() {
            self.delete();
        }
        assert!(self.selection.is_empty());
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
        self.selection = self.selection.start..self.selection.start;
    }

    /// Delete either the selected range or one left of the cursor.
    pub fn backspace(&mut self) {
        if self.selection.is_empty() {
            self.move_left(&ModifiersState::SHIFT);
        }
        for (span_id, span_range) in self.selected_spans() {
            self.spans[span_id].delete_range(span_range);
        }
        self.selection = self.selection.start..self.selection.start;
    }

    // Panic if called with a selection.
    fn cursor_position(&self) -> usize {
        assert!(self.selection.is_empty());
        self.selection.start
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
        let cursor_offset = self.selection.start + text.len();
        self.selection = cursor_offset..cursor_offset;
    }

    fn find_cursor_in_span(&mut self) -> Option<(&mut TextSpan, usize)> {
        let cursor = self.cursor_position();
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
            let span_selected = self.selected_span_region(span, span_start);
            if !span_selected.is_empty() {
                out.push((i, span_selected));
            }
            span_start += span.text.len();
        }
        out
    }

    // Find the overlap between the given span, starting at base, and the current selection.
    fn selected_span_region(&self, span: &TextSpan, base: usize) -> Range<usize> {
        let span_range = base..base + span.text.len();
        let intersect =
            self.selection.start.max(span_range.start)..self.selection.end.min(span_range.end);
        intersect.start - base..intersect.end - base
    }

    pub fn set_all_font(&mut self, font_id: FontId) {
        for span in self.spans.iter_mut() {
            span.set_font(font_id);
        }
    }

    pub fn set_all_size_pts(&mut self, size_pts: f32) {
        for span in self.spans.iter_mut() {
            span.set_size_pts(size_pts);
        }
    }

    pub fn set_all_color(&mut self, color: Color) {
        for span in self.spans.iter_mut() {
            span.set_color(color);
        }
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
    ) -> TextSpanMetrics {
        let mut selection_offset = 0;
        let selections = self.selected_spans();

        let mut char_offset = 0;
        let mut total_width = 0f32;
        let mut max_height = 0f32;
        let mut max_baseline = 0f32;
        for (span_offset, span) in self.spans.iter().enumerate() {
            let selection_area = if selections.len() > selection_offset
                && selections[selection_offset].0 == span_offset
            {
                let area = selections[selection_offset].1.clone();
                selection_offset += 1;
                Some(area)
            } else if selections.is_empty()
                && self.selection.start > char_offset
                && self.selection.start < char_offset + span.text.len()
            {
                let v = self.selection.start - char_offset;
                Some(v..v)
            } else {
                None
            };

            // FIXME: one info per span so that we can set the color? Or pass the color with the vert?
            let span_metrics = context.layout_text(
                &span.text,
                span.font_id,
                span.size_pts,
                [0., -height_offset],
                widget_info_index,
                selection_area,
                gpu,
            );
            total_width += span_metrics.width;
            // FIXME: need to be able to offset height by line.
            let line_gap = context
                .font_context
                .get_font(span.font_id)
                .read()
                .line_gap(span.size_pts);
            max_height = max_height.max(span_metrics.height + line_gap);
            max_baseline = max_baseline.max(span_metrics.baseline_height);
            char_offset += span.text.len();
        }
        TextSpanMetrics {
            width: total_width,
            baseline_height: max_baseline,
            height: max_height,
        }
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
