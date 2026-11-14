use super::*;
use serde_json::json;

/// Focus, state and expected answer per case: data, so another reader of the same files can be
/// held to the same answers.
const CASES: &str = include_str!("../../../tests/fixtures/focus_availability_cases.json");
/// A mod's focus tree and scripted triggers (tags, states and DLC that the base game does not have).
const MOD_TREE: &str = include_str!(
    "../../../tests/fixtures/focus_mod/user/mod/zeta_overhaul/common/national_focus/zeta.txt"
);
const MOD_TRIGGERS: &str = include_str!(
    "../../../tests/fixtures/focus_mod/user/mod/zeta_overhaul/common/scripted_triggers/zeta_triggers.txt"
);

const MOD_EFFECTS: &str = include_str!(
    "../../../tests/fixtures/focus_mod/user/mod/zeta_overhaul/common/scripted_effects/zeta_effects.txt"
);

fn mod_catalog() -> Catalog {
    let mut catalog = Catalog::default();
    catalog.add_focus_file(MOD_TREE);
    catalog.add_scripted_file(MOD_TRIGGERS);
    catalog.add_effects(MOD_EFFECTS);
    catalog
}

fn verdict(catalog: &Catalog, focus: &str, state: Value) -> Verdict {
    catalog.verdict(focus, &Snapshot(state))
}

#[test]
fn the_shared_cases_hold() {
    let cases: Value = serde_json::from_str(CASES).unwrap();
    assert_eq!(
        cases["focus_files"],
        json!(["focus_mod/user/mod/zeta_overhaul/common/national_focus/zeta.txt"])
    );
    assert_eq!(
        cases["scripted_files"],
        json!(["focus_mod/user/mod/zeta_overhaul/common/scripted_triggers/zeta_triggers.txt"])
    );
    assert_eq!(
        cases["effect_files"],
        json!(["focus_mod/user/mod/zeta_overhaul/common/scripted_effects/zeta_effects.txt"])
    );
    let catalog = mod_catalog();
    let list = cases["cases"].as_array().unwrap();
    assert!(list.len() >= 60);
    let mut seen = HashSet::new();
    for case in list {
        let name = case["name"].as_str().unwrap();
        let got = verdict(
            &catalog,
            case["focus"].as_str().unwrap(),
            case["state"].clone(),
        );
        let expect = &case["expect"];
        assert_eq!(got.state.label(), expect["state"], "{name}: state");
        assert_eq!(json!(got.why), expect["why"], "{name}: why");
        assert_eq!(json!(got.unknown), expect["unknown"], "{name}: unknown");
        assert_eq!(json!(got.bypass), expect["bypass"], "{name}: bypass");
        seen.insert(got.state.label());
    }
    // The cases cover every answer.
    assert_eq!(seen.len(), 3);
}

#[test]
fn a_focus_that_needs_peace_is_unavailable_in_a_war_without_looking_at_the_game() {
    let catalog = mod_catalog();
    let at_war = json!({"tag": "ZZA", "completed": ["ZZA_root"], "wars": ["QQB"]});
    let got = verdict(&catalog, "ZZA_peace_works", at_war);
    assert_eq!(got.state, State::Unavailable);
    assert_eq!(got.why, ["needs has_war = no"]);
    let at_peace = json!({"tag": "ZZA", "completed": ["ZZA_root"], "wars": []});
    assert_eq!(
        verdict(&catalog, "ZZA_peace_works", at_peace).state,
        State::Available
    );
}

#[test]
fn a_trigger_that_cannot_be_read_is_unknown_never_unavailable() {
    let catalog = mod_catalog();
    let state = json!({"tag": "ZZA", "completed": [], "wars": []});
    let got = verdict(&catalog, "ZZA_flagged", state.clone());
    assert_eq!(got.state, State::Unknown);
    assert_eq!(got.unknown, ["has_country_flag = zza_secret"]);
    assert!(got.why.is_empty());
    // A scope over other countries is not read either.
    assert_eq!(
        verdict(&catalog, "ZZA_neighbours", state).state,
        State::Unknown
    );
}

#[test]
fn nothing_known_gives_unknown_for_every_conditional_focus() {
    let catalog = mod_catalog();
    let nothing = json!({});
    for id in [
        "ZZA_peace_works",
        "ZZA_war_plan",
        "ZZA_left",
        "ZZA_flagged",
        "ZZA_dlc_branch",
        "ZZA_bypassed",
        "ZZA_late",
        "ZZA_state",
        "ZZA_numbers",
        "ZZS_shared",
    ] {
        assert_eq!(
            verdict(&catalog, id, nothing.clone()).state,
            State::Unknown,
            "{id}"
        );
    }
}

#[test]
fn a_reading_inside_its_margin_does_not_decide() {
    struct Rounded;
    impl Facts for Rounded {
        fn player(&self) -> Option<String> {
            Some("ZZA".into())
        }
        fn date(&self) -> Option<(i64, i64, i64)> {
            None
        }
        fn dlc(&self, _: &str) -> Truth {
            Truth::Unknown
        }
        fn completed(&self, _: &str) -> Truth {
            Truth::False
        }
        fn in_tree(&self, _: &str) -> Truth {
            Truth::True
        }
        fn at_war(&self) -> Truth {
            Truth::False
        }
        fn at_war_with(&self, _: &str) -> Truth {
            Truth::False
        }
        fn exists(&self, _: &str) -> Truth {
            Truth::Unknown
        }
        fn state(&self, _: i64) -> Option<Holders> {
            None
        }
        fn number(&self, name: &str) -> Option<(f64, f64)> {
            match name {
                // The top bar shows whole percent: 0.50 may be 0.495 or 0.505.
                "stability" => Some((0.50, 0.01)),
                "political_power" => Some((150.0, 0.0)),
                _ => None,
            }
        }
        fn tech(&self, _: &str) -> Truth {
            Truth::Unknown
        }
        fn faction(&self) -> Option<Vec<String>> {
            None
        }
        fn overlord(&self) -> Option<Option<String>> {
            None
        }
        fn subjects(&self) -> Option<Vec<String>> {
            None
        }
        fn government(&self) -> Option<String> {
            None
        }
    }
    let got = mod_catalog().verdict("ZZA_numbers", &Rounded);
    assert_eq!(got.state, State::Unknown);
    assert_eq!(got.unknown, ["has_stability > 0.5"]);
}

#[test]
fn the_catalogue_is_read_from_a_content_folder() {
    let dir = std::env::temp_dir().join(format!("focus-rules-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let focus = dir.join("common/national_focus");
    let triggers = dir.join("common/scripted_triggers");
    std::fs::create_dir_all(&focus).unwrap();
    std::fs::create_dir_all(&triggers).unwrap();
    std::fs::write(focus.join("zeta.txt"), MOD_TREE).unwrap();
    std::fs::write(
        focus.join("notes.md"),
        "focus_tree = { focus = { id = NOT_READ } }",
    )
    .unwrap();
    std::fs::write(triggers.join("zeta_triggers.txt"), MOD_TRIGGERS).unwrap();
    // An unlock effect can sit in any script file: here in an event, two folders down.
    let events = dir.join("events/zeta");
    std::fs::create_dir_all(&events).unwrap();
    std::fs::write(
        events.join("zeta_events.txt"),
        "country_event = { id = zeta.1 option = { unlock_national_focus = ZZA_never } }",
    )
    .unwrap();
    let catalog = Catalog::load(&dir);
    let peace = json!({"tag": "ZZA", "completed": [], "wars": []});
    assert_eq!(
        verdict(&catalog, "ZZA_never", peace.clone()).state,
        State::Unknown
    );
    assert_eq!(
        verdict(&mod_catalog(), "ZZA_never", peace).state,
        State::Unavailable
    );
    assert!(catalog.focus("ZZA_peace_works").is_some());
    assert!(catalog.focus("ZZS_shared").is_some());
    assert!(catalog.focus("NOT_READ").is_none());
    assert_eq!(
        catalog.focus("ZZA_either").unwrap().prerequisites,
        [
            vec!["ZZA_left".to_string(), "ZZA_right".to_string()],
            vec!["ZZA_peace_works".to_string()]
        ]
    );
    let ready = json!({"tag": "ZZA", "completed": [], "wars": [], "techs": ["zza_radio"]});
    assert_eq!(
        verdict(&catalog, "ZZA_scripted", ready).state,
        State::Available
    );
    // A folder without focus files gives an empty catalogue: every focus is unknown.
    let empty = Catalog::load(&dir.join("common/scripted_triggers"));
    assert!(empty.is_empty());
    assert_eq!(
        verdict(&empty, "ZZA_root", json!({"tag": "ZZA"})).state,
        State::Unknown
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rendered_triggers_are_bounded() {
    let long = format!(
        "focus_tree = {{ focus = {{ id = LONG available = {{ OR = {{ {} }} }} }} }}",
        (0..40)
            .map(|i| format!("has_country_flag = some_long_flag_name_{i}"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let mut catalog = Catalog::default();
    catalog.add_focus_file(&long);
    let got = verdict(&catalog, "LONG", json!({"tag": "ZZA", "completed": []}));
    assert_eq!(got.state, State::Unknown);
    assert_eq!(got.unknown.len(), REASONS);
    assert!(got.unknown.iter().all(|u| u.len() <= RENDER));
}

#[test]
fn dlcs_come_from_the_install_folders_and_the_disabled_list() {
    use super::live::Dlcs;
    let dir = std::env::temp_dir().join(format!("focus-dlcs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (path, name) in [
        ("integrated_dlc/dlc018_built_in/dlc018.dlc", "Built In"),
        ("dlc/dlc028_switched_on/dlc028.dlc", "Switched On"),
        ("dlc/dlc034_switched_off/dlc034.dlc", "Switched Off"),
    ] {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, format!("name = \"{name}\"\npath = \"x\"\n")).unwrap();
    }
    let off = vec!["dlc/dlc034_switched_off.dlc".to_string()];
    let known = Dlcs::read(&dir, Some(&off));
    assert_eq!(known.has("Built In"), Truth::True);
    assert_eq!(known.has("Switched On"), Truth::True);
    assert_eq!(known.has("Switched Off"), Truth::False);
    assert_eq!(known.has("Never Installed"), Truth::False);
    // Without the user folder's list a DLC in dlc/ is neither surely on nor off.
    let unread = Dlcs::read(&dir, None);
    assert_eq!(unread.has("Built In"), Truth::True);
    assert_eq!(unread.has("Switched On"), Truth::Unknown);
    assert_eq!(unread.has("Never Installed"), Truth::False);
    // An install whose DLC folders cannot be listed says nothing about any DLC.
    let nowhere = Dlcs::read(&dir.join("missing"), Some(&[]));
    assert_eq!(nowhere.has("Built In"), Truth::Unknown);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_game_hour_gives_the_calendar_date() {
    use super::live::ymd;
    let hour = |text: &str| crate::plan_predicate::date_hour(text).unwrap();
    assert_eq!(ymd(hour("1936.01.01")), Some((1936, 1, 1)));
    assert_eq!(ymd(hour("1936.01.01") + 23), Some((1936, 1, 1)));
    assert_eq!(ymd(hour("1936.12.31")), Some((1936, 12, 31)));
    assert_eq!(ymd(hour("1939.09.01") + 12), Some((1939, 9, 1)));
    assert_eq!(ymd(hour("1940.03.01") - 1), Some((1940, 2, 28)));
    assert_eq!(ymd(-1), None);
}

#[test]
fn live_facts_come_from_the_read_tools() {
    use super::live::{report, Live};
    use crate::plan_predicate::Ctx;
    use crate::plan_run::Tools;
    struct Game;
    impl Tools for Game {
        fn call(&self, name: &str, args: &Value) -> crate::error::Result<Value> {
            Ok(match name {
                "hoi4_overview" => json!({"player": "ZZA"}),
                "hoi4_focus_completed" => json!({"completed": ["ZZA_root"], "next_offset": null}),
                "hoi4_focus_list" => {
                    let key = args["filter"].as_str().unwrap();
                    // The loaded tree has every focus but this one.
                    let rows = if key == "ZZA_left" {
                        json!([])
                    } else {
                        json!([{"key": key}])
                    };
                    json!({"focuses": rows, "next_offset": null})
                }
                "hoi4_wars" => json!({"wars": {"at_war": true, "enemies": ["QQB"]}}),
                "hoi4_states" => {
                    json!({"states": [{"state_id": 9001, "owner": "ZZA", "controller": "ZZA",
                    "provinces": [{"controller": "ZZA"}, {"controller": "QQB"}]}]})
                }
                "hoi4_country_status" => json!({"fields": {"stability": {"value": 0.62}}}),
                "hoi4_economy" => json!({"political_power": {"raw_signed": 15_000_000}}),
                "hoi4_diplomacy_state" => json!({
                    "faction": {"in_faction": true, "faction": {"unresolved_members": 0,
                        "members": [{"tag": "ZZA"}, {"tag": "QQC"}]}},
                    "overlord": [], "subjects": [{"with": {"tag": "QQD"}, "autonomy": 1}]}),
                _ => {
                    return Err(crate::error::Error::new(
                        "capability_unavailable",
                        "no such read",
                    ))
                }
            })
        }
    }
    let catalog = mod_catalog();
    let mut ctx = Ctx::new(
        &Game,
        crate::plan_predicate::date_hour("1936.03.01").unwrap(),
    );
    let wars = ctx.war_enemies();
    let facts = Live::new(&mut ctx, None);
    assert_eq!(facts.faction(), Some(vec!["QQC".to_string()]));
    assert_eq!(facts.overlord(), Some(None));
    assert_eq!(facts.subjects(), Some(vec!["QQD".to_string()]));
    assert_eq!(
        facts.state(9001),
        Some(Holders {
            owner: Some("ZZA".into()),
            controller: Some("ZZA".into()),
            full_controller: Some(None),
        })
    );
    let keys: Vec<String> = [
        "ZZA_peace_works",
        "ZZA_war_plan",
        "ZZA_left",
        "ZZA_numbers",
        "ZZA_faction",
        "ZZA_flagged",
        "ZZA_dlc_branch",
        "ZZA_late",
    ]
    .iter()
    .map(|k| k.to_string())
    .collect();
    let reply = report(&catalog, &facts, &keys, wars);
    assert_eq!(reply["player"], "ZZA");
    assert_eq!(reply["date"], "1936.3.1");
    assert_eq!(reply["wars"], json!(["QQB"]));
    let state = |key: &str| {
        reply["focuses"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["focus"] == key)
            .unwrap()
            .clone()
    };
    assert_eq!(state("ZZA_peace_works")["state"], "unavailable");
    assert_eq!(
        state("ZZA_peace_works")["why"],
        json!(["needs has_war = no"])
    );
    assert_eq!(state("ZZA_war_plan")["state"], "available");
    assert_eq!(
        state("ZZA_left")["why"],
        json!(["not in the loaded focus tree"])
    );
    // Stability 0.62 and 150 political power are clear of their thresholds.
    assert_eq!(state("ZZA_numbers")["state"], "available");
    assert_eq!(state("ZZA_faction")["state"], "available");
    // Flags and DLC switches were not readable here.
    assert_eq!(state("ZZA_flagged")["state"], "unknown");
    assert_eq!(state("ZZA_dlc_branch")["state"], "unknown");
    assert_eq!(state("ZZA_late")["state"], "unavailable");
}
