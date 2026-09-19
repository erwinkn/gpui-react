//! Prints the sizes of GPUI types to match allocation histograms.
use std::mem::size_of;
fn main() {
    println!("Style {}", size_of::<gpui::Style>());
    println!("StyleRefinement {}", size_of::<gpui::StyleRefinement>());
    println!("TextStyle {}", size_of::<gpui::TextStyle>());
    println!("TextStyleRefinement {}", size_of::<gpui::TextStyleRefinement>());
    println!("TextRun {}", size_of::<gpui::TextRun>());
    println!("Font {}", size_of::<gpui::Font>());
    println!("LineLayout {}", size_of::<gpui::LineLayout>());
    println!("WrappedLine {}", size_of::<gpui::WrappedLine>());
    println!("ShapedRun {}", size_of::<gpui::ShapedRun>());
    println!("ShapedGlyph {}", size_of::<gpui::ShapedGlyph>());
    println!("ShapedLine {}", size_of::<gpui::ShapedLine>());
    println!("Hitbox {}", size_of::<gpui::Hitbox>());
    println!("ElementId {}", size_of::<gpui::ElementId>());
    println!("GlobalElementId {}", size_of::<gpui::GlobalElementId>());
    println!("SharedString {}", size_of::<gpui::SharedString>());
    println!("AnyElement {}", size_of::<gpui::AnyElement>());
    println!("Div {}", size_of::<gpui::Div>());
    println!("Bounds<Pixels> {}", size_of::<gpui::Bounds<gpui::Pixels>>());
    println!("TextLayout {}", size_of::<gpui::TextLayout>());
    println!("StyledText {}", size_of::<gpui::StyledText>());
}
