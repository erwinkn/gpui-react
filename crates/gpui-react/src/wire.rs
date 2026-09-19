//! Binary transaction wire. A flat operation stream; props and values are a
//! self-describing tree of tagged values with map keys interned into a table
//! at the end of the buffer. `Reader` is a serde `Deserializer`, so every
//! component's typed props decode from it with the same derive as from JSON,
//! and strings are borrowed from the buffer until the typed value copies them.
use serde::de::{
    self, DeserializeSeed, EnumAccess, IntoDeserializer, MapAccess, SeqAccess, VariantAccess,
    Visitor, value::BorrowedStrDeserializer,
};
use std::fmt;

pub(crate) const T_NULL: u8 = 0;
pub(crate) const T_FALSE: u8 = 1;
pub(crate) const T_TRUE: u8 = 2;
pub(crate) const T_INT: u8 = 3;
pub(crate) const T_F64: u8 = 4;
pub(crate) const T_STR: u8 = 5;
pub(crate) const T_ARR: u8 = 6;
pub(crate) const T_MAP: u8 = 7;

/// `u32` sentinel for an absent or null id.
pub(crate) const NONE: u32 = u32::MAX;
/// `u32` sentinel for an explicit null where absent is also possible.
pub(crate) const NULL: u32 = u32::MAX - 1;

#[derive(Debug)]
pub struct Error(String);
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}
impl de::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Self(msg.to_string())
    }
}
type Result<T, E = Error> = std::result::Result<T, E>;

fn short() -> Error {
    Error("wire ends early".into())
}

pub struct Reader<'de> {
    bytes: &'de [u8],
    at: usize,
    /// Where the key table starts; the operation stream ends here.
    end: usize,
    keys: Vec<&'de str>,
}

impl<'de> Reader<'de> {
    /// Reads the key table from the trailer and positions at the first byte.
    pub fn new(bytes: &'de [u8]) -> Result<Self> {
        if bytes.len() < 4 {
            return Err(short());
        }
        let table = u32::from_le_bytes(bytes[bytes.len() - 4..].try_into().unwrap()) as usize;
        if table + 2 > bytes.len() - 4 {
            return Err(Error("wire key table is out of range".into()));
        }
        // The table is read first, then the stream is bounded to end before it.
        let mut reader = Self {
            bytes: &bytes[..bytes.len() - 4],
            at: table,
            end: bytes.len() - 4,
            keys: Vec::new(),
        };
        let count = reader.u16()? as usize;
        reader.keys.reserve_exact(count);
        for _ in 0..count {
            let key = reader.str()?;
            reader.keys.push(key);
        }
        if reader.at != reader.bytes.len() {
            return Err(Error("wire key table has trailing bytes".into()));
        }
        reader.at = 0;
        reader.end = table;
        Ok(reader)
    }
    /// True when the operation stream has been consumed exactly.
    pub fn finished(&self) -> bool {
        self.at == self.end
    }
    fn take(&mut self, n: usize) -> Result<&'de [u8]> {
        let end = self.at.checked_add(n).ok_or_else(short)?;
        if end > self.end {
            return Err(short());
        }
        let slice = &self.bytes[self.at..end];
        self.at = end;
        Ok(slice)
    }
    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub fn f64(&mut self) -> Result<f64> {
        Ok(f64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    pub fn str(&mut self) -> Result<&'de str> {
        let len = self.u32()? as usize;
        std::str::from_utf8(self.take(len)?).map_err(|_| Error("wire string is not UTF-8".into()))
    }
    /// A nullable id: `NONE` reads as `None`.
    pub fn id(&mut self) -> Result<Option<u32>> {
        Ok(match self.u32()? {
            NONE => None,
            id => Some(id),
        })
    }
    fn peek(&self) -> Result<u8> {
        self.bytes.get(self.at).copied().filter(|_| self.at < self.end).ok_or_else(short)
    }
    fn key(&mut self) -> Result<&'de str> {
        let index = self.u16()? as usize;
        self.keys.get(index).copied().ok_or_else(|| Error("wire key index is out of range".into()))
    }
}

impl<'de> de::Deserializer<'de> for &mut Reader<'de> {
    type Error = Error;
    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self.u8()? {
            T_NULL => visitor.visit_unit(),
            T_FALSE => visitor.visit_bool(false),
            T_TRUE => visitor.visit_bool(true),
            T_INT => visitor.visit_i32(self.i32()?),
            T_F64 => visitor.visit_f64(self.f64()?),
            T_STR => visitor.visit_borrowed_str(self.str()?),
            T_ARR => {
                let remaining = self.u32()? as usize;
                visitor.visit_seq(Seq { reader: self, remaining })
            }
            T_MAP => {
                let remaining = self.u16()? as usize;
                visitor.visit_map(Map { reader: self, remaining })
            }
            tag => Err(Error(format!("unknown wire value tag {tag}"))),
        }
    }
    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        if self.peek()? == T_NULL {
            self.at += 1;
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }
    fn deserialize_newtype_struct<V: Visitor<'de>>(self, _: &'static str, visitor: V) -> Result<V::Value> {
        visitor.visit_newtype_struct(self)
    }
    /// A unit variant is a string; other variants are a one-entry map.
    fn deserialize_enum<V: Visitor<'de>>(self, _: &'static str, _: &'static [&'static str], visitor: V) -> Result<V::Value> {
        match self.u8()? {
            T_STR => visitor.visit_enum(self.str()?.into_deserializer()),
            T_MAP => {
                if self.u16()? != 1 {
                    return Err(Error("enum map must have one entry".into()));
                }
                visitor.visit_enum(Enum { reader: self })
            }
            _ => Err(Error("expected a string or map for an enum".into())),
        }
    }
    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf unit unit_struct seq tuple tuple_struct map struct
        identifier ignored_any
    }
}

struct Seq<'a, 'de> {
    reader: &'a mut Reader<'de>,
    remaining: usize,
}
impl<'de> SeqAccess<'de> for Seq<'_, 'de> {
    type Error = Error;
    fn next_element_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<Option<T::Value>> {
        if self.remaining == 0 {
            return Ok(None);
        }
        self.remaining -= 1;
        seed.deserialize(&mut *self.reader).map(Some)
    }
    fn size_hint(&self) -> Option<usize> {
        Some(self.remaining)
    }
}

struct Map<'a, 'de> {
    reader: &'a mut Reader<'de>,
    remaining: usize,
}
impl<'de> MapAccess<'de> for Map<'_, 'de> {
    type Error = Error;
    fn next_key_seed<K: DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>> {
        if self.remaining == 0 {
            return Ok(None);
        }
        self.remaining -= 1;
        let key = self.reader.key()?;
        seed.deserialize(BorrowedStrDeserializer::new(key)).map(Some)
    }
    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value> {
        seed.deserialize(&mut *self.reader)
    }
    fn size_hint(&self) -> Option<usize> {
        Some(self.remaining)
    }
}

struct Enum<'a, 'de> {
    reader: &'a mut Reader<'de>,
}
impl<'de> EnumAccess<'de> for Enum<'_, 'de> {
    type Error = Error;
    type Variant = Self;
    fn variant_seed<V: DeserializeSeed<'de>>(self, seed: V) -> Result<(V::Value, Self)> {
        let key = self.reader.key()?;
        let variant = seed.deserialize(BorrowedStrDeserializer::new(key))?;
        Ok((variant, self))
    }
}
impl<'de> VariantAccess<'de> for Enum<'_, 'de> {
    type Error = Error;
    fn unit_variant(self) -> Result<()> {
        de::Deserialize::deserialize(&mut *self.reader)
    }
    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value> {
        seed.deserialize(&mut *self.reader)
    }
    fn tuple_variant<V: Visitor<'de>>(self, _: usize, visitor: V) -> Result<V::Value> {
        de::Deserializer::deserialize_any(&mut *self.reader, visitor)
    }
    fn struct_variant<V: Visitor<'de>>(self, _: &'static [&'static str], visitor: V) -> Result<V::Value> {
        de::Deserializer::deserialize_any(&mut *self.reader, visitor)
    }
}

// ---- props schema ----------------------------------------------------------

/// How one prop travels. Integers are 4 bytes, floats 4 or 8, strings are
/// length-prefixed UTF-8, a style is the id of a definition sent earlier, and
/// a value is the tagged tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WireType {
    Bool,
    I32,
    U32,
    F32,
    F64,
    Str,
    Style,
    Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Field {
    pub name: &'static str,
    #[serde(rename = "type")]
    pub kind: WireType,
    pub required: bool,
}

/// A component's props on the wire: positional fields from the derive, or a
/// self-describing map for props types without a schema.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Schema {
    Fields(&'static [Field]),
    Map,
}
impl serde::Serialize for Schema {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Schema::Fields(fields) => fields.serialize(s),
            Schema::Map => s.serialize_none(),
        }
    }
}

/// Implemented by `#[derive(ComponentProps)]` on every props struct. The worker
/// reads the props object by these names in this order.
pub trait ComponentProps {
    const SCHEMA: Schema;
}
impl ComponentProps for () {
    const SCHEMA: Schema = Schema::Fields(&[]);
}
impl ComponentProps for serde_json::Map<String, serde_json::Value> {
    const SCHEMA: Schema = Schema::Map;
}
impl ComponentProps for serde_json::Value {
    const SCHEMA: Schema = Schema::Map;
}

/// Checks a derived schema against serde's own field list for the same type:
/// same names in the same order. A mismatch means the derive and serde read
/// the struct differently, which would misplace fields on the wire.
pub fn verify<T: ComponentProps + serde::de::DeserializeOwned>() -> anyhow::Result<()> {
    let Schema::Fields(fields) = T::SCHEMA else {
        return Ok(());
    };
    let Some(serde_fields) = serde_fields::<T>() else {
        // Not a struct derive (unit, newtype, map): nothing positional to check.
        anyhow::ensure!(fields.is_empty(), "ComponentProps lists fields but serde does not deserialize a struct");
        return Ok(());
    };
    let names: Vec<&str> = fields.iter().map(|f| f.name).collect();
    anyhow::ensure!(
        names == serde_fields,
        "ComponentProps fields {names:?} differ from serde fields {serde_fields:?}"
    );
    Ok(())
}

/// The field list a serde struct derive asks for, captured from its
/// `deserialize_struct` call.
fn serde_fields<T: serde::de::DeserializeOwned>() -> Option<Vec<&'static str>> {
    struct Capture(std::cell::Cell<Option<&'static [&'static str]>>);
    impl<'de> de::Deserializer<'de> for &Capture {
        type Error = Error;
        fn deserialize_struct<V: Visitor<'de>>(self, _: &'static str, fields: &'static [&'static str], _: V) -> Result<V::Value> {
            self.0.set(Some(fields));
            Err(Error("captured".into()))
        }
        fn deserialize_any<V: Visitor<'de>>(self, _: V) -> Result<V::Value> {
            Err(Error("not a struct".into()))
        }
        serde::forward_to_deserialize_any! {
            bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
            bytes byte_buf option unit unit_struct newtype_struct seq tuple
            tuple_struct map enum identifier ignored_any
        }
    }
    let capture = Capture(std::cell::Cell::new(None));
    let _ = T::deserialize(&capture);
    capture.0.get().map(|fields| fields.to_vec())
}

#[cfg(test)]
mod schema_tests {
    use super::{Field, Schema, ComponentProps, WireType, verify};
    use crate::style::SharedStyle;
    use gpui::SharedString;
    use serde::Deserialize;

    #[derive(Deserialize, gpui_react_macros::ComponentProps)]
    #[wire(crate = "crate")]
    #[serde(rename_all = "camelCase")]
    #[allow(dead_code)]
    struct Every {
        flag: bool,
        count: usize,
        signed: i32,
        ratio: f32,
        wide: f64,
        text: String,
        shared: SharedString,
        style: SharedStyle,
        maybe_text: Option<String>,
        #[serde(default)]
        defaulted: u32,
        #[serde(rename = "renamed")]
        original_name: bool,
        #[serde(skip)]
        hidden: u32,
        tree: Vec<u32>,
        boxed: Box<f32>,
    }
    #[derive(Default, Deserialize, gpui_react_macros::ComponentProps)]
    #[wire(crate = "crate")]
    #[serde(default)]
    #[allow(dead_code)]
    struct AllDefault {
        value: u32,
    }

    #[test]
    fn derive_maps_types_names_and_required() {
        let Schema::Fields(fields) = Every::SCHEMA else { panic!() };
        let expect = |name: &str, kind: WireType, required: bool| Field { name: Box::leak(name.to_owned().into_boxed_str()), kind, required };
        let want = [
            expect("flag", WireType::Bool, true),
            expect("count", WireType::U32, true),
            expect("signed", WireType::I32, true),
            expect("ratio", WireType::F32, true),
            expect("wide", WireType::F64, true),
            expect("text", WireType::Str, true),
            expect("shared", WireType::Str, true),
            expect("style", WireType::Style, true),
            expect("maybeText", WireType::Str, false),
            expect("defaulted", WireType::U32, false),
            expect("renamed", WireType::Bool, true),
            expect("tree", WireType::Value, true),
            expect("boxed", WireType::F32, true),
        ];
        assert_eq!(fields, &want);
        verify::<Every>().unwrap();
        let Schema::Fields(fields) = AllDefault::SCHEMA else { panic!() };
        assert!(!fields[0].required);
        verify::<AllDefault>().unwrap();
        assert_eq!(<() as ComponentProps>::SCHEMA, Schema::Fields(&[]));
        verify::<()>().unwrap();
        verify::<serde_json::Map<String, serde_json::Value>>().unwrap();
    }

    /// A hand-written schema that disagrees with serde must be caught.
    #[test]
    fn verify_rejects_a_mismatch() {
        #[derive(Deserialize)]
        #[allow(dead_code)]
        struct Real {
            a: u32,
            b: u32,
        }
        impl ComponentProps for Real {
            const SCHEMA: Schema = Schema::Fields(&[Field { name: "b", kind: WireType::U32, required: true }, Field { name: "a", kind: WireType::U32, required: true }]);
        }
        let error = verify::<Real>().unwrap_err().to_string();
        assert!(error.contains("differ"), "{error}");
    }
}

// ---- positional props ------------------------------------------------------

/// Decodes one component's props written positionally against its schema: a
/// presence mask, then each present field in schema order as its wire type.
/// It feeds the props struct's own `Deserialize` derive as a map of
/// `(name, value)` pairs, so the derive, its defaults, and its custom field
/// deserializers are unchanged.
pub struct PropsReader<'a, 'de> {
    pub reader: &'a mut Reader<'de>,
    pub fields: &'static [Field],
}

impl<'de> de::Deserializer<'de> for PropsReader<'_, 'de> {
    type Error = Error;
    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let bytes = self.fields.len().div_ceil(8);
        let mut mask = 0u32;
        for i in 0..bytes {
            mask |= (self.reader.u8()? as u32) << (8 * i);
        }
        visitor.visit_map(PropsMap { reader: self.reader, fields: self.fields, mask, next: 0 })
    }
    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

struct PropsMap<'a, 'de> {
    reader: &'a mut Reader<'de>,
    fields: &'static [Field],
    mask: u32,
    next: usize,
}
impl<'de> MapAccess<'de> for PropsMap<'_, 'de> {
    type Error = Error;
    fn next_key_seed<K: DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>> {
        while self.next < self.fields.len() && self.mask & (1 << self.next) == 0 {
            self.next += 1;
        }
        if self.next >= self.fields.len() {
            return Ok(None);
        }
        let name = self.fields[self.next].name;
        seed.deserialize(BorrowedStrDeserializer::new(name)).map(Some)
    }
    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value> {
        let field = self.fields[self.next];
        self.next += 1;
        match field.kind {
            WireType::Value => seed.deserialize(&mut *self.reader),
            kind => seed.deserialize(FieldReader { reader: self.reader, kind }),
        }
    }
}

/// One scalar field read as its declared wire type, with no tag byte.
struct FieldReader<'a, 'de> {
    reader: &'a mut Reader<'de>,
    kind: WireType,
}
impl<'de> de::Deserializer<'de> for FieldReader<'_, 'de> {
    type Error = Error;
    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self.kind {
            WireType::Bool => visitor.visit_bool(self.reader.u8()? != 0),
            WireType::I32 => visitor.visit_i32(self.reader.i32()?),
            WireType::U32 | WireType::Style => visitor.visit_u32(self.reader.u32()?),
            WireType::F32 => visitor.visit_f32(self.reader.f32()?),
            WireType::F64 => visitor.visit_f64(self.reader.f64()?),
            WireType::Str => visitor.visit_borrowed_str(self.reader.str()?),
            WireType::Value => unreachable!("values take the tagged reader"),
        }
    }
    /// A present scalar is always `Some`; absence is the mask bit.
    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_some(self)
    }
    fn deserialize_newtype_struct<V: Visitor<'de>>(self, _: &'static str, visitor: V) -> Result<V::Value> {
        visitor.visit_newtype_struct(self)
    }
    /// A unit variant travels as a string field.
    fn deserialize_enum<V: Visitor<'de>>(self, _: &'static str, _: &'static [&'static str], visitor: V) -> Result<V::Value> {
        match self.kind {
            WireType::Str => visitor.visit_enum(self.reader.str()?.into_deserializer()),
            _ => Err(Error("expected a string field for an enum".into())),
        }
    }
    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf unit unit_struct seq tuple tuple_struct map struct
        identifier ignored_any
    }
}

#[cfg(test)]
mod field_tests {
    use super::*;
    use crate::style::{STYLES, SharedStyle, Style};
    use serde::Deserialize;

    #[derive(Default, Deserialize, Debug)]
    #[serde(default, rename_all = "camelCase")]
    struct Doc {
        style: SharedStyle,
        search: Option<serde_json::Value>,
    }
    const FIELDS: &[Field] = &[
        Field { name: "style", kind: WireType::Style, required: false },
        Field { name: "search", kind: WireType::Value, required: false },
    ];

    #[test]
    fn positional_style_id_resolves() {
        STYLES.with(|s| s.borrow_mut().push(Some(std::sync::Arc::new(Style::default()))));
        // mask 0b01, style id 0, then the trailer: key count 0, table offset.
        let bytes = [1u8, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0];
        let mut reader = Reader::new(&bytes).unwrap();
        let doc = Doc::deserialize(PropsReader { reader: &mut reader, fields: FIELDS }).unwrap();
        assert!(doc.search.is_none());
        assert!(reader.finished());
        let _ = doc.style;
    }
}
