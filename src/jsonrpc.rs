//! JSON-RPC 2.0 envelope types.
//!
//! [`Message`] is one JSON-RPC message: a [`Request`], a [`Notification`] or a
//! [`Response`]. Parsing is strict. It refuses a message that the JSON-RPC 2.0
//! specification does not allow, and some that it allows but this protocol
//! does not (see `SPEC.md`), with an [`EnvelopeError`] that maps to the
//! standard error code and says whether, and with which id, to reply.
//!
//! Batches are not supported: a JSON array is refused with one error response
//! whose id is `null`.
//!
//! # Public dependency
//!
//! `serde_json` is a public dependency of this crate: [`Params`],
//! [`Response`] and [`ErrorObject`] hold [`serde_json::Value`] and
//! [`serde_json::Map`]. A semver-incompatible `serde_json` release would be a
//! breaking release of this crate.

use std::fmt;

use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Number, Value};

/// The value of the `jsonrpc` member in every message.
pub const JSONRPC_VERSION: &str = "2.0";

/// Method names with this prefix are reserved by JSON-RPC 2.0 for
/// rpc-internal methods. This protocol defines none and refuses them.
pub const RESERVED_METHOD_PREFIX: &str = "rpc.";

/// Error codes: the JSON-RPC 2.0 codes, this protocol's codes and the
/// registry of code blocks for the damavand crates.
///
/// The registry is also written down in `docs/error-codes.md`.
pub mod codes {
    /// Invalid JSON was received.
    pub const PARSE_ERROR: i64 = -32700;
    /// The JSON sent is not a valid request object.
    pub const INVALID_REQUEST: i64 = -32600;
    /// The method does not exist or is not available.
    pub const METHOD_NOT_FOUND: i64 = -32601;
    /// Invalid method parameters.
    pub const INVALID_PARAMS: i64 = -32602;
    /// Internal JSON-RPC error.
    pub const INTERNAL_ERROR: i64 = -32603;
    /// Lowest code of the range reserved for implementation-defined server errors.
    pub const SERVER_ERROR_MIN: i64 = -32099;
    /// Highest code of the range reserved for implementation-defined server errors.
    pub const SERVER_ERROR_MAX: i64 = -32000;
    /// Lowest code of the whole range that JSON-RPC 2.0 reserves.
    pub const JSONRPC_RESERVED_MIN: i64 = -32768;
    /// Highest code of the whole range that JSON-RPC 2.0 reserves.
    pub const JSONRPC_RESERVED_MAX: i64 = -32000;
    /// Lowest code reserved for this protocol (damavand-proto), inside the
    /// server-error range.
    pub const PROTO_MIN: i64 = -32019;
    /// Highest code reserved for this protocol (damavand-proto).
    pub const PROTO_MAX: i64 = -32000;
    /// The peers support no common protocol version.
    pub const INCOMPATIBLE_VERSION: i64 = -32001;

    /// The code block of each damavand crate. Each block is 1000 codes wide
    /// and lies outside the range that JSON-RPC 2.0 reserves.
    pub mod blocks {
        use std::ops::RangeInclusive;

        /// damavand-policy.
        pub const POLICY: RangeInclusive<i64> = 1000..=1999;
        /// damavand-manifest.
        pub const MANIFEST: RangeInclusive<i64> = 2000..=2999;
        /// damavand-audit.
        pub const AUDIT: RangeInclusive<i64> = 3000..=3999;
        /// damavand-redact.
        pub const REDACT: RangeInclusive<i64> = 4000..=4999;
        /// damavand-tools.
        pub const TOOLS: RangeInclusive<i64> = 5000..=5999;
        /// damavand-memory.
        pub const MEMORY: RangeInclusive<i64> = 6000..=6999;
        /// damavand-eval.
        pub const EVAL: RangeInclusive<i64> = 7000..=7999;

        /// Every crate block, by crate name, in code order.
        pub const ALL: [(&str, RangeInclusive<i64>); 7] = [
            ("damavand-policy", POLICY),
            ("damavand-manifest", MANIFEST),
            ("damavand-audit", AUDIT),
            ("damavand-redact", REDACT),
            ("damavand-tools", TOOLS),
            ("damavand-memory", MEMORY),
            ("damavand-eval", EVAL),
        ];

        /// The crate that owns `code`: `damavand-proto` for its reserved
        /// range, the crate of a block, or `None`.
        pub fn owner(code: i64) -> Option<&'static str> {
            if (super::PROTO_MIN..=super::PROTO_MAX).contains(&code) {
                return Some("damavand-proto");
            }
            ALL.iter()
                .find(|(_, range)| range.contains(&code))
                .map(|(name, _)| *name)
        }
    }
}

/// A request id: a string or an integer.
///
/// JSON-RPC 2.0 also allows `null` and fractional numbers but discourages both.
/// This protocol refuses them in requests. A response carries `null` only when
/// the request id could not be read (see [`Response::id`]).
///
/// Integer ids are `i64`: an id above `i64::MAX` is not supported and is
/// refused as [`EnvelopeErrorKind::InvalidId`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Id {
    /// An integer id.
    Number(i64),
    /// A string id.
    String(String),
}

/// Request parameters: by position (array) or by name (object).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Params {
    /// Positional parameters.
    Array(Vec<Value>),
    /// Named parameters.
    Object(Map<String, Value>),
}

/// A request: a call that expects a [`Response`] with the same id.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    id: Id,
    method: String,
    params: Option<Params>,
}

/// A notification: a call with no id, which gets no response.
#[derive(Debug, Clone, PartialEq)]
pub struct Notification {
    method: String,
    params: Option<Params>,
}

/// The error member of an error response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorObject {
    code: i64,
    message: String,
    /// `Some(Value::Null)` is an explicit `"data": null`; `None` is no `data`
    /// member.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    data: Option<Value>,
}

/// A response: exactly one of a result or an error.
#[derive(Debug, Clone, PartialEq)]
pub struct Response {
    id: Option<Id>,
    outcome: Result<Value, ErrorObject>,
}

/// One JSON-RPC 2.0 message.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Message {
    /// A request.
    Request(Request),
    /// A notification.
    Notification(Notification),
    /// A response.
    Response(Response),
}

/// What is wrong with a message that is not a valid JSON-RPC 2.0 message.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EnvelopeErrorKind {
    /// The input is not valid JSON.
    Parse(String),
    /// An object in the input has two members with the same name.
    DuplicateMember(String),
    /// The message is a JSON array: batches are not supported.
    Batch,
    /// The message is not a JSON object.
    NotAnObject,
    /// The `jsonrpc` member is missing or is not `"2.0"`.
    BadVersion,
    /// The `method` member is not a string.
    InvalidMethod,
    /// The method name starts with the reserved prefix `rpc.`.
    ReservedMethod(String),
    /// The `id` member is not a string or an `i64` integer (or is `null` where
    /// that is not allowed).
    InvalidId,
    /// The `params` member is not an array or an object.
    InvalidParams,
    /// A request or notification also has a `result` or `error` member.
    UnexpectedResponseMember,
    /// A response has both `result` and `error`.
    ResultAndError,
    /// A message has no `method`, no `result` and no `error`.
    NoResultOrError,
    /// A response has no `id` member.
    MissingResponseId,
    /// A success response has a `null` id.
    NullIdOnSuccess,
    /// The `error` member is not a valid error object.
    InvalidErrorObject(String),
}

/// Why a message was refused, and how to answer it.
///
/// [`EnvelopeError::to_response`] gives the error response to send, or `None`
/// when the protocol forbids a reply: the message looked like a response, or
/// it is a notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvelopeError {
    kind: EnvelopeErrorKind,
    id: Option<Id>,
    reply: bool,
}

impl EnvelopeErrorKind {
    /// The JSON-RPC error code to report for this error.
    pub fn code(&self) -> i64 {
        match self {
            EnvelopeErrorKind::Parse(_) | EnvelopeErrorKind::DuplicateMember(_) => {
                codes::PARSE_ERROR
            }
            _ => codes::INVALID_REQUEST,
        }
    }
}

impl fmt::Display for EnvelopeErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EnvelopeErrorKind::Parse(detail) => write!(f, "parse error: {detail}"),
            EnvelopeErrorKind::DuplicateMember(detail) => {
                write!(f, "duplicate member name: {detail}")
            }
            EnvelopeErrorKind::Batch => f.write_str("batches are not supported"),
            EnvelopeErrorKind::NotAnObject => f.write_str("message is not a JSON object"),
            EnvelopeErrorKind::BadVersion => {
                f.write_str("member `jsonrpc` is missing or is not \"2.0\"")
            }
            EnvelopeErrorKind::InvalidMethod => f.write_str("member `method` is not a string"),
            EnvelopeErrorKind::ReservedMethod(method) => {
                write!(
                    f,
                    "method name {method:?} uses the reserved prefix \"rpc.\""
                )
            }
            EnvelopeErrorKind::InvalidId => {
                f.write_str("member `id` is not a string or a 64-bit signed integer")
            }
            EnvelopeErrorKind::InvalidParams => {
                f.write_str("member `params` is not an array or an object")
            }
            EnvelopeErrorKind::UnexpectedResponseMember => {
                f.write_str("request or notification has a `result` or `error` member")
            }
            EnvelopeErrorKind::ResultAndError => {
                f.write_str("response has both `result` and `error`")
            }
            EnvelopeErrorKind::NoResultOrError => {
                f.write_str("message has no `method`, `result` or `error` member")
            }
            EnvelopeErrorKind::MissingResponseId => f.write_str("response has no `id` member"),
            EnvelopeErrorKind::NullIdOnSuccess => f.write_str("success response has a null `id`"),
            EnvelopeErrorKind::InvalidErrorObject(detail) => {
                write!(f, "invalid error object: {detail}")
            }
        }
    }
}

impl EnvelopeError {
    /// An error with no readable id that expects a reply.
    fn unreadable(kind: EnvelopeErrorKind) -> Self {
        Self {
            kind,
            id: None,
            reply: true,
        }
    }

    /// What is wrong with the message.
    pub fn kind(&self) -> &EnvelopeErrorKind {
        &self.kind
    }

    /// The request id, when the message had one that could be read.
    pub fn id(&self) -> Option<&Id> {
        self.id.as_ref()
    }

    /// The JSON-RPC error code to report for this error.
    pub fn code(&self) -> i64 {
        self.kind.code()
    }

    /// Whether the receiver must reply. It must not reply to a message that
    /// looked like a response, nor to a notification.
    pub fn expects_reply(&self) -> bool {
        self.reply
    }

    /// The error response to send, with the request id when it was readable
    /// and `null` otherwise. `None` when no reply is allowed.
    pub fn to_response(&self) -> Option<Response> {
        self.reply.then(|| {
            Response::error(
                self.id.clone(),
                ErrorObject::new(self.code(), self.kind.to_string()),
            )
        })
    }
}

impl fmt::Display for EnvelopeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(f)
    }
}

impl std::error::Error for EnvelopeError {}

impl Request {
    /// A request. The method should not start with
    /// [`RESERVED_METHOD_PREFIX`]: a peer refuses such a request.
    pub fn new(id: Id, method: impl Into<String>, params: Option<Params>) -> Self {
        Self {
            id,
            method: method.into(),
            params,
        }
    }

    /// The request id.
    pub fn id(&self) -> &Id {
        &self.id
    }

    /// The method name.
    pub fn method(&self) -> &str {
        &self.method
    }

    /// The parameters, if any.
    pub fn params(&self) -> Option<&Params> {
        self.params.as_ref()
    }

    /// Consumes the request and returns its parameters.
    pub fn into_params(self) -> Option<Params> {
        self.params
    }
}

impl Notification {
    /// A notification. The method should not start with
    /// [`RESERVED_METHOD_PREFIX`].
    pub fn new(method: impl Into<String>, params: Option<Params>) -> Self {
        Self {
            method: method.into(),
            params,
        }
    }

    /// The method name.
    pub fn method(&self) -> &str {
        &self.method
    }

    /// The parameters, if any.
    pub fn params(&self) -> Option<&Params> {
        self.params.as_ref()
    }

    /// Consumes the notification and returns its parameters.
    pub fn into_params(self) -> Option<Params> {
        self.params
    }
}

impl ErrorObject {
    /// An error object with no `data`.
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    /// Sets the `data` member.
    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }

    /// The error code. See [`codes`].
    pub fn code(&self) -> i64 {
        self.code
    }

    /// A short description of the error.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Extra information. `Some(&Value::Null)` is an explicit `"data": null`.
    pub fn data(&self) -> Option<&Value> {
        self.data.as_ref()
    }

    /// The error object as a JSON object. This cannot fail, so an error
    /// response never carries `"error": null`.
    fn to_value(&self) -> Value {
        let mut obj = Map::new();
        obj.insert("code".into(), Value::from(self.code));
        obj.insert("message".into(), Value::from(self.message.as_str()));
        if let Some(data) = &self.data {
            obj.insert("data".into(), data.clone());
        }
        Value::Object(obj)
    }
}

impl Response {
    /// A success response.
    pub fn success(id: Id, result: Value) -> Self {
        Self {
            id: Some(id),
            outcome: Ok(result),
        }
    }

    /// An error response. Use `None` for the id only when the request id could
    /// not be read.
    pub fn error(id: Option<Id>, error: ErrorObject) -> Self {
        Self {
            id,
            outcome: Err(error),
        }
    }

    /// The id of the request this answers. `None` is a `null` id.
    pub fn id(&self) -> Option<&Id> {
        self.id.as_ref()
    }

    /// The result or the error.
    pub fn outcome(&self) -> Result<&Value, &ErrorObject> {
        self.outcome.as_ref()
    }

    /// Consumes the response and returns the result or the error.
    pub fn into_outcome(self) -> Result<Value, ErrorObject> {
        self.outcome
    }
}

impl Message {
    /// Parses one message from JSON text.
    ///
    /// Stricter than `serde_json::from_str::<Value>`: an object with two
    /// members of the same name, anywhere in the text, is refused as
    /// [`EnvelopeErrorKind::DuplicateMember`].
    pub fn from_json_str(text: &str) -> Result<Self, EnvelopeError> {
        let value = parse_strict(text).map_err(EnvelopeError::unreadable)?;
        Self::from_value(value)
    }

    /// Validates one message from a JSON value.
    pub fn from_value(value: Value) -> Result<Self, EnvelopeError> {
        let mut obj = match value {
            Value::Object(obj) => obj,
            Value::Array(_) => return Err(EnvelopeError::unreadable(EnvelopeErrorKind::Batch)),
            _ => return Err(EnvelopeError::unreadable(EnvelopeErrorKind::NotAnObject)),
        };
        // How to answer, worked out before validation so that every refusal
        // can use it: no reply to a response-shaped message, the request id
        // when it can be read.
        let response_shaped = !obj.contains_key("method")
            && (obj.contains_key("result") || obj.contains_key("error"));
        let readable_id = obj.get("id").cloned().and_then(|id| Id::try_from(id).ok());
        let has_id = obj.contains_key("id");
        let refuse = |kind: EnvelopeErrorKind| EnvelopeError {
            reply: !response_shaped,
            id: readable_id.clone(),
            kind,
        };

        if obj.get("jsonrpc").and_then(Value::as_str) != Some(JSONRPC_VERSION) {
            return Err(refuse(EnvelopeErrorKind::BadVersion));
        }
        let id = obj.remove("id");
        let result = obj.remove("result");
        let error = obj.remove("error");

        if let Some(method) = obj.remove("method") {
            let Value::String(method) = method else {
                return Err(refuse(EnvelopeErrorKind::InvalidMethod));
            };
            if result.is_some() || error.is_some() {
                return Err(refuse(EnvelopeErrorKind::UnexpectedResponseMember));
            }
            if method.starts_with(RESERVED_METHOD_PREFIX) {
                // A notification gets no reply, also when it is refused and
                // also when the rest of it is invalid.
                let mut err = refuse(EnvelopeErrorKind::ReservedMethod(method));
                err.reply = has_id;
                return Err(err);
            }
            let params = obj
                .remove("params")
                .map(Params::try_from)
                .transpose()
                .map_err(refuse)?;
            let id = id.map(Id::try_from).transpose().map_err(refuse)?;
            return Ok(match id {
                None => Message::Notification(Notification { method, params }),
                Some(id) => Message::Request(Request { id, method, params }),
            });
        }

        let outcome = match (result, error) {
            (Some(_), Some(_)) => return Err(refuse(EnvelopeErrorKind::ResultAndError)),
            (None, None) => return Err(refuse(EnvelopeErrorKind::NoResultOrError)),
            (Some(result), None) => Ok(result),
            (None, Some(error)) => Err(serde_json::from_value::<ErrorObject>(error)
                .map_err(|err| refuse(EnvelopeErrorKind::InvalidErrorObject(err.to_string())))?),
        };
        let id = match id {
            None => return Err(refuse(EnvelopeErrorKind::MissingResponseId)),
            Some(Value::Null) => None,
            Some(id) => Some(Id::try_from(id).map_err(refuse)?),
        };
        if id.is_none() && outcome.is_ok() {
            return Err(refuse(EnvelopeErrorKind::NullIdOnSuccess));
        }
        Ok(Message::Response(Response { id, outcome }))
    }

    /// The message as a JSON value.
    pub fn to_value(&self) -> Value {
        let mut obj = Map::new();
        obj.insert("jsonrpc".into(), Value::from(JSONRPC_VERSION));
        match self {
            Message::Request(Request { id, method, params }) => {
                obj.insert("id".into(), id.into());
                obj.insert("method".into(), Value::from(method.as_str()));
                if let Some(params) = params {
                    obj.insert("params".into(), params.into());
                }
            }
            Message::Notification(Notification { method, params }) => {
                obj.insert("method".into(), Value::from(method.as_str()));
                if let Some(params) = params {
                    obj.insert("params".into(), params.into());
                }
            }
            Message::Response(Response { id, outcome }) => {
                obj.insert("id".into(), id.as_ref().map_or(Value::Null, Value::from));
                match outcome {
                    Ok(result) => obj.insert("result".into(), result.clone()),
                    Err(error) => obj.insert("error".into(), error.to_value()),
                };
            }
        }
        Value::Object(obj)
    }

    /// The message as JSON text.
    pub fn to_json_string(&self) -> String {
        self.to_value().to_string()
    }
}

impl TryFrom<Value> for Id {
    type Error = EnvelopeErrorKind;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::String(text) => Ok(Id::String(text)),
            Value::Number(n) => n
                .as_i64()
                .map(Id::Number)
                .ok_or(EnvelopeErrorKind::InvalidId),
            _ => Err(EnvelopeErrorKind::InvalidId),
        }
    }
}

impl From<&Id> for Value {
    fn from(id: &Id) -> Self {
        match id {
            Id::Number(n) => Value::from(*n),
            Id::String(text) => Value::from(text.as_str()),
        }
    }
}

impl TryFrom<Value> for Params {
    type Error = EnvelopeErrorKind;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::Array(items) => Ok(Params::Array(items)),
            Value::Object(members) => Ok(Params::Object(members)),
            _ => Err(EnvelopeErrorKind::InvalidParams),
        }
    }
}

impl From<&Params> for Value {
    fn from(params: &Params) -> Self {
        match params {
            Params::Array(items) => Value::Array(items.clone()),
            Params::Object(members) => Value::Object(members.clone()),
        }
    }
}

impl From<Params> for Value {
    fn from(params: Params) -> Self {
        match params {
            Params::Array(items) => Value::Array(items),
            Params::Object(members) => Value::Object(members),
        }
    }
}

impl Serialize for Message {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_value().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Message {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = StrictValue.deserialize(deserializer)?;
        Message::from_value(value).map_err(de::Error::custom)
    }
}

/// Deserializes a member that is present, keeping an explicit `null` as
/// `Some(Value::Null)`. With `#[serde(default)]`, a missing member is `None`.
fn present<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}

/// Reads JSON text into a [`Value`], refusing duplicate member names and
/// trailing text.
fn parse_strict(text: &str) -> Result<Value, EnvelopeErrorKind> {
    let mut deserializer = serde_json::Deserializer::from_str(text);
    StrictValue
        .deserialize(&mut deserializer)
        .and_then(|value| deserializer.end().map(|()| value))
        .map_err(|err| {
            if err.is_data() {
                // The only data error StrictValue raises is a duplicate name.
                EnvelopeErrorKind::DuplicateMember(err.to_string())
            } else {
                EnvelopeErrorKind::Parse(err.to_string())
            }
        })
}

/// A [`Value`] deserializer that refuses an object with two members of the
/// same name. `serde_json::Value` keeps the last one silently.
#[derive(Clone, Copy)]
struct StrictValue;

impl<'de> DeserializeSeed<'de> for StrictValue {
    type Value = Value;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for StrictValue {
    type Value = Value;

    // Unreachable through serde_json: `deserialize_any` on JSON text always
    // finds a value type, so this message is never shown.
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("any JSON value")
    }

    // `Value::default()` is also `Value::Null`.
    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_bool<E>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }

    fn visit_i64<E>(self, v: i64) -> Result<Value, E> {
        Ok(Value::from(v))
    }

    fn visit_u64<E>(self, v: u64) -> Result<Value, E> {
        Ok(Value::from(v))
    }

    fn visit_f64<E>(self, v: f64) -> Result<Value, E> {
        // JSON text has no NaN or infinity, so `from_f64` always succeeds here.
        Ok(Number::from_f64(v).map_or(Value::Null, Value::Number))
    }

    fn visit_str<E>(self, v: &str) -> Result<Value, E> {
        Ok(Value::from(v))
    }

    fn visit_string<E>(self, v: String) -> Result<Value, E> {
        Ok(Value::String(v))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element_seed(StrictValue)? {
            items.push(item);
        }
        Ok(Value::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut members = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if members.contains_key(&key) {
                return Err(de::Error::custom(format_args!("{key:?}")));
            }
            let value = map.next_value_seed(StrictValue)?;
            members.insert(key, value);
        }
        Ok(Value::Object(members))
    }
}
