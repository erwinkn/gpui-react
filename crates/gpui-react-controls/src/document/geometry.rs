//! Geometry uses GPUI's shaped glyphs and wrap boundaries; no second layout.
use gpui::{Bounds, LineLayout, Pixels, Point, TextAlign, TextLayout, point, px, size};
use std::{ops::Range, sync::Arc};

struct Row {
    source: Range<usize>,
    line_start: usize,
    x_start: Pixels,
    origin: Point<Pixels>,
    height: Pixels,
    layout: Arc<LineLayout>,
}
fn rows(layout: &TextLayout, align: TextAlign) -> Vec<Row> {
    let mut rows = Vec::new();
    let bounds = layout.bounds();
    let height = layout.line_height();
    let mut line_start = 0;
    let mut y = bounds.top();
    for line in layout.line_layouts() {
        let mut start = 0;
        for end in line
            .wrap_boundaries
            .iter()
            .map(|boundary| line.runs()[boundary.run_ix].glyphs[boundary.glyph_ix].index)
            .chain([line.len()])
        {
            let x_start = line.unwrapped_layout.x_for_index(start);
            let width = line.unwrapped_layout.x_for_index(end) - x_start;
            let x = bounds.left()
                + match align {
                    TextAlign::Left => px(0.),
                    TextAlign::Center => (bounds.size.width - width) / 2.,
                    TextAlign::Right => bounds.size.width - width,
                };
            rows.push(Row {
                source: line_start + start..line_start + end,
                line_start,
                x_start,
                origin: point(x, y),
                height,
                layout: line.unwrapped_layout.clone(),
            });
            start = end;
            y += height;
        }
        line_start += line.len() + 1; // hard newline between shaped lines
    }
    rows
}
pub struct Geometry {
    pub bounds: Bounds<Pixels>,
    length: usize,
    rows: Vec<Row>,
}
impl Geometry {
    pub fn new(layout: &TextLayout, align: TextAlign) -> Self {
        Self {
            bounds: layout.bounds(),
            length: layout.len(),
            rows: rows(layout, align),
        }
    }
    pub fn range_rects(&self, range: Range<usize>) -> Vec<Bounds<Pixels>> {
        self.rows
            .iter()
            .filter_map(|row| {
                let start = range.start.max(row.source.start);
                let end = range.end.min(row.source.end);
                if start >= end {
                    return None;
                }
                let a = row.layout.x_for_index(start - row.line_start) - row.x_start;
                let b = row.layout.x_for_index(end - row.line_start) - row.x_start;
                let left = a.min(b);
                let right = a.max(b);
                (right > left).then(|| {
                    Bounds::new(
                        row.origin + point(left, px(0.)),
                        size(right - left, row.height),
                    )
                })
            })
            .collect()
    }
    pub fn index_for_position(&self, position: Point<Pixels>) -> usize {
        self.position_index(position, false)
    }
    pub fn character_for_position(&self, position: Point<Pixels>) -> usize {
        self.position_index(position, true)
    }
    fn position_index(&self, position: Point<Pixels>, character: bool) -> usize {
        let Some(row) = self
            .rows
            .iter()
            .find(|row| position.y < row.origin.y + row.height)
            .or(self.rows.last())
        else {
            return 0;
        };
        if position.y < self.rows[0].origin.y {
            return 0;
        }
        if position.y > row.origin.y + row.height {
            return self.length;
        }
        let x = position.x - row.origin.x + row.x_start;
        let index = if character {
            row.layout.index_for_x(x).unwrap_or(row.layout.len)
        } else {
            row.layout.closest_index_for_x(x)
        };
        (row.line_start + index).clamp(row.source.start, row.source.end)
    }
}
pub fn utf16(text: &str, byte: usize) -> usize {
    text[..byte].encode_utf16().count()
}
pub fn byte_offset(text: &str, offset: usize) -> anyhow::Result<usize> {
    let mut units = 0;
    for (byte, ch) in text.char_indices() {
        if units == offset {
            return Ok(byte);
        }
        units += ch.len_utf16();
        anyhow::ensure!(units <= offset, "offset splits a UTF-16 surrogate pair");
    }
    anyhow::ensure!(units == offset, "text offset exceeds length");
    Ok(text.len())
}
