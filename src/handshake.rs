//! The `initialize` handshake.
//!
//! The client sends an `initialize` request with [`InitializeParams`]: the
//! versions and the capabilities it supports, and optionally `clientInfo`. The
//! server answers with an [`InitializeResult`]: the negotiated version (see
//! [`SupportedVersions::highest_common`]), the capabilities both announce
//! with the server's parameters, and optionally `serverInfo`. When no version
//! is common, the server answers with an error whose code is
//! [`codes::INCOMPATIBLE_VERSION`] and whose `data` holds the server's list as
//! `{"supported": ["1.4", "2.0"]}`.
//!
//! [`Supported`] is what one peer supports. A client turns it into
//! [`InitializeParams`]; a server passes it to [`negotiate`] or
//! [`answer_initialize`].
//!
//! [`codes::INCOMPATIBLE_VERSION`]: crate::jsonrpc::codes::INCOMPATIBLE_VERSION

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::capability::Capabilities;
use crate::jsonrpc::{ErrorObject, Id, Params, Request, Response, codes};
use crate::version::{ProtocolVersion, SupportedVersions};

/// The method name of the handshake request.
pub const METHOD_INITIALIZE: &str = "initialize";

/// The name and version of a peer implementation: `clientInfo` or
/// `serverInfo`. Informational only; negotiation does not use it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PeerInfo {
    name: String,
    version: String,
}

impl PeerInfo {
    /// A peer info.
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
        }
    }

    /// The implementation name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The implementation version.
    pub fn version(&self) -> &str {
        &self.version
    }
}

/// What one peer supports: its versions, its capabilities and, optionally,
/// its name and version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Supported {
    versions: SupportedVersions,
    capabilities: Capabilities,
    info: Option<PeerInfo>,
}

impl Supported {
    /// A peer that supports `versions`, with no capabilities and no info.
    pub fn new(versions: SupportedVersions) -> Self {
        Self {
            versions,
            capabilities: Capabilities::new(),
            info: None,
        }
    }

    /// Sets the capabilities.
    pub fn with_capabilities(mut self, capabilities: Capabilities) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Sets the peer info.
    pub fn with_info(mut self, info: PeerInfo) -> Self {
        self.info = Some(info);
        self
    }

    /// The supported versions.
    pub fn versions(&self) -> &SupportedVersions {
        &self.versions
    }

    /// The supported capabilities.
    pub fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    /// The peer info, if set.
    pub fn info(&self) -> Option<&PeerInfo> {
        self.info.as_ref()
    }

    /// The `initialize` params a client with this support sends.
    pub fn to_params(&self) -> InitializeParams {
        InitializeParams {
            versions: self.versions.clone(),
            capabilities: self.capabilities.clone(),
            client_info: self.info.clone(),
        }
    }

    /// The `initialize` request a client with this support sends.
    pub fn to_request(&self, id: Id) -> Request {
        self.to_params().to_request(id)
    }
}

/// The params of an `initialize` request.
///
/// Unknown members are ignored, so that a minor protocol version can add
/// members.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    versions: SupportedVersions,
    /// A missing member is an empty set.
    #[serde(default)]
    capabilities: Capabilities,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    client_info: Option<PeerInfo>,
}

impl InitializeParams {
    /// Params offering `versions`, with no capabilities and no `clientInfo`.
    pub fn new(versions: SupportedVersions) -> Self {
        Self {
            versions,
            capabilities: Capabilities::new(),
            client_info: None,
        }
    }

    /// Sets the capabilities.
    pub fn with_capabilities(mut self, capabilities: Capabilities) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Sets `clientInfo`.
    pub fn with_client_info(mut self, info: PeerInfo) -> Self {
        self.client_info = Some(info);
        self
    }

    /// The versions the client supports.
    pub fn versions(&self) -> &SupportedVersions {
        &self.versions
    }

    /// The capabilities the client supports.
    pub fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    /// The client's name and version, if sent.
    pub fn client_info(&self) -> Option<&PeerInfo> {
        self.client_info.as_ref()
    }

    /// An `initialize` request with these params.
    pub fn to_request(&self, id: Id) -> Request {
        let params = match serde_json::to_value(self) {
            Ok(serde_json::Value::Object(members)) => Some(Params::Object(members)),
            // InitializeParams always serializes to an object.
            _ => None,
        };
        Request::new(id, METHOD_INITIALIZE, params)
    }
}

/// The outcome of a successful handshake: the result of `initialize`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    version: ProtocolVersion,
    /// A missing member is an empty set.
    #[serde(default)]
    capabilities: Capabilities,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    server_info: Option<PeerInfo>,
}

impl InitializeResult {
    /// A result with no `serverInfo`.
    pub fn new(version: ProtocolVersion, capabilities: Capabilities) -> Self {
        Self {
            version,
            capabilities,
            server_info: None,
        }
    }

    /// Sets `serverInfo`.
    pub fn with_server_info(mut self, info: PeerInfo) -> Self {
        self.server_info = Some(info);
        self
    }

    /// The protocol version the session uses.
    pub fn version(&self) -> ProtocolVersion {
        self.version
    }

    /// The capabilities both peers announced, with the server's parameters.
    pub fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    /// The server's name and version, if sent.
    pub fn server_info(&self) -> Option<&PeerInfo> {
        self.server_info.as_ref()
    }
}

/// The peers support no common protocol version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncompatibleVersion {
    local: SupportedVersions,
    remote: SupportedVersions,
}

impl IncompatibleVersion {
    /// The versions this side supports.
    pub fn local(&self) -> &SupportedVersions {
        &self.local
    }

    /// The versions the other side offered.
    pub fn remote(&self) -> &SupportedVersions {
        &self.remote
    }

    /// The error object a server sends for this failure.
    pub fn to_error_object(&self) -> ErrorObject {
        ErrorObject::new(codes::INCOMPATIBLE_VERSION, self.to_string())
            .with_data(serde_json::json!({ "supported": self.local }))
    }
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

/// The server side of negotiation: the version from
/// [`SupportedVersions::highest_common`], the capabilities in both sets with
/// the server's parameters, and the server's info.
///
/// The version and the capability names do not depend on which peer is the
/// server; the capability parameters do.
pub fn negotiate(
    server: &Supported,
    client: &InitializeParams,
) -> Result<InitializeResult, IncompatibleVersion> {
    let version = server
        .versions
        .highest_common(&client.versions)
        .ok_or_else(|| IncompatibleVersion {
            local: server.versions.clone(),
            remote: client.versions.clone(),
        })?;
    Ok(InitializeResult {
        version,
        capabilities: server.capabilities.intersection(&client.capabilities),
        server_info: server.info.clone(),
    })
}

/// The reference server side of the handshake: answers `request` for a server
/// that supports `server`.
///
/// - another method: error [`METHOD_NOT_FOUND`](crate::jsonrpc::codes::METHOD_NOT_FOUND);
/// - params that are not [`InitializeParams`]: error
///   [`INVALID_PARAMS`](crate::jsonrpc::codes::INVALID_PARAMS);
/// - no common version: error
///   [`INCOMPATIBLE_VERSION`](crate::jsonrpc::codes::INCOMPATIBLE_VERSION);
/// - otherwise: the [`InitializeResult`] from [`negotiate`].
///
/// The response always carries the request id.
pub fn answer_initialize(server: &Supported, request: &Request) -> Response {
    let id = Some(request.id().clone());
    if request.method() != METHOD_INITIALIZE {
        return Response::error(
            id,
            ErrorObject::new(
                codes::METHOD_NOT_FOUND,
                format!("method not found: {}", request.method()),
            ),
        );
    }
    let client = match request.params() {
        Some(Params::Object(members)) => {
            serde_json::from_value::<InitializeParams>(serde_json::Value::Object(members.clone()))
                .map_err(|err| err.to_string())
        }
        Some(_) => Err("params must be an object".to_owned()),
        None => Err("params are required".to_owned()),
    };
    let client = match client {
        Ok(client) => client,
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
    match negotiate(server, &client) {
        Ok(result) => match serde_json::to_value(result) {
            Ok(value) => Response::success(request.id().clone(), value),
            Err(err) => {
                Response::error(id, ErrorObject::new(codes::INTERNAL_ERROR, err.to_string()))
            }
        },
        Err(incompatible) => Response::error(id, incompatible.to_error_object()),
    }
}
