//! Reusable conformance test suite (feature `conformance`).
//!
//! An implementation of the protocol runs this suite against itself. The suite
//! talks to the implementation in JSON text, so it checks the wire format and
//! not only the Rust types. Each case returns a pass or a failure message;
//! [`Report::assert_passed`] panics with every failing case name.
//!
//! ```
//! use damavand_proto::conformance::{ReferenceServer, run_handshake};
//! use damavand_proto::handshake::{PeerInfo, Supported};
//! use damavand_proto::version::{CURRENT, SupportedVersions};
//! use damavand_proto::Capabilities;
//!
//! let mut capabilities = Capabilities::new();
//! capabilities.insert("tools", serde_json::Map::new()).unwrap();
//! let mut server = ReferenceServer::new(
//!     Supported::new(SupportedVersions::single(CURRENT))
//!         .with_capabilities(capabilities)
//!         .with_info(PeerInfo::new("my-server", "0.1.0")),
//! );
//! run_handshake(&mut server).assert_passed();
//! ```

use serde_json::{Map, Value, json};

use crate::handshake::{InitializeResult, METHOD_INITIALIZE, Supported, answer_initialize};
use crate::jsonrpc::{ErrorObject, Id, Message, codes};
use crate::version::ProtocolVersion;

/// A capability name no implementation announces. The suite offers it to check
/// that unknown capabilities are ignored.
pub const UNKNOWN_CAPABILITY: &str = "x-damavand-conformance-unknown";

/// A method name no implementation defines.
pub const UNKNOWN_METHOD: &str = "x-damavand-conformance/unknown";

/// The server side of the handshake, as the suite sees it.
pub trait HandshakeServer {
    /// What the server supports.
    fn supported(&self) -> Supported;

    /// Handles one message, given as JSON text. Returns the reply as JSON
    /// text, or `None` when the protocol forbids a reply: to a notification
    /// and to a response.
    fn handle(&mut self, message: &str) -> Option<String>;
}

/// The outcome of one conformance case.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CaseOutcome {
    /// The case name.
    pub name: &'static str,
    /// `Ok` when the case passed, else what went wrong.
    pub result: Result<(), String>,
}

/// The outcomes of a suite run.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
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

/// [`answer_initialize`] over JSON text, with the reply rules of the
/// protocol: the reference implementation the suite is checked against.
#[derive(Debug, Clone)]
pub struct ReferenceServer {
    supported: Supported,
}

impl ReferenceServer {
    /// A reference server that supports `supported`.
    pub fn new(supported: Supported) -> Self {
        Self { supported }
    }
}

impl HandshakeServer for ReferenceServer {
    fn supported(&self) -> Supported {
        self.supported.clone()
    }

    fn handle(&mut self, message: &str) -> Option<String> {
        let response = match Message::from_json_str(message) {
            Ok(Message::Request(request)) => answer_initialize(&self.supported, &request),
            // No reply to a notification or a response.
            Ok(_) => return None,
            Err(err) => err.to_response()?,
        };
        Some(Message::Response(response).to_json_string())
    }
}

type CaseFn = fn(&mut dyn HandshakeServer, &Supported) -> Result<(), String>;

const CASES: [(&str, CaseFn); 22] = [
    ("picks_highest_common_major", picks_highest_common_major),
    ("minor_is_minimum_of_both", minor_is_minimum_of_both),
    ("disjoint_above_rejected", disjoint_above_rejected),
    ("disjoint_below_rejected", disjoint_below_rejected),
    ("unknown_capabilities_ignored", unknown_capabilities_ignored),
    (
        "capabilities_are_intersection",
        capabilities_are_intersection,
    ),
    (
        "absent_capabilities_mean_empty",
        absent_capabilities_mean_empty,
    ),
    ("server_info_reported", server_info_reported),
    ("invalid_params_rejected", invalid_params_rejected),
    ("malformed_version_rejected", malformed_version_rejected),
    ("duplicate_major_rejected", duplicate_major_rejected),
    (
        "invalid_capability_name_rejected",
        invalid_capability_name_rejected,
    ),
    ("duplicate_member_rejected", duplicate_member_rejected),
    ("unknown_method_not_found", unknown_method_not_found),
    ("reserved_method_rejected", reserved_method_rejected),
    ("invalid_envelope_rejected", invalid_envelope_rejected),
    ("null_request_id_rejected", null_request_id_rejected),
    (
        "fractional_request_id_rejected",
        fractional_request_id_rejected,
    ),
    ("batch_rejected", batch_rejected),
    ("parse_error_rejected", parse_error_rejected),
    ("notification_gets_no_reply", notification_gets_no_reply),
    ("response_gets_no_reply", response_gets_no_reply),
];

type Outcome = Result<Value, ErrorObject>;

/// A version in a compatibility line above `version`'s, if there is room.
fn line_above(version: ProtocolVersion) -> Option<ProtocolVersion> {
    if version.major() == 0 {
        version
            .minor()
            .checked_add(1)
            .map(|minor| ProtocolVersion::new(0, minor))
    } else {
        version
            .major()
            .checked_add(1)
            .map(|major| ProtocolVersion::new(major, 0))
    }
}

/// A version in a compatibility line below `version`'s, if there is one.
fn line_below(version: ProtocolVersion) -> Option<ProtocolVersion> {
    match (version.major(), version.minor()) {
        (0, 0) => None,
        (0, minor) => Some(ProtocolVersion::new(0, minor - 1)),
        (major, _) => Some(ProtocolVersion::new(major - 1, 0)),
    }
}

fn list(versions: &[ProtocolVersion]) -> Value {
    Value::Array(
        versions
            .iter()
            .map(|version| Value::String(version.to_string()))
            .collect(),
    )
}

/// Sends `text`, and checks the answer is a response with id `expect_id`.
fn exchange(
    server: &mut dyn HandshakeServer,
    text: &str,
    expect_id: Option<&Id>,
) -> Result<Outcome, String> {
    let answer = server
        .handle(text)
        .ok_or_else(|| format!("no reply to a message that needs one: {text}"))?;
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

/// Sends an initialize request with `params`, and checks the response id.
fn initialize(
    server: &mut dyn HandshakeServer,
    case: &str,
    params: Value,
) -> Result<Outcome, String> {
    let id = Id::String(format!("conformance-{case}"));
    let text = json!({
        "jsonrpc": "2.0",
        "id": format!("conformance-{case}"),
        "method": METHOD_INITIALIZE,
        "params": params,
    })
    .to_string();
    exchange(server, &text, Some(&id))
}

/// The params offering `versions` and the `capabilities` object.
fn offer(versions: &[ProtocolVersion], capabilities: Value) -> Value {
    json!({ "versions": list(versions), "capabilities": capabilities })
}

fn caps_value(supported: &Supported) -> Result<Value, String> {
    serde_json::to_value(supported.capabilities()).map_err(|err| err.to_string())
}

fn expect_result(outcome: Outcome) -> Result<InitializeResult, String> {
    let value = outcome.map_err(|err| format!("expected a result, got error {err:?}"))?;
    serde_json::from_value(value.clone())
        .map_err(|err| format!("result is not an InitializeResult ({err}): {value}"))
}

fn expect_error(outcome: Outcome, code: i64) -> Result<ErrorObject, String> {
    match outcome {
        Ok(value) => Err(format!("expected error {code}, got result {value}")),
        Err(err) if err.code() == code => Ok(err),
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

fn versions_of(supported: &Supported) -> Vec<ProtocolVersion> {
    supported.versions().iter().collect()
}

fn picks_highest_common_major(
    server: &mut dyn HandshakeServer,
    supported: &Supported,
) -> Result<(), String> {
    // Offer every server line, and a higher line if there is room: the
    // highest common line is the server's highest.
    let mut versions = versions_of(supported);
    versions.extend(line_above(supported.versions().highest()));
    let result = expect_result(initialize(
        server,
        "highest",
        offer(&versions, caps_value(supported)?),
    )?)?;
    expect_eq("version", result.version(), supported.versions().highest())
}

fn minor_is_minimum_of_both(
    server: &mut dyn HandshakeServer,
    supported: &Supported,
) -> Result<(), String> {
    for mine in supported.versions().iter() {
        let mut probes = vec![(mine, mine)];
        if mine.major() > 0 {
            // A lower client minor wins; a higher client minor loses.
            if let Some(lower) = mine.minor().checked_sub(1) {
                let lower = ProtocolVersion::new(mine.major(), lower);
                probes.push((lower, lower));
            }
            if let Some(higher) = mine.minor().checked_add(1) {
                probes.push((ProtocolVersion::new(mine.major(), higher), mine));
            }
        }
        for (sent, want) in probes {
            let result = expect_result(initialize(server, "minor", offer(&[sent], json!({})))?)?;
            expect_eq(&format!("version for offer {sent}"), result.version(), want)?;
        }
    }
    Ok(())
}

fn expect_incompatible(
    server: &mut dyn HandshakeServer,
    supported: &Supported,
    case: &str,
    sent: ProtocolVersion,
) -> Result<(), String> {
    let err = expect_error(
        initialize(server, case, offer(&[sent], caps_value(supported)?))?,
        codes::INCOMPATIBLE_VERSION,
    )?;
    let want = json!({ "supported": supported.versions() });
    expect_eq("error data", err.data(), Some(&want))
}

fn disjoint_above_rejected(
    server: &mut dyn HandshakeServer,
    supported: &Supported,
) -> Result<(), String> {
    match line_above(supported.versions().highest()) {
        Some(above) => expect_incompatible(server, supported, "above", above),
        // The server supports the highest possible line: none above it.
        None => Ok(()),
    }
}

fn disjoint_below_rejected(
    server: &mut dyn HandshakeServer,
    supported: &Supported,
) -> Result<(), String> {
    let lowest = supported
        .versions()
        .iter()
        .next()
        .unwrap_or(supported.versions().highest());
    match line_below(lowest) {
        Some(below) => expect_incompatible(server, supported, "below", below),
        // The server supports 0.0: no line below it.
        None => Ok(()),
    }
}

fn unknown_capabilities_ignored(
    server: &mut dyn HandshakeServer,
    supported: &Supported,
) -> Result<(), String> {
    let mut capabilities = caps_value(supported)?;
    capabilities[UNKNOWN_CAPABILITY] = json!({});
    let result = expect_result(initialize(
        server,
        "unknown-cap",
        offer(&versions_of(supported), capabilities),
    )?)?;
    if result.capabilities().contains(UNKNOWN_CAPABILITY) {
        return Err(format!("result announces {UNKNOWN_CAPABILITY}"));
    }
    expect_eq(
        "capabilities",
        result.capabilities(),
        supported.capabilities(),
    )
}

fn capabilities_are_intersection(
    server: &mut dyn HandshakeServer,
    supported: &Supported,
) -> Result<(), String> {
    // Offer every second server capability, with parameters of the client's
    // own: the result holds those names with the server's parameters.
    let mut offered = Map::new();
    let mut want = crate::Capabilities::new();
    for (name, params) in supported.capabilities().iter().step_by(2) {
        offered.insert(name.to_owned(), json!({ "conformanceProbe": true }));
        want.insert(name, params.clone())
            .map_err(|err| err.to_string())?;
    }
    let result = expect_result(initialize(
        server,
        "intersection",
        offer(&versions_of(supported), Value::Object(offered)),
    )?)?;
    expect_eq("capabilities", result.capabilities(), &want)
}

fn absent_capabilities_mean_empty(
    server: &mut dyn HandshakeServer,
    supported: &Supported,
) -> Result<(), String> {
    let params = json!({ "versions": list(&versions_of(supported)) });
    let result = expect_result(initialize(server, "no-caps", params)?)?;
    expect_eq(
        "capabilities",
        result.capabilities(),
        &crate::Capabilities::new(),
    )
}

fn server_info_reported(
    server: &mut dyn HandshakeServer,
    supported: &Supported,
) -> Result<(), String> {
    let params = serde_json::to_value(supported.to_params()).map_err(|err| err.to_string())?;
    let result = expect_result(initialize(server, "server-info", params)?)?;
    expect_eq("serverInfo", result.server_info(), supported.info())
}

fn expect_invalid_params(
    server: &mut dyn HandshakeServer,
    case: &str,
    params: Value,
) -> Result<(), String> {
    expect_error(initialize(server, case, params)?, codes::INVALID_PARAMS).map(drop)
}

fn invalid_params_rejected(server: &mut dyn HandshakeServer, _: &Supported) -> Result<(), String> {
    expect_invalid_params(
        server,
        "invalid-params",
        json!({ "versions": "not-a-list" }),
    )?;
    expect_invalid_params(server, "invalid-params", json!({ "versions": [] }))
}

fn malformed_version_rejected(
    server: &mut dyn HandshakeServer,
    supported: &Supported,
) -> Result<(), String> {
    // A leading zero: a lax reader would see the server's highest version.
    let text = format!("0{}", supported.versions().highest());
    expect_invalid_params(server, "leading-zero", json!({ "versions": [text] }))
}

fn duplicate_major_rejected(
    server: &mut dyn HandshakeServer,
    supported: &Supported,
) -> Result<(), String> {
    let top = supported.versions().highest();
    let other = match (top.major(), top.minor()) {
        (0, _) => top,
        (major, 0) => ProtocolVersion::new(major, 1),
        (major, _) => ProtocolVersion::new(major, 0),
    };
    expect_invalid_params(
        server,
        "duplicate-major",
        json!({ "versions": list(&[top, other]) }),
    )
}

fn invalid_capability_name_rejected(
    server: &mut dyn HandshakeServer,
    supported: &Supported,
) -> Result<(), String> {
    let versions = versions_of(supported);
    for capabilities in [
        json!({ "Tools": {} }),
        json!({ "": {} }),
        json!({ "tools": true }),
        json!(["tools"]),
    ] {
        expect_invalid_params(server, "bad-capability", offer(&versions, capabilities))?;
    }
    Ok(())
}

fn duplicate_member_rejected(
    server: &mut dyn HandshakeServer,
    supported: &Supported,
) -> Result<(), String> {
    // serde_json::Value would keep only the last "tools"; the protocol refuses
    // the text.
    let text = format!(
        r#"{{"jsonrpc":"2.0","id":"conformance-duplicate","method":"{METHOD_INITIALIZE}","params":{{"versions":{},"capabilities":{{"tools":{{}},"tools":{{}}}}}}}}"#,
        list(&versions_of(supported))
    );
    expect_error(exchange(server, &text, None)?, codes::PARSE_ERROR).map(drop)
}

fn unknown_method_not_found(server: &mut dyn HandshakeServer, _: &Supported) -> Result<(), String> {
    let id = Id::String("conformance-unknown-method".into());
    let text = json!({
        "jsonrpc": "2.0",
        "id": "conformance-unknown-method",
        "method": UNKNOWN_METHOD,
    })
    .to_string();
    expect_error(exchange(server, &text, Some(&id))?, codes::METHOD_NOT_FOUND).map(drop)
}

fn reserved_method_rejected(server: &mut dyn HandshakeServer, _: &Supported) -> Result<(), String> {
    let text = json!({ "jsonrpc": "2.0", "id": 7, "method": "rpc.discover" }).to_string();
    expect_error(
        exchange(server, &text, Some(&Id::Number(7)))?,
        codes::INVALID_REQUEST,
    )
    .map(drop)
}

fn invalid_envelope_rejected(
    server: &mut dyn HandshakeServer,
    supported: &Supported,
) -> Result<(), String> {
    // Wrong `jsonrpc` version. The id is readable, so the reply carries it.
    let mut value = Message::Request(supported.to_request(Id::Number(1))).to_value();
    value["jsonrpc"] = json!("1.0");
    expect_error(
        exchange(server, &value.to_string(), Some(&Id::Number(1)))?,
        codes::INVALID_REQUEST,
    )
    .map(drop)
}

fn expect_unreadable_id(
    server: &mut dyn HandshakeServer,
    supported: &Supported,
    id: Value,
) -> Result<(), String> {
    let mut value = Message::Request(supported.to_request(Id::Number(1))).to_value();
    value["id"] = id;
    expect_error(
        exchange(server, &value.to_string(), None)?,
        codes::INVALID_REQUEST,
    )
    .map(drop)
}

fn null_request_id_rejected(
    server: &mut dyn HandshakeServer,
    supported: &Supported,
) -> Result<(), String> {
    expect_unreadable_id(server, supported, Value::Null)
}

fn fractional_request_id_rejected(
    server: &mut dyn HandshakeServer,
    supported: &Supported,
) -> Result<(), String> {
    expect_unreadable_id(server, supported, json!(1.5))
}

fn batch_rejected(server: &mut dyn HandshakeServer, supported: &Supported) -> Result<(), String> {
    // One error response with a null id, not an array of responses.
    let request = Message::Request(supported.to_request(Id::Number(1))).to_value();
    let text = Value::Array(vec![request]).to_string();
    expect_error(exchange(server, &text, None)?, codes::INVALID_REQUEST).map(drop)
}

fn parse_error_rejected(server: &mut dyn HandshakeServer, _: &Supported) -> Result<(), String> {
    expect_error(
        exchange(server, "{\"jsonrpc\": \"2.0\", \"id\": 1, ", None)?,
        codes::PARSE_ERROR,
    )
    .map(drop)
}

fn expect_no_reply(server: &mut dyn HandshakeServer, messages: &[Value]) -> Result<(), String> {
    for message in messages {
        let text = message.to_string();
        if let Some(answer) = server.handle(&text) {
            return Err(format!("replied to {text} with {answer}"));
        }
    }
    Ok(())
}

fn notification_gets_no_reply(
    server: &mut dyn HandshakeServer,
    supported: &Supported,
) -> Result<(), String> {
    let params = serde_json::to_value(supported.to_params()).map_err(|err| err.to_string())?;
    expect_no_reply(
        server,
        &[
            json!({ "jsonrpc": "2.0", "method": METHOD_INITIALIZE, "params": params }),
            json!({ "jsonrpc": "2.0", "method": METHOD_INITIALIZE, "params": [] }),
            json!({ "jsonrpc": "2.0", "method": UNKNOWN_METHOD }),
            json!({ "jsonrpc": "2.0", "method": "rpc.cancel" }),
            json!({ "jsonrpc": "2.0", "method": "rpc.cancel", "params": 1 }),
        ],
    )
}

fn response_gets_no_reply(server: &mut dyn HandshakeServer, _: &Supported) -> Result<(), String> {
    expect_no_reply(
        server,
        &[
            json!({ "jsonrpc": "2.0", "id": 1, "result": {} }),
            json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32600, "message": "x" } }),
            json!({ "jsonrpc": "2.0", "id": 2, "result": 1, "error": { "code": 1, "message": "x" } }),
        ],
    )
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
