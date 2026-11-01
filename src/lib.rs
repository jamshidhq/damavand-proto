//! Versioned out-of-process plugin wire protocol for the damavand
//! agent-safety crates.
//!
//! The protocol is JSON-RPC 2.0, in the style of LSP and MCP. A session starts
//! with an `initialize` handshake: each peer states the protocol versions and
//! the capabilities it supports, and the handshake picks the highest common
//! version and the common capabilities. `SPEC.md` is the normative,
//! language-neutral specification; this crate is its reference
//! implementation.
//!
//! - [`jsonrpc`]: the JSON-RPC 2.0 envelope (request, notification, response,
//!   error object) with strict validation of incoming messages, and the
//!   error-code registry ([`jsonrpc::codes`]).
//! - [`version`]: [`ProtocolVersion`], [`SupportedVersions`] and the
//!   negotiation rule.
//! - [`capability`]: [`Capabilities`], named features with parameter objects.
//! - [`handshake`]: the `initialize` exchange.
//! - `conformance` (feature `conformance`): a reusable test suite that other
//!   crates run against their implementation.
//!
//! # Stability
//!
//! The wire protocol is versioned and negotiated from its first release:
//! a peer keeps working with peers that speak an older protocol version, by
//! negotiation. The Rust API of this crate can change in any release before
//! 1.0.
//!
//! `serde_json` is a public dependency: its `Value` and `Map` types appear in
//! this crate's API.
//!
//! This crate has no transport yet. It defines the messages only.
#![forbid(unsafe_code)]

pub mod capability;
#[cfg(feature = "conformance")]
pub mod conformance;
pub mod handshake;
pub mod jsonrpc;
pub mod version;

pub use capability::Capabilities;
pub use handshake::{IncompatibleVersion, InitializeParams, InitializeResult, negotiate};
pub use jsonrpc::{EnvelopeError, ErrorObject, Id, Message, Notification, Request, Response};
pub use version::{ProtocolVersion, SupportedVersions};
