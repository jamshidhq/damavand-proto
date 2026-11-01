//! JSON-RPC 2.0 envelope: accepted shapes, refused shapes, wire output.

use damavand_proto::jsonrpc::{EnvelopeErrorKind, Params, codes};
use damavand_proto::{EnvelopeError, ErrorObject, Id, Message, Notification, Request, Response};
use serde_json::{Value, json};

fn parse(value: Value) -> Result<Message, EnvelopeError> {
    Message::from_value(value)
}

// --- accepted --------------------------------------------------------------

#[test]
fn request_with_named_params() {
    let msg = parse(json!({"jsonrpc": "2.0", "id": 1, "method": "m", "params": {"a": 1}})).unwrap();
    let Message::Request(req) = msg else {
        panic!("not a request: {msg:?}")
    };
    assert_eq!(req.id(), &Id::Number(1));
    assert_eq!(req.method(), "m");
    let Some(Params::Object(map)) = req.into_params() else {
        panic!("params not an object")
    };
    assert_eq!(map.get("a"), Some(&json!(1)));
}

#[test]
fn request_with_string_id_and_positional_params() {
    let msg =
        parse(json!({"jsonrpc": "2.0", "id": "abc", "method": "m", "params": [1, 2]})).unwrap();
    assert_eq!(
        msg,
        Message::Request(Request::new(
            Id::String("abc".into()),
            "m",
            Some(Params::Array(vec![json!(1), json!(2)])),
        ))
    );
}

#[test]
fn notification_has_no_id() {
    let msg = parse(json!({"jsonrpc": "2.0", "method": "note"})).unwrap();
    assert_eq!(msg, Message::Notification(Notification::new("note", None)));
}

#[test]
fn success_response_with_null_result_is_valid() {
    let msg = parse(json!({"jsonrpc": "2.0", "id": 7, "result": null})).unwrap();
    let Message::Response(resp) = msg else {
        panic!("not a response")
    };
    assert_eq!(resp.id(), Some(&Id::Number(7)));
    assert_eq!(resp.outcome(), Ok(&Value::Null));
}

#[test]
fn error_response_with_null_id_is_valid() {
    let msg = parse(json!({
        "jsonrpc": "2.0", "id": null,
        "error": {"code": -32700, "message": "Parse error"}
    }))
    .unwrap();
    let Message::Response(resp) = msg else {
        panic!("not a response")
    };
    assert_eq!(resp.id(), None);
    let err = resp.outcome().unwrap_err();
    assert_eq!(err.code(), codes::PARSE_ERROR);
    assert_eq!(err.message(), "Parse error");
    assert_eq!(err.data(), None);
}

#[test]
fn error_data_null_is_kept_apart_from_absent() {
    let with_null = json!({
        "jsonrpc": "2.0", "id": 1,
        "error": {"code": 1, "message": "x", "data": null}
    });
    let msg = parse(with_null.clone()).unwrap();
    let Message::Response(resp) = &msg else {
        panic!("not a response")
    };
    assert_eq!(resp.outcome().unwrap_err().data(), Some(&Value::Null));
    assert_eq!(msg.to_value(), with_null);
}

#[test]
fn unknown_members_are_ignored() {
    let msg = parse(json!({"jsonrpc": "2.0", "method": "m", "extra": true})).unwrap();
    assert!(matches!(msg, Message::Notification(_)));
}

#[test]
fn from_json_str_parses_text() {
    let msg = Message::from_json_str(r#"{"jsonrpc":"2.0","id":2,"result":{"ok":true}}"#).unwrap();
    let Message::Response(resp) = msg else {
        panic!("not a response")
    };
    assert_eq!(resp.into_outcome(), Ok(json!({"ok": true})));
}

#[test]
fn serde_deserialize_uses_the_same_validation() {
    let ok: Message = serde_json::from_str(r#"{"jsonrpc":"2.0","method":"m"}"#).unwrap();
    assert!(matches!(ok, Message::Notification(_)));
    let bad = serde_json::from_str::<Message>(r#"{"jsonrpc":"1.0","method":"m"}"#);
    assert!(bad.is_err());
}

// --- refused ----------------------------------------------------------------

fn refused_full(value: Value) -> EnvelopeError {
    match parse(value.clone()) {
        Ok(msg) => panic!("accepted invalid message {value}: {msg:?}"),
        Err(err) => err,
    }
}

fn refused(value: Value) -> EnvelopeErrorKind {
    refused_full(value).kind().clone()
}

#[test]
fn wrong_jsonrpc_version_is_refused() {
    assert_eq!(
        refused(json!({"jsonrpc": "1.0", "id": 1, "method": "m"})),
        EnvelopeErrorKind::BadVersion
    );
    assert_eq!(
        refused(json!({"jsonrpc": 2.0, "id": 1, "method": "m"})),
        EnvelopeErrorKind::BadVersion
    );
    assert_eq!(
        refused(json!({"id": 1, "method": "m"})),
        EnvelopeErrorKind::BadVersion
    );
}

#[test]
fn both_result_and_error_is_refused() {
    assert_eq!(
        refused(json!({
            "jsonrpc": "2.0", "id": 1, "result": 1,
            "error": {"code": 1, "message": "x"}
        })),
        EnvelopeErrorKind::ResultAndError
    );
}

#[test]
fn response_without_result_or_error_is_refused() {
    assert_eq!(
        refused(json!({"jsonrpc": "2.0", "id": 1})),
        EnvelopeErrorKind::NoResultOrError
    );
}

#[test]
fn response_without_id_is_refused() {
    assert_eq!(
        refused(json!({"jsonrpc": "2.0", "result": 1})),
        EnvelopeErrorKind::MissingResponseId
    );
    assert_eq!(
        refused(json!({"jsonrpc": "2.0", "error": {"code": 1, "message": "x"}})),
        EnvelopeErrorKind::MissingResponseId
    );
}

#[test]
fn success_response_with_null_id_is_refused() {
    assert_eq!(
        refused(json!({"jsonrpc": "2.0", "id": null, "result": 1})),
        EnvelopeErrorKind::NullIdOnSuccess
    );
}

#[test]
fn request_with_null_or_fractional_or_bool_id_is_refused() {
    for id in [json!(null), json!(1.5), json!(true), json!([1]), json!({})] {
        assert_eq!(
            refused(json!({"jsonrpc": "2.0", "id": id, "method": "m"})),
            EnvelopeErrorKind::InvalidId,
            "id {id}"
        );
    }
}

#[test]
fn response_with_bad_id_type_is_refused() {
    assert_eq!(
        refused(json!({"jsonrpc": "2.0", "id": true, "result": 1})),
        EnvelopeErrorKind::InvalidId
    );
}

#[test]
fn non_string_method_is_refused() {
    assert_eq!(
        refused(json!({"jsonrpc": "2.0", "id": 1, "method": 5})),
        EnvelopeErrorKind::InvalidMethod
    );
}

#[test]
fn scalar_params_are_refused() {
    for params in [json!(1), json!("x"), json!(null), json!(true)] {
        assert_eq!(
            refused(json!({"jsonrpc": "2.0", "method": "m", "params": params})),
            EnvelopeErrorKind::InvalidParams,
            "params {params}"
        );
    }
}

#[test]
fn request_carrying_result_or_error_is_refused() {
    assert_eq!(
        refused(json!({"jsonrpc": "2.0", "id": 1, "method": "m", "result": 1})),
        EnvelopeErrorKind::UnexpectedResponseMember
    );
    assert_eq!(
        refused(json!({"jsonrpc": "2.0", "method": "m", "error": {"code": 1, "message": "x"}})),
        EnvelopeErrorKind::UnexpectedResponseMember
    );
}

#[test]
fn malformed_error_object_is_refused() {
    for error in [
        json!({"message": "no code"}),
        json!({"code": 1}),
        json!({"code": "1", "message": "x"}),
        json!({"code": 1.5, "message": "x"}),
        json!("text"),
        json!(null),
    ] {
        assert!(
            matches!(
                refused(json!({"jsonrpc": "2.0", "id": 1, "error": error})),
                EnvelopeErrorKind::InvalidErrorObject(_)
            ),
            "error {error}"
        );
    }
}

#[test]
fn non_object_is_refused() {
    for value in [json!("x"), json!(1), json!(null), json!(true)] {
        assert_eq!(refused(value), EnvelopeErrorKind::NotAnObject);
    }
}

#[test]
fn batch_is_refused_with_one_null_id_error() {
    for value in [
        json!([]),
        json!([{"jsonrpc": "2.0", "id": 1, "method": "m"}]),
    ] {
        let err = refused_full(value);
        assert_eq!(err.kind(), &EnvelopeErrorKind::Batch);
        assert_eq!(err.code(), codes::INVALID_REQUEST);
        let resp = err.to_response().expect("a batch gets one error reply");
        assert_eq!(resp.id(), None);
        assert_eq!(resp.outcome().unwrap_err().code(), codes::INVALID_REQUEST);
    }
}

#[test]
fn reserved_method_prefix_is_refused() {
    let err = refused_full(json!({"jsonrpc": "2.0", "id": 7, "method": "rpc.discover"}));
    assert_eq!(
        err.kind(),
        &EnvelopeErrorKind::ReservedMethod("rpc.discover".into())
    );
    assert_eq!(err.code(), codes::INVALID_REQUEST);
    assert_eq!(err.id(), Some(&Id::Number(7)));
    let resp = err.to_response().unwrap();
    assert_eq!(resp.id(), Some(&Id::Number(7)));
    // A notification never gets a reply, also when its method is reserved.
    let note = refused_full(json!({"jsonrpc": "2.0", "method": "rpc.x"}));
    assert!(matches!(note.kind(), EnvelopeErrorKind::ReservedMethod(_)));
    assert!(!note.expects_reply());
    assert_eq!(note.to_response(), None);
    // Also when the rest of the notification is invalid.
    for value in [
        json!({"jsonrpc": "2.0", "method": "rpc.x", "params": 1}),
        json!({"jsonrpc": "2.0", "method": "rpc.x", "params": "p"}),
    ] {
        let err = refused_full(value.clone());
        assert!(
            matches!(err.kind(), EnvelopeErrorKind::ReservedMethod(_)),
            "{value}"
        );
        assert_eq!(err.to_response(), None, "{value}");
    }
    // A reserved request with a bad id is still refused for its method,
    // with a null id.
    let bad_id = refused_full(json!({"jsonrpc": "2.0", "id": 1.5, "method": "rpc.x"}));
    assert!(matches!(
        bad_id.kind(),
        EnvelopeErrorKind::ReservedMethod(_)
    ));
    assert_eq!(bad_id.to_response().unwrap().id(), None);
    // Only the exact prefix is reserved.
    assert!(parse(json!({"jsonrpc": "2.0", "id": 1, "method": "rpc"})).is_ok());
    assert!(parse(json!({"jsonrpc": "2.0", "id": 1, "method": "rpcx.y"})).is_ok());
    assert!(parse(json!({"jsonrpc": "2.0", "id": 1, "method": "RPC.x"})).is_ok());
}

#[test]
fn invalid_json_text_is_a_parse_error() {
    let err = Message::from_json_str("{not json").unwrap_err();
    assert!(matches!(err.kind(), EnvelopeErrorKind::Parse(_)));
    assert_eq!(err.code(), codes::PARSE_ERROR);
    assert_eq!(err.id(), None);
    let resp = err.to_response().unwrap();
    assert_eq!(resp.id(), None);
    assert_eq!(resp.outcome().unwrap_err().code(), codes::PARSE_ERROR);
    // Trailing text after one value is a parse error too.
    let trailing = Message::from_json_str(r#"{"jsonrpc":"2.0","method":"m"} {}"#).unwrap_err();
    assert!(matches!(trailing.kind(), EnvelopeErrorKind::Parse(_)));
}

#[test]
fn duplicate_member_names_are_a_parse_error() {
    for text in [
        r#"{"jsonrpc":"2.0","id":1,"id":2,"method":"m"}"#,
        r#"{"jsonrpc":"2.0","id":1,"method":"m","params":{"a":1,"a":1}}"#,
        r#"{"jsonrpc":"2.0","id":1,"method":"m","params":[{"x":{"b":1,"b":2}}]}"#,
    ] {
        let err = Message::from_json_str(text).unwrap_err();
        assert!(
            matches!(err.kind(), EnvelopeErrorKind::DuplicateMember(_)),
            "{text}: {err:?}"
        );
        assert_eq!(err.code(), codes::PARSE_ERROR);
        assert_eq!(err.to_response().unwrap().id(), None);
        // The serde path refuses it too.
        assert!(serde_json::from_str::<Message>(text).is_err(), "{text}");
    }
    // Equal names in different objects are fine.
    let ok = r#"{"jsonrpc":"2.0","id":1,"method":"m","params":{"a":{"a":1}}}"#;
    assert!(Message::from_json_str(ok).is_ok());
}

#[test]
fn strict_text_reader_keeps_every_json_type() {
    let text = r#"{"jsonrpc":"2.0","id":1,"method":"m","params":[null,true,false,-3,18446744073709551615,1.5,"s",[],{}]}"#;
    let Message::Request(req) = Message::from_json_str(text).unwrap() else {
        panic!("not a request")
    };
    assert_eq!(
        Value::from(req.params().unwrap()),
        json!([
            null,
            true,
            false,
            -3,
            18446744073709551615u64,
            1.5,
            "s",
            [],
            {}
        ])
    );
}

#[test]
fn error_reply_echoes_a_readable_request_id() {
    for (value, want) in [
        (
            json!({"jsonrpc": "1.0", "id": 1, "method": "m"}),
            Some(Id::Number(1)),
        ),
        (
            json!({"jsonrpc": "2.0", "id": "s", "method": 5}),
            Some(Id::String("s".into())),
        ),
        (
            json!({"jsonrpc": "2.0", "id": 2, "method": "m", "params": 1}),
            Some(Id::Number(2)),
        ),
        (json!({"jsonrpc": "2.0", "method": 5}), None),
        (json!({"jsonrpc": "2.0", "id": null, "method": "m"}), None),
        (json!({"jsonrpc": "2.0", "id": 1.5, "method": "m"}), None),
        (json!({"jsonrpc": "2.0", "id": true, "method": "m"}), None),
        // Ids above i64::MAX are not supported: unreadable.
        (
            json!({"jsonrpc": "2.0", "id": 9223372036854775808u64, "method": "m"}),
            None,
        ),
        (json!({}), None),
        (json!({"jsonrpc": "2.0", "id": 4}), Some(Id::Number(4))),
    ] {
        let err = refused_full(value.clone());
        assert!(err.expects_reply(), "{value}");
        assert_eq!(err.id(), want.as_ref(), "{value}");
        let resp = err.to_response().unwrap();
        assert_eq!(resp.id(), want.as_ref(), "{value}");
        assert_eq!(resp.outcome().unwrap_err().code(), err.code(), "{value}");
        assert_eq!(
            resp.outcome().unwrap_err().message(),
            err.to_string(),
            "{value}"
        );
    }
}

#[test]
fn invalid_responses_get_no_reply() {
    for value in [
        json!({"jsonrpc": "2.0", "id": 1, "result": 1, "error": {"code": 1, "message": "x"}}),
        json!({"jsonrpc": "2.0", "result": 1}),
        json!({"jsonrpc": "1.0", "id": 1, "result": 1}),
        json!({"jsonrpc": "2.0", "id": null, "result": 1}),
        json!({"jsonrpc": "2.0", "id": 1, "error": null}),
    ] {
        let err = refused_full(value.clone());
        assert!(!err.expects_reply(), "{value}");
        assert_eq!(err.to_response(), None, "{value}");
    }
}

#[test]
fn large_ids_are_refused() {
    assert_eq!(
        refused(json!({"jsonrpc": "2.0", "id": 9223372036854775808u64, "method": "m"})),
        EnvelopeErrorKind::InvalidId
    );
    let max = parse(json!({"jsonrpc": "2.0", "id": i64::MAX, "method": "m"})).unwrap();
    let Message::Request(req) = max else {
        panic!("not a request")
    };
    assert_eq!(req.id(), &Id::Number(i64::MAX));
}

#[test]
fn envelope_errors_map_to_standard_codes() {
    let kinds = [
        EnvelopeErrorKind::Parse("x".into()),
        EnvelopeErrorKind::DuplicateMember("x".into()),
        EnvelopeErrorKind::Batch,
        EnvelopeErrorKind::NotAnObject,
        EnvelopeErrorKind::BadVersion,
        EnvelopeErrorKind::InvalidMethod,
        EnvelopeErrorKind::ReservedMethod("rpc.x".into()),
        EnvelopeErrorKind::InvalidId,
        EnvelopeErrorKind::InvalidParams,
        EnvelopeErrorKind::UnexpectedResponseMember,
        EnvelopeErrorKind::ResultAndError,
        EnvelopeErrorKind::NoResultOrError,
        EnvelopeErrorKind::MissingResponseId,
        EnvelopeErrorKind::NullIdOnSuccess,
        EnvelopeErrorKind::InvalidErrorObject("x".into()),
    ];
    let mut texts = std::collections::BTreeSet::new();
    for kind in &kinds {
        let want = match kind {
            EnvelopeErrorKind::Parse(_) | EnvelopeErrorKind::DuplicateMember(_) => {
                codes::PARSE_ERROR
            }
            _ => codes::INVALID_REQUEST,
        };
        assert_eq!(kind.code(), want, "{kind:?}");
        assert!(
            texts.insert(kind.to_string()),
            "duplicate text for {kind:?}"
        );
    }
    let err = refused_full(json!({"jsonrpc": "2.0", "id": 1, "method": 5}));
    assert_eq!(err.to_string(), err.kind().to_string());
    assert!(std::error::Error::source(&err).is_none());
}

// --- accessors ----------------------------------------------------------------

#[test]
fn request_and_notification_accessors() {
    let req = Request::new(Id::Number(1), "m", Some(Params::Array(vec![json!(1)])));
    assert_eq!(req.id(), &Id::Number(1));
    assert_eq!(req.method(), "m");
    assert_eq!(req.params(), Some(&Params::Array(vec![json!(1)])));
    assert_eq!(req.into_params(), Some(Params::Array(vec![json!(1)])));
    let note = Notification::new("n", None);
    assert_eq!(note.method(), "n");
    assert_eq!(note.params(), None);
    let note = Notification::new("n", Some(Params::Array(vec![])));
    assert_eq!(note.params(), Some(&Params::Array(vec![])));
    assert_eq!(note.into_params(), Some(Params::Array(vec![])));
}

#[test]
fn error_object_accessors() {
    let err = ErrorObject::new(5, "five");
    assert_eq!(err.code(), 5);
    assert_eq!(err.message(), "five");
    assert_eq!(err.data(), None);
    let err = err.with_data(json!([1]));
    assert_eq!(err.data(), Some(&json!([1])));
}

// --- wire output ------------------------------------------------------------

#[test]
fn request_serializes_to_spec_shape() {
    let msg = Message::Request(Request::new(Id::Number(3), "m", None));
    assert_eq!(
        msg.to_value(),
        json!({"jsonrpc": "2.0", "id": 3, "method": "m"})
    );
}

#[test]
fn responses_serialize_to_spec_shape() {
    let ok = Message::Response(Response::success(Id::String("a".into()), json!([1])));
    assert_eq!(
        ok.to_value(),
        json!({"jsonrpc": "2.0", "id": "a", "result": [1]})
    );

    let err = Message::Response(Response::error(
        None,
        ErrorObject::new(codes::INTERNAL_ERROR, "boom").with_data(json!({"k": 1})),
    ));
    assert_eq!(
        err.to_value(),
        json!({
            "jsonrpc": "2.0", "id": null,
            "error": {"code": -32603, "message": "boom", "data": {"k": 1}}
        })
    );
    let plain = Message::Response(Response::error(
        Some(Id::Number(1)),
        ErrorObject::new(1, "x"),
    ));
    assert_eq!(
        plain.to_value(),
        json!({"jsonrpc": "2.0", "id": 1, "error": {"code": 1, "message": "x"}})
    );
}

#[test]
fn serde_serialize_matches_to_value() {
    let msg = Message::Notification(Notification::new("n", Some(Params::Array(vec![]))));
    assert_eq!(serde_json::to_value(&msg).unwrap(), msg.to_value());
    assert_eq!(msg.to_json_string(), msg.to_value().to_string());
}

#[test]
fn standard_codes_have_spec_values() {
    assert_eq!(codes::PARSE_ERROR, -32700);
    assert_eq!(codes::INVALID_REQUEST, -32600);
    assert_eq!(codes::METHOD_NOT_FOUND, -32601);
    assert_eq!(codes::INVALID_PARAMS, -32602);
    assert_eq!(codes::INTERNAL_ERROR, -32603);
    assert_eq!(codes::SERVER_ERROR_MIN, -32099);
    assert_eq!(codes::SERVER_ERROR_MAX, -32000);
}

#[test]
fn params_convert_to_json_values() {
    assert_eq!(Value::from(Params::Array(vec![json!(1)])), json!([1]));
    let map = json!({"a": 1}).as_object().unwrap().clone();
    assert_eq!(Value::from(&Params::Object(map.clone())), json!({"a": 1}));
    assert_eq!(Value::from(Params::Object(map)), json!({"a": 1}));
}
