//! The wire format contains committed changes, never a second retained tree.
//!
//! Decoding is one pass: `Registry::parse` reads the transaction text and builds
//! each operation's typed props as it goes, because every operation names its
//! component before its props. See `decode.rs`.
use serde::Serialize;
use serde_json::Value;

/// An encoded transaction. Decoding happens in `Registry::prepare`, which
/// needs the component registry to know each operation's props type.
#[derive(Debug)]
pub struct Transaction(pub(crate) String);

impl Transaction {
    pub fn parse(json: &str) -> serde_json::Result<Self> {
        Ok(Self(json.to_owned()))
    }
    pub fn from_encoded(json: String) -> Self {
        Self(json)
    }
    /// For tests: serialize a JSON value to transaction text.
    pub fn from_json(value: &Value) -> serde_json::Result<Self> {
        Ok(Self(value.to_string()))
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(Debug, Serialize)]
pub struct Reply {
    pub sequence: u64,
    pub retired: Vec<u64>,
    pub results: Vec<CallResult>,
}

#[derive(Debug, Serialize)]
pub struct CallResult {
    pub request: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl CallResult {
    pub(crate) fn new(request: u64, result: anyhow::Result<Value>) -> Self {
        match result {
            Ok(value) => Self {
                request,
                value: Some(value),
                error: None,
            },
            Err(error) => Self {
                request,
                value: None,
                error: Some(format!("{error:#}")),
            },
        }
    }
}
