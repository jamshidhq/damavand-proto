//! The error-code registry: the consts agree with each other and with
//! `docs/error-codes.md`.

use damavand_proto::jsonrpc::codes::{self, blocks};

const DOC: &str = include_str!("../docs/error-codes.md");

#[test]
fn jsonrpc_reserved_range_has_spec_bounds() {
    assert_eq!(codes::JSONRPC_RESERVED_MIN, -32768);
    assert_eq!(codes::JSONRPC_RESERVED_MAX, -32000);
}

#[test]
fn proto_block_is_inside_the_server_error_range() {
    assert_eq!(codes::PROTO_MIN, -32019);
    assert_eq!(codes::PROTO_MAX, -32000);
    const { assert!(codes::SERVER_ERROR_MIN <= codes::PROTO_MIN) };
    const { assert!(codes::PROTO_MAX <= codes::SERVER_ERROR_MAX) };
    assert_eq!(codes::INCOMPATIBLE_VERSION, -32001);
    assert!((codes::PROTO_MIN..=codes::PROTO_MAX).contains(&codes::INCOMPATIBLE_VERSION));
}

#[test]
fn crate_blocks_have_the_registered_bounds() {
    let want = [
        ("damavand-policy", 1000, 1999),
        ("damavand-manifest", 2000, 2999),
        ("damavand-audit", 3000, 3999),
        ("damavand-redact", 4000, 4999),
        ("damavand-tools", 5000, 5999),
        ("damavand-memory", 6000, 6999),
        ("damavand-eval", 7000, 7999),
    ];
    let got: Vec<(&str, i64, i64)> = blocks::ALL
        .iter()
        .map(|(name, range)| (*name, *range.start(), *range.end()))
        .collect();
    assert_eq!(got, want);
    assert_eq!(blocks::POLICY, 1000..=1999);
    assert_eq!(blocks::MANIFEST, 2000..=2999);
    assert_eq!(blocks::AUDIT, 3000..=3999);
    assert_eq!(blocks::REDACT, 4000..=4999);
    assert_eq!(blocks::TOOLS, 5000..=5999);
    assert_eq!(blocks::MEMORY, 6000..=6999);
    assert_eq!(blocks::EVAL, 7000..=7999);
}

#[test]
fn crate_blocks_are_disjoint_and_outside_the_jsonrpc_range() {
    let reserved = codes::JSONRPC_RESERVED_MIN..=codes::JSONRPC_RESERVED_MAX;
    for (i, (name, range)) in blocks::ALL.iter().enumerate() {
        assert_eq!(range.end() - range.start(), 999, "{name} is not 1000 wide");
        assert!(!reserved.contains(range.start()) && !reserved.contains(range.end()));
        for (other, theirs) in &blocks::ALL[i + 1..] {
            assert!(
                range.end() < theirs.start() || theirs.end() < range.start(),
                "{name} overlaps {other}"
            );
        }
    }
}

#[test]
fn block_of_finds_the_owner() {
    assert_eq!(blocks::owner(1000), Some("damavand-policy"));
    assert_eq!(blocks::owner(1999), Some("damavand-policy"));
    assert_eq!(blocks::owner(7999), Some("damavand-eval"));
    assert_eq!(blocks::owner(-32001), Some("damavand-proto"));
    assert_eq!(blocks::owner(-32019), Some("damavand-proto"));
    assert_eq!(blocks::owner(-32020), None);
    assert_eq!(blocks::owner(999), None);
    assert_eq!(blocks::owner(8000), None);
    assert_eq!(blocks::owner(-32600), None);
}

#[test]
fn doc_lists_every_block() {
    assert!(
        DOC.contains("| damavand-proto | -32019 | -32000 |"),
        "proto row missing"
    );
    assert!(DOC.contains("| -32001 | `INCOMPATIBLE_VERSION` |"));
    for (name, range) in blocks::ALL {
        let row = format!("| {name} | {} | {} |", range.start(), range.end());
        assert!(DOC.contains(&row), "docs/error-codes.md lacks {row}");
    }
}
