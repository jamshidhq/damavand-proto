//! Capabilities: an object keyed by capability name.

use damavand_proto::capability::{CapabilityError, is_valid_name};
use damavand_proto::{Capabilities, Message};
use serde_json::{Map, Value, json};

fn obj(value: Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}

fn caps(value: Value) -> Capabilities {
    serde_json::from_value(value).unwrap()
}

#[test]
fn name_grammar() {
    for ok in ["a", "tools", "tools.call", "a0", "x-y_z.w", "z9-._"] {
        assert!(is_valid_name(ok), "refused {ok:?}");
    }
    for bad in [
        "",
        "Tools",
        "0a",
        ".a",
        "-a",
        "_a",
        "a b",
        "a/b",
        "tools.Call",
        "é",
        "a\n",
    ] {
        assert!(!is_valid_name(bad), "accepted {bad:?}");
    }
}

#[test]
fn basics() {
    let mut set = Capabilities::new();
    assert!(set.is_empty());
    assert_eq!(set.len(), 0);
    set.insert("tools", obj(json!({"listChanged": true})))
        .unwrap();
    set.insert("audit", Map::new()).unwrap();
    assert!(!set.is_empty());
    assert_eq!(set.len(), 2);
    assert!(set.contains("tools"));
    assert!(!set.contains("memory"));
    assert_eq!(set.get("tools"), Some(&obj(json!({"listChanged": true}))));
    assert_eq!(set.get("memory"), None);
    assert_eq!(set.names().collect::<Vec<_>>(), ["audit", "tools"]);
    let pairs: Vec<(&str, &Map<String, Value>)> = set.iter().collect();
    assert_eq!(pairs.len(), 2);
    assert_eq!(pairs[0].0, "audit");
    assert_eq!(pairs[1].1, &obj(json!({"listChanged": true})));
}

#[test]
fn insert_refuses_bad_names_and_duplicates() {
    let mut set = Capabilities::new();
    assert_eq!(
        set.insert("Tools", Map::new()),
        Err(CapabilityError::InvalidName("Tools".into()))
    );
    assert_eq!(
        set.insert("", Map::new()),
        Err(CapabilityError::InvalidName(String::new()))
    );
    set.insert("tools", Map::new()).unwrap();
    assert_eq!(
        set.insert("tools", obj(json!({"x": 1}))),
        Err(CapabilityError::Duplicate("tools".into()))
    );
    // The refused insert did not change the entry.
    assert_eq!(set.get("tools"), Some(&Map::new()));
    assert_eq!(set.len(), 1);
}

#[test]
fn wire_form_is_an_object_of_objects() {
    let value = json!({"tools": {"listChanged": true}, "audit": {}});
    let set = caps(value.clone());
    assert_eq!(serde_json::to_value(&set).unwrap(), value);
    assert_eq!(
        serde_json::to_value(Capabilities::new()).unwrap(),
        json!({})
    );
}

#[test]
fn wire_form_refuses_bad_shapes() {
    for bad in [
        json!(["tools"]),
        json!({"Tools": {}}),
        json!({"": {}}),
        json!({"tools": true}),
        json!({"tools": null}),
        json!({"tools": ["listChanged"]}),
        json!("tools"),
    ] {
        assert!(
            serde_json::from_value::<Capabilities>(bad.clone()).is_err(),
            "accepted {bad}"
        );
    }
}

#[test]
fn duplicate_names_in_text_are_refused() {
    // serde_json::Value keeps only the last of two equal keys, so this must be
    // caught while the text is read.
    let text = r#"{"tools": {}, "tools": {"listChanged": true}}"#;
    let err = serde_json::from_str::<Capabilities>(text).unwrap_err();
    assert!(err.to_string().contains("tools"), "{err}");
    // And on the message path, where the text is read into a JSON value first.
    let message = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"versions":["0.1"],"capabilities":{text}}}}}"#
    );
    assert!(Message::from_json_str(&message).is_err());
}

#[test]
fn intersection_keeps_names_in_both_and_the_left_parameters() {
    let server = caps(json!({"tools": {"listChanged": true}, "audit": {}, "memory": {}}));
    let client = caps(json!({"tools": {"listChanged": false}, "audit": {}, "x-future": {}}));
    assert_eq!(
        server.intersection(&client),
        caps(json!({"tools": {"listChanged": true}, "audit": {}}))
    );
    assert!(server.intersection(&Capabilities::new()).is_empty());
}

#[test]
fn capability_errors_display() {
    for err in [
        CapabilityError::InvalidName("Tools".into()),
        CapabilityError::Duplicate("tools".into()),
        CapabilityError::ParamsNotObject("tools".into()),
    ] {
        let text = err.to_string();
        assert!(text.contains("ools"), "{text}");
    }
}

#[test]
fn wrong_type_error_names_the_expected_shape() {
    let err = serde_json::from_value::<Capabilities>(json!(["tools"])).unwrap_err();
    assert!(err.to_string().contains("capability"), "{err}");
}
