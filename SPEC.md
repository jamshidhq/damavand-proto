# damavand plugin protocol

Status: draft. Protocol version described here: `0.1`.

This document is the normative, language-neutral specification of the wire
protocol between a damavand host and an out-of-process plugin. The Rust crate
`damavand-proto` is its reference implementation. Where the crate and this
document disagree, this document is correct and the crate has a bug.

The key words MUST, MUST NOT, REQUIRED, SHALL, SHALL NOT, SHOULD, SHOULD NOT,
RECOMMENDED, MAY and OPTIONAL are to be read as described in RFC 2119 and
RFC 8174 when, and only when, they appear in capitals.

## 1. Stability

The wire protocol is versioned and negotiated from its first release. A peer
that adds support for a new protocol version MUST keep the versions it
supported before, unless it deliberately drops a compatibility line, so that
older peers keep working through negotiation (section 5).

Implementation APIs, including the Rust API of `damavand-proto`, are not part
of this specification. The Rust API can change in any release before 1.0.

## 2. Terms

- **Peer**: one side of a session. The **client** sends `initialize`; the
  **server** answers it. Usually the host is the client and the plugin is the
  server.
- **Message**: one JSON text that is a request, a notification or a response.

## 3. Framing and transport

The transport is out of scope for this version. This specification assumes
that the transport delivers each message as one complete JSON text, in order,
and tells the receiver where each message ends. Messages MUST be UTF-8.

## 4. JSON-RPC 2.0 subset

Messages are JSON-RPC 2.0 messages, with the restrictions in this section.

### 4.1 JSON text

- A receiver MUST refuse a text that is not valid JSON with error `-32700`
  (parse error).
- An object MUST NOT have two members with the same name, at any depth. A
  receiver MUST refuse such a text with error `-32700`. (JSON parsers that
  keep the last member silently are not conformant.)
- Unknown members of the message object MUST be ignored.

### 4.2 Message kinds

- Every message MUST have `"jsonrpc": "2.0"`.
- A **request** has a `method` (a string) and an `id`.
- A **notification** has a `method` (a string) and no `id` member.
- A **response** has an `id` and exactly one of `result` and `error`. It has
  no `method`.
- `params`, when present, MUST be an array or an object.
- A request or notification MUST NOT have a `result` or `error` member.
- `error` MUST be an object with an integer `code` and a string `message`,
  and MAY have `data`. A sender MUST NOT send `"error": null`.

### 4.3 Ids

- A request id MUST be a string or an integer in the range of a 64-bit signed
  integer (-2^63 to 2^63 - 1). A receiver MUST refuse a request whose id is
  `null`, fractional, of another type, or out of that range, with error
  `-32600`.
- A response carries the id of the request it answers. Its id is `null` only
  when the request id could not be read (section 4.6).
- A success response MUST NOT have a `null` id.

### 4.4 Batches

Batches are not supported. A receiver MUST answer a JSON array (empty or not)
with exactly one error response: code `-32600`, id `null`.

### 4.5 Reserved method names

Method names that start with `rpc.` are reserved by JSON-RPC 2.0. This
protocol defines none of them. A receiver MUST refuse a request whose method
starts with `rpc.` with error `-32600`, and MUST ignore such a notification.

### 4.6 When to reply

- A server MUST answer every request with exactly one response that carries
  the request id.
- A peer MUST NOT reply to a valid notification, whether it knows the method
  or not, nor to a notification whose method is reserved (section 4.5). A
  method-level problem in a notification, such as an unknown method or params
  the method does not accept, gets no reply.
- A peer MUST NOT reply to a response. A message that has no `method` and has
  `result` or `error` is treated as a response for this rule, also when it is
  invalid.
- For any other invalid message, the receiver MUST reply with one error
  response. Its id MUST be the message's `id` when that member is a valid
  request id (section 4.3), and `null` otherwise.
- A request for a method the server does not know MUST get error `-32601`
  (method not found) with the request id.

## 5. Versions

### 5.1 Version strings

A protocol version is written `"MAJOR.MINOR"`. Each part is a decimal integer
from 0 to 65535 without leading zeros: `"0"` is valid, `"01"` and `"00"` are
not. Nothing else may appear: no sign, no spaces, no third part. Versions
compare numerically, by major and then by minor: `1.10` is above `1.9`.

### 5.2 Compatibility lines

- For major 1 and above, a major change is breaking and a minor change is
  additive. A peer that supports `MAJOR.MINOR` supports every `MAJOR.m` with
  `m <= MINOR`. All versions with the same major form one **compatibility
  line**.
- For major 0, every minor is breaking: `0.1` and `0.2` are incompatible.
  Each `0.MINOR` is its own compatibility line.

Compatibility lines order like their versions: `0.0 < 0.1 < ... < 1 < 2 < ...`.

### 5.3 Supported-versions list

A peer announces a list of version strings: one entry per compatibility line
it supports, each the highest minor it supports in that line. For example
`["1.4", "2.0"]` means 1.0 to 1.4 and 2.0.

- The list MUST NOT be empty.
- The list MUST NOT hold two entries of the same compatibility line, for
  example `["1.2", "1.4"]` or `["0.3", "0.3"]`. (`["0.1", "0.2"]` is valid.)
- The order of the list has no meaning.

A server MUST refuse an `initialize` request whose list breaks these rules, or
that holds a malformed version string, with error `-32602`.

### 5.4 Negotiation

1. Take the compatibility lines that are in both lists.
2. If there are none, negotiation fails: the server MUST answer with error
   `-32001` (incompatible version) and `data` `{"supported": [...]}`, its own
   list.
3. Otherwise take the highest common line. The negotiated version is that
   line's major with the lower of the two minors.

The result does not depend on which peer is the server.

Worked examples (client list, server list, result):

| Client | Server | Result | Why |
| --- | --- | --- | --- |
| `["1.4"]` | `["1.2"]` | `1.2` | Same line; the lower minor wins. |
| `["1.4", "2.0"]` | `["1.2"]` | `1.2` | Cross-major: only line 1 is common. |
| `["1.9", "2.1"]` | `["1.4", "2.3"]` | `2.1` | Line 2 is the highest common line. |
| `["0.1", "0.2"]` | `["0.2"]` | `0.2` | `0.2` is its own line. |
| `["0.1"]` | `["0.2"]` | error `-32001` | `0.1` and `0.2` are different lines. |
| `["2.0"]` | `["1.4"]` | error `-32001` | No common line. |

## 6. The `initialize` handshake

The client MUST send `initialize` as its first request. A peer SHOULD NOT send
other requests before the handshake succeeds.

### 6.1 Params

```json
{
  "versions": ["0.1"],
  "capabilities": {"tools": {"listChanged": true}},
  "clientInfo": {"name": "damavand-host", "version": "1.2.3"}
}
```

- `versions` (REQUIRED): the client's supported-versions list (section 5.3).
- `capabilities` (OPTIONAL): the client's capabilities (section 7). An absent
  member is the same as `{}`.
- `clientInfo` (OPTIONAL): an object with the string members `name` and
  `version` (both REQUIRED in the object). Informational only.
- Unknown members MUST be ignored.

Params that are absent, not an object, or that break these rules MUST get
error `-32602`.

### 6.2 Result

```json
{
  "version": "0.1",
  "capabilities": {"tools": {"listChanged": true}},
  "serverInfo": {"name": "damavand-tools", "version": "0.4.0"}
}
```

- `version` (REQUIRED): the negotiated version (section 5.4). Both peers MUST
  use this version for the rest of the session.
- `capabilities` (OPTIONAL for the reader): the negotiated capabilities
  (section 7.3). A server SHOULD always send it. A client MUST treat an absent
  member as `{}`.
- `serverInfo` (OPTIONAL): like `clientInfo`.

## 7. Capabilities

### 7.1 Shape

Capabilities are a JSON object. Each member name is a capability name; each
value is a JSON object that holds the parameters of that capability. `{}` is a
capability with no parameters.

### 7.2 Names

A capability name MUST match `[a-z][a-z0-9_.-]*`: a lowercase ASCII letter,
then lowercase ASCII letters, digits, `_`, `.` or `-`. The empty name is not
valid. Duplicate names are refused as described in section 4.1. A server MUST
refuse a capabilities member that is not an object, has a member that is not
an object, or has an invalid name, with error `-32602`.

### 7.3 Negotiation

- The negotiated capabilities are the names both peers announce. A name that
  only one peer announces is dropped, never an error.
- In this version, the value of each negotiated capability is the server's
  parameter object.
- How the parameters of one capability are negotiated is defined by the
  specification of that capability. A future version can replace the rule
  above for a given capability.

## 8. Errors

Error codes are registered in [docs/error-codes.md](docs/error-codes.md).
Codes from -32019 to -32000 are reserved for this protocol. Each damavand
crate has its own 1000-wide block outside the JSON-RPC reserved range
(-32768 to -32000).

| Code | Meaning in this protocol |
| --- | --- |
| -32700 | Parse error, including duplicate member names. |
| -32600 | Invalid request, including batches, bad ids and `rpc.` methods. |
| -32601 | Method not found. |
| -32602 | Invalid params. |
| -32603 | Internal error. |
| -32001 | Incompatible version. `data` is `{"supported": [...]}`. |

## 9. Conformance

The `conformance` feature of `damavand-proto` runs these rules against a server
over JSON text. A conformant server passes every case.
