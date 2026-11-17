//! Renders any `armature_core` primitive in Leptos. Nothing here knows about a
//! particular primitive.

pub mod agent;
mod parts;
mod storage;
mod viewport;

pub use armature_core::change::{Reason, Source, ValueChange};
pub use parts::{ChangeGate, Part, Root, use_primitive};
pub use storage::{CookieStorage, LocalStorage};
pub use viewport::{keyboard_lift, use_visible_viewport};
