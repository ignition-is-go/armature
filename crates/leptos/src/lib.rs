//! Renders any `armature_core` primitive in Leptos. Nothing here knows about a
//! particular primitive.

pub mod agent;
mod parts;

pub use armature_core::change::{Reason, Source, ValueChange};
pub use parts::{ChangeGate, Part, Root};
