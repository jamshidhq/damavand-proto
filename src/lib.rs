//! Versioned out-of-process plugin wire protocol for the damavand
//! agent-safety crates.
//!
//! The protocol is JSON-RPC 2.0, in the style of LSP and MCP. A session starts
//! with an `initialize` handshake: each peer states the protocol version range
//! and the capabilities it supports, and the handshake picks the highest
//! common version and the common capabilities.
//!
//! - [`jsonrpc`]: the JSON-RPC 2.0 envelope (request, notification, response,
//!   error object) with strict validation of incoming messages.
//! - [`version`]: [`ProtocolVersion`] and [`VersionRange`].
//! - [`capability`]: [`CapabilitySet`], an extensible set of named features.
//! - [`handshake`]: the `initialize` exchange and version negotiation.
//! - `conformance` (feature `conformance`): a reusable test suite that other
//!   crates run against their implementation.
//!
//! This crate has no transport yet. It defines the messages only.
#![forbid(unsafe_code)]

pub mod capability;
#[cfg(feature = "conformance")]
pub mod conformance;
pub mod handshake;
pub mod jsonrpc;
pub mod version;

pub use capability::CapabilitySet;
pub use handshake::{IncompatibleVersion, InitializeParams, InitializeResult, negotiate};
pub use jsonrpc::{EnvelopeError, ErrorObject, Id, Message, Notification, Request, Response};
pub use version::{ProtocolVersion, VersionRange};
