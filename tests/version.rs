//! Protocol versions, the supported-versions list and version negotiation.

use damavand_proto::ProtocolVersion;
use damavand_proto::version::{CURRENT, SupportedVersions, VersionError};
use proptest::prelude::*;
use serde_json::json;

fn v(major: u16, minor: u16) -> ProtocolVersion {
    ProtocolVersion::new(major, minor)
}

fn list(versions: &[ProtocolVersion]) -> SupportedVersions {
    SupportedVersions::new(versions.iter().copied()).unwrap()
}

// --- ProtocolVersion ----------------------------------------------------------

#[test]
fn version_parts() {
    let ver = v(3, 7);
    assert_eq!(ver.major(), 3);
    assert_eq!(ver.minor(), 7);
}

#[test]
fn version_orders_numerically_by_major_then_minor() {
    assert!(v(1, 0) > v(0, 99));
    assert!(v(1, 2) > v(1, 1));
    assert!(v(1, 10) > v(1, 9));
    assert!(v(10, 0) > v(9, 9));
    assert_eq!(v(2, 3), v(2, 3));
}

#[test]
fn version_text_form() {
    assert_eq!(v(1, 12).to_string(), "1.12");
    assert_eq!("0.3".parse::<ProtocolVersion>().unwrap(), v(0, 3));
    assert_eq!("0.0".parse::<ProtocolVersion>().unwrap(), v(0, 0));
    assert_eq!("10.20".parse::<ProtocolVersion>().unwrap(), v(10, 20));
    assert_eq!(serde_json::to_value(v(4, 5)).unwrap(), json!("4.5"));
    assert_eq!(
        serde_json::from_value::<ProtocolVersion>(json!("4.5")).unwrap(),
        v(4, 5)
    );
}

#[test]
fn malformed_versions_are_refused() {
    for text in [
        "", "1", "1.", ".1", "1.2.3", "a.b", "-1.0", "1.-0", "+1.0", "1.0 ", " 1.0", "65536.0",
        "1.65536",
    ] {
        assert!(
            matches!(
                text.parse::<ProtocolVersion>(),
                Err(VersionError::Malformed(_))
            ),
            "accepted {text:?}"
        );
    }
    assert!(serde_json::from_value::<ProtocolVersion>(json!(1.0)).is_err());
}

#[test]
fn leading_zeros_are_refused() {
    for text in ["01.1", "0.01", "00.1", "1.00", "007.0"] {
        assert!(
            matches!(
                text.parse::<ProtocolVersion>(),
                Err(VersionError::Malformed(_))
            ),
            "accepted {text:?}"
        );
    }
}

#[test]
fn current_version_is_0_1() {
    assert_eq!(CURRENT, v(0, 1));
}

#[test]
fn compatibility_follows_cargo_rules() {
    // Same major >= 1: compatible (minor is additive).
    assert!(v(1, 0).is_compatible_with(v(1, 4)));
    assert!(v(1, 4).is_compatible_with(v(1, 0)));
    // Different major: not compatible.
    assert!(!v(1, 4).is_compatible_with(v(2, 0)));
    // 0.x: every minor is its own line.
    assert!(v(0, 1).is_compatible_with(v(0, 1)));
    assert!(!v(0, 1).is_compatible_with(v(0, 2)));
    assert!(!v(0, 0).is_compatible_with(v(1, 0)));
}

// --- SupportedVersions --------------------------------------------------------

#[test]
fn supported_versions_wire_form_is_a_sorted_list() {
    let value = json!(["2.0", "1.4"]);
    let versions: SupportedVersions = serde_json::from_value(value).unwrap();
    assert_eq!(versions, list(&[v(1, 4), v(2, 0)]));
    assert_eq!(
        serde_json::to_value(&versions).unwrap(),
        json!(["1.4", "2.0"])
    );
    assert_eq!(versions.iter().collect::<Vec<_>>(), [v(1, 4), v(2, 0)]);
    assert_eq!(versions.len(), 2);
    assert_eq!(versions.highest(), v(2, 0));
    assert!(!versions.is_empty());
    assert_eq!(versions.to_string(), "[1.4, 2.0]");
    assert_eq!(SupportedVersions::single(v(0, 1)).to_string(), "[0.1]");
    assert_eq!(
        list(&[v(0, 1), v(0, 2), v(1, 0)]).to_string(),
        "[0.1, 0.2, 1.0]"
    );
}

#[test]
fn zero_minors_are_separate_lines() {
    let versions = list(&[v(0, 2), v(0, 1), v(1, 0)]);
    assert_eq!(
        versions.iter().collect::<Vec<_>>(),
        [v(0, 1), v(0, 2), v(1, 0)]
    );
}

#[test]
fn empty_list_is_refused() {
    assert_eq!(
        SupportedVersions::new(std::iter::empty()),
        Err(VersionError::Empty)
    );
    assert!(serde_json::from_value::<SupportedVersions>(json!([])).is_err());
}

#[test]
fn duplicate_majors_are_refused() {
    assert_eq!(
        SupportedVersions::new([v(1, 2), v(1, 4)]),
        Err(VersionError::DuplicateMajor {
            first: v(1, 2),
            second: v(1, 4)
        })
    );
    assert!(matches!(
        SupportedVersions::new([v(0, 3), v(0, 3)]),
        Err(VersionError::DuplicateMajor { .. })
    ));
    assert!(serde_json::from_value::<SupportedVersions>(json!(["1.2", "1.4"])).is_err());
    assert!(serde_json::from_value::<SupportedVersions>(json!(["1.2", "01.4"])).is_err());
    assert!(serde_json::from_value::<SupportedVersions>(json!({"min": "0.1"})).is_err());
}

#[test]
fn version_errors_display() {
    for err in [
        VersionError::Malformed("x".into()),
        VersionError::Empty,
        VersionError::DuplicateMajor {
            first: v(1, 2),
            second: v(1, 4),
        },
    ] {
        assert!(!err.to_string().is_empty());
    }
    let text = VersionError::DuplicateMajor {
        first: v(1, 2),
        second: v(1, 4),
    }
    .to_string();
    assert!(text.contains("1.2") && text.contains("1.4"), "{text}");
}

#[test]
fn single_holds_one_version() {
    let one = SupportedVersions::single(v(3, 1));
    assert_eq!(one.iter().collect::<Vec<_>>(), [v(3, 1)]);
    assert_eq!(one.highest(), v(3, 1));
}

// --- negotiation --------------------------------------------------------------

#[test]
fn highest_common_major_then_lower_minor() {
    // The worked examples in SPEC.md.
    let cases = [
        // Same major: the lower minor wins.
        (&[v(1, 4)][..], &[v(1, 2)][..], Some(v(1, 2))),
        // Cross-major: 2 is only on one side, so major 1 is used.
        (&[v(1, 4), v(2, 0)][..], &[v(1, 2)][..], Some(v(1, 2))),
        // Both support major 2: it wins over major 1.
        (
            &[v(1, 4), v(2, 3)][..],
            &[v(1, 9), v(2, 1)][..],
            Some(v(2, 1)),
        ),
        // 0.x: only the exact same minor matches.
        (&[v(0, 1), v(0, 2)][..], &[v(0, 2)][..], Some(v(0, 2))),
        (&[v(0, 1)][..], &[v(0, 2)][..], None),
        // No common major.
        (&[v(1, 4)][..], &[v(2, 0)][..], None),
        (&[v(0, 9)][..], &[v(1, 0)][..], None),
    ];
    for (a, b, want) in cases {
        let (a, b) = (list(a), list(b));
        assert_eq!(a.highest_common(&b), want, "{a:?} vs {b:?}");
        assert_eq!(b.highest_common(&a), want, "{b:?} vs {a:?}");
    }
}

/// The compatibility line of a version, as an ordered key.
fn line(ver: ProtocolVersion) -> (u16, u16) {
    if ver.major() == 0 {
        (0, ver.minor())
    } else {
        (ver.major(), 0)
    }
}

fn arb_version() -> impl Strategy<Value = ProtocolVersion> {
    (0u16..4, 0u16..4).prop_map(|(major, minor)| v(major, minor))
}

fn arb_list() -> impl Strategy<Value = SupportedVersions> {
    prop::collection::vec(arb_version(), 1..6).prop_map(|mut versions| {
        // Keep the highest minor per compatibility line.
        versions.sort();
        versions.reverse();
        let mut kept: Vec<ProtocolVersion> = Vec::new();
        for ver in versions {
            if !kept.iter().any(|k| k.is_compatible_with(ver)) {
                kept.push(ver);
            }
        }
        SupportedVersions::new(kept).unwrap()
    })
}

proptest! {
    #[test]
    fn negotiation_is_symmetric_and_sound(a in arb_list(), b in arb_list()) {
        let got = a.highest_common(&b);
        prop_assert_eq!(got, b.highest_common(&a));
        match got {
            Some(ver) => {
                // The result is compatible with an entry on each side, and is
                // no higher than either entry.
                let ea = a.iter().find(|e| e.is_compatible_with(ver)).unwrap();
                let eb = b.iter().find(|e| e.is_compatible_with(ver)).unwrap();
                prop_assert_eq!(ver, ea.min(eb));
                // No higher compatibility line is common to both.
                for x in a.iter() {
                    if b.iter().any(|y| y.is_compatible_with(x)) {
                        prop_assert!(line(x) <= line(ver), "{x} is a higher common line than {ver}");
                    }
                }
            }
            None => {
                for x in a.iter() {
                    prop_assert!(!b.iter().any(|y| y.is_compatible_with(x)));
                }
            }
        }
    }

    #[test]
    fn wire_form_round_trips(a in arb_list()) {
        let value = serde_json::to_value(&a).unwrap();
        prop_assert_eq!(serde_json::from_value::<SupportedVersions>(value).unwrap(), a);
    }

    #[test]
    fn display_and_parse_round_trip(major in any::<u16>(), minor in any::<u16>()) {
        let ver = v(major, minor);
        prop_assert_eq!(ver.to_string().parse::<ProtocolVersion>().unwrap(), ver);
    }
}
