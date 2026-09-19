use gpui::{Bounds, Pixels, Point};
use serde::Serialize;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
impl From<Bounds<Pixels>> for Rect {
    fn from(bounds: Bounds<Pixels>) -> Self {
        Self {
            x: bounds.left().into(),
            y: bounds.top().into(),
            width: bounds.size.width.into(),
            height: bounds.size.height.into(),
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Offset {
    pub x: f32,
    pub y: f32,
}
impl From<Point<Pixels>> for Offset {
    fn from(point: Point<Pixels>) -> Self {
        Self {
            x: point.x.into(),
            y: point.y.into(),
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Painted {
    pub bounds: Rect,
    pub revision: u64,
    pub frame: Option<gpui_react::FrameInfo>,
}
