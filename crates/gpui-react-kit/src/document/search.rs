use crate::Color;
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Deserializer, Serialize};
use std::{ops::Range, sync::Arc};

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Query {
    pub query: String,
    pub regex: bool,
    pub case_sensitive: bool,
    pub whole_word: bool,
}
#[derive(Debug, Clone)]
pub struct Search {
    pub(crate) matcher: Arc<Matcher>,
    pub active_index: Option<usize>,
    pub match_index_offset: usize,
    pub color: Color,
    pub active_color: Color,
}
impl Search {
    pub fn new(query: Query) -> Result<Self, regex::Error> {
        Ok(Self {
            matcher: Arc::new(Matcher::new(query)?),
            active_index: None,
            match_index_offset: 0,
            color: Color(gpui::rgba(0xffd54d66).into()),
            active_color: Color(gpui::rgba(0xff9900aa).into()),
        })
    }
    pub fn query(&self) -> &Query {
        &self.matcher.query
    }
}
#[derive(Debug)]
pub struct Matcher {
    pub query: Query,
    expression: Regex,
}
impl Matcher {
    pub fn new(query: Query) -> Result<Self, regex::Error> {
        let mut pattern = if query.regex {
            query.query.clone()
        } else {
            regex::escape(&query.query)
        };
        if query.whole_word {
            pattern = format!(r"\b(?:{pattern})\b");
        }
        Ok(Self {
            expression: RegexBuilder::new(&pattern)
                .case_insensitive(!query.case_sensitive)
                .build()?,
            query,
        })
    }
    pub fn ranges(&self, text: &str) -> Arc<[Range<usize>]> {
        if self.query.query.is_empty() {
            return Arc::from([]);
        }
        self.expression
            .find_iter(text)
            .filter(|m| !m.is_empty())
            .map(|m| m.range())
            .collect()
    }
}
impl<'de> Deserialize<'de> for Search {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Default, Deserialize)]
        #[serde(default, rename_all = "camelCase", deny_unknown_fields)]
        struct Raw {
            query: String,
            regex: bool,
            case_sensitive: bool,
            whole_word: bool,
            active_index: Option<usize>,
            match_index_offset: usize,
            color: Option<Color>,
            active_color: Option<Color>,
        }
        let raw = Raw::deserialize(deserializer)?;
        let query = Query {
            query: raw.query,
            regex: raw.regex,
            case_sensitive: raw.case_sensitive,
            whole_word: raw.whole_word,
        };
        let matcher = Matcher::new(query).map_err(serde::de::Error::custom)?;
        Ok(Self {
            matcher: Arc::new(matcher),
            active_index: raw.active_index,
            match_index_offset: raw.match_index_offset,
            color: raw.color.unwrap_or(Color(gpui::rgba(0xffd54d66).into())),
            active_color: raw
                .active_color
                .unwrap_or(Color(gpui::rgba(0xff9900aa).into())),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn search_validates_regex_and_never_matches_empty_ranges() {
        assert!(serde_json::from_value::<Search>(json!({"query":"[","regex":true})).is_err());
        assert!(
            serde_json::from_value::<Search>(json!({"query":"a","matchIndexOffset":-1})).is_err()
        );
        let search: Search =
            serde_json::from_value(json!({"query":"token|$","regex":true})).unwrap();
        assert_eq!(&*search.matcher.ranges("token token"), &[0..5, 6..11]);
    }
    #[test]
    fn literal_case_word_and_unicode_matching() {
        let search: Search =
            serde_json::from_value(json!({"query":"TOKEN","wholeWord":true})).unwrap();
        assert_eq!(
            &*search.matcher.ranges("token tokens TOKEN"),
            &[0..5, 13..18]
        );
        let search: Search = serde_json::from_value(json!({"query":"[😀]"})).unwrap();
        assert_eq!(
            &*search.matcher.ranges("x [😀] y"),
            std::slice::from_ref(&(2..8))
        );
        let search: Search =
            serde_json::from_value(json!({"query":"é","caseSensitive":true})).unwrap();
        assert_eq!(&*search.matcher.ranges("Éé"), std::slice::from_ref(&(2..4)));
    }
}
