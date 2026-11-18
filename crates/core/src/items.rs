use crate::primitive::Item;
use crate::roving::{self, Nav};

/// The ordered items of a collection primitive.
#[derive(Clone, Debug, Default)]
pub struct Items(Vec<Item>);

impl Items {
    pub fn iter(&self) -> impl Iterator<Item = &Item> {
        self.0.iter()
    }

    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Item> {
        self.0.iter().find(|i| i.key == key)
    }

    #[must_use]
    pub fn key_at(&self, index: usize) -> String {
        self.0
            .get(index)
            .map(|item| item.key.clone())
            .unwrap_or_default()
    }

    #[must_use]
    pub fn index_of(&self, key: &str) -> Option<usize> {
        self.0.iter().position(|i| i.key == key)
    }

    #[must_use]
    pub fn is_enabled(&self, key: &str) -> bool {
        self.get(key).is_some_and(|i| !i.disabled)
    }

    #[must_use]
    pub fn first_enabled(&self) -> Option<&str> {
        self.0.iter().find(|i| !i.disabled).map(|i| i.key.as_str())
    }

    #[must_use]
    pub fn enabled(&self) -> Vec<bool> {
        self.0.iter().map(|i| !i.disabled).collect()
    }

    #[must_use]
    pub fn enabled_keys(&self) -> Vec<&str> {
        self.0
            .iter()
            .filter(|i| !i.disabled)
            .map(|i| i.key.as_str())
            .collect()
    }

    #[must_use]
    pub fn step(&self, from: Option<&str>, nav: Nav, wrap: bool) -> Option<String> {
        let current = from.and_then(|key| self.index_of(key));
        roving::step(&self.enabled(), current, nav, wrap).map(|i| self.key_at(i))
    }

    pub fn upsert(&mut self, item: Item) {
        match self.0.iter_mut().find(|i| i.key == item.key) {
            Some(existing) => *existing = item,
            None => self.0.push(item),
        }
    }

    /// Removes an item and returns its nearest enabled neighbour, for focus recovery.
    pub fn remove(&mut self, key: &str) -> Option<String> {
        let index = self.index_of(key)?;
        let enabled = self.enabled();
        let neighbour = roving::step(&enabled, Some(index), Nav::Next, false)
            .or_else(|| roving::step(&enabled, Some(index), Nav::Prev, false))
            .map(|i| self.key_at(i));
        self.0.remove(index);
        neighbour
    }

    pub fn set_order(&mut self, rendered: &[(String, String)]) {
        let rank = |key: &str| {
            rendered
                .iter()
                .position(|(k, _)| k == key)
                .unwrap_or(usize::MAX)
        };
        self.0.sort_by_key(|item| rank(&item.key));
        for item in self.0.iter_mut().filter(|i| i.label.is_empty()) {
            if let Some((_, text)) = rendered.iter().find(|(k, _)| *k == item.key) {
                item.label = text.trim().to_string();
            }
        }
    }
}
