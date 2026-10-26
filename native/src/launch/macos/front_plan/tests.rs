use super::*;

/// Fixture graph. `edges` are land adjacencies; `holders` give
/// (state, controller); the owner equals the controller.
fn graph(edges: &[(i32, i32)], holders: &[(i32, &str)]) -> StateGraph {
    let mut map = StaticMap::default();
    for (a, b) in edges {
        map.adj.entry(*a).or_default().insert(*b);
        map.adj.entry(*b).or_default().insert(*a);
        map.info.entry(*a).or_insert_with(|| StateInfo::blank(*a));
        map.info.entry(*b).or_insert_with(|| StateInfo::blank(*b));
    }
    for (s, _) in holders {
        map.info.entry(*s).or_insert_with(|| StateInfo::blank(*s));
    }
    let controller: BTreeMap<i32, String> =
        holders.iter().map(|(s, t)| (*s, t.to_string())).collect();
    StateGraph {
        map: Arc::new(map),
        player: "GER".into(),
        enemies: ["POL".to_string()].into(),
        owner: controller.clone(),
        controller,
        divisions: BTreeMap::new(),
    }
}

fn edit(g: &mut StateGraph, f: impl FnOnce(&mut StaticMap)) {
    f(Arc::get_mut(&mut g.map).unwrap());
}

fn link(g: &mut StateGraph, a: i32, b: i32) {
    edit(g, |m| {
        m.adj.entry(a).or_default().insert(b);
        m.adj.entry(b).or_default().insert(a);
        m.info.entry(a).or_insert_with(|| StateInfo::blank(a));
        m.info.entry(b).or_insert_with(|| StateInfo::blank(b));
    });
}

fn ids(v: &Value) -> Vec<i64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s["state_id"].as_i64().unwrap())
        .collect()
}

#[test]
fn parses_state_files_with_bunkers_points_and_dated_blocks() {
    let text = "state={\n id=28\n manpower = 1915600\n state_category = city\n resources={ steel=42 coal = 5 }\n history={\n owner = FRA\n victory_points = { 678 5 }\n victory_points = { 9559 10 }\n add_core_of = FRA\n buildings = {\n infrastructure = 4\n industrial_complex = 3\n 549 = { bunker = 10 }\n 678 = { bunker = 10 coastal_bunker = 2 }\n }\n 1939.1.1 = { owner = GER victory_points = { 1 99 } }\n }\n provinces={ 549 678 9559 11502 }\n }";
    let s = parse_state(text).unwrap();
    assert_eq!(s.id, 28);
    assert_eq!(s.category, "city");
    assert_eq!(s.vp_sum(), 15, "dated blocks are ignored");
    assert_eq!((s.infra, s.civ, s.mil), (4, 3, 0));
    assert!((s.bunker - 5.0).abs() < 1e-9);
    assert_eq!(s.resources["steel"], 42);
    assert_eq!(s.initial_owner.as_deref(), Some("FRA"));
    assert_eq!(s.cores, vec!["FRA"]);
    assert_eq!(s.provinces.len(), 4);
    let imp = parse_state("state={ id=7 impassable = yes provinces={ 1 } history={} }").unwrap();
    assert!(imp.impassable);
}

#[test]
fn parses_terrain_table_and_definition_rows() {
    let t = parse_terrain_table(
        "categories = {\n forest = { movement_cost = 1.5 units = { attack = -0.15 } }\n plains = { movement_cost = 1.0 }\n}",
    );
    assert_eq!(t["forest"], (1.5, 0.15));
    assert_eq!(t["plains"], (1.0, 0.0));
    let rows = parse_definition_rows(
        "1;230;81;119;lake;false;lakes;7\n2;0;0;55;land;false;forest;1\nbad\n",
    );
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].terrain, "forest");
    assert_eq!(rows[1].rgb, 55);
    let imp = parse_impassable("From;To;Type\n10825;1991;impassable;-1\n1;2;sea;3\n");
    assert!(imp.contains(&(1991, 10825)) && !imp.contains(&(1, 2)));
}

fn province_rows() -> Vec<ProvinceRow> {
    let r = |id: i32, kind: &str| ProvinceRow {
        id,
        rgb: id as u32,
        kind: kind.into(),
        terrain: "plains".into(),
    };
    vec![
        r(1, "land"),
        r(2, "land"),
        r(3, "land"),
        r(4, "sea"),
        r(5, "lake"),
    ]
}

#[test]
fn sea_is_not_adjacency_but_marks_coast_and_impassable_rows_cut_edges() {
    let edges: BTreeSet<(i32, i32)> = [(1, 2), (2, 3), (1, 4), (3, 5), (1, 3)].into();
    let state_of: HashMap<i32, i32> = [(1, 10), (2, 20), (3, 30)].into();
    let imp: HashSet<(i32, i32)> = [(1, 3), (3, 1)].into();
    let (adj, coastal) = fold_map(&edges, &province_rows(), &imp, &state_of);
    assert_eq!(adj[&10], [20].into());
    assert_eq!(adj[&20], [10, 30].into());
    assert!(!adj[&30].contains(&10), "impassable pair removed");
    assert_eq!(
        coastal,
        [10].into(),
        "sea contact marks coast; lakes do not"
    );
}

#[test]
fn terrain_is_averaged_over_provinces() {
    let mut s = StateInfo::blank(1);
    s.provinces = vec![1, 2, 3];
    let terrain: HashMap<i32, String> = [
        (1, "mountain".to_string()),
        (2, "mountain".into()),
        (3, "urban".into()),
    ]
    .into();
    let table: HashMap<String, (f64, f64)> = [
        ("mountain".to_string(), (2.0, 0.3)),
        ("urban".into(), (1.0, 0.0)),
    ]
    .into();
    apply_terrain(&mut s, &terrain, &table);
    assert_eq!(s.terrain, "mountain");
    assert!((s.move_cost - 5.0 / 3.0).abs() < 1e-9);
    assert!((s.urban - 1.0 / 3.0).abs() < 1e-9);
}

// Layout: G1 - P10 - P11 - P12 (capital) - P13; G3 - P11. Taking P11 (the
// neck) cuts P10 off from the bulk of the enemy, but only G3 touches the neck.
fn salient() -> StateGraph {
    let mut g = graph(
        &[(1, 10), (2, 10), (10, 11), (3, 11), (11, 12), (12, 13)],
        &[
            (1, "GER"),
            (2, "GER"),
            (3, "GER"),
            (10, "POL"),
            (11, "POL"),
            (12, "POL"),
            (13, "POL"),
        ],
    );
    edit(&mut g, |m| {
        m.capitals.insert("POL".into(), 12);
        m.info.get_mut(&10).unwrap().vp = vec![(1, 20)];
        for (id, pos) in [
            (1, (0.0, 10.0)),
            (2, (0.0, -10.0)),
            (3, (30.0, 40.0)),
            (10, (10.0, 0.0)),
            (11, (30.0, 0.0)),
            (12, (60.0, 0.0)),
        ] {
            m.centers.insert(id, pos);
        }
    });
    g
}

#[test]
fn a_pocket_behind_a_single_neck_needs_a_second_own_side() {
    let out = encirclement_candidates(&salient(), &[], 2, 6);
    let p = &out["pockets"][0];
    assert_eq!(ids(&p["pocket_states"]), vec![10]);
    assert_eq!(ids(&p["neck_states"]), vec![11]);
    assert_eq!(p["capital_inside"], false);
    assert_eq!(p["pincer_ready"], false, "only G3 touches the neck: {p}");
    assert!(p["note"].as_str().unwrap().contains("second side"));
}

#[test]
fn two_own_flanks_on_the_neck_make_a_ready_pincer() {
    let mut g = salient();
    link(&mut g, 4, 11);
    edit(&mut g, |m| {
        m.centers.insert(4, (30.0, -40.0));
    });
    g.controller.insert(4, "GER".into());
    g.owner.insert(4, "GER".into());
    let armies = [ArmyRef {
        name: "North".into(),
        main_state: Some(4),
        divisions: 6,
    }];
    let out = encirclement_candidates(&g, &armies, 2, 6);
    let p = &out["pockets"][0];
    assert_eq!(ids(&p["neck_states"]), vec![11]);
    assert_eq!(p["pincer_ready"], true);
    let arms = p["pincer_arms"].as_array().unwrap();
    assert_eq!(arms.len(), 2);
    let froms: BTreeSet<i64> = arms
        .iter()
        .map(|a| a["from_own_state"]["state_id"].as_i64().unwrap())
        .collect();
    assert_eq!(froms, [3, 4].into(), "the two flanks farthest apart");
    assert_eq!(arms[0]["into_neck_state"]["state_id"], 11);
    assert!(arms.iter().any(|a| a["army_candidate"]["army"] == "North"));
    let calls = p["suggested_calls"].as_array().unwrap();
    assert_eq!(calls[0]["tool"], "hoi4_army_front");
    assert_eq!(calls[0]["args"], json!({"army": "North", "state_id": 11}));
}

#[test]
fn the_pocket_holding_the_capital_is_marked() {
    // G1 - P10 (capital) - P11 - P12 - P13: the capital is the small side.
    let mut g = graph(
        &[(1, 10), (2, 11), (10, 11), (11, 12), (3, 12), (12, 13)],
        &[
            (1, "GER"),
            (2, "GER"),
            (3, "GER"),
            (10, "POL"),
            (11, "POL"),
            (12, "POL"),
            (13, "POL"),
        ],
    );
    edit(&mut g, |m| {
        m.capitals.insert("POL".into(), 10);
        m.info.get_mut(&12).unwrap().civ = 30;
    });
    let out = encirclement_candidates(&g, &[], 1, 6);
    let found = out["pockets"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["capital_inside"] == true);
    assert!(found, "{out}");
}

#[test]
fn impassable_and_third_party_states_are_walls_not_enemy_connections() {
    // P10 - X20 - P30 - P31 with X20 impassable: two separate enemy blocks.
    let mut g = graph(
        &[(1, 10), (10, 20), (20, 30), (30, 31), (2, 31)],
        &[
            (1, "GER"),
            (2, "GER"),
            (10, "POL"),
            (20, "POL"),
            (30, "POL"),
            (31, "POL"),
        ],
    );
    edit(&mut g, |m| {
        m.info.get_mut(&20).unwrap().impassable = true;
        m.capitals.insert("POL".into(), 31);
    });
    let out = encirclement_candidates(&g, &[], 0, 6);
    let p = &out["pockets"][0];
    assert_eq!(p["already_cut_off"], true);
    assert_eq!(ids(&p["pocket_states"]), vec![10]);
    // A neutral state in the middle likewise splits the enemy.
    let mut h = graph(
        &[(1, 10), (10, 20), (20, 30), (30, 31), (2, 31)],
        &[
            (1, "GER"),
            (2, "GER"),
            (10, "POL"),
            (20, "FRA"),
            (30, "POL"),
            (31, "POL"),
        ],
    );
    edit(&mut h, |m| {
        m.capitals.insert("POL".into(), 31);
    });
    let out = encirclement_candidates(&h, &[], 0, 6);
    let p = &out["pockets"][0];
    assert_eq!(ids(&p["pocket_states"]), vec![10]);
    assert_eq!(p["third_party_neighbours"], json!([20]));
}

#[test]
fn islands_with_no_own_contact_are_not_candidates() {
    // The enemy island P50-P51 touches nobody of ours.
    let g = graph(
        &[(1, 10), (10, 11), (50, 51)],
        &[
            (1, "GER"),
            (10, "POL"),
            (11, "POL"),
            (50, "POL"),
            (51, "POL"),
        ],
    );
    let out = encirclement_candidates(&g, &[], 0, 6);
    assert_eq!(out["pockets_found"], 0);
}

// Front plan layout:
//  G1 - P10 (mountain) - P13 - P20 (capital)
//  G2 - P11 (plains)   - P12 - P20
//  G5 - P30 (separate northern front, forest, dead end)
//  G2 - G6 - G7 (own capital)
fn theatre() -> StateGraph {
    let mut g = graph(
        &[
            (1, 10),
            (2, 11),
            (10, 13),
            (13, 20),
            (11, 12),
            (12, 20),
            (5, 30),
            (1, 2),
            (2, 5),
            (2, 6),
            (6, 7),
        ],
        &[
            (1, "GER"),
            (2, "GER"),
            (5, "GER"),
            (6, "GER"),
            (7, "GER"),
            (10, "POL"),
            (11, "POL"),
            (12, "POL"),
            (13, "POL"),
            (20, "POL"),
            (30, "POL"),
        ],
    );
    edit(&mut g, |m| {
        m.capitals.insert("POL".into(), 20);
        m.capitals.insert("GER".into(), 7);
        let t = m.info.get_mut(&10).unwrap();
        t.terrain = "mountain".into();
        t.move_cost = 2.0;
        t.malus = 0.3;
        m.info.get_mut(&20).unwrap().vp = vec![(1, 30)];
        m.info.get_mut(&20).unwrap().civ = 4;
        let f = m.info.get_mut(&30).unwrap();
        f.terrain = "forest".into();
        f.move_cost = 1.5;
    });
    g.divisions.insert(2, 3);
    g.divisions.insert(1, 2);
    g
}

#[test]
fn front_plan_prefers_the_open_route_and_names_concrete_calls() {
    let g = theatre();
    let armies = vec![ArmyRef {
        name: "1st Army".into(),
        main_state: Some(2),
        divisions: 5,
    }];
    let out = front_plan(&g, &armies, 3);
    let fronts = out["fronts"].as_array().unwrap();
    // Fronts: {G1-P10}, {G2-P11}, {G5-P30}; borders sharing no state are separate.
    assert_eq!(fronts.len(), 3);
    let attack = fronts.iter().find(|f| f["role"] == "attack").unwrap();
    let axis = &attack["axes"][0];
    assert_eq!(axis["target_state"], 20);
    assert_eq!(axis["target_is_capital"], true);
    assert_eq!(
        axis["entry_state"], 11,
        "plains route beats the mountain: {axis}"
    );
    assert_eq!(out["main_effort_front"], attack["front"]);
    let calls = attack["suggested_calls"].as_array().unwrap();
    let tools: Vec<&str> = calls.iter().map(|c| c["tool"].as_str().unwrap()).collect();
    assert_eq!(
        &tools[..3],
        ["hoi4_army_front", "hoi4_army_offensive", "hoi4_army_plan"]
    );
    assert_eq!(
        calls[0]["args"],
        json!({"army": "1st Army", "state_id": 11})
    );
    assert_eq!(
        calls[1]["args"],
        json!({"army": "1st Army", "state_id": 11})
    );
    assert_eq!(
        calls[2]["args"],
        json!({"army": "1st Army", "execute": true})
    );
    assert_eq!(out["calls_also_need"], json!(["request_id", "lease_token"]));
    let entries: Vec<i64> = fronts
        .iter()
        .flat_map(|f| f["axes"].as_array().unwrap().iter())
        .map(|a| a["entry_state"].as_i64().unwrap())
        .collect();
    assert!(entries.contains(&10) && entries.contains(&30));
}

#[test]
fn front_plan_counts_divisions_and_reports_what_is_missing() {
    let g = theatre();
    let out = front_plan(&g, &[], 3);
    let fronts = out["fronts"].as_array().unwrap();
    for f in fronts {
        let d = &f["divisions"];
        let have = d["within_2_states"].as_u64().unwrap();
        let need = d["required_total"].as_u64().unwrap();
        assert_eq!(d["missing"].as_u64().unwrap(), need.saturating_sub(have));
    }
    assert!(out["divisions_missing_total"].as_u64().unwrap() > 0);
    assert_eq!(out["armies_known"], false);
    let hold = fronts.iter().find(|f| f["role"] == "hold").unwrap();
    assert!(!hold["defensive_line"].as_array().unwrap().is_empty());
    assert!(!out["other_fronts_defensive_lines"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn front_plan_reports_enemies_without_a_land_border() {
    let mut g = theatre();
    g.enemies.insert("SOV".into());
    let out = front_plan(&g, &[], 3);
    assert_eq!(out["enemies_without_land_border"], json!(["SOV"]));
}

#[test]
fn a_fortified_state_costs_more_so_the_route_bypasses_it() {
    let mut g = theatre();
    edit(&mut g, |m| {
        m.info.get_mut(&11).unwrap().bunker = 10.0; // Maginot-style province bunkers
        let t = m.info.get_mut(&10).unwrap();
        t.malus = 0.0;
        t.move_cost = 1.0;
    });
    let out = front_plan(&g, &[], 3);
    let mut capital_axes: Vec<(f64, i64)> = out["fronts"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|f| f["axes"].as_array().unwrap().iter())
        .filter(|a| a["target_state"] == 20)
        .map(|a| {
            (
                a["score"].as_f64().unwrap(),
                a["entry_state"].as_i64().unwrap(),
            )
        })
        .collect();
    capital_axes.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    assert_eq!(capital_axes[0].1, 10, "{capital_axes:?}");
    assert!(g.attack_need(11) > g.attack_need(10));
    assert!(g.enter_cost(11) > g.enter_cost(10));
}

#[test]
fn impassable_states_are_never_on_a_route() {
    let mut g = theatre();
    edit(&mut g, |m| {
        m.info.get_mut(&11).unwrap().impassable = true;
        m.info.get_mut(&13).unwrap().impassable = true;
    });
    let out = front_plan(&g, &[], 3);
    let routes: Vec<i64> = out["fronts"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|f| f["axes"].as_array().unwrap().iter())
        .flat_map(|a| ids(&a["route"]))
        .collect();
    assert!(!routes.contains(&11) && !routes.contains(&13));
    assert!(out["fronts"].as_array().unwrap().iter().all(|f| f["axes"]
        .as_array()
        .unwrap()
        .iter()
        .all(|a| a["target_state"] != 20)));
}

#[test]
fn war_goals_rank_value_flag_occupation_and_core_states() {
    let mut g = theatre();
    edit(&mut g, |m| {
        m.info.get_mut(&12).unwrap().mil = 6;
        m.info
            .get_mut(&12)
            .unwrap()
            .resources
            .insert("oil".into(), 20);
        m.info.get_mut(&13).unwrap().cores = vec!["GER".into()];
    });
    g.controller.insert(13, "GER".into());
    let out = war_goal_targets(&g, 25);
    let t = out["targets"].as_array().unwrap();
    assert_eq!(t[0]["state_id"], 20, "the capital first");
    let s13 = t.iter().find(|s| s["state_id"] == 13).unwrap();
    assert_eq!(s13["already_occupied_by_us"], true);
    assert_eq!(s13["own_core"], true);
    let prizes = out["resource_prizes"].as_array().unwrap();
    assert_eq!(prizes[0]["resource"], "oil");
    assert!(t.iter().any(|s| s["tier"] == "must_take"));
    assert_eq!(out["enemy_states_total"], 6);
}

#[test]
fn enemy_argument_accepts_tag_list_and_array_and_validates() {
    let m = |v: Value| parse_enemies(v.as_object().unwrap());
    assert_eq!(
        m(json!({"enemy": "POL"})).unwrap(),
        ["POL".to_string()].into()
    );
    assert_eq!(
        m(json!({"enemy": "FRA, BEL"})).unwrap(),
        ["FRA".to_string(), "BEL".into()].into()
    );
    assert_eq!(m(json!({"enemy": ["CZE"]})).unwrap().len(), 1);
    for bad in [
        json!({}),
        json!({"enemy": "pol"}),
        json!({"enemy": []}),
        json!({"enemy": 5}),
        json!({"enemy": ["POLL"]}),
    ] {
        assert!(m(bad).is_err());
    }
}

/// Manual check against the real install (1936 start ownership):
/// `HOI4_GAME_DIR=... cargo test --release real_install_war_plans -- --ignored --nocapture`
#[test]
#[ignore]
fn real_install_war_plans_print_for_manual_comparison() {
    let dir = std::env::var("HOI4_GAME_DIR").expect("HOI4_GAME_DIR");
    let started = Instant::now();
    let map = static_map(Path::new(&dir)).unwrap();
    eprintln!(
        "static map: {} states, {} adjacency rows, {} capitals in {:?}",
        map.info.len(),
        map.adj.len(),
        map.capitals.len(),
        started.elapsed()
    );
    for (a, set) in &map.adj {
        for b in set {
            assert!(map.adj[b].contains(a), "asymmetric {a}-{b}");
        }
    }
    let start_owner: BTreeMap<i32, String> = map
        .info
        .values()
        .filter_map(|s| Some((s.id, s.initial_owner.clone()?)))
        .collect();
    let graph = |player: &str, enemies: &[&str]| StateGraph {
        map: map.clone(),
        player: player.into(),
        enemies: enemies.iter().map(|s| s.to_string()).collect(),
        owner: start_owner.clone(),
        controller: start_owner.clone(),
        divisions: BTreeMap::new(),
    };
    let show = |v: &Value| {
        v.as_array()
            .unwrap()
            .iter()
            .map(|s| {
                format!(
                    "{}:{}({})",
                    s["state_id"],
                    s["name"].as_str().unwrap_or("?"),
                    s["terrain"].as_str().unwrap_or("?")
                )
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    for enemies in [
        vec!["POL"],
        vec!["CZE"],
        vec!["FRA"],
        vec!["FRA", "BEL", "HOL"],
    ] {
        let g = graph("GER", &enemies);
        let started = Instant::now();
        let out = front_plan(&g, &[], 3);
        eprintln!(
            "\n=== front_plan GER vs {enemies:?} ({:?}) ===",
            started.elapsed()
        );
        for f in out["fronts"].as_array().unwrap() {
            eprintln!(
                "front {} role {} own_border {}\n  enemy_border {}",
                f["front"],
                f["role"],
                show(&f["own_border_states"]),
                show(&f["enemy_border_states"])
            );
            for a in f["axes"].as_array().unwrap() {
                eprintln!(
                    "  axis {} score {} cost {} need {} -> {} ({}){}\n    route {}",
                    a["rank"],
                    a["score"],
                    a["route_cost"],
                    a["divisions_needed"],
                    a["target_name"],
                    a["target_state"],
                    if a["target_is_capital"] == true {
                        " CAPITAL"
                    } else {
                        ""
                    },
                    show(&a["route"])
                );
            }
            eprintln!("  hold line {}", show(&f["defensive_line"]));
        }
    }
    for enemies in [vec!["POL"], vec!["CZE"]] {
        let g = graph("GER", &enemies);
        let started = Instant::now();
        let out = encirclement_candidates(&g, &[], 3, 6);
        eprintln!(
            "\n=== encirclement GER vs {enemies:?}: {} pockets ({:?}) ===",
            out["pockets_found"],
            started.elapsed()
        );
        for p in out["pockets"].as_array().unwrap() {
            eprintln!(
                "#{} score {} value {} gap {} ready {} need/arm {} capital {}\n  pocket {}\n  neck {}\n  arms {}",
                p["rank"],
                p["score"],
                p["value"],
                p["gap"],
                p["pincer_ready"],
                p["divisions_needed_per_arm"],
                p["capital_inside"],
                show(&p["pocket_states"]),
                show(&p["neck_states"]),
                p["pincer_arms"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|a| format!(
                        "{} -> {}",
                        a["from_own_state"]["state_id"], a["into_neck_state"]["state_id"]
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
    // Worst case for the neck search: a wide coalition at maximum gap.
    let g = graph(
        "GER",
        &["POL", "SOV", "CZE", "FRA", "BEL", "HOL", "DEN", "LIT"],
    );
    let started = Instant::now();
    let out = encirclement_candidates(&g, &[], 3, 6);
    eprintln!(
        "\n=== coalition encirclement: {} pockets in {:?} ===",
        out["pockets_found"],
        started.elapsed()
    );
    let started = Instant::now();
    let _ = front_plan(&g, &[], 3);
    eprintln!("coalition front_plan {:?}", started.elapsed());
    let g = graph("GER", &["POL"]);
    let out = war_goal_targets(&g, 10);
    eprintln!("\n=== war goals GER vs POL ===");
    for t in out["targets"].as_array().unwrap() {
        eprintln!(
            "{} {} tier {} score {} vp {} industry {}",
            t["state_id"], t["name"], t["tier"], t["score"], t["victory_points"], t["industry"]
        );
    }
    assert!(map.info.len() > 800);
    assert!(map.capitals.contains_key("GER"));
}
