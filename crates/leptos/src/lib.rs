//! Renders any `armature_core` primitive in Leptos. Nothing here knows about a
//! particular primitive.

pub mod agent;
mod parts;
mod shortcut;
mod storage;
mod viewport;

pub use armature_core::change::{Reason, Source, ValueChange};
pub use parts::{ChangeGate, Part, Root, use_primitive};
pub use shortcut::use_shortcut;
pub use storage::{CookieStorage, LocalStorage, system_prefers_dark};
pub use viewport::{keyboard_lift, use_visible_viewport};
