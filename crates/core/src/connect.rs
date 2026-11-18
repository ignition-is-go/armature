//! Connections between ports, as in a node editor. A connection is begun at one port
//! and finished at another, by pointer or keyboard.

use crate::geometry::{Point, Rect};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Connection {
    pub from: String,
    pub to: String,
}

#[derive(Clone, Debug, Default)]
pub struct Connect {
    connections: Vec<Connection>,
    pending: Option<String>,
}

impl Connect {
    #[must_use]
    pub fn connections(&self) -> &[Connection] {
        &self.connections
    }

    /// The port a connection is being drawn from.
    #[must_use]
    pub fn pending(&self) -> Option<&str> {
        self.pending.as_deref()
    }

    pub fn begin(&mut self, port: &str) {
        self.pending = Some(port.to_string());
    }

    /// Finishes at `port`. `allowed` decides whether these two may be joined. A port
    /// cannot join itself, and a pair is only joined once.
    pub fn finish(
        &mut self,
        port: &str,
        allowed: impl Fn(&str, &str) -> bool,
    ) -> Option<Connection> {
        let from = self.pending.take()?;
        let duplicate = self
            .connections
            .iter()
            .any(|c| c.from == from && c.to == port);
        if from == port || duplicate || !allowed(&from, port) {
            return None;
        }
        let connection = Connection {
            from,
            to: port.to_string(),
        };
        self.connections.push(connection.clone());
        Some(connection)
    }

    pub fn cancel(&mut self) {
        self.pending = None;
    }

    pub fn disconnect(&mut self, from: &str, to: &str) -> bool {
        let before = self.connections.len();
        self.connections.retain(|c| !(c.from == from && c.to == to));
        self.connections.len() != before
    }
}

/// The port under the pointer, or the nearest one within `reach` of it.
#[must_use]
pub fn port_at(ports: &[(String, Rect)], at: Point, reach: f64) -> Option<&str> {
    ports
        .iter()
        .map(|(key, rect)| {
            (
                key,
                if rect.contains(at) {
                    0.0
                } else {
                    rect.center().distance(at)
                },
            )
        })
        .filter(|(_, distance)| *distance <= reach)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(key, _)| key.as_str())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectStep {
    Begun,
    Connected,
    Refused,
    Cancelled,
}

/// What to tell a screen reader at each step.
#[must_use]
pub fn connect_message(step: ConnectStep, from: &str, to: &str) -> String {
    match step {
        ConnectStep::Begun => format!(
            "Connecting from {from}. Choose a port to connect to, or press Escape to cancel."
        ),
        ConnectStep::Connected => format!("{from} connected to {to}."),
        ConnectStep::Refused => format!("{from} cannot connect to {to}."),
        ConnectStep::Cancelled => "Connection cancelled.".to_string(),
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

    #[test]
    fn a_connection_runs_from_the_port_it_began_at() {
        let mut connect = Connect::default();
        connect.begin("out");
        let made = connect.finish("in", |_, _| true).unwrap();
        assert_eq!((made.from.as_str(), made.to.as_str()), ("out", "in"));
        assert_eq!(connect.pending(), None);
    }

    #[test]
    fn self_duplicate_and_disallowed_connections_are_refused() {
        let mut connect = Connect::default();
        connect.begin("a");
        assert_eq!(connect.finish("a", |_, _| true), None);
        connect.begin("a");
        connect.finish("b", |_, _| true);
        connect.begin("a");
        assert_eq!(connect.finish("b", |_, _| true), None);
        connect.begin("a");
        assert_eq!(connect.finish("c", |_, to| to != "c"), None);
        assert_eq!(connect.connections().len(), 1);
    }

    #[test]
    fn finishing_without_beginning_and_cancelling_make_nothing() {
        let mut connect = Connect::default();
        assert_eq!(connect.finish("b", |_, _| true), None);
        connect.begin("a");
        connect.cancel();
        assert_eq!(connect.finish("b", |_, _| true), None);
    }

    #[test]
    fn the_pointer_snaps_to_the_nearest_port_within_reach() {
        let ports = vec![
            ("a".to_string(), Rect::new(0., 0., 10., 10.)),
            ("b".to_string(), Rect::new(100., 0., 10., 10.)),
        ];
        assert_eq!(port_at(&ports, Point::new(5., 5.), 20.), Some("a"));
        assert_eq!(port_at(&ports, Point::new(90., 5.), 20.), Some("b"));
        assert_eq!(port_at(&ports, Point::new(50., 5.), 20.), None);
    }

    #[test]
    fn a_connection_can_be_removed() {
        let mut connect = Connect::default();
        connect.begin("a");
        connect.finish("b", |_, _| true);
        assert!(connect.disconnect("a", "b"));
        assert!(!connect.disconnect("a", "b"));
    }
}
