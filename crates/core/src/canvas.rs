use crate::agent::{ActError, Snapshot, SnapshotItem};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::connect::{Connect, ConnectStep, Connection, connect_message};
use crate::geometry::{Point, Rect, Size};
use crate::keys::{Key, Modifiers};
use crate::lasso::{lasso_hits, lasso_selection};
use crate::motion::{Handle, ResizeLimits, key_delta, moved, resized};
use crate::primitive::{
    Effect, Focusable, Input, Item, Measure, PartOf, Pointer, PointerPhase, Primitive, Reaction,
    Role, Semantics,
};
use crate::spatial::{Direction, nearest_in_direction};
use crate::undo::UndoStack;
use crate::viewport::Viewport;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    /// The surface nodes sit on. Dragging on it draws a selection rectangle, or pans
    /// with Alt held.
    Surface,
    /// One node. Dragging it moves every selected node.
    Node,
    /// The corner of a node that resizes it.
    Resize,
    /// A node's outgoing port. Pressing it begins a connection.
    PortOut,
    /// A node's incoming port. Pressing it finishes a connection.
    PortIn,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub key: String,
    /// In canvas coordinates, before pan and zoom.
    pub rect: Rect,
}

#[derive(Clone, Debug, PartialEq)]
enum Gesture {
    Lasso {
        from: Point,
        to: Point,
        before: Vec<String>,
        mods: Modifiers,
    },
    Move {
        from: Point,
        origin: Vec<Node>,
    },
    Resize {
        key: String,
        from: Point,
        origin: Vec<Node>,
    },
    Pan {
        last: Point,
    },
}

const ACTIONS: &[&str] = &[
    "select", "move", "connect", "undo", "redo", "zoom_in", "zoom_out",
];
const MIN_NODE: Size = Size {
    width: 40.0,
    height: 30.0,
};
const ZOOM_STEP: f64 = 1.25;

/// A pannable, zoomable surface of nodes that can be selected with a rectangle,
/// moved, resized and connected, with undo. The value lists each node as
/// `key:x,y,width,height`.
#[derive(Clone, Debug)]
pub struct Canvas {
    view: Viewport,
    nodes: Vec<Node>,
    selected: Vec<String>,
    links: Connect,
    history: UndoStack<Vec<Node>>,
    gesture: Option<Gesture>,
}

fn written(nodes: &[Node]) -> Vec<String> {
    nodes
        .iter()
        .map(|node| {
            format!(
                "{}:{},{},{},{}",
                node.key, node.rect.x, node.rect.y, node.rect.width, node.rect.height
            )
        })
        .collect()
}

fn read(entry: &str) -> Option<Node> {
    let (key, numbers) = entry.split_once(':')?;
    let mut numbers = numbers.split(',').map(|n| n.trim().parse::<f64>());
    let mut next = || numbers.next()?.ok();
    Some(Node {
        key: key.to_string(),
        rect: Rect::new(next()?, next()?, next()?, next()?),
    })
}

impl Default for Canvas {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl Canvas {
    #[must_use]
    pub fn new(nodes: Vec<Node>) -> Self {
        Self {
            view: Viewport::default(),
            history: UndoStack::new(nodes.clone(), 100),
            nodes,
            selected: Vec::new(),
            links: Connect::default(),
            gesture: None,
        }
    }

    #[must_use]
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    #[must_use]
    pub fn selected(&self) -> &[String] {
        &self.selected
    }

    #[must_use]
    pub fn connections(&self) -> &[Connection] {
        self.links.connections()
    }

    #[must_use]
    pub const fn zoom(&self) -> f64 {
        self.view.zoom
    }

    /// Where to draw a node, relative to the surface's corner, after pan and zoom.
    #[must_use]
    pub fn screen_rect(&self, key: &str) -> Option<Rect> {
        let rect = self.nodes.iter().find(|node| node.key == key)?.rect;
        let corner = self.view.to_screen(Point::new(rect.x, rect.y));
        Some(Rect::new(
            corner.x,
            corner.y,
            rect.width * self.view.zoom,
            rect.height * self.view.zoom,
        ))
    }

    /// The selection rectangle being drawn, relative to the surface's corner.
    #[must_use]
    pub fn lasso(&self) -> Option<Rect> {
        match &self.gesture {
            Some(Gesture::Lasso { from, to, .. }) => Some(Rect::from_corners(
                self.view.to_screen(*from),
                self.view.to_screen(*to),
            )),
            _ => None,
        }
    }

    fn has(&self, key: &str) -> bool {
        self.nodes.iter().any(|node| node.key == key)
    }

    /// The pointer's position on the canvas itself.
    fn canvas_point(&self, pointer: &Pointer<Part>, measure: Measure<Part>) -> Point {
        let surface = measure(Part::Surface, None).unwrap_or_default();
        self.view.to_canvas(Point::new(
            pointer.at.x - surface.x,
            pointer.at.y - surface.y,
        ))
    }

    /// Asks the owner about the nodes as they now stand. Agreement is recorded for
    /// undo; a refusal puts `before` back.
    fn settle(&mut self, before: Vec<Node>, source: Source, reason: Reason, gate: Gate) -> bool {
        if self.nodes == before {
            return true;
        }
        let allowed = gate(&ValueChange {
            value: written(&self.nodes),
            source,
            reason,
        });
        if allowed {
            self.history.record(self.nodes.clone());
        } else {
            self.nodes = before;
        }
        allowed
    }

    fn restore(&mut self, nodes: Vec<Node>, source: Source, reason: Reason, gate: Gate) -> bool {
        let before = std::mem::replace(&mut self.nodes, nodes);
        if self.nodes == before {
            return true;
        }
        let allowed = gate(&ValueChange {
            value: written(&self.nodes),
            source,
            reason,
        });
        if !allowed {
            self.nodes = before;
        }
        allowed
    }

    fn undo(&mut self, source: Source, reason: Reason, gate: Gate) -> bool {
        let Some(nodes) = self.history.undo().cloned() else {
            return true;
        };
        let allowed = self.restore(nodes, source, reason, gate);
        if !allowed {
            self.history.redo();
        }
        allowed
    }

    fn redo(&mut self, source: Source, reason: Reason, gate: Gate) -> bool {
        let Some(nodes) = self.history.redo().cloned() else {
            return true;
        };
        let allowed = self.restore(nodes, source, reason, gate);
        if !allowed {
            self.history.undo();
        }
        allowed
    }

    fn shift_selected(&mut self, origin: &[Node], delta: Point) {
        for node in self
            .nodes
            .iter_mut()
            .filter(|node| self.selected.contains(&node.key))
        {
            if let Some(start) = origin.iter().find(|o| o.key == node.key) {
                node.rect = moved(start.rect, delta, None, None);
            }
        }
    }
}

impl Canvas {
    /// Alt with an arrow selects the nearest node in that direction.
    fn select_towards(&mut self, key: Key) -> Reaction<Part> {
        let direction = match key {
            Key::ArrowUp => Direction::Up,
            Key::ArrowDown => Direction::Down,
            Key::ArrowLeft => Direction::Left,
            Key::ArrowRight => Direction::Right,
            _ => return Reaction::default(),
        };
        let Some(from) = self
            .selected
            .first()
            .or_else(|| self.nodes.first().map(|node| &node.key))
            .cloned()
        else {
            return Reaction::default();
        };
        let origin = self
            .nodes
            .iter()
            .find(|node| node.key == from)
            .map(|node| node.rect)
            .unwrap_or_default();
        let others: Vec<(String, Rect)> = self
            .nodes
            .iter()
            .filter(|node| node.key != from)
            .map(|node| (node.key.clone(), node.rect))
            .collect();
        if let Some(next) = nearest_in_direction(&others, origin, direction) {
            self.selected = vec![next];
        }
        Reaction::handled()
    }

    /// A pointer moving with a gesture under way: a lasso grows, nodes are carried,
    /// one is resized, or the view pans.
    fn pointer_moved(&mut self, pointer: &Pointer<Part>, at: Point) -> Reaction<Part> {
        match self.gesture.clone() {
            Some(Gesture::Lasso {
                from, before, mods, ..
            }) => {
                let boxes: Vec<(String, Rect)> = self
                    .nodes
                    .iter()
                    .map(|node| (node.key.clone(), node.rect))
                    .collect();
                self.selected = lasso_selection(&before, &lasso_hits(from, at, &boxes), mods);
                self.gesture = Some(Gesture::Lasso {
                    from,
                    to: at,
                    before,
                    mods,
                });
            }
            Some(Gesture::Move { from, origin }) => {
                self.shift_selected(&origin, at.minus(from));
            }
            Some(Gesture::Resize { key, from, origin }) => {
                let limits = ResizeLimits {
                    min: MIN_NODE,
                    max: None,
                    bounds: None,
                };
                let start = origin
                    .iter()
                    .find(|node| node.key == key)
                    .map(|node| node.rect);
                if let (Some(start), Some(node)) =
                    (start, self.nodes.iter_mut().find(|node| node.key == key))
                {
                    node.rect = resized(start, Handle::BOTTOM_RIGHT, at.minus(from), limits);
                }
            }
            Some(Gesture::Pan { last }) => {
                self.view.pan_by(pointer.at.minus(last));
                self.gesture = Some(Gesture::Pan { last: pointer.at });
            }
            None => return Reaction::default(),
        }
        Reaction::handled()
    }
}

impl PartOf for Part {
    type Primitive = Canvas;
}

impl Primitive for Canvas {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[
            Part::Surface,
            Part::Node,
            Part::Resize,
            Part::PortOut,
            Part::PortIn,
        ]
    }

    fn is_item(_: Part) -> bool {
        false
    }

    fn tracks_pointer(part: Part) -> bool {
        matches!(part, Part::Surface | Part::Node | Part::Resize)
    }

    fn upsert_item(&mut self, _: Item) {}
    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        let exists = item.is_some_and(|key| self.has(key));
        let selected = item.is_some_and(|key| self.selected.iter().any(|k| k == key));
        match part {
            Part::Surface => Semantics {
                role: Some(Role::Group),
                focusable: Focusable::TabStop,
                ..Default::default()
            },
            Part::Node => Semantics {
                role: Some(Role::ListItem),
                selected: Some(selected),
                hidden: !exists,
                focusable: if exists {
                    Focusable::Programmatic
                } else {
                    Focusable::No
                },
                ..Default::default()
            },
            Part::Resize => Semantics {
                hidden: !(exists && selected),
                ..Default::default()
            },
            Part::PortOut | Part::PortIn => Semantics {
                role: Some(Role::Button),
                hidden: !exists,
                focusable: if exists {
                    Focusable::Programmatic
                } else {
                    Focusable::No
                },
                active: part == Part::PortOut && self.links.pending() == item && item.is_some(),
                ..Default::default()
            },
        }
    }
    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        match input {
            Input::Press {
                part: Part::PortOut,
                item: Some(port),
                ..
            } if self.has(&port) => {
                self.links.begin(&port);
                Reaction::handled().announcing(connect_message(ConnectStep::Begun, &port, ""))
            }
            Input::Press {
                part: Part::PortIn,
                item: Some(port),
                ..
            } if self.has(&port) => {
                let Some(from) = self.links.pending().map(str::to_string) else {
                    return Reaction::default();
                };
                let step = if self.links.finish(&port, |_, _| true).is_some() {
                    ConnectStep::Connected
                } else {
                    ConnectStep::Refused
                };
                Reaction::handled().announcing(connect_message(step, &from, &port))
            }
            Input::Key {
                key: Key::Char('z' | 'Z'),
                mods,
                ..
            } if mods.ctrl => {
                if mods.shift {
                    self.redo(Source::User, Reason::Keyboard, gate);
                } else {
                    self.undo(Source::User, Reason::Keyboard, gate);
                }
                Reaction::handled()
            }
            Input::Key {
                key: Key::Char('+' | '='),
                ..
            } => {
                self.view.zoom_at(Point::default(), ZOOM_STEP);
                Reaction::handled()
            }
            Input::Key {
                key: Key::Char('-'),
                ..
            } => {
                self.view.zoom_at(Point::default(), 1.0 / ZOOM_STEP);
                Reaction::handled()
            }
            Input::Key {
                key: Key::Escape, ..
            } => {
                if self.links.pending().is_some() {
                    self.links.cancel();
                    return Reaction::handled().announcing(connect_message(
                        ConnectStep::Cancelled,
                        "",
                        "",
                    ));
                }
                let had_selection = !self.selected.is_empty();
                self.selected.clear();
                Reaction {
                    handled: had_selection,
                    ..Default::default()
                }
            }
            // Alt with an arrow moves the selection to the nearest node that way.
            Input::Key { key, mods, .. } if mods.alt => self.select_towards(key),
            Input::Key { key, mods, .. } => {
                let Some(delta) = key_delta(key, mods, 1.0, 10.0) else {
                    return Reaction::default();
                };
                if self.selected.is_empty() {
                    return Reaction::default();
                }
                let before = self.nodes.clone();
                self.shift_selected(&before, delta);
                self.settle(before, Source::User, Reason::Keyboard, gate);
                Reaction::handled()
            }
            _ => Reaction::default(),
        }
    }
    fn pointer(
        &mut self,
        pointer: Pointer<Part>,
        measure: Measure<Part>,
        gate: Gate,
    ) -> Reaction<Part> {
        let at = self.canvas_point(&pointer, measure);
        match pointer.phase {
            PointerPhase::Down => {
                let key = pointer.item.clone().filter(|key| self.has(key));
                self.gesture = match (pointer.part, key) {
                    (Part::Surface, _) if pointer.mods.alt => {
                        Some(Gesture::Pan { last: pointer.at })
                    }
                    (Part::Surface, _) => {
                        let before = self.selected.clone();
                        self.selected = lasso_selection(&before, &[], pointer.mods);
                        Some(Gesture::Lasso {
                            from: at,
                            to: at,
                            before,
                            mods: pointer.mods,
                        })
                    }
                    (Part::Node, Some(key)) => {
                        if !self.selected.contains(&key) {
                            if !pointer.mods.shift {
                                self.selected.clear();
                            }
                            self.selected.push(key);
                        }
                        Some(Gesture::Move {
                            from: at,
                            origin: self.nodes.clone(),
                        })
                    }
                    (Part::Resize, Some(key)) => Some(Gesture::Resize {
                        key,
                        from: at,
                        origin: self.nodes.clone(),
                    }),
                    _ => None,
                };
                Reaction::focusing(Part::Surface).with(Effect::CapturePointer)
            }
            PointerPhase::Move => self.pointer_moved(&pointer, at),
            PointerPhase::Up => {
                if let Some(Gesture::Move { origin, .. } | Gesture::Resize { origin, .. }) =
                    self.gesture.take()
                {
                    self.settle(origin, Source::User, Reason::Drag, gate);
                }
                Reaction::handled()
            }
            PointerPhase::Cancel => {
                match self.gesture.take() {
                    Some(Gesture::Move { origin, .. } | Gesture::Resize { origin, .. }) => {
                        self.nodes = origin;
                    }
                    Some(Gesture::Lasso { before, .. }) => self.selected = before,
                    _ => {}
                }
                Reaction::handled()
            }
        }
    }

    fn value(&self) -> Vec<String> {
        written(&self.nodes)
    }

    fn set_value(&mut self, value: Vec<String>) {
        for incoming in value.iter().filter_map(|entry| read(entry)) {
            if let Some(node) = self.nodes.iter_mut().find(|node| node.key == incoming.key) {
                node.rect = incoming.rect;
            }
        }
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        let items = self
            .nodes
            .iter()
            .map(|node| SnapshotItem {
                key: node.key.clone(),
                label: node.key.clone(),
                disabled: false,
                selected: false,
            })
            .collect();
        Snapshot {
            name: "canvas",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: ACTIONS,
            items,
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        let need = |action: &'static str| item.ok_or(ActError::MissingItem { action });
        let allowed = match action {
            "undo" => self.undo(Source::Agent, Reason::Action, gate),
            "redo" => self.redo(Source::Agent, Reason::Action, gate),
            "zoom_in" => {
                self.view.zoom_at(Point::default(), ZOOM_STEP);
                true
            }
            "zoom_out" => {
                self.view.zoom_at(Point::default(), 1.0 / ZOOM_STEP);
                true
            }
            "select" => {
                let key = need("select")?;
                if !self.has(key) {
                    return Err(ActError::UnknownItem(key.to_string()));
                }
                self.selected = vec![key.to_string()];
                true
            }
            // `a>b` joins a's outgoing port to b's incoming one.
            "connect" => {
                let text = need("connect")?;
                let unknown = || ActError::UnknownItem(text.to_string());
                let (from, to) = text.split_once('>').ok_or_else(unknown)?;
                if !self.has(from) || !self.has(to) {
                    return Err(unknown());
                }
                self.links.begin(from);
                self.links.finish(to, |_, _| true);
                true
            }
            // `key:x,y` puts a node's corner at a point.
            "move" => {
                let text = need("move")?;
                let unknown = || ActError::UnknownItem(text.to_string());
                let (key, point) = text.split_once(':').ok_or_else(unknown)?;
                let (x, y) = point.split_once(',').ok_or_else(unknown)?;
                let (x, y) = (
                    x.trim().parse::<f64>().map_err(|_| unknown())?,
                    y.trim().parse::<f64>().map_err(|_| unknown())?,
                );
                let before = self.nodes.clone();
                let node = self
                    .nodes
                    .iter_mut()
                    .find(|node| node.key == key)
                    .ok_or_else(unknown)?;
                node.rect = Rect { x, y, ..node.rect };
                self.settle(before, Source::Agent, Reason::Action, gate)
            }
            other => {
                return Err(ActError::UnknownAction {
                    action: other.to_string(),
                    valid: ACTIONS,
                });
            }
        };
        if allowed {
            Ok(())
        } else {
            Err(ActError::Refused)
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::string_slice,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_lossless,
    clippy::float_cmp,
    clippy::too_many_lines,
    reason = "a test fails by panicking, and its arithmetic is on values it wrote itself"
)]
mod tests {
    use super::*;

    fn canvas() -> Canvas {
        Canvas::new(vec![
            Node {
                key: "a".into(),
                rect: Rect::new(0.0, 0.0, 100.0, 60.0),
            },
            Node {
                key: "b".into(),
                rect: Rect::new(200.0, 0.0, 100.0, 60.0),
            },
            Node {
                key: "c".into(),
                rect: Rect::new(0.0, 200.0, 100.0, 60.0),
            },
        ])
    }

    /// The surface sits at the window's corner, so screen and canvas agree at zoom 1.
    fn surface(part: Part, _: Option<&str>) -> Option<Rect> {
        (part == Part::Surface).then_some(Rect::new(0.0, 0.0, 800.0, 600.0))
    }

    fn pointer(
        canvas: &mut Canvas,
        phase: PointerPhase,
        part: Part,
        item: Option<&str>,
        at: (f64, f64),
        mods: Modifiers,
        gate: Gate,
    ) {
        let pointer = Pointer {
            phase,
            part,
            item: item.map(str::to_string),
            at: Point::new(at.0, at.1),
            mods,
            now_ms: 0,
        };
        canvas.pointer(pointer, &surface, gate);
    }

    fn drag(
        canvas: &mut Canvas,
        part: Part,
        item: Option<&str>,
        from: (f64, f64),
        to: (f64, f64),
        mods: Modifiers,
        gate: Gate,
    ) {
        pointer(canvas, PointerPhase::Down, part, item, from, mods, gate);
        pointer(canvas, PointerPhase::Move, part, item, to, mods, gate);
        pointer(canvas, PointerPhase::Up, part, item, to, mods, gate);
    }

    fn rect_of(canvas: &Canvas, key: &str) -> Rect {
        canvas
            .nodes()
            .iter()
            .find(|node| node.key == key)
            .unwrap()
            .rect
    }

    const NONE: Modifiers = Modifiers {
        shift: false,
        ctrl: false,
        alt: false,
    };
    const CTRL: Modifiers = Modifiers {
        shift: false,
        ctrl: true,
        alt: false,
    };

    #[test]
    fn a_rectangle_drawn_on_the_surface_selects_what_it_touches() {
        let mut canvas = canvas();
        pointer(
            &mut canvas,
            PointerPhase::Down,
            Part::Surface,
            None,
            (150.0, 100.0),
            NONE,
            &mut |_| true,
        );
        pointer(
            &mut canvas,
            PointerPhase::Move,
            Part::Surface,
            None,
            (50.0, 30.0),
            NONE,
            &mut |_| true,
        );
        assert_eq!(canvas.selected(), ["a"]);
        assert_eq!(canvas.lasso(), Some(Rect::new(50.0, 30.0, 100.0, 70.0)));
        pointer(
            &mut canvas,
            PointerPhase::Move,
            Part::Surface,
            None,
            (250.0, 30.0),
            NONE,
            &mut |_| true,
        );
        assert_eq!(canvas.selected(), ["b"]);
        pointer(
            &mut canvas,
            PointerPhase::Up,
            Part::Surface,
            None,
            (250.0, 30.0),
            NONE,
            &mut |_| true,
        );
        assert_eq!(canvas.lasso(), None);
    }

    #[test]
    fn dragging_a_selected_node_moves_the_whole_selection() {
        let mut canvas = canvas();
        drag(
            &mut canvas,
            Part::Surface,
            None,
            (-10.0, -10.0),
            (350.0, 100.0),
            NONE,
            &mut |_| true,
        );
        assert_eq!(canvas.selected(), ["a", "b"]);
        drag(
            &mut canvas,
            Part::Node,
            Some("a"),
            (10.0, 10.0),
            (40.0, 30.0),
            NONE,
            &mut |_| true,
        );
        assert_eq!(
            (rect_of(&canvas, "a").x, rect_of(&canvas, "b").x),
            (30.0, 230.0)
        );
        assert_eq!(rect_of(&canvas, "c").x, 0.0);
    }

    #[test]
    fn undo_and_redo_step_through_moves() {
        let mut canvas = canvas();
        drag(
            &mut canvas,
            Part::Node,
            Some("a"),
            (10.0, 10.0),
            (60.0, 10.0),
            NONE,
            &mut |_| true,
        );
        let key = |canvas: &mut Canvas, mods| {
            canvas.send(
                Input::Key {
                    key: Key::Char('z'),
                    mods,
                    now_ms: 0,
                },
                &mut |_| true,
            )
        };
        key(&mut canvas, CTRL);
        assert_eq!(rect_of(&canvas, "a").x, 0.0);
        key(
            &mut canvas,
            Modifiers {
                shift: true,
                ..CTRL
            },
        );
        assert_eq!(rect_of(&canvas, "a").x, 50.0);
    }

    #[test]
    fn a_refused_move_snaps_back_and_leaves_nothing_to_undo() {
        let mut canvas = canvas();
        drag(
            &mut canvas,
            Part::Node,
            Some("a"),
            (10.0, 10.0),
            (60.0, 10.0),
            NONE,
            &mut |_| false,
        );
        assert_eq!(rect_of(&canvas, "a").x, 0.0);
        canvas.send(
            Input::Key {
                key: Key::Char('z'),
                mods: CTRL,
                now_ms: 0,
            },
            &mut |_| true,
        );
        assert_eq!(rect_of(&canvas, "a").x, 0.0);
    }

    #[test]
    fn zooming_scales_how_far_a_drag_moves_a_node() {
        let mut canvas = canvas();
        canvas.act("zoom_in", None, &mut |_| true).unwrap();
        assert_eq!(canvas.zoom(), 1.25);
        assert_eq!(canvas.screen_rect("b").unwrap().x, 250.0);
        drag(
            &mut canvas,
            Part::Node,
            Some("a"),
            (10.0, 10.0),
            (60.0, 10.0),
            NONE,
            &mut |_| true,
        );
        assert_eq!(
            rect_of(&canvas, "a").x,
            40.0,
            "50 on screen is 40 on the canvas"
        );
    }

    #[test]
    fn alt_dragging_the_surface_pans_and_selects_nothing() {
        let mut canvas = canvas();
        let alt = Modifiers { alt: true, ..NONE };
        drag(
            &mut canvas,
            Part::Surface,
            None,
            (10.0, 10.0),
            (110.0, 60.0),
            alt,
            &mut |_| true,
        );
        assert_eq!(canvas.selected().len(), 0);
        assert_eq!(
            canvas.screen_rect("a").unwrap(),
            Rect::new(100.0, 50.0, 100.0, 60.0)
        );
    }

    #[test]
    fn the_corner_resizes_a_node_down_to_a_minimum() {
        let mut canvas = canvas();
        drag(
            &mut canvas,
            Part::Resize,
            Some("a"),
            (100.0, 60.0),
            (150.0, 90.0),
            NONE,
            &mut |_| true,
        );
        assert_eq!(
            (rect_of(&canvas, "a").width, rect_of(&canvas, "a").height),
            (150.0, 90.0)
        );
        drag(
            &mut canvas,
            Part::Resize,
            Some("a"),
            (150.0, 90.0),
            (-500.0, -500.0),
            NONE,
            &mut |_| true,
        );
        assert_eq!(
            (rect_of(&canvas, "a").width, rect_of(&canvas, "a").height),
            (40.0, 30.0)
        );
    }

    #[test]
    fn pressing_an_out_port_then_an_in_port_connects_them() {
        let mut canvas = canvas();
        let press = |canvas: &mut Canvas, part, item: &str| {
            canvas.send(
                Input::Press {
                    part,
                    item: Some(item.into()),
                    mods: NONE,
                },
                &mut |_| true,
            )
        };
        press(&mut canvas, Part::PortOut, "a");
        assert!(canvas.semantics(Part::PortOut, Some("a")).active);
        let joined = press(&mut canvas, Part::PortIn, "b");
        assert_eq!(
            joined.effects,
            [Effect::Announce {
                message: "a connected to b.".into(),
                assertive: false
            }]
        );
        assert_eq!(canvas.connections().len(), 1);
        press(&mut canvas, Part::PortOut, "a");
        let refused = press(&mut canvas, Part::PortIn, "a");
        assert_eq!(
            refused.effects,
            [Effect::Announce {
                message: "a cannot connect to a.".into(),
                assertive: false
            }]
        );
    }

    #[test]
    fn arrows_nudge_the_selection_and_escape_clears_it() {
        let mut canvas = canvas();
        canvas.act("select", Some("c"), &mut |_| true).unwrap();
        let key = |canvas: &mut Canvas, key, mods| {
            canvas.send(
                Input::Key {
                    key,
                    mods,
                    now_ms: 0,
                },
                &mut |_| true,
            )
        };
        key(
            &mut canvas,
            Key::ArrowRight,
            Modifiers {
                shift: true,
                ..NONE
            },
        );
        key(&mut canvas, Key::ArrowUp, NONE);
        assert_eq!(
            (rect_of(&canvas, "c").x, rect_of(&canvas, "c").y),
            (10.0, 199.0)
        );
        assert!(key(&mut canvas, Key::Escape, NONE).handled);
        assert_eq!(canvas.selected().len(), 0);
    }

    #[test]
    fn alt_with_an_arrow_moves_the_selection_to_the_nearest_node_that_way() {
        let mut canvas = canvas();
        canvas.act("select", Some("a"), &mut |_| true).unwrap();
        let alt = Modifiers { alt: true, ..NONE };
        let key = |canvas: &mut Canvas, key| {
            canvas.send(
                Input::Key {
                    key,
                    mods: alt,
                    now_ms: 0,
                },
                &mut |_| true,
            )
        };
        key(&mut canvas, Key::ArrowRight);
        assert_eq!(canvas.selected(), ["b"]);
        key(&mut canvas, Key::ArrowRight);
        assert_eq!(canvas.selected(), ["b"], "nothing further that way");
        key(&mut canvas, Key::ArrowLeft);
        key(&mut canvas, Key::ArrowDown);
        assert_eq!(canvas.selected(), ["c"]);
        assert_eq!(
            rect_of(&canvas, "c").y,
            200.0,
            "the nodes themselves did not move"
        );
    }

    #[test]
    fn it_conforms() {
        let nodes: Vec<Item> = ["a", "b"]
            .iter()
            .map(|key| Item {
                key: key.to_string(),
                label: key.to_string(),
                disabled: false,
            })
            .collect();
        crate::testing::assert_conforms(canvas, &nodes);
    }
}
