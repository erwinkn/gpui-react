//! Native components compiled outside the GPUiX renderer.
//!
//! A composition crate links GPUiX and the component crates into one native
//! library. Register its extensions before creating a renderer. Factories run
//! on the renderer's UI thread. No GPUI entity crosses the JavaScript boundary.

use std::sync::Mutex;

pub use crate::accessibility::apply_accessibility;
pub use crate::automation::{bounds_tracker, track_own_bounds};
pub use crate::color::parse_color_rgba;
pub use crate::custom_elements::{
    custom_surface, wire_standard_events, CustomElement as NativeElement,
    CustomElementFactory as NativeElementFactory, CustomRenderContext as NativeRenderContext,
};
pub use crate::element_tree::EventPayload;
pub use crate::renderer::{
    apply_interactive_styles, apply_styles, emit_event_full, parse_font_weight, EventCallback,
    GpuixView as NativeView,
};
pub use crate::style::{font_features, BoxShadowStack, FontFeatureMap, StyleDesc};
pub use crate::text::{
    chrome_text, log_painted_text, selectable_text, selection_key, HighlightSource, SelectableText,
    SharedSelection,
};
pub use gpui;

/// Version of the Rust extension contract. Native crates share the exact GPUI
/// and GPUiX build; this is not a stable ABI for dynamically loaded libraries.
pub const NATIVE_EXTENSION_API_VERSION: u32 = 1;

/// A loader checks this contract before it constructs a native renderer.
#[cfg_attr(
    not(all(target_arch = "wasm32", target_os = "unknown")),
    napi_derive::napi
)]
#[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), wasm_bindgen::prelude::wasm_bindgen(js_name = nativeRuntimeInfo))]
pub fn native_runtime_info() -> String {
    serde_json::json!({
        "apiVersion": 1,
        "extensionApiVersion": NATIVE_EXTENSION_API_VERSION,
        "version": env!("CARGO_PKG_VERSION"),
        "extensions": registered_extensions(),
    })
    .to_string()
}

#[derive(Clone, Copy)]
pub struct NativeElementRegistration {
    pub name: &'static str,
    pub factory: fn() -> Box<dyn NativeElementFactory>,
}

#[derive(Clone, Copy)]
pub struct NativeExtension {
    pub id: &'static str,
    pub version: &'static str,
    pub api_version: u32,
    pub elements: &'static [NativeElementRegistration],
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeExtensionInfo {
    pub id: &'static str,
    pub version: &'static str,
    pub api_version: u32,
    pub elements: Vec<&'static str>,
}

#[derive(Default)]
struct Catalog {
    extensions: Vec<NativeExtension>,
    frozen: bool,
}

static CATALOG: Mutex<Catalog> = Mutex::new(Catalog {
    extensions: Vec::new(),
    frozen: false,
});

impl Catalog {
    fn register(&mut self, extension: NativeExtension) -> Result<(), String> {
        if extension.api_version != NATIVE_EXTENSION_API_VERSION {
            return Err(format!(
                "Native extension {} requires API {}, runtime provides {}",
                extension.id, extension.api_version, NATIVE_EXTENSION_API_VERSION
            ));
        }
        if extension.id.is_empty() || extension.version.is_empty() || extension.elements.is_empty()
        {
            return Err(
                "A native extension requires an id, version, and element registrations".into(),
            );
        }
        if let Some(existing) = self.extensions.iter().find(|item| item.id == extension.id) {
            let identical = existing.version == extension.version
                && existing.elements.len() == extension.elements.len()
                && existing
                    .elements
                    .iter()
                    .zip(extension.elements)
                    .all(|(a, b)| a.name == b.name && std::ptr::fn_addr_eq(a.factory, b.factory));
            return if identical {
                Ok(())
            } else {
                Err(format!(
                    "Native extension {} is already registered with a different implementation",
                    extension.id
                ))
            };
        }
        if self.frozen {
            return Err(
                "Native extensions must be registered before the first renderer is created".into(),
            );
        }
        let mut names = std::collections::HashSet::new();
        for item in extension.elements {
            // A hyphen reserves built-in host names for GPUiX and provides an
            // explicit namespace for external JSX elements.
            if !item.name.contains('-')
                || !item
                    .name
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
            {
                return Err(format!(
                    "Native element {} must use a lowercase hyphenated name",
                    item.name
                ));
            }
            if matches!(item.name, "virtual-list")
                || !names.insert(item.name)
                || self
                    .extensions
                    .iter()
                    .any(|ext| ext.elements.iter().any(|old| old.name == item.name))
            {
                return Err(format!(
                    "Native element {} is already registered",
                    item.name
                ));
            }
        }
        self.extensions.push(extension);
        Ok(())
    }

    fn freeze(&mut self) -> Vec<NativeExtension> {
        self.frozen = true;
        self.extensions.clone()
    }
}

/// Register a compiled extension. The identical registration is idempotent so
/// host and application-worker imports can share one native library. Replacing
/// implementations or adding elements after startup returns an error.
pub fn register_extension(extension: NativeExtension) -> Result<(), String> {
    CATALOG
        .lock()
        .map_err(|_| "Native extension catalog lock failed")?
        .register(extension)
}

/// Inspect the compiled extension set without creating a window.
pub fn registered_extensions() -> Vec<NativeExtensionInfo> {
    CATALOG
        .lock()
        .expect("native extension catalog")
        .extensions
        .iter()
        .map(|ext| NativeExtensionInfo {
            id: ext.id,
            version: ext.version,
            api_version: ext.api_version,
            elements: ext.elements.iter().map(|item| item.name).collect(),
        })
        .collect()
}

pub(crate) fn install_into(registry: &mut crate::custom_elements::CustomElementRegistry) {
    let extensions = CATALOG.lock().expect("native extension catalog").freeze();
    for extension in extensions {
        for registration in extension.elements {
            let factory = (registration.factory)();
            assert_eq!(
                factory.element_type(),
                registration.name,
                "Native extension {} factory name differs from its registration",
                extension.id
            );
            registry.register(factory);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn unused_factory() -> Box<dyn NativeElementFactory> {
        unreachable!()
    }
    const A: NativeExtension = NativeExtension {
        id: "test.a",
        version: "1.0.0",
        api_version: NATIVE_EXTENSION_API_VERSION,
        elements: &[NativeElementRegistration {
            name: "test-component",
            factory: unused_factory,
        }],
    };

    #[test]
    fn duplicate_names_and_versions_do_not_replace_installed_components() {
        let mut catalog = Catalog::default();
        catalog.register(A).unwrap();
        assert!(catalog
            .register(NativeExtension { id: "test.b", ..A })
            .unwrap_err()
            .contains("already registered"));
        assert!(catalog
            .register(NativeExtension {
                version: "2.0.0",
                ..A
            })
            .unwrap_err()
            .contains("different implementation"));
        assert_eq!(catalog.extensions.len(), 1);
    }

    #[test]
    fn worker_registration_is_idempotent_but_runtime_extension_is_closed() {
        let mut catalog = Catalog::default();
        catalog.register(A).unwrap();
        catalog.freeze();
        catalog.register(A).unwrap();
        assert!(catalog
            .register(NativeExtension { id: "test.b", ..A })
            .unwrap_err()
            .contains("before the first renderer"));
    }

    #[test]
    fn incompatible_and_reserved_registrations_fail_before_startup() {
        let mut catalog = Catalog::default();
        assert!(catalog
            .register(NativeExtension {
                api_version: 999,
                ..A
            })
            .is_err());
        assert!(catalog
            .register(NativeExtension {
                elements: &[NativeElementRegistration {
                    name: "virtual-list",
                    factory: unused_factory
                }],
                ..A
            })
            .is_err());
        assert!(catalog
            .register(NativeExtension {
                elements: &[NativeElementRegistration {
                    name: "div",
                    factory: unused_factory
                }],
                ..A
            })
            .is_err());
        assert!(catalog.extensions.is_empty());
    }
}
