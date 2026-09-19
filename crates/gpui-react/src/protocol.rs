//! The wire format contains committed changes, never a second retained tree.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transaction {
    pub version: u32,
    pub sequence: u64,
    pub operations: Vec<Operation>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", deny_unknown_fields)]
pub enum Operation {
    Create {
        id: u64,
        component: String,
        props: Value,
        subscription: Option<u64>,
    },
    Props {
        id: u64,
        props: Value,
    },
    Listen {
        id: u64,
        subscription: Option<u64>,
    },
    Place {
        parent: Option<u64>,
        child: u64,
        before: Option<u64>,
    },
    Remove {
        id: u64,
    },
    Hidden {
        id: u64,
        hidden: bool,
    },
    Command {
        id: u64,
        request: u64,
        value: Value,
    },
    Query {
        id: u64,
        request: u64,
        value: Value,
    },
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
