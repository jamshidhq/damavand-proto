# damavand-proto

The versioned, out-of-process plugin wire protocol for the damavand
agent-safety crates. Plugins and hosts are separate processes. They talk
JSON-RPC 2.0, in the style of LSP and MCP, and agree on a protocol version and
a capability set in an `initialize` handshake.

[SPEC.md](SPEC.md) is the normative, language-neutral specification. This
crate is its reference implementation.

## Stability

- **Wire protocol:** versioned and negotiated from the first release. Peers
  announce the versions they support and negotiate one, so a peer keeps
  working with peers that speak an older protocol version.
- **Rust API:** can change in any release before 1.0.
- `serde_json` is a public dependency: its `Value` and `Map` types are part of
  the API.
- MSRV: Rust 1.89 for the library and tests (the MSRV of the whole damavand
  family).

## What is in the crate

| Module | Contents |
| --- | --- |
| `jsonrpc` | `Message` (`Request`, `Notification`, `Response`), `ErrorObject`, `EnvelopeError`, and the error codes and code registry (`codes`). Parsing is strict (see SPEC.md section 4): duplicate member names, batches, `rpc.` methods, `null`, fractional or out-of-range request ids and scalar `params` are refused. `EnvelopeError::to_response` gives the reply to send, with the request id when it is readable, or `None` when no reply is allowed. |
| `version` | `ProtocolVersion` (`"major.minor"`, no leading zeros) and `SupportedVersions`, the list a peer announces, with the negotiation rule. |
| `capability` | `Capabilities`: an object keyed by capability name, each value a parameter object. |
| `handshake` | `Supported`, `InitializeParams`, `InitializeResult`, `PeerInfo`, `negotiate`, and `answer_initialize`, the reference server side. |
| `conformance` (feature `conformance`) | A test suite that another crate runs against its own implementation. |

Error codes: see [docs/error-codes.md](docs/error-codes.md).

## Protocol overview

1. The client sends `initialize` with the versions it supports (one entry per
   major, each the highest minor it supports) and its capabilities:

   ```json
   {"jsonrpc": "2.0", "id": 1, "method": "initialize",
    "params": {"versions": ["1.4", "2.0"],
               "capabilities": {"tools": {"listChanged": true}},
               "clientInfo": {"name": "damavand-host", "version": "1.2.3"}}}
   ```

2. The server picks the highest major both support and, in it, the lower of
   the two minors. It keeps the capabilities both peers announce, with its own
   parameters. A capability that one peer does not know is dropped, not
   refused:

   ```json
   {"jsonrpc": "2.0", "id": 1,
    "result": {"version": "1.2", "capabilities": {"tools": {"listChanged": true}},
               "serverInfo": {"name": "damavand-tools", "version": "0.4.0"}}}
   ```

3. When no major is common, the server answers with error code `-32001`
   (`INCOMPATIBLE_VERSION`) and its own list in `data`:

   ```json
   {"jsonrpc": "2.0", "id": 1,
    "error": {"code": -32001, "message": "...", "data": {"supported": ["3.0"]}}}
   ```

In major 0, every minor is its own compatibility line: `0.1` and `0.2` do not
negotiate. The current protocol version is `0.1`.

## Conformance suite

```toml
[dev-dependencies]
damavand-proto = { version = "0.1", features = ["conformance"] }
```

```rust
use damavand_proto::conformance::{HandshakeServer, run_handshake};

// Implement HandshakeServer for your server: `supported()` returns what it
// supports, `handle()` takes one JSON text message and returns the reply as
// JSON text, or `None` for a notification or a response.
run_handshake(&mut my_server).assert_passed();
```

The suite talks JSON text, so it checks the wire format as well as the
behaviour. A failure names each failing case.

## Not yet in the crate

- A transport. The socket (Unix socket only, or more) is not decided.
- Batches, cancellation, and methods other than `initialize`.

## License

Apache-2.0. See [LICENSE](LICENSE).
