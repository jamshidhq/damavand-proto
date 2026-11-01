//! Capabilities: named features a peer supports, each with a parameter object.
//!
//! On the wire, capabilities are a JSON object keyed by capability name, in the
//! style of MCP: `{"tools": {"listChanged": true}, "audit": {}}`. A name
//! matches `[a-z][a-z0-9_.-]*`. Each value is a JSON object that holds the
//! capability's parameters; `{}` is a capability with no parameters.
//!
//! The set is open: a peer may announce names the other peer does not know.
//! Negotiation keeps only the names both peers announce, so an unknown name is
//! ignored, never an error. For each kept name the result holds the server's
//! parameter object. How the parameters of one capability are negotiated is
//! defined by that capability.

use std::collections::BTreeMap;
use std::fmt;

use serde::de::{self, MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

/// Whether `name` is a valid capability name: `[a-z][a-z0-9_.-]*`.
pub fn is_valid_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_lowercase())
        && bytes.all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'.' | b'-')
        })
}

/// Why a capability cannot be added or read.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CapabilityError {
    /// The name does not match `[a-z][a-z0-9_.-]*`.
    InvalidName(String),
    /// The name is already in the set.
    Duplicate(String),
    /// The value of the named capability is not a JSON object.
    ParamsNotObject(String),
}

impl fmt::Display for CapabilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CapabilityError::InvalidName(name) => write!(
                f,
                "invalid capability name {name:?}: expected [a-z][a-z0-9_.-]*"
            ),
            CapabilityError::Duplicate(name) => write!(f, "duplicate capability {name:?}"),
            CapabilityError::ParamsNotObject(name) => {
                write!(f, "parameters of capability {name:?} are not a JSON object")
            }
        }
    }
}

impl std::error::Error for CapabilityError {}

/// A set of capabilities, by name, each with its parameter object.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Capabilities(BTreeMap<String, Map<String, Value>>);

impl Capabilities {
    /// An empty set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a capability with its parameters. Fails when the name is not
    /// valid or is already in the set; the set is then unchanged.
    pub fn insert(
        &mut self,
        name: impl Into<String>,
        params: Map<String, Value>,
    ) -> Result<(), CapabilityError> {
        let name = name.into();
        if !is_valid_name(&name) {
            return Err(CapabilityError::InvalidName(name));
        }
        if self.0.contains_key(&name) {
            return Err(CapabilityError::Duplicate(name));
        }
        self.0.insert(name, params);
        Ok(())
    }

    /// Whether the set holds `name`.
    pub fn contains(&self, name: &str) -> bool {
        self.0.contains_key(name)
    }

    /// The parameters of `name`.
    pub fn get(&self, name: &str) -> Option<&Map<String, Value>> {
        self.0.get(name)
    }

    /// The names, in sorted order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(String::as_str)
    }

    /// The names and their parameters, in sorted order of name.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Map<String, Value>)> {
        self.0.iter().map(|(name, params)| (name.as_str(), params))
    }

    /// The number of capabilities.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the set is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The capabilities whose names are in both sets, with the parameters
    /// from `self`. The server calls this on its own set.
    pub fn intersection(&self, other: &Capabilities) -> Capabilities {
        Capabilities(
            self.0
                .iter()
                .filter(|(name, _)| other.0.contains_key(*name))
                .map(|(name, params)| (name.clone(), params.clone()))
                .collect(),
        )
    }
}

impl Serialize for Capabilities {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (name, params) in &self.0 {
            map.serialize_entry(name, params)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for Capabilities {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(CapabilitiesVisitor)
    }
}

struct CapabilitiesVisitor;

impl<'de> Visitor<'de> for CapabilitiesVisitor {
    type Value = Capabilities;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a JSON object of capability parameter objects")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Capabilities, A::Error> {
        let mut set = Capabilities::new();
        while let Some(name) = map.next_key::<String>()? {
            let params = match map.next_value::<Value>()? {
                Value::Object(params) => params,
                _ => return Err(de::Error::custom(CapabilityError::ParamsNotObject(name))),
            };
            set.insert(name, params).map_err(de::Error::custom)?;
        }
        Ok(set)
    }
}
