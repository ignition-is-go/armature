#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    User,
    Agent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    Press,
    Keyboard,
    Action,
}

/// A value change offered to the owner before it is committed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueChange {
    pub value: Vec<String>,
    pub source: Source,
    pub reason: Reason,
}

/// Decides whether a proposed change is committed. Returning `false` cancels it.
pub type Gate<'a> = &'a mut dyn FnMut(&ValueChange) -> bool;
