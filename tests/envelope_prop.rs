//! Property tests: every valid message survives a round trip through JSON
//! text, and parsing never panics on arbitrary JSON.

use damavand_proto::jsonrpc::Params;
use damavand_proto::{ErrorObject, Id, Message, Notification, Request, Response};
use proptest::prelude::*;
use serde_json::{Map, Value};

fn json_value() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(Value::from),
        any::<u64>().prop_map(Value::from),
        ".{0,8}".prop_map(Value::String),
    ];
    leaf.prop_recursive(3, 24, 4, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..4).prop_map(Value::Array),
            prop::collection::btree_map(".{0,6}", inner, 0..4)
                .prop_map(|m| Value::Object(m.into_iter().collect())),
        ]
    })
}

fn id() -> impl Strategy<Value = Id> {
    prop_oneof![
        any::<i64>().prop_map(Id::Number),
        ".{0,8}".prop_map(Id::String),
    ]
}

fn params() -> impl Strategy<Value = Option<Params>> {
    prop_oneof![
        Just(None),
        prop::collection::vec(json_value(), 0..4).prop_map(|v| Some(Params::Array(v))),
        prop::collection::btree_map(".{0,6}", json_value(), 0..4)
            .prop_map(|m| Some(Params::Object(m.into_iter().collect::<Map<_, _>>()))),
    ]
}

fn error_object() -> impl Strategy<Value = ErrorObject> {
    (
        any::<i64>(),
        ".{0,12}",
        prop_oneof![Just(None), json_value().prop_map(Some)],
    )
        .prop_map(|(code, message, data)| ErrorObject {
            code,
            message,
            data,
        })
}

fn message() -> impl Strategy<Value = Message> {
    prop_oneof![
        (id(), ".{0,12}", params()).prop_map(|(id, method, params)| Message::Request(Request {
            id,
            method,
            params
        })),
        (".{0,12}", params())
            .prop_map(|(method, params)| Message::Notification(Notification { method, params })),
        (id(), json_value())
            .prop_map(|(id, result)| Message::Response(Response::success(id, result))),
        (prop_oneof![Just(None), id().prop_map(Some)], error_object())
            .prop_map(|(id, err)| Message::Response(Response::error(id, err))),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn message_round_trips_through_text(msg in message()) {
        let text = msg.to_json_string();
        let back = Message::from_json_str(&text).expect("valid message must parse");
        prop_assert_eq!(back, msg);
    }

    #[test]
    fn message_round_trips_through_serde(msg in message()) {
        let value = serde_json::to_value(&msg).unwrap();
        let back: Message = serde_json::from_value(value).unwrap();
        prop_assert_eq!(back, msg);
    }

    #[test]
    fn parsing_arbitrary_json_never_panics(value in json_value()) {
        let _ = Message::from_value(value);
    }

    #[test]
    fn parsing_arbitrary_objects_with_jsonrpc_never_panics(
        id in prop_oneof![Just(None), json_value().prop_map(Some)],
        method in prop_oneof![Just(None), json_value().prop_map(Some)],
        result in prop_oneof![Just(None), json_value().prop_map(Some)],
        error in prop_oneof![Just(None), json_value().prop_map(Some)],
    ) {
        let mut obj = Map::new();
        obj.insert("jsonrpc".into(), Value::from("2.0"));
        for (key, v) in [("id", id), ("method", method), ("result", result), ("error", error)] {
            if let Some(v) = v {
                obj.insert(key.into(), v);
            }
        }
        if let Ok(msg) = Message::from_value(Value::Object(obj)) {
            // Anything accepted must also round trip.
            let back = Message::from_json_str(&msg.to_json_string()).unwrap();
            prop_assert_eq!(back, msg);
        }
    }
}
