use super::*;

const POSITION: u64 = 0x370000;

fn owned_gate(button: u64, parent: u64, descend: u8) -> Fixture {
    let mut f = controls_fixture();
    f.word(button + 0x1a8, POSITION);
    f.word(parent + 0xe78, POSITION);
    f.word(POSITION, BASE + 0x43484f0);
    // No other position payload is mapped: only the admitted byte is needed.
    f.put(POSITION + 0x128, &[descend]);
    f
}

#[test]
fn exact_owned_parent_gate_reports_both_active_and_inactive_traversal() {
    for (button, parent, name) in [(OPEN, INTEL, "opener"), (CHOOSE, CW, "choice")] {
        for descend in [0, 1] {
            let f = owned_gate(button, parent, descend);
            let v = observe(&f).unwrap();
            let local = &v[name]["local"];
            assert_eq!(local["optional_gate_1a8_null"], false);
            assert_eq!(local["optional_gate"]["own_position_join_verified"], true);
            assert_eq!(local["optional_gate"]["clear"], descend == 1);
            assert_eq!(local["conservative_dispatch_ready"], descend == 1);
            assert_eq!(v["input_authorized"], false);
            assert_eq!(v["current_frame_verified"], false);
        }
    }
}

#[test]
fn unknown_gate_identity_and_type_do_not_reach_optional_payload() {
    let mut f = controls_fixture();
    f.word(OPEN + 0x1a8, POSITION);
    // Even an already-known different window's position does not bind this one.
    f.word(CW + 0xe78, POSITION);
    let v = observe(&f).unwrap();
    assert_eq!(
        v["opener"]["local"]["optional_gate"]["clear"],
        serde_json::Value::Null
    );
    production_not_read(&f, &[POSITION, POSITION + 0x128]);

    let mut f = owned_gate(OPEN, INTEL, 1);
    f.word(POSITION, BASE + 0x43484f1);
    assert_eq!(
        observe(&f).unwrap()["opener"]["local"]["conservative_dispatch_ready"],
        false
    );
    production_not_read(&f, &[POSITION + 0x128]);

    let f = owned_gate(OPEN, INTEL, 2);
    assert_eq!(observe(&f).unwrap_err().code, "invalid_construction");

    let f = controls_fixture();
    assert_eq!(
        observe(&f).unwrap()["opener"]["local"]["optional_gate"]["absent"],
        true
    );
    production_not_read(&f, &[INTEL + 0xe78, CW + 0xe78]);
}

#[test]
fn owned_optional_identity_and_flag_are_final_witnesses() {
    let baseline = owned_gate(OPEN, INTEL, 1);
    observe(&baseline).unwrap();
    for (address, width) in [
        (OPEN + 0x1a8, 8),
        (INTEL + 0xe78, 8),
        (POSITION, 8),
        (POSITION + 0x128, 1),
    ] {
        let mut bytes = (0..width)
            .map(|i| baseline.data[&(address + i)])
            .collect::<Vec<_>>();
        bytes[0] ^= 1;
        let mut f = owned_gate(OPEN, INTEL, 1);
        f.fault = Some((
            address,
            baseline.tracked_reads.borrow()[&address] - 1,
            bytes,
        ));
        assert_eq!(observe(&f).unwrap_err().code, "torn_read");
    }
}
