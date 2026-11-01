//! JSON-RPC 2.0 envelope: accepted shapes, refused shapes, wire output.

use damavand_proto::jsonrpc::{Params, codes};
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
    assert_eq!(req.id, Id::Number(1));
    assert_eq!(req.method, "m");
    let Some(Params::Object(map)) = req.params else {
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
        Message::Request(Request {
            id: Id::String("abc".into()),
            method: "m".into(),
            params: Some(Params::Array(vec![json!(1), json!(2)])),
        })
    );
}

#[test]
fn notification_has_no_id() {
    let msg = parse(json!({"jsonrpc": "2.0", "method": "note"})).unwrap();
    assert_eq!(
        msg,
        Message::Notification(Notification {
            method: "note".into(),
            params: None
        })
    );
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
    assert_eq!(err.code, codes::PARSE_ERROR);
    assert_eq!(err.data, None);
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
    assert_eq!(resp.outcome().unwrap_err().data, Some(Value::Null));
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

fn refused(value: Value) -> EnvelopeError {
    match parse(value.clone()) {
        Ok(msg) => panic!("accepted invalid message {value}: {msg:?}"),
        Err(err) => err,
    }
}

#[test]
fn wrong_jsonrpc_version_is_refused() {
    assert_eq!(
        refused(json!({"jsonrpc": "1.0", "id": 1, "method": "m"})),
        EnvelopeError::BadVersion
    );
    assert_eq!(
        refused(json!({"jsonrpc": 2.0, "id": 1, "method": "m"})),
        EnvelopeError::BadVersion
    );
    assert_eq!(
        refused(json!({"id": 1, "method": "m"})),
        EnvelopeError::BadVersion
    );
}

#[test]
fn both_result_and_error_is_refused() {
    assert_eq!(
        refused(json!({
            "jsonrpc": "2.0", "id": 1, "result": 1,
            "error": {"code": 1, "message": "x"}
        })),
        EnvelopeError::ResultAndError
    );
}

#[test]
fn response_without_result_or_error_is_refused() {
    assert_eq!(
        refused(json!({"jsonrpc": "2.0", "id": 1})),
        EnvelopeError::NoResultOrError
    );
}

#[test]
fn response_without_id_is_refused() {
    assert_eq!(
        refused(json!({"jsonrpc": "2.0", "result": 1})),
        EnvelopeError::MissingResponseId
    );
    assert_eq!(
        refused(json!({"jsonrpc": "2.0", "error": {"code": 1, "message": "x"}})),
        EnvelopeError::MissingResponseId
    );
}

#[test]
fn success_response_with_null_id_is_refused() {
    assert_eq!(
        refused(json!({"jsonrpc": "2.0", "id": null, "result": 1})),
        EnvelopeError::NullIdOnSuccess
    );
}

#[test]
fn request_with_null_or_fractional_or_bool_id_is_refused() {
    for id in [json!(null), json!(1.5), json!(true), json!([1]), json!({})] {
        assert_eq!(
            refused(json!({"jsonrpc": "2.0", "id": id, "method": "m"})),
            EnvelopeError::InvalidId,
            "id {id}"
        );
    }
}

#[test]
fn response_with_bad_id_type_is_refused() {
    assert_eq!(
        refused(json!({"jsonrpc": "2.0", "id": true, "result": 1})),
        EnvelopeError::InvalidId
    );
}

#[test]
fn non_string_method_is_refused() {
    assert_eq!(
        refused(json!({"jsonrpc": "2.0", "id": 1, "method": 5})),
        EnvelopeError::InvalidMethod
    );
}

#[test]
fn scalar_params_are_refused() {
    for params in [json!(1), json!("x"), json!(null), json!(true)] {
        assert_eq!(
            refused(json!({"jsonrpc": "2.0", "method": "m", "params": params})),
            EnvelopeError::InvalidParams,
            "params {params}"
        );
    }
}

#[test]
fn request_carrying_result_or_error_is_refused() {
    assert_eq!(
        refused(json!({"jsonrpc": "2.0", "id": 1, "method": "m", "result": 1})),
        EnvelopeError::UnexpectedResponseMember
    );
    assert_eq!(
        refused(json!({"jsonrpc": "2.0", "method": "m", "error": {"code": 1, "message": "x"}})),
        EnvelopeError::UnexpectedResponseMember
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
                EnvelopeError::InvalidErrorObject(_)
            ),
            "error {error}"
        );
    }
}

#[test]
fn non_object_is_refused() {
    for value in [
        json!([]),
        json!([{"jsonrpc": "2.0", "method": "m"}]),
        json!("x"),
        json!(1),
    ] {
        assert_eq!(refused(value), EnvelopeError::NotAnObject);
    }
}

#[test]
fn invalid_json_text_is_a_parse_error() {
    let err = Message::from_json_str("{not json").unwrap_err();
    assert!(matches!(err, EnvelopeError::Parse(_)));
    assert_eq!(err.code(), codes::PARSE_ERROR);
}

#[test]
fn envelope_errors_map_to_standard_codes() {
    assert_eq!(EnvelopeError::BadVersion.code(), codes::INVALID_REQUEST);
    assert_eq!(EnvelopeError::ResultAndError.code(), codes::INVALID_REQUEST);
    assert_eq!(EnvelopeError::NotAnObject.code(), codes::INVALID_REQUEST);
    let resp = EnvelopeError::InvalidMethod.to_response();
    assert_eq!(resp.id(), None);
    assert_eq!(resp.outcome().unwrap_err().code, codes::INVALID_REQUEST);
    assert!(!EnvelopeError::InvalidMethod.to_string().is_empty());
}

// --- wire output ------------------------------------------------------------

#[test]
fn request_serializes_to_spec_shape() {
    let msg = Message::Request(Request {
        id: Id::Number(3),
        method: "m".into(),
        params: None,
    });
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
}

#[test]
fn serde_serialize_matches_to_value() {
    let msg = Message::Notification(Notification {
        method: "n".into(),
        params: Some(Params::Array(vec![])),
    });
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
