//! Protocol versions and version ranges.
//!
//! A [`ProtocolVersion`] is `major.minor`. It is written on the wire as the
//! string `"major.minor"`. Versions order by major, then by minor. A
//! [`VersionRange`] is an inclusive range of versions that a peer supports.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A protocol version, `major.minor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProtocolVersion {
    /// The major version.
    pub major: u16,
    /// The minor version.
    pub minor: u16,
}

/// The protocol version this crate implements.
pub const CURRENT: ProtocolVersion = ProtocolVersion::new(0, 1);

/// Why a string is not a protocol version, or two versions are not a range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionError {
    /// The string is not `major.minor` with two decimal `u16` parts.
    Malformed(String),
    /// The range minimum is greater than its maximum.
    EmptyRange {
        /// The minimum given.
        min: ProtocolVersion,
        /// The maximum given.
        max: ProtocolVersion,
    },
}

impl fmt::Display for VersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VersionError::Malformed(text) => {
                write!(
                    f,
                    "invalid protocol version {text:?}: expected \"major.minor\""
                )
            }
            VersionError::EmptyRange { min, max } => {
                write!(
                    f,
                    "empty version range: min {min} is greater than max {max}"
                )
            }
        }
    }
}

impl std::error::Error for VersionError {}

impl ProtocolVersion {
    /// A version from its parts.
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }
}

impl fmt::Display for ProtocolVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

impl FromStr for ProtocolVersion {
    type Err = VersionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let malformed = || VersionError::Malformed(s.to_owned());
        // Digits only: `u16::from_str` alone would accept a leading `+`.
        let part = |text: &str| {
            if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
                return Err(malformed());
            }
            text.parse::<u16>().map_err(|_| malformed())
        };
        let (major, minor) = s.split_once('.').ok_or_else(malformed)?;
        Ok(Self::new(part(major)?, part(minor)?))
    }
}

impl Serialize for ProtocolVersion {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ProtocolVersion {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// An inclusive, non-empty range of protocol versions.
///
/// On the wire: `{"min": "0.1", "max": "0.3"}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "RawRange")]
pub struct VersionRange {
    min: ProtocolVersion,
    max: ProtocolVersion,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRange {
    min: ProtocolVersion,
    max: ProtocolVersion,
}

impl TryFrom<RawRange> for VersionRange {
    type Error = VersionError;

    fn try_from(raw: RawRange) -> Result<Self, Self::Error> {
        VersionRange::new(raw.min, raw.max)
    }
}

impl VersionRange {
    /// The range `min..=max`. Fails when `min > max`.
    pub fn new(min: ProtocolVersion, max: ProtocolVersion) -> Result<Self, VersionError> {
        if min > max {
            return Err(VersionError::EmptyRange { min, max });
        }
        Ok(Self { min, max })
    }

    /// The range that holds only `version`.
    pub const fn single(version: ProtocolVersion) -> Self {
        Self {
            min: version,
            max: version,
        }
    }

    /// The lowest version in the range.
    pub const fn min(&self) -> ProtocolVersion {
        self.min
    }

    /// The highest version in the range.
    pub const fn max(&self) -> ProtocolVersion {
        self.max
    }

    /// Whether `version` is in the range.
    pub fn contains(&self, version: ProtocolVersion) -> bool {
        self.min <= version && version <= self.max
    }

    /// The highest version in both ranges, or `None` when they do not overlap.
    pub fn highest_common(&self, other: &VersionRange) -> Option<ProtocolVersion> {
        let high = self.max.min(other.max);
        (high >= self.min.max(other.min)).then_some(high)
    }
}

impl fmt::Display for VersionRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..={}", self.min, self.max)
    }
}
