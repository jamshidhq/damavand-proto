//! Capabilities: named features a peer supports.
//!
//! A capability is a string name, for example `"tools.call"`. The set is open:
//! a peer may announce names the other peer does not know. Negotiation keeps
//! only the names both peers announce, so an unknown name is ignored, never an
//! error. On the wire a set is a JSON array of strings, sorted, with no
//! duplicates.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// A set of capability names.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CapabilitySet(BTreeSet<String>);

impl CapabilitySet {
    /// An empty set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a capability. Returns `false` when it was already in the set.
    pub fn insert(&mut self, name: impl Into<String>) -> bool {
        self.0.insert(name.into())
    }

    /// Whether the set holds `name`.
    pub fn contains(&self, name: &str) -> bool {
        self.0.contains(name)
    }

    /// The names in both sets.
    pub fn intersection(&self, other: &CapabilitySet) -> CapabilitySet {
        CapabilitySet(self.0.intersection(&other.0).cloned().collect())
    }

    /// The names, in sorted order.
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(String::as_str)
    }

    /// The number of names.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the set is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<S: Into<String>> FromIterator<S> for CapabilitySet {
    fn from_iter<I: IntoIterator<Item = S>>(iter: I) -> Self {
        let mut set = Self::new();
        for name in iter {
            set.insert(name);
        }
        set
    }
}
