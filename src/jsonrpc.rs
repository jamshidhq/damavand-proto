//! JSON-RPC 2.0 envelope types.
//!
//! [`Message`] is one JSON-RPC message: a [`Request`], a [`Notification`] or a
//! [`Response`]. Parsing is strict. It refuses a message that the JSON-RPC 2.0
//! specification does not allow, with an [`EnvelopeError`] that maps to the
//! standard error code. Batches are not supported yet.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

/// The value of the `jsonrpc` member in every message.
pub const JSONRPC_VERSION: &str = "2.0";

/// Standard JSON-RPC 2.0 error codes, and the codes this protocol defines.
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
    /// The peers support no common protocol version (this protocol's code, in
    /// the server-error range).
    pub const INCOMPATIBLE_VERSION: i64 = -32001;
}

/// A request id: a string or an integer.
///
/// JSON-RPC 2.0 also allows `null` and fractional numbers but discourages both.
/// This protocol refuses them in requests. A response carries `null` only when
/// the request id could not be read (see [`Response::id`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Id {
    /// An integer id.
    Number(i64),
    /// A string id.
    String(String),
}

/// Request parameters: by position (array) or by name (object).
#[derive(Debug, Clone, PartialEq)]
pub enum Params {
    /// Positional parameters.
    Array(Vec<Value>),
    /// Named parameters.
    Object(Map<String, Value>),
}

/// A request: a call that expects a [`Response`] with the same id.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    /// The request id.
    pub id: Id,
    /// The method name.
    pub method: String,
    /// The parameters, if any.
    pub params: Option<Params>,
}

/// A notification: a call with no id, which gets no response.
#[derive(Debug, Clone, PartialEq)]
pub struct Notification {
    /// The method name.
    pub method: String,
    /// The parameters, if any.
    pub params: Option<Params>,
}

/// The error member of an error response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorObject {
    /// The error code. See [`codes`].
    pub code: i64,
    /// A short description of the error.
    pub message: String,
    /// Extra information. `Some(Value::Null)` is an explicit `"data": null`;
    /// `None` is no `data` member.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub data: Option<Value>,
}

/// A response: exactly one of a result or an error.
#[derive(Debug, Clone, PartialEq)]
pub struct Response {
    id: Option<Id>,
    outcome: Result<Value, ErrorObject>,
}

/// One JSON-RPC 2.0 message.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    /// A request.
    Request(Request),
    /// A notification.
    Notification(Notification),
    /// A response.
    Response(Response),
}

/// Why a JSON value is not a valid JSON-RPC 2.0 message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvelopeError {
    /// The input is not valid JSON.
    Parse(String),
    /// The message is not a JSON object (batches are not supported).
    NotAnObject,
    /// The `jsonrpc` member is missing or is not `"2.0"`.
    BadVersion,
    /// The `method` member is not a string.
    InvalidMethod,
    /// The `id` member is not a string or an integer (or is `null` where that
    /// is not allowed).
    InvalidId,
    /// The `params` member is not an array or an object.
    InvalidParams,
    /// A request or notification also has a `result` or `error` member.
    UnexpectedResponseMember,
    /// A response has both `result` and `error`.
    ResultAndError,
    /// A response has neither `result` nor `error`.
    NoResultOrError,
    /// A response has no `id` member.
    MissingResponseId,
    /// A success response has a `null` id.
    NullIdOnSuccess,
    /// The `error` member is not a valid error object.
    InvalidErrorObject(String),
}

impl EnvelopeError {
    /// The JSON-RPC error code to report for this error.
    pub fn code(&self) -> i64 {
        match self {
            EnvelopeError::Parse(_) => codes::PARSE_ERROR,
            _ => codes::INVALID_REQUEST,
        }
    }

    /// The error response a server sends for a message it cannot read. Its id
    /// is `null`.
    pub fn to_response(&self) -> Response {
        Response::error(None, ErrorObject::new(self.code(), self.to_string()))
    }
}

impl fmt::Display for EnvelopeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EnvelopeError::Parse(detail) => write!(f, "parse error: {detail}"),
            EnvelopeError::NotAnObject => f.write_str("message is not a JSON object"),
            EnvelopeError::BadVersion => {
                f.write_str("member `jsonrpc` is missing or is not \"2.0\"")
            }
            EnvelopeError::InvalidMethod => f.write_str("member `method` is not a string"),
            EnvelopeError::InvalidId => f.write_str("member `id` is not a string or an integer"),
            EnvelopeError::InvalidParams => {
                f.write_str("member `params` is not an array or an object")
            }
            EnvelopeError::UnexpectedResponseMember => {
                f.write_str("request or notification has a `result` or `error` member")
            }
            EnvelopeError::ResultAndError => f.write_str("response has both `result` and `error`"),
            EnvelopeError::NoResultOrError => {
                f.write_str("response has neither `result` nor `error`")
            }
            EnvelopeError::MissingResponseId => f.write_str("response has no `id` member"),
            EnvelopeError::NullIdOnSuccess => f.write_str("success response has a null `id`"),
            EnvelopeError::InvalidErrorObject(detail) => {
                write!(f, "invalid error object: {detail}")
            }
        }
    }
}

impl std::error::Error for EnvelopeError {}

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
    pub fn from_json_str(text: &str) -> Result<Self, EnvelopeError> {
        let value: Value =
            serde_json::from_str(text).map_err(|err| EnvelopeError::Parse(err.to_string()))?;
        Self::from_value(value)
    }

    /// Validates one message from a JSON value.
    pub fn from_value(value: Value) -> Result<Self, EnvelopeError> {
        let Value::Object(mut obj) = value else {
            return Err(EnvelopeError::NotAnObject);
        };
        if obj.get("jsonrpc").and_then(Value::as_str) != Some(JSONRPC_VERSION) {
            return Err(EnvelopeError::BadVersion);
        }
        let id = obj.remove("id");
        let result = obj.remove("result");
        let error = obj.remove("error");

        if let Some(method) = obj.remove("method") {
            let Value::String(method) = method else {
                return Err(EnvelopeError::InvalidMethod);
            };
            if result.is_some() || error.is_some() {
                return Err(EnvelopeError::UnexpectedResponseMember);
            }
            let params = obj.remove("params").map(Params::try_from).transpose()?;
            return Ok(match id {
                None => Message::Notification(Notification { method, params }),
                Some(id) => Message::Request(Request {
                    id: Id::try_from(id)?,
                    method,
                    params,
                }),
            });
        }

        let id = match id {
            None => return Err(EnvelopeError::MissingResponseId),
            Some(Value::Null) => None,
            Some(id) => Some(Id::try_from(id)?),
        };
        let outcome = match (result, error) {
            (Some(_), Some(_)) => return Err(EnvelopeError::ResultAndError),
            (None, None) => return Err(EnvelopeError::NoResultOrError),
            (Some(result), None) => {
                if id.is_none() {
                    return Err(EnvelopeError::NullIdOnSuccess);
                }
                Ok(result)
            }
            (None, Some(error)) => Err(serde_json::from_value::<ErrorObject>(error)
                .map_err(|err| EnvelopeError::InvalidErrorObject(err.to_string()))?),
        };
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
                    Err(error) => obj.insert(
                        "error".into(),
                        serde_json::to_value(error).unwrap_or(Value::Null),
                    ),
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
    type Error = EnvelopeError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::String(text) => Ok(Id::String(text)),
            Value::Number(n) => n.as_i64().map(Id::Number).ok_or(EnvelopeError::InvalidId),
            _ => Err(EnvelopeError::InvalidId),
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
    type Error = EnvelopeError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::Array(items) => Ok(Params::Array(items)),
            Value::Object(members) => Ok(Params::Object(members)),
            _ => Err(EnvelopeError::InvalidParams),
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
        let value = Value::deserialize(deserializer)?;
        Message::from_value(value).map_err(serde::de::Error::custom)
    }
}

/// Deserializes a member that is present, keeping an explicit `null` as
/// `Some(Value::Null)`. With `#[serde(default)]`, a missing member is `None`.
fn present<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}
