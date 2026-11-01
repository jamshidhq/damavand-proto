# Error-code registry

Every damavand crate that reports errors over the damavand protocol uses
codes from its own block. The blocks never overlap, so a code names the crate
that sent it.

JSON-RPC 2.0 reserves -32768 to -32000. Inside that range, only the codes
below are used. Every crate block lies outside it.

## JSON-RPC 2.0 codes

| Code | Name | Meaning |
| --- | --- | --- |
| -32700 | `PARSE_ERROR` | The text is not valid JSON, or an object has two members with the same name. |
| -32600 | `INVALID_REQUEST` | The message is not a valid request (includes batches and `rpc.` methods). |
| -32601 | `METHOD_NOT_FOUND` | The method does not exist. |
| -32602 | `INVALID_PARAMS` | The params are not valid for the method. |
| -32603 | `INTERNAL_ERROR` | Internal error. |

## Blocks

| Owner | First | Last |
| --- | --- | --- |
| damavand-proto | -32019 | -32000 |
| damavand-policy | 1000 | 1999 |
| damavand-manifest | 2000 | 2999 |
| damavand-audit | 3000 | 3999 |
| damavand-redact | 4000 | 4999 |
| damavand-tools | 5000 | 5999 |
| damavand-memory | 6000 | 6999 |
| damavand-eval | 7000 | 7999 |

The damavand-proto block is inside the JSON-RPC server-error range
(-32099 to -32000), which JSON-RPC 2.0 leaves to implementations.

## damavand-proto codes

| Code | Name | Meaning |
| --- | --- | --- |
| -32001 | `INCOMPATIBLE_VERSION` | The peers support no common protocol version. `data` is `{"supported": [...]}`, the server's supported-versions list. |

All other codes from -32019 to -32000 are reserved for future protocol
versions.

## Rules

- A crate MUST use only codes from its own block for its own errors.
- A new crate gets the next free 1000-wide block. Add it here and to
  `damavand_proto::jsonrpc::codes::blocks` in the same change.
- A code, once released, keeps its meaning. A retired code is not reused.

The Rust consts are in `damavand_proto::jsonrpc::codes`. A test checks that
this file and the consts agree.
