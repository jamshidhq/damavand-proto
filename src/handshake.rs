//! The `initialize` handshake.
//!
//! The client sends an `initialize` request with [`InitializeParams`]: the
//! version range and the capabilities it supports. The server answers with an
//! [`InitializeResult`]: the highest version both support and the capabilities
//! both announce. When the ranges do not overlap, the server answers with an
//! error whose code is [`codes::INCOMPATIBLE_VERSION`] and whose `data` holds
//! the server's range as `{"supported": {"min": .., "max": ..}}`.
//!
//! [`codes::INCOMPATIBLE_VERSION`]: crate::jsonrpc::codes::INCOMPATIBLE_VERSION

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::capability::CapabilitySet;
use crate::jsonrpc::{ErrorObject, Id, Params, Request, Response, codes};
use crate::version::{ProtocolVersion, VersionRange};

/// The method name of the handshake request.
pub const METHOD_INITIALIZE: &str = "initialize";

/// What a peer supports: the params of an `initialize` request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InitializeParams {
    /// The protocol versions the peer supports.
    pub versions: VersionRange,
    /// The capabilities the peer supports. A missing member is an empty set.
    #[serde(default)]
    pub capabilities: CapabilitySet,
}

/// The outcome of a successful handshake: the result of `initialize`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InitializeResult {
    /// The protocol version the session uses.
    pub version: ProtocolVersion,
    /// The capabilities both peers announced.
    pub capabilities: CapabilitySet,
}

/// The peers support no common protocol version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IncompatibleVersion {
    /// The range this side supports.
    pub local: VersionRange,
    /// The range the other side offered.
    pub remote: VersionRange,
}

impl fmt::Display for IncompatibleVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "no common protocol version: this side supports {}, the other side offered {}",
            self.local, self.remote
        )
    }
}

impl std::error::Error for IncompatibleVersion {}

impl IncompatibleVersion {
    /// The error object a server sends for this failure.
    pub fn to_error_object(&self) -> ErrorObject {
        ErrorObject::new(codes::INCOMPATIBLE_VERSION, self.to_string())
            .with_data(serde_json::json!({ "supported": self.local }))
    }
}

impl InitializeParams {
    /// An `initialize` request with these params.
    pub fn to_request(&self, id: Id) -> Request {
        let params = match serde_json::to_value(self) {
            Ok(serde_json::Value::Object(members)) => Some(Params::Object(members)),
            // InitializeParams always serializes to an object.
            _ => None,
        };
        Request {
            id,
            method: METHOD_INITIALIZE.to_owned(),
            params,
        }
    }
}

/// Picks the highest common version and the common capabilities.
pub fn negotiate(
    local: &InitializeParams,
    remote: &InitializeParams,
) -> Result<InitializeResult, IncompatibleVersion> {
    let version = local
        .versions
        .highest_common(&remote.versions)
        .ok_or(IncompatibleVersion {
            local: local.versions,
            remote: remote.versions,
        })?;
    Ok(InitializeResult {
        version,
        capabilities: local.capabilities.intersection(&remote.capabilities),
    })
}

/// The reference server side of the handshake: answers `request` for a server
/// that supports `local`.
///
/// - another method: error [`METHOD_NOT_FOUND`](crate::jsonrpc::codes::METHOD_NOT_FOUND);
/// - params that are not [`InitializeParams`]: error
///   [`INVALID_PARAMS`](crate::jsonrpc::codes::INVALID_PARAMS);
/// - no common version: error
///   [`INCOMPATIBLE_VERSION`](crate::jsonrpc::codes::INCOMPATIBLE_VERSION);
/// - otherwise: the [`InitializeResult`] from [`negotiate`].
///
/// The response always carries the request id.
pub fn answer_initialize(local: &InitializeParams, request: &Request) -> Response {
    let id = Some(request.id.clone());
    if request.method != METHOD_INITIALIZE {
        return Response::error(
            id,
            ErrorObject::new(
                codes::METHOD_NOT_FOUND,
                format!("method not found: {}", request.method),
            ),
        );
    }
    let remote = match &request.params {
        Some(Params::Object(members)) => {
            serde_json::from_value::<InitializeParams>(serde_json::Value::Object(members.clone()))
                .map_err(|err| err.to_string())
        }
        Some(Params::Array(_)) => Err("params must be an object".to_owned()),
        None => Err("params are required".to_owned()),
    };
    let remote = match remote {
        Ok(remote) => remote,
        Err(detail) => {
            return Response::error(
                id,
                ErrorObject::new(
                    codes::INVALID_PARAMS,
                    format!("invalid initialize params: {detail}"),
                ),
            );
        }
    };
    match negotiate(local, &remote) {
        Ok(result) => match serde_json::to_value(result) {
            Ok(value) => Response::success(request.id.clone(), value),
            Err(err) => {
                Response::error(id, ErrorObject::new(codes::INTERNAL_ERROR, err.to_string()))
            }
        },
        Err(incompatible) => Response::error(id, incompatible.to_error_object()),
    }
}
