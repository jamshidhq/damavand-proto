//! Versions, capabilities and the `initialize` handshake.

use damavand_proto::handshake::{METHOD_INITIALIZE, answer_initialize};
use damavand_proto::jsonrpc::{Params, codes};
use damavand_proto::version::{CURRENT, VersionError};
use damavand_proto::{
    CapabilitySet, Id, IncompatibleVersion, InitializeParams, InitializeResult, ProtocolVersion,
    Request, VersionRange, negotiate,
};
use proptest::prelude::*;
use serde_json::json;

fn v(major: u16, minor: u16) -> ProtocolVersion {
    ProtocolVersion::new(major, minor)
}

fn range(min: ProtocolVersion, max: ProtocolVersion) -> VersionRange {
    VersionRange::new(min, max).unwrap()
}

fn caps(names: &[&str]) -> CapabilitySet {
    names.iter().copied().collect()
}

fn peer(min: ProtocolVersion, max: ProtocolVersion, names: &[&str]) -> InitializeParams {
    InitializeParams {
        versions: range(min, max),
        capabilities: caps(names),
    }
}

// --- ProtocolVersion ----------------------------------------------------------

#[test]
fn version_orders_by_major_then_minor() {
    assert!(v(1, 0) > v(0, 99));
    assert!(v(1, 2) > v(1, 1));
    assert_eq!(v(2, 3), v(2, 3));
}

#[test]
fn version_text_form() {
    assert_eq!(v(1, 12).to_string(), "1.12");
    assert_eq!("0.3".parse::<ProtocolVersion>().unwrap(), v(0, 3));
    assert_eq!(serde_json::to_value(v(4, 5)).unwrap(), json!("4.5"));
    assert_eq!(
        serde_json::from_value::<ProtocolVersion>(json!("4.5")).unwrap(),
        v(4, 5)
    );
}

#[test]
fn malformed_versions_are_refused() {
    for text in [
        "", "1", "1.", ".1", "1.2.3", "a.b", "-1.0", "1.-0", "+1.0", "1.0 ", " 1.0", "65536.0",
    ] {
        assert!(
            matches!(
                text.parse::<ProtocolVersion>(),
                Err(VersionError::Malformed(_))
            ),
            "accepted {text:?}"
        );
    }
    assert!(serde_json::from_value::<ProtocolVersion>(json!(1.0)).is_err());
}

#[test]
fn current_version_is_0_1() {
    assert_eq!(CURRENT, v(0, 1));
}

// --- VersionRange -------------------------------------------------------------

#[test]
fn empty_range_is_refused() {
    assert_eq!(
        VersionRange::new(v(1, 1), v(1, 0)),
        Err(VersionError::EmptyRange {
            min: v(1, 1),
            max: v(1, 0)
        })
    );
    assert!(serde_json::from_value::<VersionRange>(json!({"min": "2.0", "max": "1.0"})).is_err());
    assert!(!VersionError::Malformed("x".into()).to_string().is_empty());
}

#[test]
fn range_wire_form() {
    let r = range(v(0, 1), v(1, 3));
    let value = json!({"min": "0.1", "max": "1.3"});
    assert_eq!(serde_json::to_value(r).unwrap(), value);
    assert_eq!(serde_json::from_value::<VersionRange>(value).unwrap(), r);
}

#[test]
fn range_contains_is_inclusive() {
    let r = range(v(0, 2), v(1, 0));
    assert!(r.contains(v(0, 2)));
    assert!(r.contains(v(0, 9)));
    assert!(r.contains(v(1, 0)));
    assert!(!r.contains(v(0, 1)));
    assert!(!r.contains(v(1, 1)));
}

#[test]
fn highest_common_version() {
    let a = range(v(0, 1), v(0, 5));
    assert_eq!(a.highest_common(&range(v(0, 3), v(2, 0))), Some(v(0, 5)));
    assert_eq!(a.highest_common(&range(v(0, 0), v(0, 2))), Some(v(0, 2)));
    assert_eq!(a.highest_common(&range(v(0, 5), v(0, 5))), Some(v(0, 5)));
    assert_eq!(a.highest_common(&range(v(0, 6), v(1, 0))), None);
    assert_eq!(a.highest_common(&VersionRange::single(v(0, 0))), None);
}

proptest! {
    #[test]
    fn highest_common_is_the_max_of_the_overlap(
        a in (any::<u8>(), any::<u8>(), any::<u8>(), any::<u8>()),
        b in (any::<u8>(), any::<u8>(), any::<u8>(), any::<u8>()),
    ) {
        let mk = |(p, q, r, s): (u8, u8, u8, u8)| {
            let x = ProtocolVersion::new(u16::from(p % 4), u16::from(q));
            let y = ProtocolVersion::new(u16::from(r % 4), u16::from(s));
            VersionRange::new(x.min(y), x.max(y)).unwrap()
        };
        let (ra, rb) = (mk(a), mk(b));
        let got = ra.highest_common(&rb);
        prop_assert_eq!(got, rb.highest_common(&ra));
        match got {
            Some(ver) => {
                prop_assert!(ra.contains(ver) && rb.contains(ver));
                prop_assert_eq!(ver, ra.max().min(rb.max()));
            }
            None => prop_assert!(ra.max() < rb.min() || rb.max() < ra.min()),
        }
    }
}

// --- CapabilitySet ------------------------------------------------------------

#[test]
fn capability_set_basics() {
    let mut set = CapabilitySet::new();
    assert!(set.is_empty());
    assert!(set.insert("b"));
    assert!(!set.is_empty());
    assert!(set.insert("a"));
    assert!(!set.insert("a"));
    assert!(set.contains("a"));
    assert!(!set.contains("c"));
    assert_eq!(set.len(), 2);
    assert_eq!(set.iter().collect::<Vec<_>>(), ["a", "b"]);
}

#[test]
fn capability_wire_form_is_a_sorted_array() {
    let set = caps(&["z", "a", "a"]);
    assert_eq!(serde_json::to_value(&set).unwrap(), json!(["a", "z"]));
    assert_eq!(
        serde_json::from_value::<CapabilitySet>(json!(["z", "a"])).unwrap(),
        set
    );
    assert!(serde_json::from_value::<CapabilitySet>(json!(["a", 1])).is_err());
}

#[test]
fn capability_intersection() {
    let both = caps(&["a", "b", "c"]).intersection(&caps(&["b", "c", "d"]));
    assert_eq!(both, caps(&["b", "c"]));
}

// --- negotiate ------------------------------------------------------------------

#[test]
fn negotiate_picks_highest_common_version_and_common_capabilities() {
    let local = peer(v(0, 1), v(0, 4), &["tools.call", "audit"]);
    let remote = peer(v(0, 2), v(1, 0), &["tools.call", "x-future-feature"]);
    assert_eq!(
        negotiate(&local, &remote),
        Ok(InitializeResult {
            version: v(0, 4),
            capabilities: caps(&["tools.call"]),
        })
    );
}

#[test]
fn negotiate_is_symmetric() {
    let a = peer(v(0, 1), v(0, 4), &["x", "y"]);
    let b = peer(v(0, 3), v(0, 9), &["y", "z"]);
    assert_eq!(negotiate(&a, &b), negotiate(&b, &a));
}

#[test]
fn negotiate_refuses_disjoint_ranges() {
    let local = peer(v(1, 0), v(1, 2), &[]);
    let remote = peer(v(0, 1), v(0, 9), &[]);
    assert_eq!(
        negotiate(&local, &remote),
        Err(IncompatibleVersion {
            local: local.versions,
            remote: remote.versions,
        })
    );
}

#[test]
fn incompatible_version_error_object() {
    let err = IncompatibleVersion {
        local: range(v(1, 0), v(1, 2)),
        remote: range(v(0, 1), v(0, 9)),
    };
    let obj = err.to_error_object();
    assert_eq!(obj.code, codes::INCOMPATIBLE_VERSION);
    assert_eq!(
        obj.data,
        Some(json!({"supported": {"min": "1.0", "max": "1.2"}}))
    );
    let text = err.to_string();
    assert!(
        text.contains("1.0..=1.2") && text.contains("0.1..=0.9"),
        "{text}"
    );
}

#[test]
fn incompatible_version_code_is_in_server_error_range() {
    assert!(
        (codes::SERVER_ERROR_MIN..=codes::SERVER_ERROR_MAX).contains(&codes::INCOMPATIBLE_VERSION)
    );
}

// --- wire: initialize ------------------------------------------------------------

#[test]
fn initialize_params_wire_form() {
    let p = peer(v(0, 1), v(0, 2), &["b", "a"]);
    let value = json!({"versions": {"min": "0.1", "max": "0.2"}, "capabilities": ["a", "b"]});
    assert_eq!(serde_json::to_value(&p).unwrap(), value);
    assert_eq!(
        serde_json::from_value::<InitializeParams>(value).unwrap(),
        p
    );
    let no_caps: InitializeParams =
        serde_json::from_value(json!({"versions": {"min": "0.1", "max": "0.1"}})).unwrap();
    assert!(no_caps.capabilities.is_empty());
}

#[test]
fn to_request_builds_initialize() {
    let p = peer(v(0, 1), v(0, 1), &["a"]);
    let req = p.to_request(Id::Number(1));
    assert_eq!(req.id, Id::Number(1));
    assert_eq!(req.method, METHOD_INITIALIZE);
    let Some(Params::Object(map)) = &req.params else {
        panic!("params not an object")
    };
    assert_eq!(
        serde_json::Value::Object(map.clone()),
        serde_json::to_value(&p).unwrap()
    );
}

#[test]
fn answer_initialize_success() {
    let local = peer(v(0, 1), v(0, 3), &["a", "b"]);
    let client = peer(v(0, 2), v(0, 9), &["b", "unknown"]);
    let resp = answer_initialize(&local, &client.to_request(Id::String("h".into())));
    assert_eq!(resp.id(), Some(&Id::String("h".into())));
    let result: InitializeResult = serde_json::from_value(resp.outcome().unwrap().clone()).unwrap();
    assert_eq!(result.version, v(0, 3));
    assert_eq!(result.capabilities, caps(&["b"]));
}

#[test]
fn answer_initialize_incompatible() {
    let local = peer(v(1, 0), v(1, 0), &[]);
    let client = peer(v(0, 1), v(0, 1), &[]);
    let resp = answer_initialize(&local, &client.to_request(Id::Number(9)));
    assert_eq!(resp.id(), Some(&Id::Number(9)));
    assert_eq!(
        resp.outcome().unwrap_err().code,
        codes::INCOMPATIBLE_VERSION
    );
}

#[test]
fn answer_initialize_invalid_params() {
    let local = peer(v(0, 1), v(0, 1), &[]);
    for params in [
        None,
        Some(Params::Array(vec![])),
        Some(Params::Object(
            json!({"versions": "0.1"}).as_object().unwrap().clone(),
        )),
        Some(Params::Object(
            json!({"versions": {"min": "0.2", "max": "0.1"}})
                .as_object()
                .unwrap()
                .clone(),
        )),
    ] {
        let req = Request {
            id: Id::Number(1),
            method: METHOD_INITIALIZE.into(),
            params: params.clone(),
        };
        let resp = answer_initialize(&local, &req);
        assert_eq!(resp.id(), Some(&Id::Number(1)));
        assert_eq!(
            resp.outcome().unwrap_err().code,
            codes::INVALID_PARAMS,
            "params {params:?}"
        );
    }
}

#[test]
fn answer_initialize_wrong_method() {
    let local = peer(v(0, 1), v(0, 1), &[]);
    let mut req = local.to_request(Id::Number(1));
    req.method = "shutdown".into();
    let resp = answer_initialize(&local, &req);
    assert_eq!(resp.outcome().unwrap_err().code, codes::METHOD_NOT_FOUND);
}
