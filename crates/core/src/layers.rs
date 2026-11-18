//! The stack of floating layers on screen: popups, menus, dialogs. It decides which
//! layer Escape or an outside press dismisses, and which layers a modal makes inert.

use crate::keys::Direction;

/// Which of a person's own gestures close a layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DismissedBy {
    pub escape: bool,
    pub outside_press: bool,
}

impl DismissedBy {
    /// Escape and a press outside both close it.
    pub const EITHER: Self = Self {
        escape: true,
        outside_press: true,
    };
    /// Escape closes it and a press outside does not.
    pub const ESCAPE: Self = Self {
        escape: true,
        outside_press: false,
    };
    /// A press outside closes it and Escape does not.
    pub const OUTSIDE_PRESS: Self = Self {
        escape: false,
        outside_press: true,
    };
    /// Neither closes it.
    pub const NEITHER: Self = Self {
        escape: false,
        outside_press: false,
    };
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layer {
    pub id: String,
    /// Blocks interaction with everything beneath it.
    pub modal: bool,
    pub dismissed_by: DismissedBy,
    /// Stays usable whatever opens above it, even a modal, and is never dismissed by
    /// a press elsewhere. A stack of notifications is the usual case.
    pub persistent: bool,
}

impl Layer {
    /// A layer that Escape and outside presses dismiss, as a menu or popover is.
    #[must_use]
    pub fn popup(id: &str) -> Self {
        Self {
            id: id.to_string(),
            modal: false,
            dismissed_by: DismissedBy::EITHER,
            persistent: false,
        }
    }

    /// A layer that stays usable above everything and that nothing else dismisses.
    #[must_use]
    pub fn persistent(id: &str) -> Self {
        Self {
            id: id.to_string(),
            modal: false,
            dismissed_by: DismissedBy::NEITHER,
            persistent: true,
        }
    }

    /// A modal layer that Escape dismisses and outside presses do not.
    #[must_use]
    pub fn dialog(id: &str) -> Self {
        Self {
            id: id.to_string(),
            modal: true,
            dismissed_by: DismissedBy::ESCAPE,
            persistent: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dismissal {
    Escape,
    OutsidePress,
}

#[derive(Clone, Debug, Default)]
pub struct LayerStack {
    layers: Vec<Layer>,
}

impl LayerStack {
    /// Opens a layer on top. Reopening one moves it to the top.
    pub fn open(&mut self, layer: Layer) {
        self.close(&layer.id);
        self.layers.push(layer);
    }

    /// Closes a layer and every layer opened above it, since those belong to it.
    pub fn close(&mut self, id: &str) -> Vec<String> {
        match self.layers.iter().position(|layer| layer.id == id) {
            Some(at) => self
                .layers
                .drain(at..)
                .rev()
                .map(|layer| layer.id)
                .collect(),
            None => Vec::new(),
        }
    }

    #[must_use]
    pub fn top(&self) -> Option<&str> {
        self.layers.last().map(|layer| layer.id.as_str())
    }

    /// Whether the layer at `at` stays open through whatever closes the others.
    fn is_persistent(&self, at: usize) -> bool {
        self.layers.get(at).is_some_and(|layer| layer.persistent)
    }

    /// Whether a layer cannot be interacted with because a modal sits above it. A
    /// persistent layer never is.
    #[must_use]
    pub fn is_inert(&self, id: &str) -> bool {
        match self.layers.iter().position(|layer| layer.id == id) {
            Some(at) if self.is_persistent(at) => false,
            Some(at) => self
                .layers
                .get(at.saturating_add(1)..)
                .unwrap_or_default()
                .iter()
                .any(|layer| layer.modal),
            None => self.layers.iter().any(|layer| layer.modal),
        }
    }

    /// Escape was pressed. Only the top layer may answer. Returns what closed.
    pub fn escape(&mut self) -> Vec<String> {
        match self.layers.iter().rev().find(|layer| !layer.persistent) {
            Some(top) if top.dismissed_by.escape => {
                let id = top.id.clone();
                self.remove(&id)
            }
            _ => Vec::new(),
        }
    }

    /// Removes one layer and the non-persistent layers opened above it.
    fn remove(&mut self, id: &str) -> Vec<String> {
        let Some(at) = self.layers.iter().position(|layer| layer.id == id) else {
            return Vec::new();
        };
        let mut closed = Vec::new();
        let mut index = self.layers.len();
        while index > at {
            index = index.saturating_sub(1);
            if index == at || !self.is_persistent(index) {
                closed.push(self.layers.remove(index).id);
            }
        }
        closed
    }

    /// A press landed inside layer `inside`, or outside every layer. Layers above the
    /// pressed one close from the top down, stopping at the first that does not
    /// dismiss this way. Returns what closed.
    pub fn press(&mut self, inside: Option<&str>) -> Vec<String> {
        let pressed = inside.and_then(|id| self.layers.iter().position(|layer| layer.id == id));
        if pressed.is_some_and(|at| self.is_persistent(at)) {
            return Vec::new();
        }
        let keep = pressed.map_or(0, |at| at.saturating_add(1));
        let mut closed = Vec::new();
        let mut index = self.layers.len();
        while index > keep {
            index = index.saturating_sub(1);
            if self.is_persistent(index) {
                continue;
            }
            if !self
                .layers
                .get(index)
                .is_some_and(|layer| layer.dismissed_by.outside_press)
            {
                break;
            }
            closed.push(self.layers.remove(index).id);
        }
        closed
    }
}

/// The element Tab moves to inside a modal layer that traps focus: it wraps at both
/// ends. `count` is how many tabbable elements the layer has.
#[must_use]
pub fn trapped_focus(current: Option<usize>, count: usize, direction: Direction) -> Option<usize> {
    if count == 0 {
        return None;
    }
    Some(match (current, direction) {
        (None, Direction::Forward) => 0,
        (None, Direction::Backward) => count.saturating_sub(1),
        (Some(at), Direction::Forward) => (at.saturating_add(1))
            .checked_rem(count)
            .unwrap_or_default(),
        (Some(at), Direction::Backward) => ((at.saturating_add(count)).saturating_sub(1))
            .checked_rem(count)
            .unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(layers: &[Layer]) -> LayerStack {
        let mut stack = LayerStack::default();
        layers.iter().cloned().for_each(|layer| stack.open(layer));
        stack
    }

    #[test]
    fn escape_closes_only_the_top_layer() {
        let mut stack = stack(&[Layer::dialog("settings"), Layer::popup("menu")]);
        assert_eq!(stack.escape(), ["menu"]);
        assert_eq!(stack.top(), Some("settings"));
    }

    #[test]
    fn a_press_outside_everything_stops_at_a_layer_that_stays_open() {
        let mut stack = stack(&[
            Layer::popup("menu"),
            Layer::dialog("confirm"),
            Layer::popup("tip"),
        ]);
        assert_eq!(stack.press(None), ["tip"]);
        assert_eq!(stack.top(), Some("confirm"));
    }

    #[test]
    fn a_press_inside_a_layer_closes_only_the_layers_above_it() {
        let mut stack = stack(&[
            Layer::popup("menu"),
            Layer::popup("submenu"),
            Layer::popup("tip"),
        ]);
        assert_eq!(stack.press(Some("menu")), ["tip", "submenu"]);
        assert_eq!(stack.press(Some("menu")), Vec::<String>::new());
    }

    #[test]
    fn closing_a_layer_takes_the_layers_it_opened_with_it() {
        let mut stack = stack(&[Layer::popup("menu"), Layer::popup("submenu")]);
        assert_eq!(stack.close("menu"), ["submenu", "menu"]);
        assert_eq!(stack.top(), None);
    }

    #[test]
    fn a_modal_makes_everything_beneath_it_inert() {
        let stack = stack(&[Layer::popup("menu"), Layer::dialog("confirm")]);
        assert!(stack.is_inert("menu"));
        assert!(!stack.is_inert("confirm"));
        assert!(stack.is_inert("the page"));
    }

    #[test]
    fn trapped_focus_wraps_at_both_ends() {
        assert_eq!(trapped_focus(Some(2), 3, Direction::Forward), Some(0));
        assert_eq!(trapped_focus(Some(0), 3, Direction::Backward), Some(2));
        assert_eq!(trapped_focus(None, 3, Direction::Backward), Some(2));
        assert_eq!(trapped_focus(None, 0, Direction::Forward), None);
    }

    #[test]
    fn a_persistent_layer_survives_everything_and_stays_usable_under_a_modal() {
        let mut stack = stack(&[
            Layer::persistent("toasts"),
            Layer::dialog("confirm"),
            Layer::popup("menu"),
        ]);
        assert!(!stack.is_inert("toasts"));
        assert_eq!(
            stack.press(Some("toasts")),
            Vec::<String>::new(),
            "pressing a toast dismisses nothing"
        );
        assert_eq!(stack.press(None), ["menu"]);
        assert_eq!(stack.escape(), ["confirm"]);
        assert_eq!(stack.escape(), Vec::<String>::new());
        assert_eq!(stack.top(), Some("toasts"));
    }
}
