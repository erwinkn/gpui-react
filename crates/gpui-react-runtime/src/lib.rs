//! Default composition. Applications with custom views build their own composition.
pub use gpui_react_host::*;

#[napi_derive::module_init]
fn register() {
    register_components(gpui_react_controls::register);
}
