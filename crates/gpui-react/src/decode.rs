//! Transaction decoding. The structural pass borrows each props and value
//! slice from the input text without copying, resolves component names to
//! registry indices without allocating, and decodes each props slice into
//! its typed struct with static serde as soon as it is read. Type-erased
//! single-pass decoding was measured slower: it boxes intermediate values.
//! Command and query values are decoded after the object is complete, so a
//! bad value is a request error, not a transaction failure.
use crate::{
    registry::{Kind, Op, Payload, Prepared, Registry},
    style::{STYLES, Style},
};
use anyhow::Result;
use serde::de::{self, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::value::RawValue;
use std::{fmt, sync::Arc};

/// Decodes one session's transactions. It owns the session's style
/// definitions, which are declared once on the wire and referenced by id, so
/// typed props already hold their shared style when they reach the UI thread.
pub struct Decoder {
    registry: Registry,
    styles: Vec<Option<Arc<Style>>>,
}

impl Decoder {
    pub fn new(registry: Registry) -> Self {
        Self {
            registry,
            styles: Vec::new(),
        }
    }
    pub fn registry(&self) -> &Registry {
        &self.registry
    }
    /// Decode transaction text into typed operations.
    pub fn parse(&mut self, json: &str) -> Result<Prepared> {
        struct Installed<'a>(&'a mut Vec<Option<Arc<Style>>>);
        impl Drop for Installed<'_> {
            fn drop(&mut self) {
                STYLES.with(|cell| std::mem::swap(&mut *cell.borrow_mut(), self.0));
            }
        }
        STYLES.with(|cell| std::mem::swap(&mut *cell.borrow_mut(), &mut self.styles));
        let _installed = Installed(&mut self.styles);
        let mut deserializer = serde_json::Deserializer::from_str(json);
        let prepared = TransactionSeed(&self.registry).deserialize(&mut deserializer)?;
        deserializer.end()?;
        Ok(prepared)
    }
}

fn expecting(f: &mut fmt::Formatter, what: &str) -> fmt::Result {
    f.write_str(what)
}

/// A JSON string matched against a fixed set of names without allocating.
struct Name(&'static [&'static str]);
impl<'de> DeserializeSeed<'de> for Name {
    type Value = usize;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<usize, D::Error> {
        struct V(&'static [&'static str]);
        impl Visitor<'_> for V {
            type Value = usize;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                expecting(f, "a known name")
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<usize, E> {
                self.0
                    .iter()
                    .position(|name| *name == value)
                    .ok_or_else(|| de::Error::unknown_variant(value, self.0))
            }
        }
        d.deserialize_str(V(self.0))
    }
}

const TRANSACTION_FIELDS: &[&str] = &["version", "sequence", "operations"];

struct TransactionSeed<'r>(&'r Registry);
impl<'de> DeserializeSeed<'de> for TransactionSeed<'_> {
    type Value = Prepared;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Prepared, D::Error> {
        d.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for TransactionSeed<'_> {
    type Value = Prepared;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        expecting(f, "a transaction object")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Prepared, A::Error> {
        let mut version = None;
        let mut sequence = None;
        let mut operations = None;
        while let Some(field) = map.next_key_seed(Name(TRANSACTION_FIELDS))? {
            match field {
                0 => version = Some(map.next_value::<u32>()?),
                1 => sequence = Some(map.next_value::<u64>()?),
                _ => operations = Some(map.next_value_seed(OperationsSeed(self.0))?),
            }
        }
        if version != Some(1) {
            return Err(de::Error::custom("unsupported protocol version"));
        }
        Ok(Prepared {
            sequence: sequence.ok_or_else(|| de::Error::missing_field("sequence"))?,
            operations: operations.ok_or_else(|| de::Error::missing_field("operations"))?,
        })
    }
}

struct OperationsSeed<'r>(&'r Registry);
impl<'de> DeserializeSeed<'de> for OperationsSeed<'_> {
    type Value = Vec<Op>;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Vec<Op>, D::Error> {
        d.deserialize_seq(self)
    }
}
impl<'de> Visitor<'de> for OperationsSeed<'_> {
    type Value = Vec<Op>;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        expecting(f, "an array of operations")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<Op>, A::Error> {
        let mut operations = Vec::with_capacity(seq.size_hint().unwrap_or(0));
        while let Some(operation) = seq.next_element_seed(OperationSeed(self.0))? {
            if let Some(operation) = operation {
                operations.push(operation);
            }
        }
        Ok(operations)
    }
}

const OPS: &[&str] = &[
    "create",
    "props",
    "listen",
    "place",
    "remove",
    "hidden",
    "command",
    "query",
    "style",
    "dropStyle",
];
const OPERATION_FIELDS: &[&str] = &[
    "op",
    "id",
    "component",
    "props",
    "subscription",
    "parent",
    "child",
    "before",
    "hidden",
    "request",
    "value",
    "style",
];

struct OperationSeed<'r>(&'r Registry);
impl<'de> DeserializeSeed<'de> for OperationSeed<'_> {
    type Value = Option<Op>;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Option<Op>, D::Error> {
        d.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for OperationSeed<'_> {
    type Value = Option<Op>;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        expecting(f, "a native operation object")
    }
    /// Style definitions are consumed here and produce no operation: later
    /// props in the same session resolve their style id to the shared value.
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Option<Op>, A::Error> {
        let registry = self.0;
        let mut op = None;
        let mut id = None;
        let mut kind = None;
        let mut props: Option<Payload> = None;
        let mut subscription: Option<Option<u64>> = None;
        let mut parent: Option<Option<u32>> = None;
        let mut child = None;
        let mut before: Option<Option<u32>> = None;
        let mut hidden = None;
        let mut request = None;
        let mut value: Option<&'de RawValue> = None;
        let mut style: Option<Style> = None;
        while let Some(field) = map.next_key_seed(Name(OPERATION_FIELDS))? {
            match field {
                0 => op = Some(map.next_value_seed(Name(OPS))?),
                1 => id = Some(map.next_value()?),
                2 => kind = Some(map.next_value_seed(ComponentSeed(registry))?),
                3 => {
                    let kind = kind.ok_or_else(|| {
                        de::Error::custom("component must precede props in an operation")
                    })?;
                    let raw: &'de RawValue = map.next_value()?;
                    props = Some(
                        registry
                            .binding(kind)
                            .decode(Kind::Props, raw)
                            .map_err(|error| de::Error::custom(format!("{error:#}")))?,
                    );
                }
                4 => subscription = Some(map.next_value()?),
                5 => parent = Some(map.next_value()?),
                6 => child = Some(map.next_value()?),
                7 => before = Some(map.next_value()?),
                8 => hidden = Some(map.next_value()?),
                9 => request = Some(map.next_value()?),
                10 => value = Some(map.next_value()?),
                _ => style = Some(map.next_value()?),
            }
        }
        fn need<T, E: de::Error>(value: Option<T>, name: &'static str) -> Result<T, E> {
            value.ok_or_else(|| de::Error::missing_field(name))
        }
        let op = need(op, "op")?;
        if op >= 8 {
            let id: u32 = need(id, "id")?;
            let definition = if op == 8 {
                Some(Arc::new(need(style, "style")?))
            } else {
                None
            };
            STYLES.with(|styles| {
                let mut styles = styles.borrow_mut();
                let slot = id as usize;
                if slot >= styles.len() {
                    if definition.is_none() {
                        return Ok(());
                    }
                    if slot > styles.len() + 4096 {
                        return Err(de::Error::custom("style id skips too far"));
                    }
                    styles.resize(slot + 1, None);
                }
                styles[slot] = definition;
                Ok(())
            })?;
            return Ok(None);
        }
        let call = |command: bool| -> Result<Op, A::Error> {
            let kind = need(kind, "component")?;
            let raw = need(value, "value")?;
            let value = registry
                .binding(kind)
                .decode(if command { Kind::Command } else { Kind::Query }, raw);
            Ok(Op::Call {
                id: need(id, "id")?,
                kind,
                request: need(request, "request")?,
                command,
                value,
            })
        };
        Ok(Some(match op {
            0 => Op::Create {
                id: need(id, "id")?,
                kind: need(kind, "component")?,
                props: need(props, "props")?,
                subscription: subscription.flatten(),
                place: parent.map(|parent| (parent, before.flatten())),
            },
            1 => Op::Props {
                id: need(id, "id")?,
                kind: need(kind, "component")?,
                props: need(props, "props")?,
            },
            2 => Op::Listen {
                id: need(id, "id")?,
                subscription: subscription.flatten(),
            },
            3 => Op::Place {
                parent: parent.flatten(),
                child: need(child, "child")?,
                before: before.flatten(),
            },
            4 => Op::Remove {
                id: need(id, "id")?,
            },
            5 => Op::Hidden {
                id: need(id, "id")?,
                hidden: need(hidden, "hidden")?,
            },
            6 => call(true)?,
            _ => call(false)?,
        }))
    }
}

/// Resolves a component name to its registry index without allocating.
struct ComponentSeed<'r>(&'r Registry);
impl<'de> DeserializeSeed<'de> for ComponentSeed<'_> {
    type Value = u16;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<u16, D::Error> {
        d.deserialize_str(self)
    }
}
impl Visitor<'_> for ComponentSeed<'_> {
    type Value = u16;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        expecting(f, "a registered component name")
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<u16, E> {
        self.0.kind(value).map_err(de::Error::custom)
    }
}
