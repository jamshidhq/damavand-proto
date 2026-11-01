//! Protocol versions and version negotiation.
//!
//! A [`ProtocolVersion`] is `major.minor`, written on the wire as the string
//! `"major.minor"` with decimal parts and no leading zeros. Versions compare
//! numerically, by major and then by minor.
//!
//! Compatibility follows Cargo: a major version change is breaking and a minor
//! change is additive, so a peer that supports `1.4` also supports `1.0` to
//! `1.3`. In major `0` every minor is breaking: `0.1` and `0.2` are separate
//! compatibility lines, like two majors.
//!
//! A peer announces [`SupportedVersions`]: one entry per compatibility line,
//! each the highest minor it supports in that line, for example
//! `["1.4", "2.0"]`. Negotiation picks the highest line both peers support,
//! and in it the lower of the two minors.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A protocol version, `major.minor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProtocolVersion {
    major: u16,
    minor: u16,
}

/// The protocol version this crate implements.
pub const CURRENT: ProtocolVersion = ProtocolVersion::new(0, 1);

/// Why a string is not a protocol version, or a list of versions is not a
/// valid supported-versions list.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum VersionError {
    /// The string is not `major.minor` with two decimal `u16` parts without
    /// leading zeros.
    Malformed(String),
    /// The list is empty.
    Empty,
    /// Two entries are in the same compatibility line (the same major, or the
    /// same `0.minor`).
    DuplicateMajor {
        /// The lower entry.
        first: ProtocolVersion,
        /// The higher entry.
        second: ProtocolVersion,
    },
}

impl fmt::Display for VersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VersionError::Malformed(text) => write!(
                f,
                "invalid protocol version {text:?}: expected \"major.minor\" without leading zeros"
            ),
            VersionError::Empty => f.write_str("the supported-versions list is empty"),
            VersionError::DuplicateMajor { first, second } => write!(
                f,
                "versions {first} and {second} are in the same compatibility line: \
                 list only the highest minor"
            ),
        }
    }
}

impl std::error::Error for VersionError {}

impl ProtocolVersion {
    /// A version from its parts.
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }

    /// The major version.
    pub const fn major(self) -> u16 {
        self.major
    }

    /// The minor version.
    pub const fn minor(self) -> u16 {
        self.minor
    }

    /// Whether the two versions are in the same compatibility line: the same
    /// major `>= 1`, or the same `0.minor`.
    pub fn is_compatible_with(self, other: ProtocolVersion) -> bool {
        self.line() == other.line()
    }

    /// The compatibility line as an ordered key. Lines order like their
    /// versions: every `0.minor` line is below `1`.
    fn line(self) -> (u16, u16) {
        if self.major == 0 {
            (0, self.minor)
        } else {
            (self.major, 0)
        }
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
        // Digits only (`u16::from_str` alone would accept a leading `+`), and
        // no leading zero: "0" is fine, "01" and "00" are not.
        let part = |text: &str| {
            let digits = !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit());
            if !digits || (text.len() > 1 && text.starts_with('0')) {
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

/// The versions a peer supports: a non-empty list with one entry per
/// compatibility line, each the highest minor the peer supports in it.
///
/// On the wire: a JSON array of version strings, for example
/// `["1.4", "2.0"]`. The order on the wire does not matter; this type keeps
/// the entries in ascending order.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "Vec<ProtocolVersion>")]
pub struct SupportedVersions(Vec<ProtocolVersion>);

impl TryFrom<Vec<ProtocolVersion>> for SupportedVersions {
    type Error = VersionError;

    fn try_from(versions: Vec<ProtocolVersion>) -> Result<Self, Self::Error> {
        SupportedVersions::new(versions)
    }
}

impl SupportedVersions {
    /// The list of `versions`. Fails when it is empty or two entries are in
    /// the same compatibility line.
    pub fn new(versions: impl IntoIterator<Item = ProtocolVersion>) -> Result<Self, VersionError> {
        let mut versions: Vec<ProtocolVersion> = versions.into_iter().collect();
        versions.sort_unstable();
        if versions.is_empty() {
            return Err(VersionError::Empty);
        }
        if let Some(pair) = versions
            .windows(2)
            .find(|pair| pair[0].is_compatible_with(pair[1]))
        {
            return Err(VersionError::DuplicateMajor {
                first: pair[0],
                second: pair[1],
            });
        }
        Ok(Self(versions))
    }

    /// The list that holds only `version`.
    pub fn single(version: ProtocolVersion) -> Self {
        Self(vec![version])
    }

    /// The entries, in ascending order.
    pub fn iter(&self) -> impl Iterator<Item = ProtocolVersion> + '_ {
        self.0.iter().copied()
    }

    /// The number of entries.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Always `false`: a list is never empty. Present for API symmetry with
    /// [`SupportedVersions::len`].
    pub fn is_empty(&self) -> bool {
        // Always false by construction, so a mutant returning `false` is
        // equivalent.
        self.0.is_empty()
    }

    /// The highest entry.
    pub fn highest(&self) -> ProtocolVersion {
        // `new` and `single` never build an empty list.
        self.0[self.0.len() - 1]
    }

    /// The negotiated version: in the highest compatibility line both lists
    /// hold, the lower of the two minors. `None` when no line is common.
    pub fn highest_common(&self, other: &SupportedVersions) -> Option<ProtocolVersion> {
        self.0.iter().rev().find_map(|&mine| {
            other
                .0
                .iter()
                .find(|theirs| theirs.is_compatible_with(mine))
                .map(|&theirs| mine.min(theirs))
        })
    }
}

impl fmt::Display for SupportedVersions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[")?;
        for (i, version) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{version}")?;
        }
        f.write_str("]")
    }
}
