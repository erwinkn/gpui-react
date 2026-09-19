use gpui::px;
use serde::{Deserialize, Deserializer};

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
impl Style {
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
pub(crate) fn finite<'de, D: Deserializer<'de>>(d: D) -> Result<f32, D::Error> {
    checked_f32(f64::deserialize(d)?)
}
pub(crate) fn optional_finite<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f32>, D::Error> {
    Option::<f64>::deserialize(d)?.map(checked_f32).transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_styles_fail_before_reaching_gpui() {
        assert!(serde_json::from_str::<Style>(r##"{"background":"not-a-color"}"##).is_err());
        assert!(serde_json::from_str::<Style>(r##"{"widht":12}"##).is_err());
        assert!(
            serde_json::from_str::<Style>(r##"{"padding":1e100}"##).is_err(),
            "finite JSON numbers must not overflow GPUI's f32 geometry"
        );
        assert!(serde_json::from_str::<Style>(r##"{"width":1e100}"##).is_err());
    }
}
