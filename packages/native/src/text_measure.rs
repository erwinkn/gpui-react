//! Font-shaped measurements for preparing compact native layouts before first paint.
pub(crate) fn widths(window: &mut gpui::Window, family: String, size: f64, weight: f64, texts: Vec<String>) -> Vec<f64> {
    let mut font = gpui::font(family);
    font.weight = gpui::FontWeight(weight as f32);
    texts.into_iter().map(|text| {
        text.split('\n').map(|line| {
            let run = gpui::TextRun { len: line.len(), font: font.clone(), color: gpui::black(), background_color: None, underline: None, strikethrough: None };
            let shaped = window.text_system().shape_line(line.to_string().into(), gpui::px(size as f32), &[run], None);
            f32::from(shaped.width) as f64
        }).fold(0.0, f64::max)
    }).collect()
}
