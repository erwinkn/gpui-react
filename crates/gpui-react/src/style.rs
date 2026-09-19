//! Typed style props shared by every control. A `SharedStyle` is one `Arc`
//! per node; equal styles are defined once on the wire and referenced by id.
use gpui::px;
use serde::{Deserialize, Deserializer};
use std::{
    cell::RefCell,
    ops::Deref,
    sync::{Arc, LazyLock},
};

/// A CSS color parsed once when props enter Rust.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color(pub gpui::Hsla);
impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = String::deserialize(d)?;
        let color = value
            .parse::<csscolorparser::Color>()
            .map_err(serde::de::Error::custom)?;
        Ok(Self(
            gpui::Rgba {
                r: color.r,
                g: color.g,
                b: color.b,
                a: color.a,
            }
            .into(),
        ))
    }
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Length {
    Pixels(#[serde(deserialize_with = "finite")] f32),
    Named(NamedLength),
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub enum NamedLength {
    #[serde(rename = "100%")]
    Fill,
    #[serde(rename = "auto")]
    Auto,
}
impl Length {
    fn length(self) -> gpui::Length {
        match self {
            Self::Pixels(n) => px(n).into(),
            Self::Named(NamedLength::Fill) => gpui::relative(1.).into(),
            Self::Named(NamedLength::Auto) => gpui::Length::Auto,
        }
    }
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    Row,
    Column,
    RowReverse,
    ColumnReverse,
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Align {
    Start,
    Center,
    End,
    Stretch,
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Style {
    pub width: Option<Length>,
    pub height: Option<Length>,
    pub min_width: Option<Length>,
    pub min_height: Option<Length>,
    pub max_width: Option<Length>,
    pub max_height: Option<Length>,
    pub direction: Option<Direction>,
    pub align: Option<Align>,
    #[serde(deserialize_with = "optional_finite")]
    pub grow: Option<f32>,
    #[serde(deserialize_with = "optional_finite")]
    pub shrink: Option<f32>,
    #[serde(deserialize_with = "optional_finite")]
    pub gap: Option<f32>,
    #[serde(deserialize_with = "optional_finite")]
    pub padding: Option<f32>,
    #[serde(deserialize_with = "optional_finite")]
    pub padding_x: Option<f32>,
    #[serde(deserialize_with = "optional_finite")]
    pub padding_y: Option<f32>,
    pub background: Option<Color>,
    pub color: Option<Color>,
    #[serde(deserialize_with = "optional_finite")]
    pub font_size: Option<f32>,
    #[serde(deserialize_with = "optional_finite")]
    pub line_height: Option<f32>,
    #[serde(deserialize_with = "optional_finite")]
    pub border_width: Option<f32>,
    pub border_color: Option<Color>,
    #[serde(deserialize_with = "optional_finite")]
    pub radius: Option<f32>,
    #[serde(deserialize_with = "optional_finite")]
    pub opacity: Option<f32>,
    pub hover: Option<Box<Style>>,
    pub active: Option<Box<Style>>,
    pub focus: Option<Box<Style>>,
}

static DEFAULT_STYLE: LazyLock<Arc<Style>> = LazyLock::new(|| Arc::new(Style::default()));

thread_local! {
    /// The decoding session's style definitions, installed by `Decoder::parse`
    /// for the duration of one transaction.
    pub(crate) static STYLES: RefCell<Vec<Option<Arc<Style>>>> = const { RefCell::new(Vec::new()) };
}

/// A style value shared between every node that declared it. On the wire a
/// style is either a number, the id of a definition sent earlier in the
/// session, or an inline object, which is accepted for tests and native
/// callers and allocated on its own.
#[derive(Clone, Debug)]
pub struct SharedStyle(Arc<Style>);

impl SharedStyle {
    pub fn new(style: Style) -> Self {
        Self(Arc::new(style))
    }
    pub fn as_arc(&self) -> &Arc<Style> {
        &self.0
    }
}
impl Default for SharedStyle {
    fn default() -> Self {
        Self(DEFAULT_STYLE.clone())
    }
}
impl Deref for SharedStyle {
    type Target = Style;
    fn deref(&self) -> &Style {
        &self.0
    }
}
impl PartialEq for SharedStyle {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || *self.0 == *other.0
    }
}
impl From<Style> for SharedStyle {
    fn from(style: Style) -> Self {
        Self::new(style)
    }
}
impl<'de> Deserialize<'de> for SharedStyle {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        #[allow(clippy::large_enum_variant)] // transient, one per decoded prop
        enum Wire {
            Id(u32),
            Inline(Style),
        }
        match Wire::deserialize(d)? {
            Wire::Inline(style) => Ok(Self::new(style)),
            Wire::Id(id) => STYLES.with(|styles| {
                styles
                    .borrow()
                    .get(id as usize)
                    .and_then(|slot| slot.clone())
                    .map(Self)
                    .ok_or_else(|| serde::de::Error::custom(format!("unknown style {id}")))
            }),
        }
    }
}

impl Style {
    /// Whether the style needs GPUI element state (hover, active, or focus).
    pub fn is_interactive(&self) -> bool {
        self.hover.is_some() || self.active.is_some() || self.focus.is_some()
    }
    pub fn apply<E: gpui::Styled>(&self, mut el: E) -> E {
        if let Some(v) = self.width {
            el = el.w(v.length());
        }
        if let Some(v) = self.height {
            el = el.h(v.length());
        }
        if let Some(v) = self.min_width {
            el = el.min_w(v.length());
        }
        if let Some(v) = self.min_height {
            el = el.min_h(v.length());
        }
        if let Some(v) = self.max_width {
            el = el.max_w(v.length());
        }
        if let Some(v) = self.max_height {
            el = el.max_h(v.length());
        }
        if let Some(v) = self.direction {
            el = match v {
                Direction::Row => el.flex_row(),
                Direction::Column => el.flex_col(),
                Direction::RowReverse => el.flex_row_reverse(),
                Direction::ColumnReverse => el.flex_col_reverse(),
            };
        }
        if let Some(v) = self.align {
            el = match v {
                Align::Start => el.items_start(),
                Align::Center => el.items_center(),
                Align::End => el.items_end(),
                Align::Stretch => el.items_stretch(),
            };
        }
        if let Some(v) = self.grow {
            el = el.flex_grow(v);
        }
        if let Some(v) = self.shrink {
            el = el.flex_shrink(v);
        }
        if let Some(v) = self.gap {
            el = el.gap(px(v));
        }
        if let Some(v) = self.padding {
            el = el.p(px(v));
        }
        if let Some(v) = self.padding_x {
            el = el.px(px(v));
        }
        if let Some(v) = self.padding_y {
            el = el.py(px(v));
        }
        if let Some(v) = self.background {
            el = el.bg(v.0);
        }
        if let Some(v) = self.color {
            el = el.text_color(v.0);
        }
        if let Some(v) = self.font_size {
            el = el.text_size(px(v));
        }
        if let Some(v) = self.line_height {
            el = el.line_height(px(v));
        }
        if let Some(v) = self.border_width {
            el = el.border(px(v));
        }
        if let Some(v) = self.border_color {
            el = el.border_color(v.0);
        }
        if let Some(v) = self.radius {
            el = el.rounded(px(v));
        }
        if let Some(v) = self.opacity {
            el = el.opacity(v);
        }
        el
    }
    pub fn apply_interactive<E: gpui::Styled + gpui::StatefulInteractiveElement>(
        &self,
        el: E,
    ) -> E {
        let mut el = self.apply(el);
        if let Some(v) = &self.hover {
            el = el.hover(|el| v.apply(el));
        }
        if let Some(v) = &self.active {
            el = el.active(|el| v.apply(el));
        }
        if let Some(v) = &self.focus {
            el = el.focus(|el| v.apply(el));
        }
        el
    }
}

fn checked_f32<E: serde::de::Error>(value: f64) -> Result<f32, E> {
    let number = value as f32;
    if !number.is_finite() {
        return Err(E::custom("style number exceeds finite f32 geometry"));
    }
    Ok(number)
}
pub fn finite<'de, D: Deserializer<'de>>(d: D) -> Result<f32, D::Error> {
    checked_f32(f64::deserialize(d)?)
}
pub fn optional_finite<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f32>, D::Error> {
    Option::<f64>::deserialize(d)?.map(checked_f32).transpose()
}
