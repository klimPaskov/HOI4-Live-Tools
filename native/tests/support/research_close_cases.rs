mod research_close_cases {
    use super::*;
    const FACILITIES: u64 = 0x330000;
    const FACILITIES_ROOT: u64 = 0x331000;
    const FACILITIES_PARENT: u64 = 0x332000;
    const FOCUS_ROOT: u64 = 0x333000;
    const MENU: u64 = 0x350000;

    fn window(f: &mut Fixture, address: u64, shown: bool) {
        f.region(address, 0xf00);
        f.word(address, 0x10000 + 0x4347d58);
        f.word(address + 0x98, 0x70000);
        f.put(address + 0xa5, &[if shown { 8 } else { 0 }]);
    }
    fn long_string(f: &mut Fixture, address: u64, text: &str, heap: u64) {
        f.region(address, 32);
        f.word(address, heap);
        f.word(address + 8, text.len() as u64);
        f.word(address + 16, text.len() as u64);
        f.put(heap, text.as_bytes());
    }
    fn fixture(shown: bool) -> Fixture {
        let mut f = politics_focus_fixture();
        let base = 0x10000;
        f.number(0x903d8, if shown { 5 } else { 23 });
        f.region(0x91400, 0x580);
        f.region(0x700c0, 0x180);
        f.region(0x32700, 0x200);
        f.region(0x30738, 0x328);
        f.put(0x30a59, &[1]);
        f.word(base + 0x45778e0, 0x70000);
        f.word(0x90218, MENU);
        f.region(MENU, 0x1a00);
        f.word(MENU, base + 0x425bf80);
        f.word(MENU + 0x19a0, 0x32000);
        f.word(MENU + 0x19a8, 0x90000);
        f.put(POLITICS_WINDOW + 0xa5, &[0]);
        f.word(FOCUS + 0x2f8, FOCUS_ROOT);
        window(&mut f, FOCUS_ROOT, false);
        f.array(FOCUS + 0x1180, 0, 0);
        f.array(0x92dc0, 0, 0);
        f.number(0x91300, 0);
        f.word(0x91900, FACILITIES);
        f.word(0x91910, FACILITIES_PARENT);
        f.region(FACILITIES, 0x100);
        f.word(FACILITIES, base + 0x42f2708);
        f.word(FACILITIES + 0x60, FACILITIES_PARENT);
        f.word(FACILITIES + 0x68, 0x32000);
        f.word(FACILITIES + 0x70, FACILITIES_ROOT);
        window(&mut f, 0xa0000, shown);
        window(&mut f, 0xd0000, false);
        window(&mut f, FACILITIES_ROOT, false);
        window(&mut f, FACILITIES_PARENT, false);
        f.array(0xa0bb0, 0x334000, 1);
        f.word(0x334000, FACILITIES_PARENT);
        f.array(0xa0bc8, 0x335000, 1);
        f.string(0x335000, "facilities");
        f.array(FACILITIES_PARENT + 0xbb0, 0x336000, 1);
        f.word(0x336000, FACILITIES_ROOT);
        f.array(FACILITIES_PARENT + 0xbc8, 0x337000, 1);
        long_string(&mut f, 0x337000, "facilities_view_Instance", 0x338000);
        f
    }
    fn selected_folder(f: &mut Fixture) {
        let base = 0x10000;
        f.word(base + 0x43a40e0, 0x340000);
        f.word(0x340000, base + 0x42113f0);
        folder_catalog_key(f, 0x341000, "industry_folder", 0);
        f.array(0x360a8, 0x343000, 2);
        f.word(0x343000, 0x340000);
        f.word(0x343008, 0x341000);
        window(f, 0x342000, false);
        f.array(0x92d78, 0x344000, 1);
        f.word(0x344000, 0x342000);
        f.array(0x92d90, 0x345000, 1);
        f.word(0x345000, 0x349000);
        f.number(0x92e10, 0);
        f.array(0xd0bb0, 0x346000, 1);
        f.word(0x346000, 0x342000);
        f.array(0xd0bc8, 0x347000, 1);
        f.string(0x347000, "industry_folder");
    }
    #[test]
    fn research_close_producer_joins_complete_canonical_own_state_and_original_panel() {
        let mut f = fixture(true);
        selected_folder(&mut f);
        let observed = research_ui::closing::observe(&f).unwrap();
        assert_eq!(observed.context.campaign.player, "GER");
        assert_eq!(observed.context.slots, [0x2c000, 0x2c100, 0x2c200, 0x2c300]);
        assert_eq!(
            observed.binding.anchors,
            [0x20000, 0x28000, 0x70000, 0x90000, 0x91000, 0x92000]
        );
        assert_eq!(
            observed.binding.roots,
            [0xa0000, 0xd0000, FACILITIES, FACILITIES_ROOT]
        );
        assert_eq!(observed.binding.facilities_parent, FACILITIES_PARENT);
        assert_eq!(observed.binding.selected_slot_pointer, 0x2c100);
        assert_eq!(observed.binding.tab, 0);
        assert_eq!(observed.binding.folder_windows, [0x342000]);
        assert_eq!(observed.selected_folder.as_deref(), Some("industry_folder"));
        assert_eq!(
            observed.body["canonical_research"]["slots"][0]["researching"],
            "tools1"
        );
        assert_eq!(observed.body["own_slot_count"], 4);
        assert_eq!(observed.body["selected_slot"], 1);
        assert_eq!(observed.body["panel"]["visible_predicate"], true);
        assert_eq!(f.checks.get(), 2, "Exactly one process snapshot bracket");
        for pointer in [0x20000u64, 0x28000, 0xa0000, FACILITIES, FACILITIES_ROOT] {
            assert!(!observed.body.to_string().contains(&pointer.to_string()));
        }
    }
    #[test]
    fn research_close_producer_proves_closed_idle_with_unmaterialized_folder_selection() {
        let f = fixture(false);
        let observed = research_ui::closing::observe(&f).unwrap();
        assert_eq!(observed.body["active_mode_raw"], 23);
        assert_eq!(observed.body["panel"]["visible_predicate"], false);
        assert_eq!(observed.binding.folder_index, 0);
        assert!(observed.binding.folder_windows.is_empty());
        assert_eq!(observed.selected_folder, None);
        assert_eq!(observed.binding.selected_slot_pointer, 0x2c100);
        assert_eq!(f.checks.get(), 2);
    }
    #[test]
    fn research_close_producer_rejects_facilities_type_and_owner_before_child_payload() {
        for (field, value) in [
            (0, 0x10000 + 0x4347d58),
            (0x68, 0xdead0000),
            (0x60, 0xdead0000),
        ] {
            let mut f = fixture(true);
            f.word(FACILITIES + field, value);
            assert!(research_ui::closing::observe(&f).is_err());
            f.assert_not_read(&[
                FACILITIES + 0x70,
                FACILITIES_ROOT,
                FACILITIES_ROOT + 0x98,
                0xdead0000,
            ]);
            if field == 0 {
                f.assert_not_read(&[FACILITIES + 0x68, FACILITIES + 0x60]);
            }
        }
    }
    #[test]
    fn research_close_producer_tab_one_refuses_before_any_facilities_or_root_traversal() {
        let mut f = fixture(true);
        f.number(0x91300, 1);
        assert_eq!(
            research_ui::closing::observe(&f).err().unwrap().code,
            "research_precondition"
        );
        f.assert_not_read(&[
            0x91900,
            0x91910,
            0x912f8,
            FACILITIES,
            FACILITIES_PARENT,
            FACILITIES_ROOT,
        ]);
    }
    #[test]
    fn research_close_producer_final_tab_root_slot_and_assignment_witnesses_reject_torn_reads() {
        for (address, changed) in [
            (0x91300, 1i32.to_le_bytes().to_vec()),
            (0x91900, (FACILITIES + 8).to_le_bytes().to_vec()),
            (
                FACILITIES + 0x70,
                (FACILITIES_ROOT + 8).to_le_bytes().to_vec(),
            ),
            (0x92e08, 0x2c200u64.to_le_bytes().to_vec()),
            (0x2c018, 0u64.to_le_bytes().to_vec()),
        ] {
            let mut f = fixture(true);
            f.fault = Some((address, 1, changed));
            assert_eq!(
                research_ui::closing::observe(&f).err().unwrap().code,
                "torn_read",
                "address={address:x}"
            );
            assert!(f.fault_reads.get() >= 2);
        }
    }
    #[test]
    fn research_close_producer_engine_gui_and_common_window_blockers_refuse() {
        for (address, value) in [
            (0x903e1, 1),
            (0x328f0, 1),
            (0x30a59, 0),
            (0x70150, 1),
            (POLITICS_WINDOW + 0xa5, 8),
            (FOCUS_ROOT + 0xa5, 8),
        ] {
            let mut f = fixture(true);
            f.put(address, &[value]);
            assert_eq!(
                research_ui::closing::observe(&f).err().unwrap().code,
                "research_precondition",
                "address={address:x}"
            );
        }
    }
    #[test]
    fn research_close_producer_unsupported_string_abi_refuses_without_memory_reads() {
        struct Unsupported(Fixture);
        impl ReadMemory for Unsupported {
            fn read(&self, address: u64, size: usize) -> Result<Vec<u8>> {
                self.0.read(address, size)
            }
            fn string_abi(&self) -> hoi4_live_native::memory::StringAbi {
                hoi4_live_native::memory::StringAbi::LibCxx
            }
        }
        impl Process for Unsupported {
            fn image_base(&self) -> u64 {
                self.0.image_base()
            }
            fn check(&self) -> Result<()> {
                self.0.check()
            }
        }
        let unsupported = Unsupported(fixture(true));
        assert_eq!(
            research_ui::closing::observe(&unsupported)
                .err()
                .unwrap()
                .code,
            "capability_unavailable"
        );
        assert!(unsupported.0.tracked_reads.borrow().is_empty());
        assert_eq!(unsupported.0.checks.get(), 0);
    }
}
