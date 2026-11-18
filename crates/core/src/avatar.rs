use crate::agent::{ActError, Snapshot};
use crate::change::Gate;
use crate::primitive::{Input, Item, PartOf, Primitive, Reaction, Role, Semantics};

/// Where an avatar's picture is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImageStatus {
    /// There is no picture to load.
    #[default]
    Idle,
    Loading,
    Loaded,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Root,
    /// Holds the picture. Hidden until the picture has loaded.
    Image,
    /// Initials or an icon, shown until the picture has loaded and if it fails.
    Fallback,
}

/// A person's picture with something to show in its place. The adapter reports
/// loading through `Input::Loaded`.
#[derive(Clone, Debug, Default)]
pub struct Avatar {
    status: ImageStatus,
    /// How long to wait before showing the fallback, so a picture that loads at
    /// once never flashes initials first.
    fallback_delay_ms: u64,
    /// When the wait began, and whether it is over.
    waited: (Option<u64>, bool),
}

impl Avatar {
    /// An avatar whose picture is on its way.
    #[must_use]
    pub fn loading() -> Self {
        Self {
            status: ImageStatus::Loading,
            ..Default::default()
        }
    }

    #[must_use]
    pub const fn fallback_after(mut self, delay_ms: u64) -> Self {
        self.fallback_delay_ms = delay_ms;
        self
    }

    fn waiting(&self) -> bool {
        self.fallback_delay_ms > 0 && !self.waited.1 && self.status == ImageStatus::Loading
    }

    #[must_use]
    pub const fn status(&self) -> ImageStatus {
        self.status
    }
}

impl PartOf for Part {
    type Primitive = Avatar;
}

impl Primitive for Avatar {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Root, Part::Image, Part::Fallback]
    }

    fn is_item(_: Part) -> bool {
        false
    }

    fn named_part(&self) -> Option<Part> {
        Some(Part::Root)
    }

    fn wake_after(&self) -> Option<u64> {
        (self.waiting() && self.waited.0.is_none()).then_some(0)
    }

    fn upsert_item(&mut self, _: Item) {}
    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, part: Part, _: Option<&str>) -> Semantics {
        let loaded = self.status == ImageStatus::Loaded;
        match part {
            Part::Root => Semantics {
                role: Some(Role::Image),
                ..Default::default()
            },
            // Hidden from view, but still in the page so the picture loads.
            Part::Image => Semantics {
                hidden: !loaded,
                findable: !loaded,
                ..Default::default()
            },
            Part::Fallback => Semantics {
                hidden: loaded || self.waiting(),
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<Part>, _: Gate) -> Reaction<Part> {
        match input {
            Input::Loaded { ok, .. } => {
                self.status = if ok {
                    ImageStatus::Loaded
                } else {
                    ImageStatus::Error
                }
            }
            Input::Tick { now_ms } if self.waiting() => match self.waited.0 {
                None => {
                    self.waited.0 = Some(now_ms);
                    let at_ms = now_ms.saturating_add(self.fallback_delay_ms);
                    return Reaction::default().with(crate::primitive::Effect::Wake { at_ms });
                }
                Some(since) if now_ms >= since.saturating_add(self.fallback_delay_ms) => {
                    self.waited.1 = true
                }
                Some(_) => {}
            },
            _ => {}
        }
        Reaction::default()
    }

    /// Loading is not a choice anyone makes, so there is no value. Read `status`.
    fn value(&self) -> Vec<String> {
        Vec::new()
    }

    fn set_value(&mut self, _: Vec<String>) {}

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            name: "avatar",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: &[],
            items: Vec::new(),
        }
    }

    fn act(&mut self, action: &str, _: Option<&str>, _: Gate) -> Result<(), ActError> {
        Err(ActError::UnknownAction {
            action: action.to_string(),
            valid: &[],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fallback_shows_until_the_picture_loads_and_if_it_fails() {
        let mut avatar = Avatar::loading();
        assert!(!avatar.semantics(Part::Fallback, None).hidden);
        assert!(avatar.semantics(Part::Image, None).hidden);
        avatar.send(
            Input::Loaded {
                part: Part::Image,
                ok: true,
            },
            &mut |_| true,
        );
        assert!(avatar.semantics(Part::Fallback, None).hidden);
        assert_eq!(avatar.status(), ImageStatus::Loaded);
        avatar.send(
            Input::Loaded {
                part: Part::Image,
                ok: false,
            },
            &mut |_| true,
        );
        assert_eq!(avatar.status(), ImageStatus::Error);
        assert!(!avatar.semantics(Part::Fallback, None).hidden);
    }

    #[test]
    fn it_conforms() {
        crate::testing::assert_conforms(Avatar::default, &[]);
    }

    #[test]
    fn the_fallback_can_wait_so_a_quick_picture_never_flashes_it() {
        let mut avatar = Avatar::loading().fallback_after(300);
        assert!(avatar.semantics(Part::Fallback, None).hidden);
        assert_eq!(avatar.wake_after(), Some(0));
        avatar.send(Input::Tick { now_ms: 1000 }, &mut |_| true);
        avatar.send(Input::Tick { now_ms: 1200 }, &mut |_| true);
        assert!(avatar.semantics(Part::Fallback, None).hidden);
        avatar.send(Input::Tick { now_ms: 1300 }, &mut |_| true);
        assert!(
            !avatar.semantics(Part::Fallback, None).hidden,
            "the picture is slow, so the initials show"
        );
    }
}
