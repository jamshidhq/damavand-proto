# damavand-proto

The versioned, out-of-process plugin wire protocol for the damavand
agent-safety crates. Plugins and hosts are separate processes. They talk
JSON-RPC 2.0, in the style of LSP and MCP, and agree on a protocol version and
a capability set in an `initialize` handshake.

**Status: pre-0.1.** The API and the wire format can change in any release.

## What is in the crate

| Module | Contents |
| --- | --- |
| `jsonrpc` | `Message` (`Request`, `Notification`, `Response`), `ErrorObject`, the standard error codes. Parsing is strict: a wrong `jsonrpc` value, a response with both `result` and `error`, a response with no `id`, a `null` or fractional request id, or scalar `params` are refused with a typed `EnvelopeError`. |
| `version` | `ProtocolVersion` (`major.minor`, written `"0.1"`) and `VersionRange` (inclusive, non-empty). |
| `capability` | `CapabilitySet`: an open set of named string features. |
| `handshake` | `InitializeParams`, `InitializeResult`, `negotiate`, and `answer_initialize`, the reference server side. |
| `conformance` (feature `conformance`) | A test suite that another crate runs against its own implementation. |

## Protocol overview

1. The client sends `initialize` with the version range and the capabilities
   it supports:

   ```json
   {"jsonrpc": "2.0", "id": 1, "method": "initialize",
    "params": {"versions": {"min": "0.1", "max": "0.3"}, "capabilities": ["tools.call"]}}
   ```

2. The server answers with the highest version in both ranges and the
   capabilities both peers announce. A capability that one peer does not know
   is dropped, not refused:

   ```json
   {"jsonrpc": "2.0", "id": 1, "result": {"version": "0.1", "capabilities": ["tools.call"]}}
   ```

3. When the ranges do not overlap, the server answers with error code
   `-32001` (`INCOMPATIBLE_VERSION`) and its own range in `data`:

   ```json
   {"jsonrpc": "2.0", "id": 1,
    "error": {"code": -32001, "message": "...", "data": {"supported": {"min": "0.1", "max": "0.1"}}}}
   ```

## Conformance suite

```toml
[dev-dependencies]
damavand-proto = { version = "0.1", features = ["conformance"] }
```

```rust
use damavand_proto::conformance::{HandshakeServer, run_handshake};

// Implement HandshakeServer for your server: `supported()` returns what it
// supports, `handle()` answers one JSON text message with JSON text.
run_handshake(&mut my_server).assert_passed();
```

The suite talks JSON text, so it checks the wire format as well as the
behaviour. A failure names each failing case.

## Not yet in the crate

- A transport. The socket (Unix socket only, or more) is not decided.
- Batches, cancellation, and methods other than `initialize`.
- Rules for announcing a breaking protocol change.

## License

Apache-2.0. See [LICENSE](LICENSE).
