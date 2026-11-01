//! The conformance suite passes the reference server and catches broken ones.
#![cfg(feature = "conformance")]

use damavand_proto::conformance::{HandshakeServer, ReferenceServer, run_handshake};
use damavand_proto::handshake::answer_initialize;
use damavand_proto::version::CURRENT;
use damavand_proto::{
    CapabilitySet, Id, InitializeParams, InitializeResult, Message, ProtocolVersion, Response,
    VersionRange,
};
use serde_json::{Value, json};

const CASES: [&str; 8] = [
    "picks_highest_common_version",
    "uses_only_overlapping_version",
    "disjoint_ranges_rejected",
    "unknown_capabilities_ignored",
    "capabilities_are_intersection",
    "invalid_params_rejected",
    "invalid_envelope_rejected",
    "parse_error_rejected",
];

fn supported() -> InitializeParams {
    InitializeParams {
        versions: VersionRange::new(ProtocolVersion::new(0, 1), ProtocolVersion::new(0, 4))
            .unwrap(),
        capabilities: CapabilitySet::from_iter(["tools.call", "audit.write"]),
    }
}

fn failing(report: &damavand_proto::conformance::Report) -> Vec<&'static str> {
    report.failures().map(|case| case.name).collect()
}

#[test]
fn reference_server_passes_every_case() {
    for params in [
        supported(),
        InitializeParams {
            versions: VersionRange::single(CURRENT),
            capabilities: CapabilitySet::new(),
        },
    ] {
        let report = run_handshake(&mut ReferenceServer::new(params));
        let names: Vec<_> = report.cases.iter().map(|case| case.name).collect();
        assert_eq!(names, CASES);
        assert_eq!(failing(&report), Vec::<&str>::new(), "{report:?}");
        report.assert_passed();
    }
}

/// Wraps the reference server and rewrites successful results.
struct Tamper<F: FnMut(&InitializeParams, InitializeResult) -> InitializeResult> {
    inner: ReferenceServer,
    rewrite: F,
}

impl<F: FnMut(&InitializeParams, InitializeResult) -> InitializeResult> HandshakeServer
    for Tamper<F>
{
    fn supported(&self) -> InitializeParams {
        self.inner.supported()
    }

    fn handle(&mut self, message: &str) -> String {
        let answer = self.inner.handle(message);
        let Ok(Message::Response(resp)) = Message::from_json_str(&answer) else {
            return answer;
        };
        let Some(id) = resp.id().cloned() else {
            return answer;
        };
        let Ok(result) = resp.into_outcome() else {
            return answer;
        };
        let Ok(Message::Request(req)) = Message::from_json_str(message) else {
            return answer;
        };
        let client: InitializeParams = serde_json::to_value(req.params.map(|p| match p {
            damavand_proto::jsonrpc::Params::Object(m) => Value::Object(m),
            damavand_proto::jsonrpc::Params::Array(a) => Value::Array(a),
        }))
        .and_then(serde_json::from_value)
        .unwrap();
        let result = (self.rewrite)(&client, serde_json::from_value(result).unwrap());
        Message::Response(Response::success(id, serde_json::to_value(result).unwrap()))
            .to_json_string()
    }
}

#[test]
fn server_picking_lowest_version_fails() {
    let mut server = Tamper {
        inner: ReferenceServer::new(supported()),
        rewrite: |client: &InitializeParams, mut r: InitializeResult| {
            r.version = client.versions.min().max(supported().versions.min());
            r
        },
    };
    let report = run_handshake(&mut server);
    assert!(
        failing(&report).contains(&"picks_highest_common_version"),
        "{report:?}"
    );
}

#[test]
fn server_echoing_client_capabilities_fails() {
    let mut server = Tamper {
        inner: ReferenceServer::new(supported()),
        rewrite: |client: &InitializeParams, mut r: InitializeResult| {
            r.capabilities = client.capabilities.clone();
            r
        },
    };
    let report = run_handshake(&mut server);
    let failed = failing(&report);
    assert!(
        failed.contains(&"unknown_capabilities_ignored"),
        "{report:?}"
    );
}

#[test]
fn server_dropping_capabilities_fails() {
    let mut server = Tamper {
        inner: ReferenceServer::new(supported()),
        rewrite: |_: &InitializeParams, mut r: InitializeResult| {
            r.capabilities = CapabilitySet::new();
            r
        },
    };
    let report = run_handshake(&mut server);
    assert!(
        failing(&report).contains(&"capabilities_are_intersection"),
        "{report:?}"
    );
}

/// Accepts any version range: answers with its own maximum.
struct AcceptsAnything(InitializeParams);

impl HandshakeServer for AcceptsAnything {
    fn supported(&self) -> InitializeParams {
        self.0.clone()
    }

    fn handle(&mut self, message: &str) -> String {
        let Ok(Message::Request(req)) = Message::from_json_str(message) else {
            return ReferenceServer::new(self.0.clone()).handle(message);
        };
        let result = InitializeResult {
            version: self.0.versions.max(),
            capabilities: self.0.capabilities.clone(),
        };
        Message::Response(Response::success(
            req.id,
            serde_json::to_value(result).unwrap(),
        ))
        .to_json_string()
    }
}

#[test]
fn server_ignoring_version_mismatch_fails() {
    let report = run_handshake(&mut AcceptsAnything(supported()));
    let failed = failing(&report);
    assert!(failed.contains(&"disjoint_ranges_rejected"), "{report:?}");
    assert!(failed.contains(&"invalid_params_rejected"), "{report:?}");
}

/// Answers with the right payload but the wrong id.
struct WrongId(ReferenceServer);

impl HandshakeServer for WrongId {
    fn supported(&self) -> InitializeParams {
        self.0.supported()
    }

    fn handle(&mut self, message: &str) -> String {
        let answer = self.0.handle(message);
        let mut value: Value = serde_json::from_str(&answer).unwrap();
        value["id"] = json!("not-the-request-id");
        value.to_string()
    }
}

#[test]
fn server_with_wrong_response_id_fails() {
    let report = run_handshake(&mut WrongId(ReferenceServer::new(supported())));
    let failed = failing(&report);
    assert!(
        failed.contains(&"picks_highest_common_version"),
        "{report:?}"
    );
    assert!(failed.contains(&"parse_error_rejected"), "{report:?}");
}

/// Returns text that is not a JSON-RPC message.
struct Garbage;

impl HandshakeServer for Garbage {
    fn supported(&self) -> InitializeParams {
        supported()
    }

    fn handle(&mut self, _: &str) -> String {
        "{\"jsonrpc\":\"2.0\"}".into()
    }
}

#[test]
fn server_sending_invalid_envelopes_fails_every_case() {
    let report = run_handshake(&mut Garbage);
    assert_eq!(failing(&report), CASES);
}

#[test]
#[should_panic(expected = "disjoint_ranges_rejected")]
fn assert_passed_names_failing_cases() {
    run_handshake(&mut AcceptsAnything(supported())).assert_passed();
}

#[test]
fn answer_initialize_matches_reference_server_text() {
    let local = supported();
    let req = local.to_request(Id::Number(5));
    let direct = Message::Response(answer_initialize(&local, &req)).to_value();
    let text = ReferenceServer::new(local).handle(&Message::Request(req).to_json_string());
    assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), direct);
}

/// Answers every error with code INTERNAL_ERROR.
struct WrongErrorCode(ReferenceServer);

impl HandshakeServer for WrongErrorCode {
    fn supported(&self) -> InitializeParams {
        self.0.supported()
    }

    fn handle(&mut self, message: &str) -> String {
        let mut value: Value = serde_json::from_str(&self.0.handle(message)).unwrap();
        if let Some(error) = value.get_mut("error") {
            error["code"] = json!(damavand_proto::jsonrpc::codes::INTERNAL_ERROR);
        }
        value.to_string()
    }
}

#[test]
fn server_with_wrong_error_codes_fails() {
    let report = run_handshake(&mut WrongErrorCode(ReferenceServer::new(supported())));
    assert_eq!(
        failing(&report),
        [
            "disjoint_ranges_rejected",
            "invalid_params_rejected",
            "invalid_envelope_rejected",
            "parse_error_rejected",
        ]
    );
}
