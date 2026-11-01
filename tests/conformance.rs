//! The conformance suite passes the reference server and catches broken ones.
#![cfg(feature = "conformance")]

use damavand_proto::conformance::{HandshakeServer, ReferenceServer, Report, run_handshake};
use damavand_proto::handshake::{PeerInfo, Supported, answer_initialize};
use damavand_proto::version::{CURRENT, SupportedVersions};
use damavand_proto::{Capabilities, Id, InitializeResult, Message, ProtocolVersion, Response};
use serde_json::{Value, json};

const CASES: [&str; 22] = [
    "picks_highest_common_major",
    "minor_is_minimum_of_both",
    "disjoint_above_rejected",
    "disjoint_below_rejected",
    "unknown_capabilities_ignored",
    "capabilities_are_intersection",
    "absent_capabilities_mean_empty",
    "server_info_reported",
    "invalid_params_rejected",
    "malformed_version_rejected",
    "duplicate_major_rejected",
    "invalid_capability_name_rejected",
    "duplicate_member_rejected",
    "unknown_method_not_found",
    "reserved_method_rejected",
    "invalid_envelope_rejected",
    "null_request_id_rejected",
    "fractional_request_id_rejected",
    "batch_rejected",
    "parse_error_rejected",
    "notification_gets_no_reply",
    "response_gets_no_reply",
];

fn v(major: u16, minor: u16) -> ProtocolVersion {
    ProtocolVersion::new(major, minor)
}

fn supported() -> Supported {
    Supported::new(SupportedVersions::new([v(0, 1), v(0, 4), v(1, 3), v(2, 0)]).unwrap())
        .with_capabilities(
            serde_json::from_value(json!({
                "tools": {"listChanged": true},
                "audit.write": {},
                "memory": {"k": 1},
            }))
            .unwrap(),
        )
        .with_info(PeerInfo::new("reference", "0.0.0"))
}

fn failing(report: &Report) -> Vec<&'static str> {
    report.failures().map(|case| case.name).collect()
}

#[test]
fn reference_server_passes_every_case() {
    for params in [
        supported(),
        Supported::new(SupportedVersions::single(CURRENT)),
        // Edges: no line below 0.0, no line above 65535.x, no lower minor.
        Supported::new(SupportedVersions::new([v(0, 0), v(65535, 65535)]).unwrap()),
        Supported::new(SupportedVersions::single(v(1, 0))),
    ] {
        let report = run_handshake(&mut ReferenceServer::new(params));
        let names: Vec<_> = report.cases.iter().map(|case| case.name).collect();
        assert_eq!(names, CASES);
        assert_eq!(failing(&report), Vec::<&str>::new(), "{report:?}");
        report.assert_passed();
    }
}

/// Wraps the reference server and rewrites successful results as JSON.
struct Tamper<F: FnMut(&Value, Value) -> Value> {
    inner: ReferenceServer,
    rewrite: F,
}

impl<F: FnMut(&Value, Value) -> Value> HandshakeServer for Tamper<F> {
    fn supported(&self) -> Supported {
        self.inner.supported()
    }

    fn handle(&mut self, message: &str) -> Option<String> {
        let answer = self.inner.handle(message)?;
        let Ok(Message::Response(resp)) = Message::from_json_str(&answer) else {
            return Some(answer);
        };
        let Some(id) = resp.id().cloned() else {
            return Some(answer);
        };
        let Ok(result) = resp.into_outcome() else {
            return Some(answer);
        };
        let request: Value = serde_json::from_str(message).unwrap();
        let result = (self.rewrite)(&request["params"], result);
        Some(Message::Response(Response::success(id, result)).to_json_string())
    }
}

fn tampered(rewrite: impl FnMut(&Value, Value) -> Value) -> Vec<&'static str> {
    let mut server = Tamper {
        inner: ReferenceServer::new(supported()),
        rewrite,
    };
    failing(&run_handshake(&mut server))
}

#[test]
fn server_picking_lowest_version_fails() {
    let failed = tampered(|_, mut r| {
        r["version"] = json!("0.1");
        r
    });
    assert!(failed.contains(&"picks_highest_common_major"), "{failed:?}");
}

#[test]
fn server_picking_its_own_minor_fails() {
    // Answers with the server's minor in the negotiated major, not the lower one.
    let failed = tampered(|_, mut r| {
        let got: ProtocolVersion = serde_json::from_value(r["version"].clone()).unwrap();
        if let Some(own) = supported()
            .versions()
            .iter()
            .find(|own| own.is_compatible_with(got))
        {
            r["version"] = json!(own.to_string());
        }
        r
    });
    assert_eq!(failed, ["minor_is_minimum_of_both"]);
}

#[test]
fn server_echoing_client_capabilities_fails() {
    let failed = tampered(|params, mut r| {
        r["capabilities"] = params.get("capabilities").cloned().unwrap_or(json!({}));
        r
    });
    assert!(
        failed.contains(&"unknown_capabilities_ignored"),
        "{failed:?}"
    );
    assert!(
        failed.contains(&"capabilities_are_intersection"),
        "{failed:?}"
    );
}

#[test]
fn server_dropping_capabilities_fails() {
    let failed = tampered(|_, mut r| {
        r["capabilities"] = json!({});
        r
    });
    assert!(
        failed.contains(&"capabilities_are_intersection"),
        "{failed:?}"
    );
}

#[test]
fn server_reading_absent_capabilities_as_all_fails() {
    let failed = tampered(|params, mut r| {
        if params.get("capabilities").is_none() {
            r["capabilities"] = serde_json::to_value(supported().capabilities()).unwrap();
        }
        r
    });
    assert_eq!(failed, ["absent_capabilities_mean_empty"]);
}

#[test]
fn server_dropping_server_info_fails() {
    let failed = tampered(|_, mut r| {
        r.as_object_mut().unwrap().remove("serverInfo");
        r
    });
    assert_eq!(failed, ["server_info_reported"]);
}

/// Answers every request with success and its own highest version.
struct AcceptsAnything(Supported);

impl HandshakeServer for AcceptsAnything {
    fn supported(&self) -> Supported {
        self.0.clone()
    }

    fn handle(&mut self, message: &str) -> Option<String> {
        let Ok(Message::Request(req)) = Message::from_json_str(message) else {
            return ReferenceServer::new(self.0.clone()).handle(message);
        };
        let result = InitializeResult::new(self.0.versions().highest(), Capabilities::new());
        Some(
            Message::Response(Response::success(
                req.id().clone(),
                serde_json::to_value(result).unwrap(),
            ))
            .to_json_string(),
        )
    }
}

#[test]
fn server_ignoring_errors_fails() {
    let failed = failing(&run_handshake(&mut AcceptsAnything(supported())));
    for case in [
        "minor_is_minimum_of_both",
        "disjoint_above_rejected",
        "disjoint_below_rejected",
        "invalid_params_rejected",
        "malformed_version_rejected",
        "duplicate_major_rejected",
        "invalid_capability_name_rejected",
        "unknown_method_not_found",
    ] {
        assert!(failed.contains(&case), "{case} passed: {failed:?}");
    }
}

/// Rewrites every answer's id.
struct RewriteId(ReferenceServer, Value);

impl HandshakeServer for RewriteId {
    fn supported(&self) -> Supported {
        self.0.supported()
    }

    fn handle(&mut self, message: &str) -> Option<String> {
        let answer = self.0.handle(message)?;
        let mut value: Value = serde_json::from_str(&answer).unwrap();
        value["id"] = self.1.clone();
        Some(value.to_string())
    }
}

#[test]
fn server_with_wrong_response_id_fails() {
    let failed = failing(&run_handshake(&mut RewriteId(
        ReferenceServer::new(supported()),
        json!("not-the-request-id"),
    )));
    assert!(failed.contains(&"picks_highest_common_major"), "{failed:?}");
    assert!(failed.contains(&"parse_error_rejected"), "{failed:?}");
    assert!(failed.contains(&"batch_rejected"), "{failed:?}");
}

#[test]
fn server_answering_errors_with_null_id_fails() {
    // A null id is right only when the request id could not be read.
    let failed = failing(&run_handshake(&mut RewriteId(
        ReferenceServer::new(supported()),
        Value::Null,
    )));
    for case in [
        "invalid_params_rejected",
        "unknown_method_not_found",
        "reserved_method_rejected",
        "invalid_envelope_rejected",
    ] {
        assert!(failed.contains(&case), "{case} passed: {failed:?}");
    }
    for case in [
        "duplicate_member_rejected",
        "null_request_id_rejected",
        "fractional_request_id_rejected",
        "batch_rejected",
        "parse_error_rejected",
    ] {
        assert!(!failed.contains(&case), "{case} failed: {failed:?}");
    }
}

/// Returns text that is not a JSON-RPC message.
struct Garbage;

impl HandshakeServer for Garbage {
    fn supported(&self) -> Supported {
        supported()
    }

    fn handle(&mut self, _: &str) -> Option<String> {
        Some("{\"jsonrpc\":\"2.0\"}".into())
    }
}

#[test]
fn server_sending_invalid_envelopes_fails_every_case() {
    let report = run_handshake(&mut Garbage);
    assert_eq!(failing(&report), CASES);
}

/// Answers every message that should get no answer.
struct RepliesToEverything(ReferenceServer);

impl HandshakeServer for RepliesToEverything {
    fn supported(&self) -> Supported {
        self.0.supported()
    }

    fn handle(&mut self, message: &str) -> Option<String> {
        Some(self.0.handle(message).unwrap_or_else(|| {
            Message::Response(Response::error(
                None,
                damavand_proto::ErrorObject::new(-32600, "unexpected"),
            ))
            .to_json_string()
        }))
    }
}

#[test]
fn server_replying_to_notifications_or_responses_fails() {
    let failed = failing(&run_handshake(&mut RepliesToEverything(
        ReferenceServer::new(supported()),
    )));
    assert_eq!(
        failed,
        ["notification_gets_no_reply", "response_gets_no_reply"]
    );
}

/// Treats `rpc.` methods like any other method.
struct AllowsReservedMethods(ReferenceServer);

impl HandshakeServer for AllowsReservedMethods {
    fn supported(&self) -> Supported {
        self.0.supported()
    }

    fn handle(&mut self, message: &str) -> Option<String> {
        let mut value: Value = serde_json::from_str(message).ok()?;
        if let Some(method) = value.get("method").and_then(Value::as_str) {
            if let Some(rest) = method.strip_prefix("rpc.") {
                value["method"] = json!(format!("x-{rest}"));
            }
        }
        self.0.handle(&value.to_string())
    }
}

#[test]
fn server_allowing_reserved_methods_fails() {
    let failed = failing(&run_handshake(&mut AllowsReservedMethods(
        ReferenceServer::new(supported()),
    )));
    assert!(failed.contains(&"reserved_method_rejected"), "{failed:?}");
}

/// Reads JSON text leniently: the last of two equal member names wins.
struct LenientJson(ReferenceServer);

impl HandshakeServer for LenientJson {
    fn supported(&self) -> Supported {
        self.0.supported()
    }

    fn handle(&mut self, message: &str) -> Option<String> {
        match serde_json::from_str::<Value>(message) {
            Ok(value) => self.0.handle(&value.to_string()),
            Err(_) => self.0.handle(message),
        }
    }
}

#[test]
fn server_accepting_duplicate_members_fails() {
    let failed = failing(&run_handshake(&mut LenientJson(ReferenceServer::new(
        supported(),
    ))));
    assert_eq!(failed, ["duplicate_member_rejected"]);
}

/// Answers every error with code INTERNAL_ERROR.
struct WrongErrorCode(ReferenceServer);

impl HandshakeServer for WrongErrorCode {
    fn supported(&self) -> Supported {
        self.0.supported()
    }

    fn handle(&mut self, message: &str) -> Option<String> {
        let mut value: Value = serde_json::from_str(&self.0.handle(message)?).unwrap();
        if let Some(error) = value.get_mut("error") {
            error["code"] = json!(damavand_proto::jsonrpc::codes::INTERNAL_ERROR);
        }
        Some(value.to_string())
    }
}

#[test]
fn server_with_wrong_error_codes_fails() {
    let failed = failing(&run_handshake(&mut WrongErrorCode(ReferenceServer::new(
        supported(),
    ))));
    assert_eq!(
        failed,
        [
            "disjoint_above_rejected",
            "disjoint_below_rejected",
            "invalid_params_rejected",
            "malformed_version_rejected",
            "duplicate_major_rejected",
            "invalid_capability_name_rejected",
            "duplicate_member_rejected",
            "unknown_method_not_found",
            "reserved_method_rejected",
            "invalid_envelope_rejected",
            "null_request_id_rejected",
            "fractional_request_id_rejected",
            "batch_rejected",
            "parse_error_rejected",
        ]
    );
}

#[test]
#[should_panic(expected = "disjoint_above_rejected")]
fn assert_passed_names_failing_cases() {
    run_handshake(&mut AcceptsAnything(supported())).assert_passed();
}

#[test]
fn reference_server_matches_answer_initialize() {
    let local = supported();
    let req = local.to_request(Id::Number(5));
    let direct = Message::Response(answer_initialize(&local, &req)).to_value();
    let text = ReferenceServer::new(local)
        .handle(&Message::Request(req).to_json_string())
        .unwrap();
    assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), direct);
}

#[test]
fn reference_server_is_silent_for_notifications_and_responses() {
    let mut server = ReferenceServer::new(supported());
    for text in [
        r#"{"jsonrpc":"2.0","method":"initialize","params":{"versions":["0.1"]}}"#,
        r#"{"jsonrpc":"2.0","method":"anything"}"#,
        r#"{"jsonrpc":"2.0","method":"rpc.cancel"}"#,
        r#"{"jsonrpc":"2.0","id":1,"result":{}}"#,
        r#"{"jsonrpc":"2.0","id":null,"error":{"code":1,"message":"x"}}"#,
        r#"{"jsonrpc":"2.0","id":1,"result":1,"error":{"code":1,"message":"x"}}"#,
    ] {
        assert_eq!(server.handle(text), None, "{text}");
    }
}

/// Records every message the suite sends.
struct Recorder(ReferenceServer, Vec<String>);

impl HandshakeServer for Recorder {
    fn supported(&self) -> Supported {
        self.0.supported()
    }

    fn handle(&mut self, message: &str) -> Option<String> {
        self.1.push(message.to_owned());
        self.0.handle(message)
    }
}

/// The versions offered by the suite's `case` initialize requests.
fn offered(list: &[ProtocolVersion], case: &str) -> Vec<Value> {
    let mut server = Recorder(
        ReferenceServer::new(Supported::new(
            SupportedVersions::new(list.iter().copied()).unwrap(),
        )),
        Vec::new(),
    );
    run_handshake(&mut server).assert_passed();
    let id = json!(format!("conformance-{case}"));
    server
        .1
        .iter()
        .filter_map(|text| serde_json::from_str::<Value>(text).ok())
        .filter(|value| value["id"] == id)
        .map(|value| value["params"]["versions"].clone())
        .collect()
}

#[test]
fn disjoint_cases_offer_the_adjacent_line() {
    assert_eq!(offered(&[v(0, 2)], "below"), [json!(["0.1"])]);
    assert_eq!(offered(&[v(0, 2)], "above"), [json!(["0.3"])]);
    assert_eq!(offered(&[v(2, 5)], "below"), [json!(["1.0"])]);
    assert_eq!(offered(&[v(2, 5)], "above"), [json!(["3.0"])]);
    assert_eq!(offered(&[v(1, 0)], "below"), [json!(["0.0"])]);
    assert_eq!(offered(&[v(0, 0)], "below"), Vec::<Value>::new());
}
