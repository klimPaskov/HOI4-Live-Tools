use super::*;

const SCOPES: [(u64, bool, &str, u64); 5] = [
    (ROOT, true, "country_menu_window", MENU),
    (MENU, true, "after_faction_button", FACTION),
    (FACTION, true, "after_intel_agency_button", INTEL),
    (INTEL, false, "construction_button", OPEN),
    (CW, false, "start_construction_button", CHOOSE),
];

fn registry(f: &mut Fixture, window: u64, children: bool, entries: &[(&str, u64)]) {
    let (a, b) = if children {
        (0xbb0, 0xbc8)
    } else {
        (0x9a0, 0x9b8)
    };
    f.array(window + a, window + 0x3000, entries.len() as i32);
    f.array(window + b, window + 0x4000, entries.len() as i32);
    for (i, (name, pointer)) in entries.iter().enumerate() {
        f.word(window + 0x3000 + i as u64 * 8, *pointer);
        text(f, window + 0x4000 + i as u64 * 32, name);
    }
}

#[test]
fn unrelated_repeated_and_empty_names_allow_one_requested_key_at_any_position() {
    for (window, children, key, target) in SCOPES {
        for position in [0, 2, 4] {
            let mut entries = vec![
                ("shared", 0x500000),
                ("", 0x500100),
                ("shared", 0x500200),
                ("", 0x500300),
            ];
            entries.insert(position, (key, target));
            let mut f = controls_fixture();
            registry(&mut f, window, children, &entries);
            let v = observe(&f).unwrap();
            assert_eq!(v["opener"]["callback"]["verified"], true);
            assert_eq!(v["choice"]["callback"]["verified"], true);
            production_not_read(&f, &[0x500000, 0x500100, 0x500200, 0x500300]);
        }
    }
}

#[test]
fn repeated_requested_key_refuses_before_target_payload_even_after_first_match() {
    for (window, children, key, target) in SCOPES {
        let mut f = controls_fixture();
        registry(
            &mut f,
            window,
            children,
            &[
                (key, target),
                ("", 0x500000),
                ("", 0x500100),
                (key, 0x500200),
            ],
        );
        let error = observe(&f).unwrap_err();
        assert_eq!(error.code, "invalid_construction");
        assert!(error.message.contains(key));
        assert!(error
            .message
            .contains(if children { "container" } else { "button" }));
        production_not_read(&f, &[target, 0x500000, 0x500100, 0x500200]);
    }
}

#[test]
fn unrelated_names_and_pointers_still_validate_after_a_requested_match() {
    for (window, children, key, target) in SCOPES {
        for corruption in 0..3 {
            let mut f = controls_fixture();
            registry(
                &mut f,
                window,
                children,
                &[(key, target), ("shared", 0x500000), ("shared", 0x500100)],
            );
            match corruption {
                0 => f.word(window + 0x4000 + 64 + 8, 129),
                1 => f.word(window + 0x3000 + 16, 0),
                _ => f.word(window + 0x3000 + 16, 0x500000),
            }
            assert!(observe(&f).is_err());
            production_not_read(&f, &[target, 0x500000, 0x500100]);
        }
    }
}

#[test]
fn repeated_unrelated_name_bytes_remain_in_final_witnesses() {
    for (window, children, key, target) in SCOPES {
        let build = || {
            let mut f = controls_fixture();
            registry(
                &mut f,
                window,
                children,
                &[(key, target), ("shared", 0x500000), ("shared", 0x500100)],
            );
            f
        };
        let baseline = build();
        observe(&baseline).unwrap();
        // Mutate the third name in the coalesced header block at final witness
        // time. The first matching name must not hide unrelated-name changes.
        let address = window + 0x4000;
        let mut bytes = (0..96)
            .map(|i| baseline.data[&(address + i)])
            .collect::<Vec<_>>();
        bytes[64] ^= 1;
        let mut f = build();
        f.fault = Some((
            address,
            baseline.tracked_reads.borrow()[&address] - 1,
            bytes,
        ));
        assert_eq!(observe(&f).unwrap_err().code, "torn_read");
    }
}
