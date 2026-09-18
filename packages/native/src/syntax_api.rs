//! Expose the existing bounded native syntax cache to replaceable React token views.
use napi_derive::napi;
#[napi(object)]
pub struct SyntaxToken {
    pub text: String,
    pub kind: String,
    pub start: u32,
    pub end: u32,
}
pub fn tokens(source: &str, path: Option<&str>, language: Option<&str>) -> Vec<Vec<SyntaxToken>> {
    let normalized = source.replace("\r\n", "\n");
    let highlighted = crate::syntax::cache::highlight_cached(&normalized, path, language);
    normalized
        .split('\n')
        .enumerate()
        .map(|(line, text)| {
            let spans = highlighted.as_ref().and_then(|d| d.lines.get(line));
            let mut result = Vec::new();
            let mut cursor = 0;
            let mut utf16 = 0u32;
            let mut append = |value: &str, kind: String| {
                if !value.is_empty() {
                    let count = value.encode_utf16().count() as u32;
                    result.push(SyntaxToken {
                        text: value.to_owned(),
                        kind,
                        start: utf16,
                        end: utf16 + count,
                    });
                    utf16 += count;
                }
            };
            if let Some(spans) = spans {
                for span in spans {
                    let start = span.range.start.min(text.len());
                    let end = span.range.end.min(text.len());
                    if start < cursor
                        || end < start
                        || !text.is_char_boundary(start)
                        || !text.is_char_boundary(end)
                    {
                        continue;
                    }
                    append(&text[cursor..start], "Plain".into());
                    append(&text[start..end], format!("{:?}", span.kind));
                    cursor = end;
                }
            }
            append(&text[cursor..], "Plain".into());
            result
        })
        .collect()
}
