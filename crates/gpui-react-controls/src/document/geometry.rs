//! Geometry uses GPUI's shaped glyphs and wrap boundaries; no second layout.
//! An entry keeps only the shared line layouts and derives rows when a
//! selection, search wash, or hit test asks for them.
use gpui::{Bounds, LineLayout, Pixels, Point, TextAlign, TextLayout, WrappedLineLayout, point, px, size};
use smallvec::SmallVec;
use std::{ops::Range, sync::Arc};

struct Row<'a> {
    source: Range<usize>,
    line_start: usize,
    x_start: Pixels,
    origin: Point<Pixels>,
    height: Pixels,
    layout: &'a Arc<LineLayout>,
}
pub struct Geometry {
    pub bounds: Bounds<Pixels>,
    line_height: Pixels,
    align: TextAlign,
    length: usize,
    lines: SmallVec<[Arc<WrappedLineLayout>; 1]>,
}
impl Geometry {
    pub fn new(layout: &TextLayout, align: TextAlign) -> Self {
        Self {
            bounds: layout.bounds(),
            line_height: layout.line_height(),
            align,
            length: layout.len(),
            lines: layout.line_layouts(),
        }
    }
    fn rows(&self) -> impl Iterator<Item = Row<'_>> {
        let bounds = self.bounds;
        let height = self.line_height;
        let align = self.align;
        let mut line_start = 0;
        let mut y = bounds.top();
        self.lines.iter().flat_map(move |line| {
            let this_line_start = line_start;
            line_start += line.len() + 1; // hard newline between shaped lines
            let mut start = 0;
            let mut y_row = y;
            let ends: SmallVec<[usize; 2]> = line
                .wrap_boundaries
                .iter()
                .map(|boundary| line.unwrapped_layout.runs[boundary.run_ix].glyphs[boundary.glyph_ix].index)
                .chain([line.len()])
                .collect();
            y += height * ends.len() as f32;
            ends.into_iter().map(move |end| {
                let x_start = line.unwrapped_layout.x_for_index(start);
                let width = line.unwrapped_layout.x_for_index(end) - x_start;
                let x = bounds.left()
                    + match align {
                        TextAlign::Left => px(0.),
                        TextAlign::Center => (bounds.size.width - width) / 2.,
                        TextAlign::Right => bounds.size.width - width,
                    };
                let row = Row {
                    source: this_line_start + start..this_line_start + end,
                    line_start: this_line_start,
                    x_start,
                    origin: point(x, y_row),
                    height,
                    layout: &line.unwrapped_layout,
                };
                start = end;
                y_row += height;
                row
            })
        })
    }
    pub fn range_rects(&self, range: Range<usize>) -> Vec<Bounds<Pixels>> {
        self.rows()
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
        let mut rows = self.rows().peekable();
        let Some(first) = rows.peek() else {
            return 0;
        };
        if position.y < first.origin.y {
            return 0;
        }
        let mut last = None;
        let mut chosen = None;
        for row in rows {
            if position.y < row.origin.y + row.height {
                chosen = Some(row);
                break;
            }
            last = Some(row);
        }
        let Some(row) = chosen.or(last) else {
            return 0;
        };
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
