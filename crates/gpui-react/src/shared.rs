//! Values declared once per session and referenced by id from props. The
//! engine has no opinion about what is shared: a kit declares one definition
//! type with `Registry::shared::<T>()`, the wire's `style` operations decode
//! into it, and props fields typed `Shared<T>` resolve the id.
use serde::{Deserialize, Deserializer, de::DeserializeOwned};
use std::{any::Any, cell::RefCell, fmt, ops::Deref, sync::Arc};

/// A definition in the session table with its concrete type erased.
pub(crate) type Erased = Arc<dyn Any + Send + Sync>;

/// The definition type a session shares by id.
pub trait SharedDefinition: DeserializeOwned + Send + Sync + 'static {
    /// The value a `Shared<Self>` prop holds when the wire omits it. Return one
    /// cached `Arc` so defaulted props allocate nothing.
    fn default_shared() -> Arc<Self>;
}

thread_local! {
    /// The decoding session's definitions, installed by `Decoder` for the
    /// duration of one transaction.
    static TABLE: RefCell<Vec<Option<Erased>>> = const { RefCell::new(Vec::new()) };
}

/// Runs `decode` with `table` as the session's definitions.
pub(crate) fn with_table<R>(table: &mut Vec<Option<Erased>>, decode: impl FnOnce() -> R) -> R {
    struct Installed<'a>(&'a mut Vec<Option<Erased>>);
    impl Drop for Installed<'_> {
        fn drop(&mut self) {
            TABLE.with(|cell| std::mem::swap(&mut *cell.borrow_mut(), self.0));
        }
    }
    TABLE.with(|cell| std::mem::swap(&mut *cell.borrow_mut(), table));
    let _installed = Installed(table);
    decode()
}

/// Records or drops a definition in the installed session table.
pub(crate) fn install(id: u32, definition: Option<Erased>) -> Result<(), &'static str> {
    TABLE.with(|table| {
        let mut table = table.borrow_mut();
        let slot = id as usize;
        if slot >= table.len() {
            if definition.is_none() {
                return Ok(());
            }
            if slot > table.len() + 4096 {
                return Err("style id skips too far");
            }
            table.resize(slot + 1, None);
        }
        table[slot] = definition;
        Ok(())
    })
}

/// A value shared between every node that declared it. On the wire it is
/// either a number, the id of a definition sent earlier in the session, or an
/// inline object, which is accepted for tests and native callers and
/// allocated on its own.
pub struct Shared<T>(Arc<T>);

impl<T> Shared<T> {
    pub fn new(value: T) -> Self {
        Self(Arc::new(value))
    }
    pub fn as_arc(&self) -> &Arc<T> {
        &self.0
    }
}
impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<T: fmt::Debug> fmt::Debug for Shared<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Shared").field(&self.0).finish()
    }
}
impl<T: SharedDefinition> Default for Shared<T> {
    fn default() -> Self {
        Self(T::default_shared())
    }
}
impl<T> Deref for Shared<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}
impl<T: PartialEq> PartialEq for Shared<T> {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || *self.0 == *other.0
    }
}
impl<T> From<T> for Shared<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}
impl<'de, T: SharedDefinition> Deserialize<'de> for Shared<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Wire<T> {
            Id(u32),
            Inline(T),
        }
        match Wire::<T>::deserialize(d)? {
            Wire::Inline(value) => Ok(Self::new(value)),
            Wire::Id(id) => {
                let erased = TABLE
                    .with(|table| table.borrow().get(id as usize).and_then(|slot| slot.clone()))
                    .ok_or_else(|| serde::de::Error::custom(format!("unknown style {id}")))?;
                erased.downcast::<T>().map(Self).map_err(|_| {
                    serde::de::Error::custom(format!(
                        "style {id} is not a {}",
                        std::any::type_name::<T>()
                    ))
                })
            }
        }
    }
}
