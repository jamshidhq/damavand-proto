//! The `initialize` handshake: params, result, negotiation and the reference
//! server side.

use damavand_proto::handshake::{METHOD_INITIALIZE, PeerInfo, Supported, answer_initialize};
use damavand_proto::jsonrpc::{Params, codes};
use damavand_proto::version::SupportedVersions;
use damavand_proto::{
    Capabilities, Id, IncompatibleVersion, InitializeParams, InitializeResult, ProtocolVersion,
    Request, negotiate,
};
use serde_json::{Value, json};

fn v(major: u16, minor: u16) -> ProtocolVersion {
    ProtocolVersion::new(major, minor)
}

fn versions(list: &[ProtocolVersion]) -> SupportedVersions {
    SupportedVersions::new(list.iter().copied()).unwrap()
}

fn caps(value: Value) -> Capabilities {
    serde_json::from_value(value).unwrap()
}

fn server(list: &[ProtocolVersion], capabilities: Value) -> Supported {
    Supported::new(versions(list)).with_capabilities(caps(capabilities))
}

fn client(list: &[ProtocolVersion], capabilities: Value) -> InitializeParams {
    InitializeParams::new(versions(list)).with_capabilities(caps(capabilities))
}

// --- PeerInfo -------------------------------------------------------------------

#[test]
fn peer_info_wire_form() {
    let info = PeerInfo::new("damavand-host", "1.2.3");
    assert_eq!(info.name(), "damavand-host");
    assert_eq!(info.version(), "1.2.3");
    let value = json!({"name": "damavand-host", "version": "1.2.3"});
    assert_eq!(serde_json::to_value(&info).unwrap(), value);
    assert_eq!(serde_json::from_value::<PeerInfo>(value).unwrap(), info);
    assert!(serde_json::from_value::<PeerInfo>(json!({"name": "x"})).is_err());
}

// --- InitializeParams -----------------------------------------------------------

#[test]
fn initialize_params_wire_form() {
    let p = client(&[v(1, 4), v(2, 0)], json!({"tools": {"listChanged": true}}))
        .with_client_info(PeerInfo::new("c", "0.1.0"));
    let value = json!({
        "versions": ["1.4", "2.0"],
        "capabilities": {"tools": {"listChanged": true}},
        "clientInfo": {"name": "c", "version": "0.1.0"},
    });
    assert_eq!(serde_json::to_value(&p).unwrap(), value);
    assert_eq!(
        serde_json::from_value::<InitializeParams>(value).unwrap(),
        p
    );
    assert_eq!(p.versions(), &versions(&[v(1, 4), v(2, 0)]));
    assert_eq!(
        p.capabilities(),
        &caps(json!({"tools": {"listChanged": true}}))
    );
    assert_eq!(p.client_info(), Some(&PeerInfo::new("c", "0.1.0")));
}

#[test]
fn initialize_params_optional_members() {
    let minimal: InitializeParams = serde_json::from_value(json!({"versions": ["0.1"]})).unwrap();
    assert!(minimal.capabilities().is_empty());
    assert_eq!(minimal.client_info(), None);
    // An absent clientInfo is not written as null.
    assert_eq!(
        serde_json::to_value(&minimal).unwrap(),
        json!({"versions": ["0.1"], "capabilities": {}})
    );
    // Unknown members are ignored: a minor version may add members.
    let extra: InitializeParams =
        serde_json::from_value(json!({"versions": ["0.1"], "future": 1})).unwrap();
    assert_eq!(extra, minimal);
}

#[test]
fn initialize_params_refuses_bad_members() {
    for bad in [
        json!({}),
        json!({"versions": {"min": "0.1", "max": "0.1"}}),
        json!({"versions": []}),
        json!({"versions": ["01.1"]}),
        json!({"versions": ["1.1", "1.2"]}),
        json!({"versions": ["0.1"], "capabilities": ["tools"]}),
        json!({"versions": ["0.1"], "capabilities": {"Tools": {}}}),
        json!({"versions": ["0.1"], "clientInfo": "me"}),
    ] {
        assert!(
            serde_json::from_value::<InitializeParams>(bad.clone()).is_err(),
            "accepted {bad}"
        );
    }
}

#[test]
fn to_request_builds_initialize() {
    let p = client(&[v(0, 1)], json!({"a": {}}));
    let req = p.to_request(Id::Number(1));
    assert_eq!(req.id(), &Id::Number(1));
    assert_eq!(req.method(), METHOD_INITIALIZE);
    let Some(Params::Object(map)) = req.params() else {
        panic!("params not an object")
    };
    assert_eq!(
        Value::Object(map.clone()),
        serde_json::to_value(&p).unwrap()
    );
}

// --- InitializeResult -----------------------------------------------------------

#[test]
fn initialize_result_wire_form() {
    let r = InitializeResult::new(v(1, 2), caps(json!({"tools": {}})))
        .with_server_info(PeerInfo::new("s", "9.9.9"));
    assert_eq!(r.version(), v(1, 2));
    assert_eq!(r.capabilities(), &caps(json!({"tools": {}})));
    assert_eq!(r.server_info(), Some(&PeerInfo::new("s", "9.9.9")));
    let value = json!({
        "version": "1.2",
        "capabilities": {"tools": {}},
        "serverInfo": {"name": "s", "version": "9.9.9"},
    });
    assert_eq!(serde_json::to_value(&r).unwrap(), value);
    assert_eq!(
        serde_json::from_value::<InitializeResult>(value).unwrap(),
        r
    );
}

#[test]
fn initialize_result_absent_capabilities_are_empty() {
    let r: InitializeResult = serde_json::from_value(json!({"version": "0.1"})).unwrap();
    assert!(r.capabilities().is_empty());
    assert_eq!(r.server_info(), None);
    assert_eq!(
        serde_json::to_value(&r).unwrap(),
        json!({"version": "0.1", "capabilities": {}})
    );
}

// --- Supported --------------------------------------------------------------------

#[test]
fn supported_builds_the_client_params() {
    let s = server(&[v(0, 1)], json!({"a": {"k": 1}})).with_info(PeerInfo::new("me", "1.0.0"));
    assert_eq!(s.versions(), &versions(&[v(0, 1)]));
    assert_eq!(s.capabilities(), &caps(json!({"a": {"k": 1}})));
    assert_eq!(s.info(), Some(&PeerInfo::new("me", "1.0.0")));
    let params = s.to_params();
    assert_eq!(params.versions(), s.versions());
    assert_eq!(params.capabilities(), s.capabilities());
    assert_eq!(params.client_info(), s.info());
    let req = s.to_request(Id::Number(4));
    assert_eq!(req, params.to_request(Id::Number(4)));
    // Without info, no clientInfo.
    assert_eq!(
        server(&[v(0, 1)], json!({})).to_params().client_info(),
        None
    );
}

// --- negotiate --------------------------------------------------------------------

#[test]
fn negotiate_picks_version_and_common_capabilities_with_server_parameters() {
    let local = server(
        &[v(1, 4), v(2, 0)],
        json!({"tools": {"listChanged": true}, "audit": {}}),
    )
    .with_info(PeerInfo::new("srv", "3.0.0"));
    let remote = client(
        &[v(1, 2)],
        json!({"tools": {"listChanged": false}, "x-future": {}}),
    );
    assert_eq!(
        negotiate(&local, &remote),
        Ok(
            InitializeResult::new(v(1, 2), caps(json!({"tools": {"listChanged": true}})))
                .with_server_info(PeerInfo::new("srv", "3.0.0"))
        )
    );
}

#[test]
fn negotiate_version_and_names_do_not_depend_on_who_is_server() {
    let a = (&[v(0, 1), v(1, 4)][..], json!({"x": {"p": 1}, "y": {}}));
    let b = (&[v(1, 9)][..], json!({"y": {"q": 2}, "z": {}}));
    let a_serves = negotiate(&server(a.0, a.1.clone()), &client(b.0, b.1.clone())).unwrap();
    let b_serves = negotiate(&server(b.0, b.1), &client(a.0, a.1)).unwrap();
    assert_eq!(a_serves.version(), v(1, 4));
    assert_eq!(a_serves.version(), b_serves.version());
    assert_eq!(
        a_serves.capabilities().names().collect::<Vec<_>>(),
        b_serves.capabilities().names().collect::<Vec<_>>()
    );
    // The parameters are the server's.
    assert_eq!(
        a_serves.capabilities().get("y"),
        Some(&serde_json::Map::new())
    );
    assert_eq!(
        b_serves.capabilities().get("y"),
        Some(json!({"q": 2}).as_object().unwrap())
    );
}

#[test]
fn negotiate_refuses_disjoint_versions() {
    let local = server(&[v(1, 2)], json!({}));
    let remote = client(&[v(0, 9), v(2, 0)], json!({}));
    let err = negotiate(&local, &remote).unwrap_err();
    assert_eq!(err.local(), local.versions());
    assert_eq!(err.remote(), remote.versions());
}

#[test]
fn incompatible_version_error_object() {
    let local = server(&[v(1, 2), v(2, 0)], json!({}));
    let remote = client(&[v(0, 9)], json!({}));
    let err: IncompatibleVersion = negotiate(&local, &remote).unwrap_err();
    let obj = err.to_error_object();
    assert_eq!(obj.code(), codes::INCOMPATIBLE_VERSION);
    assert_eq!(obj.data(), Some(&json!({"supported": ["1.2", "2.0"]})));
    let text = err.to_string();
    assert!(
        text.contains("1.2") && text.contains("2.0") && text.contains("0.9"),
        "{text}"
    );
}

// --- answer_initialize ------------------------------------------------------------

#[test]
fn answer_initialize_success() {
    let local = server(&[v(0, 1), v(0, 3)], json!({"a": {}, "b": {"k": true}}));
    let remote = client(&[v(0, 3), v(1, 0)], json!({"b": {}, "unknown": {}}));
    let resp = answer_initialize(&local, &remote.to_request(Id::String("h".into())));
    assert_eq!(resp.id(), Some(&Id::String("h".into())));
    let result: InitializeResult = serde_json::from_value(resp.outcome().unwrap().clone()).unwrap();
    assert_eq!(result.version(), v(0, 3));
    assert_eq!(result.capabilities(), &caps(json!({"b": {"k": true}})));
}

#[test]
fn answer_initialize_incompatible() {
    let local = server(&[v(1, 0)], json!({}));
    let remote = client(&[v(0, 1)], json!({}));
    let resp = answer_initialize(&local, &remote.to_request(Id::Number(9)));
    assert_eq!(resp.id(), Some(&Id::Number(9)));
    let err = resp.outcome().unwrap_err();
    assert_eq!(err.code(), codes::INCOMPATIBLE_VERSION);
    assert_eq!(err.data(), Some(&json!({"supported": ["1.0"]})));
}

#[test]
fn answer_initialize_invalid_params() {
    let local = server(&[v(0, 1)], json!({}));
    for params in [
        None,
        Some(Params::Array(vec![])),
        Some(Params::Object(
            json!({"versions": "0.1"}).as_object().unwrap().clone(),
        )),
        Some(Params::Object(
            json!({"versions": ["01.1"]}).as_object().unwrap().clone(),
        )),
        Some(Params::Object(
            json!({"versions": ["0.1"], "capabilities": {"A": {}}})
                .as_object()
                .unwrap()
                .clone(),
        )),
    ] {
        let req = Request::new(Id::Number(1), METHOD_INITIALIZE, params.clone());
        let resp = answer_initialize(&local, &req);
        assert_eq!(resp.id(), Some(&Id::Number(1)));
        assert_eq!(
            resp.outcome().unwrap_err().code(),
            codes::INVALID_PARAMS,
            "params {params:?}"
        );
    }
}

#[test]
fn answer_initialize_wrong_method() {
    let local = server(&[v(0, 1)], json!({}));
    let req = Request::new(Id::Number(3), "shutdown", None);
    let resp = answer_initialize(&local, &req);
    assert_eq!(resp.id(), Some(&Id::Number(3)));
    let err = resp.outcome().unwrap_err();
    assert_eq!(err.code(), codes::METHOD_NOT_FOUND);
    assert!(err.message().contains("shutdown"), "{}", err.message());
}
