use super::*;
use serde_json::json;

fn queue() -> Intent {
    Intent::Economy {
        target: EconomyTarget::ConstructionQueue {
            building: "arms_factory".into(),
            state_id: 64,
            minimum_queued: 2,
        },
    }
}

#[test]
fn construction_queue_schema_roundtrips_as_a_distinct_explicit_target() {
    let expected = r#"{"kind":"Economy","target":{"kind":"ConstructionQueue","building":"arms_factory","state_id":64,"minimum_queued":2}}"#;
    let value: Intent = serde_json::from_str(expected).unwrap();
    assert_eq!(value, queue());
    value.validate().unwrap();
    assert_eq!(serde_json::to_string(&value).unwrap(), expected);
    assert_eq!(value.capability(), "ordinary_economy");
}

#[test]
fn construction_queue_schema_never_aliases_or_defaults_legacy_total_levels() {
    for target in [
        json!({"kind":"ConstructionQueue","building":"arms_factory","state_id":64,"total_levels":2}),
        json!({"kind":"ConstructionQueue","building":"arms_factory","state_id":64}),
        json!({"kind":"ConstructionQueue","building":"arms_factory","state_id":64,"minimum_queued":2,"total_levels":2}),
        json!({"kind":"Construction","building":"arms_factory","state_id":64,"minimum_queued":2}),
        json!({"kind":"Construction","building":"arms_factory","state_id":64,"total_levels":2,"minimum_queued":2}),
        json!({"kind":"construction_queue","building":"arms_factory","state_id":64,"minimum_queued":2}),
    ] {
        assert!(
            serde_json::from_value::<Intent>(json!({"kind":"Economy","target":target})).is_err()
        );
    }
}

#[test]
fn construction_queue_schema_requires_canonical_ordinary_key_and_bounded_goal() {
    for (building, state_id, minimum_queued) in [
        ("", 64, 2),
        ("arms-factory", 64, 2),
        ("arms factory", 64, 2),
        ("å", 64, 2),
        ("rail_way", 64, 2),
        ("arms_factory", 0, 2),
        ("arms_factory", 4096, 2),
        ("arms_factory", 64, 0),
        ("arms_factory", 64, 201),
    ] {
        let v = Intent::Economy {
            target: EconomyTarget::ConstructionQueue {
                building: building.into(),
                state_id,
                minimum_queued,
            },
        };
        assert_eq!(v.validate().unwrap_err().code, "invalid_plan");
    }
    let long = Intent::Economy {
        target: EconomyTarget::ConstructionQueue {
            building: "x".repeat(129),
            state_id: 1,
            minimum_queued: 1,
        },
    };
    assert!(long.validate().is_err());
    for (state_id, minimum_queued) in [(1, 1), (4095, 200)] {
        Intent::Economy {
            target: EconomyTarget::ConstructionQueue {
                building: "syntactically_valid_unknown_key".into(),
                state_id,
                minimum_queued,
            },
        }
        .validate()
        .unwrap();
    }
}

#[test]
fn construction_queue_schema_rejects_wrong_numeric_and_boolean_types() {
    for (field, value) in [
        ("minimum_queued", json!(true)),
        ("minimum_queued", json!("2")),
        ("minimum_queued", json!(2.0)),
        ("minimum_queued", json!(-1)),
        ("state_id", json!(false)),
        ("state_id", json!("64")),
        ("state_id", json!(-1)),
    ] {
        let mut value_json = serde_json::to_value(queue()).unwrap();
        value_json["target"][field] = value;
        assert!(serde_json::from_value::<Intent>(value_json).is_err());
    }
}

#[test]
fn construction_queue_schema_does_not_restrict_or_reencode_legacy_construction() {
    let exact = r#"{"kind":"Economy","target":{"kind":"Construction","building":"arms-factory","state_id":5000,"total_levels":4}}"#;
    let old: Intent = serde_json::from_str(exact).unwrap();
    // Legacy validation intentionally remains its historical key/state policy.
    old.validate().unwrap();
    assert_eq!(serde_json::to_string(&old).unwrap(), exact);
    assert!(matches!(
        old,
        Intent::Economy {
            target: EconomyTarget::Construction {
                total_levels: 4,
                ..
            }
        }
    ));
    assert_ne!(old, queue());
}
