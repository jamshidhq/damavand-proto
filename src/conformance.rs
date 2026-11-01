//! Reusable conformance test suite (feature `conformance`).
//!
//! An implementation of the protocol runs this suite against itself. The suite
//! talks to the implementation in JSON text, so it checks the wire format and
//! not only the Rust types. Each case returns a pass or a failure message;
//! [`Report::assert_passed`] panics with every failing case name.
//!
//! ```
//! use damavand_proto::conformance::{ReferenceServer, run_handshake};
//! use damavand_proto::{CapabilitySet, InitializeParams, VersionRange};
//! use damavand_proto::version::CURRENT;
//!
//! let mut server = ReferenceServer::new(InitializeParams {
//!     versions: VersionRange::single(CURRENT),
//!     capabilities: CapabilitySet::from_iter(["tools.call"]),
//! });
//! run_handshake(&mut server).assert_passed();
//! ```

use serde_json::{Value, json};

use crate::capability::CapabilitySet;
use crate::handshake::{InitializeParams, InitializeResult, METHOD_INITIALIZE, answer_initialize};
use crate::jsonrpc::{ErrorObject, Id, Message, codes};
use crate::version::{ProtocolVersion, VersionRange};

/// A capability name no implementation announces. The suite offers it to check
/// that unknown capabilities are ignored.
pub const UNKNOWN_CAPABILITY: &str = "x-damavand-conformance-unknown";

/// The server side of the handshake, as the suite sees it.
pub trait HandshakeServer {
    /// The versions and capabilities the server supports.
    fn supported(&self) -> InitializeParams;

    /// Answers one message, given and returned as JSON text.
    fn handle(&mut self, message: &str) -> String;
}

/// The outcome of one conformance case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseOutcome {
    /// The case name.
    pub name: &'static str,
    /// `Ok` when the case passed, else what went wrong.
    pub result: Result<(), String>,
}

/// The outcomes of a suite run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// One outcome per case, in run order.
    pub cases: Vec<CaseOutcome>,
}

impl Report {
    /// The failing cases.
    pub fn failures(&self) -> impl Iterator<Item = &CaseOutcome> {
        self.cases.iter().filter(|case| case.result.is_err())
    }

    /// Panics when a case failed, naming each failing case and its message.
    pub fn assert_passed(&self) {
        let failures: Vec<String> = self
            .failures()
            .map(|case| {
                let reason = case.result.as_ref().err().map_or("", String::as_str);
                format!("{}: {reason}", case.name)
            })
            .collect();
        assert!(
            failures.is_empty(),
            "{} of {} conformance cases failed:\n{}",
            failures.len(),
            self.cases.len(),
            failures.join("\n")
        );
    }
}

/// [`answer_initialize`] over JSON text:
/// the reference implementation the suite is checked against.
#[derive(Debug, Clone)]
pub struct ReferenceServer {
    supported: InitializeParams,
}

impl ReferenceServer {
    /// A reference server that supports `supported`.
    pub fn new(supported: InitializeParams) -> Self {
        Self { supported }
    }
}

impl HandshakeServer for ReferenceServer {
    fn supported(&self) -> InitializeParams {
        self.supported.clone()
    }

    fn handle(&mut self, message: &str) -> String {
        let response = match Message::from_json_str(message) {
            Ok(Message::Request(request)) => answer_initialize(&self.supported, &request),
            Ok(other) => crate::jsonrpc::Response::error(
                None,
                ErrorObject::new(
                    codes::INVALID_REQUEST,
                    format!("expected an initialize request, got {other:?}"),
                ),
            ),
            Err(err) => err.to_response(),
        };
        Message::Response(response).to_json_string()
    }
}

type CaseFn = fn(&mut dyn HandshakeServer, &InitializeParams) -> Result<(), String>;

const CASES: [(&str, CaseFn); 8] = [
    ("picks_highest_common_version", picks_highest_common_version),
    (
        "uses_only_overlapping_version",
        uses_only_overlapping_version,
    ),
    ("disjoint_ranges_rejected", disjoint_ranges_rejected),
    ("unknown_capabilities_ignored", unknown_capabilities_ignored),
    (
        "capabilities_are_intersection",
        capabilities_are_intersection,
    ),
    ("invalid_params_rejected", invalid_params_rejected),
    ("invalid_envelope_rejected", invalid_envelope_rejected),
    ("parse_error_rejected", parse_error_rejected),
];

/// The version one step above `version`, or `None` at the top.
fn next(version: ProtocolVersion) -> Option<ProtocolVersion> {
    match version.minor.checked_add(1) {
        Some(minor) => Some(ProtocolVersion::new(version.major, minor)),
        None => version
            .major
            .checked_add(1)
            .map(|major| ProtocolVersion::new(major, 0)),
    }
}

/// Sends `text`, and checks the answer is a response with id `expect_id`.
fn exchange(
    server: &mut dyn HandshakeServer,
    text: &str,
    expect_id: Option<&Id>,
) -> Result<Result<Value, ErrorObject>, String> {
    let answer = server.handle(text);
    let message = Message::from_json_str(&answer)
        .map_err(|err| format!("answer is not a JSON-RPC message ({err}): {answer}"))?;
    let Message::Response(response) = message else {
        return Err(format!("answer is not a response: {answer}"));
    };
    if response.id() != expect_id {
        return Err(format!(
            "response id is {:?}, expected {expect_id:?}",
            response.id()
        ));
    }
    Ok(response.into_outcome())
}

/// Sends an initialize request offering `offer`.
fn initialize(
    server: &mut dyn HandshakeServer,
    case: &str,
    offer: &InitializeParams,
) -> Result<Result<Value, ErrorObject>, String> {
    let id = Id::String(format!("conformance-{case}"));
    let request = Message::Request(offer.to_request(id.clone()));
    exchange(server, &request.to_json_string(), Some(&id))
}

fn expect_result(outcome: Result<Value, ErrorObject>) -> Result<InitializeResult, String> {
    let value = outcome.map_err(|err| format!("expected a result, got error {err:?}"))?;
    serde_json::from_value(value.clone())
        .map_err(|err| format!("result is not an InitializeResult ({err}): {value}"))
}

fn expect_error(outcome: Result<Value, ErrorObject>, code: i64) -> Result<ErrorObject, String> {
    match outcome {
        Ok(value) => Err(format!("expected error {code}, got result {value}")),
        Err(err) if err.code == code => Ok(err),
        Err(err) => Err(format!("expected error {code}, got {err:?}")),
    }
}

fn expect_eq<T: PartialEq + std::fmt::Debug>(what: &str, got: T, want: T) -> Result<(), String> {
    if got == want {
        Ok(())
    } else {
        Err(format!("{what} is {got:?}, expected {want:?}"))
    }
}

fn picks_highest_common_version(
    server: &mut dyn HandshakeServer,
    supported: &InitializeParams,
) -> Result<(), String> {
    // Offer the server's whole range and more above it, if there is room.
    let top = next(supported.versions.max()).unwrap_or(supported.versions.max());
    let offer = InitializeParams {
        versions: VersionRange::new(supported.versions.min(), top).map_err(|e| e.to_string())?,
        capabilities: supported.capabilities.clone(),
    };
    let result = expect_result(initialize(server, "highest", &offer)?)?;
    expect_eq("version", result.version, supported.versions.max())
}

fn uses_only_overlapping_version(
    server: &mut dyn HandshakeServer,
    supported: &InitializeParams,
) -> Result<(), String> {
    let low = supported.versions.min();
    let offer = InitializeParams {
        versions: VersionRange::single(low),
        capabilities: CapabilitySet::new(),
    };
    let result = expect_result(initialize(server, "overlap", &offer)?)?;
    expect_eq("version", result.version, low)
}

fn disjoint_ranges_rejected(
    server: &mut dyn HandshakeServer,
    supported: &InitializeParams,
) -> Result<(), String> {
    let Some(above) = next(supported.versions.max()) else {
        // The server supports the highest possible version: no range above it.
        return Ok(());
    };
    let offer = InitializeParams {
        versions: VersionRange::single(above),
        capabilities: supported.capabilities.clone(),
    };
    let err = expect_error(
        initialize(server, "disjoint", &offer)?,
        codes::INCOMPATIBLE_VERSION,
    )?;
    let want = json!({ "supported": supported.versions });
    expect_eq("error data", err.data, Some(want))
}

fn unknown_capabilities_ignored(
    server: &mut dyn HandshakeServer,
    supported: &InitializeParams,
) -> Result<(), String> {
    let mut capabilities = supported.capabilities.clone();
    capabilities.insert(UNKNOWN_CAPABILITY);
    let offer = InitializeParams {
        versions: supported.versions,
        capabilities,
    };
    let result = expect_result(initialize(server, "unknown-cap", &offer)?)?;
    if result.capabilities.contains(UNKNOWN_CAPABILITY) {
        return Err(format!("result announces {UNKNOWN_CAPABILITY}"));
    }
    expect_eq(
        "capabilities",
        result.capabilities,
        supported.capabilities.clone(),
    )
}

fn capabilities_are_intersection(
    server: &mut dyn HandshakeServer,
    supported: &InitializeParams,
) -> Result<(), String> {
    // Offer every second server capability.
    let offered: CapabilitySet = supported.capabilities.iter().step_by(2).collect();
    let offer = InitializeParams {
        versions: supported.versions,
        capabilities: offered.clone(),
    };
    let result = expect_result(initialize(server, "intersection", &offer)?)?;
    expect_eq("capabilities", result.capabilities, offered)
}

fn invalid_params_rejected(
    server: &mut dyn HandshakeServer,
    _: &InitializeParams,
) -> Result<(), String> {
    let id = Id::String("conformance-invalid-params".into());
    let text = json!({
        "jsonrpc": "2.0",
        "id": "conformance-invalid-params",
        "method": METHOD_INITIALIZE,
        "params": { "versions": "not-a-range" },
    })
    .to_string();
    expect_error(exchange(server, &text, Some(&id))?, codes::INVALID_PARAMS).map(drop)
}

fn invalid_envelope_rejected(
    server: &mut dyn HandshakeServer,
    supported: &InitializeParams,
) -> Result<(), String> {
    // Wrong `jsonrpc` version. The id is readable, but a server may also
    // answer with a null id, so both are accepted.
    let mut value = Message::Request(supported.to_request(Id::Number(1))).to_value();
    value["jsonrpc"] = json!("1.0");
    let answer = server.handle(&value.to_string());
    let message = Message::from_json_str(&answer)
        .map_err(|err| format!("answer is not a JSON-RPC message ({err}): {answer}"))?;
    let Message::Response(response) = message else {
        return Err(format!("answer is not a response: {answer}"));
    };
    if !matches!(response.id(), None | Some(Id::Number(1))) {
        return Err(format!(
            "response id is {:?}, expected null or 1",
            response.id()
        ));
    }
    expect_error(response.into_outcome(), codes::INVALID_REQUEST).map(drop)
}

fn parse_error_rejected(
    server: &mut dyn HandshakeServer,
    _: &InitializeParams,
) -> Result<(), String> {
    expect_error(
        exchange(server, "{\"jsonrpc\": \"2.0\", \"id\": 1, ", None)?,
        codes::PARSE_ERROR,
    )
    .map(drop)
}

/// Runs the handshake conformance cases against `server`.
pub fn run_handshake<S: HandshakeServer>(server: &mut S) -> Report {
    let supported = server.supported();
    let cases = CASES
        .iter()
        .map(|(name, case)| CaseOutcome {
            name,
            result: case(server, &supported),
        })
        .collect();
    Report { cases }
}
