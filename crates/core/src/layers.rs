//! The stack of floating layers on screen: popups, menus, dialogs. It decides which
//! layer Escape or an outside press dismisses, and which layers a modal makes inert.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layer {
    pub id: String,
    /// Blocks interaction with everything beneath it.
    pub modal: bool,
    pub dismiss_on_escape: bool,
    pub dismiss_on_outside_press: bool,
}

impl Layer {
    /// A layer that Escape and outside presses dismiss, as a menu or popover is.
    pub fn popup(id: &str) -> Self {
        Layer {
            id: id.to_string(),
            modal: false,
            dismiss_on_escape: true,
            dismiss_on_outside_press: true,
        }
    }

    /// A modal layer that Escape dismisses and outside presses do not.
    pub fn dialog(id: &str) -> Self {
        Layer {
            id: id.to_string(),
            modal: true,
            dismiss_on_escape: true,
            dismiss_on_outside_press: false,
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

    pub fn top(&self) -> Option<&str> {
        self.layers.last().map(|layer| layer.id.as_str())
    }

    /// Whether a layer cannot be interacted with because a modal sits above it.
    pub fn is_inert(&self, id: &str) -> bool {
        match self.layers.iter().position(|layer| layer.id == id) {
            Some(at) => self.layers[at + 1..].iter().any(|layer| layer.modal),
            None => self.layers.iter().any(|layer| layer.modal),
        }
    }

    /// Escape was pressed. Only the top layer may answer. Returns what closed.
    pub fn escape(&mut self) -> Vec<String> {
        match self.layers.last() {
            Some(top) if top.dismiss_on_escape => {
                let id = top.id.clone();
                self.close(&id)
            }
            _ => Vec::new(),
        }
    }

    /// A press landed inside layer `inside`, or outside every layer. Layers above the
    /// pressed one close from the top down, stopping at the first that does not
    /// dismiss this way. Returns what closed.
    pub fn press(&mut self, inside: Option<&str>) -> Vec<String> {
        let keep = inside
            .and_then(|id| self.layers.iter().position(|layer| layer.id == id))
            .map_or(0, |at| at + 1);
        let mut closed = Vec::new();
        while self.layers.len() > keep
            && self
                .layers
                .last()
                .is_some_and(|top| top.dismiss_on_outside_press)
        {
            closed.extend(self.layers.pop().map(|layer| layer.id));
        }
        closed
    }
}

/// The element Tab moves to inside a modal layer that traps focus: it wraps at both
/// ends. `count` is how many tabbable elements the layer has.
pub fn trapped_focus(current: Option<usize>, count: usize, backwards: bool) -> Option<usize> {
    if count == 0 {
        return None;
    }
    Some(match (current, backwards) {
        (None, false) => 0,
        (None, true) => count - 1,
        (Some(at), false) => (at + 1) % count,
        (Some(at), true) => (at + count - 1) % count,
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
        assert_eq!(trapped_focus(Some(2), 3, false), Some(0));
        assert_eq!(trapped_focus(Some(0), 3, true), Some(2));
        assert_eq!(trapped_focus(None, 3, true), Some(2));
        assert_eq!(trapped_focus(None, 0, false), None);
    }
}
