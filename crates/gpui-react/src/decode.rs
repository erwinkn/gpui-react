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
    wire,
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
        self.with_styles(|registry| {
            let mut deserializer = serde_json::Deserializer::from_str(json);
            let prepared = TransactionSeed(registry).deserialize(&mut deserializer)?;
            deserializer.end()?;
            Ok(prepared)
        })
    }
    /// Decode a binary transaction into typed operations. See `wire`.
    pub fn parse_binary(&mut self, bytes: &[u8]) -> Result<Prepared> {
        self.with_styles(|registry| {
            let mut reader = wire::Reader::new(bytes)?;
            let prepared = read_transaction(registry, &mut reader)?;
            if !reader.finished() {
                anyhow::bail!("wire transaction has trailing bytes");
            }
            Ok(prepared)
        })
    }
    /// Runs a decode with this session's style definitions installed.
    fn with_styles<R>(&mut self, decode: impl FnOnce(&Registry) -> R) -> R {
        struct Installed<'a>(&'a mut Vec<Option<Arc<Style>>>);
        impl Drop for Installed<'_> {
            fn drop(&mut self) {
                STYLES.with(|cell| std::mem::swap(&mut *cell.borrow_mut(), self.0));
            }
        }
        STYLES.with(|cell| std::mem::swap(&mut *cell.borrow_mut(), &mut self.styles));
        let _installed = Installed(&mut self.styles);
        decode(&self.registry)
    }
}

/// Records or drops a style definition in the installed session table.
fn install_style(id: u32, definition: Option<Arc<Style>>) -> Result<(), &'static str> {
    STYLES.with(|styles| {
        let mut styles = styles.borrow_mut();
        let slot = id as usize;
        if slot >= styles.len() {
            if definition.is_none() {
                return Ok(());
            }
            if slot > styles.len() + 4096 {
                return Err("style id skips too far");
            }
            styles.resize(slot + 1, None);
        }
        styles[slot] = definition;
        Ok(())
    })
}

fn read_transaction(registry: &Registry, reader: &mut wire::Reader<'_>) -> Result<Prepared> {
    use serde::Deserialize as _;
    if reader.u8()? != 1 {
        anyhow::bail!("unsupported protocol version");
    }
    let sequence = reader.u32()? as u64;
    let count = reader.u32()? as usize;
    let mut operations = Vec::with_capacity(count.min(1 << 20));
    let kinds = registry.len() as u16;
    let component = |reader: &mut wire::Reader<'_>| -> Result<u16> {
        let kind = reader.u16()?;
        anyhow::ensure!(kind < kinds, "unknown component kind {kind}");
        Ok(kind)
    };
    for _ in 0..count {
        let tag = reader.u8()?;
        let operation = match tag {
            1 => {
                let id = reader.u32()?;
                let kind = component(reader)?;
                let subscription = reader.id()?.map(u64::from);
                let parent = reader.u32()?;
                let before = reader.id()?;
                let props = registry.binding(kind).decode_wire(Kind::Props, reader)?;
                Op::Create {
                    id,
                    kind,
                    props,
                    subscription,
                    place: match parent {
                        wire::NONE => None,
                        wire::NULL => Some((None, before)),
                        parent => Some((Some(parent), before)),
                    },
                }
            }
            2 => {
                let id = reader.u32()?;
                let kind = component(reader)?;
                let props = registry.binding(kind).decode_wire(Kind::Props, reader)?;
                Op::Props { id, kind, props }
            }
            3 => Op::Listen {
                id: reader.u32()?,
                subscription: reader.id()?.map(u64::from),
            },
            4 => Op::Place {
                parent: reader.id()?,
                child: reader.u32()?,
                before: reader.id()?,
            },
            5 => Op::Remove { id: reader.u32()? },
            6 => Op::Hidden {
                id: reader.u32()?,
                hidden: reader.u8()? != 0,
            },
            7 | 8 => {
                let id = reader.u32()?;
                let kind = component(reader)?;
                let request = reader.u32()? as u64;
                let command = tag == 7;
                let value = registry
                    .binding(kind)
                    .decode_wire(if command { Kind::Command } else { Kind::Query }, reader);
                // A schema failure inside a value leaves the stream position
                // undefined, so it fails the transaction here rather than
                // becoming a request error.
                if value.is_err() {
                    anyhow::bail!("wire {} value failed to decode", if command { "command" } else { "query" });
                }
                Op::Call { id, kind, request, command, value }
            }
            9 => {
                let id = reader.u32()?;
                let style = Style::deserialize(&mut *reader)?;
                install_style(id, Some(Arc::new(style))).map_err(anyhow::Error::msg)?;
                continue;
            }
            10 => {
                install_style(reader.u32()?, None).map_err(anyhow::Error::msg)?;
                continue;
            }
            tag => anyhow::bail!("unknown wire operation tag {tag}"),
        };
        operations.push(operation);
    }
    Ok(Prepared { sequence, operations })
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
            install_style(id, definition).map_err(de::Error::custom)?;
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

/// Both wires must decode a real mount into the same typed operations. The
/// payloads are what the JavaScript bridge sealed for the frame-cost scene
/// (`fixtures/bridge-counter/js-wire-dump.tsx`); the props types here mirror
/// the controls' field lists exactly, because the binary wire is positional.
/// The test is skipped when the dumps are absent.
#[cfg(test)]
mod wire_equivalence {
    use super::Decoder;
    use crate::{
        ElementContext, ReactElement, RenderContext,
        registry::{HostElement, Op, Payload, Registry},
        style::SharedStyle,
    };
    use gpui::{AnyElement, IntoElement as _, ParentElement as _};
    use serde::Deserialize;
    use serde_json::Value;

    macro_rules! mirror {
        ($name:ident { $($field:ident: $ty:ty),* $(,)? }) => {
            #[derive(Debug, Default, Deserialize, gpui_react_macros::ComponentProps)]
            #[wire(crate = "crate")]
            #[serde(default, rename_all = "camelCase", deny_unknown_fields)]
            #[allow(dead_code)]
            struct $name { $($field: $ty),* }
            impl ReactElement for $name {
                type Props = $name;
                type Extras = ();
                fn create(props: $name, _: &mut (), _: &mut ElementContext) -> Self { props }
                fn set_props(&mut self, _: $name, _: &mut (), _: &mut ElementContext) {}
                fn render(&self, _: &(), _: &mut RenderContext) -> AnyElement { gpui::div().child("").into_any_element() }
            }
        };
    }
    mirror!(Document { style: SharedStyle, search: Option<Value>, selection_color: Option<Value> });
    mirror!(List { style: SharedStyle, item_count: Option<usize>, window_start: usize, estimated_item_height: Option<f32>, overdraw: Option<f32>, alignment: Value, follow_tail: bool });
    mirror!(Container { style: SharedStyle, scroll: Value, focusable: bool, label: String, scroll_group: Option<String>, block_mouse: bool, measure: bool });
    mirror!(Text { text: String, style: SharedStyle, text_key: Option<String>, selectable: bool, searchable: bool, match_index_offset: Option<u32>, measure: bool });
    mirror!(Input { initial_value: String, initial_multiline: bool, placeholder: String, label: String, read_only: bool, min_rows: Option<usize>, max_rows: Option<usize>, submit_on_enter: bool, capture_keys: Value, style: SharedStyle, caret_color: Option<Value>, selection_color: Option<Value> });

    fn decoder() -> Decoder {
        let mut registry = Registry::default();
        registry.register(HostElement::<Document>::new("document").children()).unwrap();
        registry.register(HostElement::<List>::new("list").children()).unwrap();
        registry.register(HostElement::<Container>::new("container").children()).unwrap();
        registry.register(HostElement::<Text>::new("text")).unwrap();
        registry.register(HostElement::<Input>::new("input")).unwrap();
        Decoder::new(registry)
    }
    fn props(kind: u16, payload: &Payload) -> String {
        match kind {
            0 => format!("{:?}", payload.downcast_ref::<Document>().unwrap()),
            1 => format!("{:?}", payload.downcast_ref::<List>().unwrap()),
            2 => format!("{:?}", payload.downcast_ref::<Container>().unwrap()),
            3 => format!("{:?}", payload.downcast_ref::<Text>().unwrap()),
            _ => format!("{:?}", payload.downcast_ref::<Input>().unwrap()),
        }
    }
    fn describe(op: &Op) -> String {
        match op {
            Op::Create { id, kind, props: p, subscription, place } => format!("create {id} {kind} {subscription:?} {place:?} {}", props(*kind, p)),
            Op::Props { id, kind, props: p } => format!("props {id} {kind} {}", props(*kind, p)),
            Op::Listen { id, subscription } => format!("listen {id} {subscription:?}"),
            Op::Place { parent, child, before } => format!("place {parent:?} {child} {before:?}"),
            Op::Remove { id } => format!("remove {id}"),
            Op::Hidden { id, hidden } => format!("hidden {id} {hidden}"),
            Op::Call { id, kind, request, command, .. } => format!("call {id} {kind} {request} {command}"),
        }
    }

    #[test]
    fn binary_and_json_wires_decode_alike() {
        let dir = std::env::var("GPUI_REACT_WIRE_DIR").unwrap_or_else(|_| "/tmp/gpui-react-wire".into());
        for scene in ["flow", "list"] {
            let Ok(json) = std::fs::read_to_string(format!("{dir}/mount-{scene}-1000.json")) else {
                eprintln!("skipped: no wire dump in {dir}");
                return;
            };
            let bytes = std::fs::read(format!("{dir}/mount-{scene}-1000.bin")).unwrap();
            let (mut from_json, mut from_binary) = (decoder(), decoder());
            let a = from_json.parse(&json).unwrap();
            let b = from_binary.parse_binary(&bytes).unwrap();
            assert_eq!(a.sequence, b.sequence);
            assert_eq!(a.operations.len(), b.operations.len());
            assert!(a.operations.len() > 1000);
            for (x, y) in a.operations.iter().zip(&b.operations) {
                assert_eq!(describe(x), describe(y));
            }
            assert!(a.operations.iter().any(|op| matches!(op, Op::Create { kind: 3, .. })));
            assert_eq!(from_json.styles.len(), from_binary.styles.len());
            assert!(from_json.styles.len() >= 3);
            for (x, y) in from_json.styles.iter().zip(&from_binary.styles) {
                assert_eq!(x.as_deref(), y.as_deref());
            }
        }
    }
}
