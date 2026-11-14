use super::*;
use std::collections::HashMap;

const OPS: &str = r#"
operation_collaboration_government = {
    priority = 10
    days = 90
    network_strength = 50
    operatives = 2
    visible = { network_strength = { target = FROM value > 25 } }
    equipment = {
        support_equipment = 200
        infantry_equipment = 1000
        civilian_factories = { amount = 10 days = 60 }
    }
    allowed = { has_rule = can_create_collaboration_government }
    available = { has_collaboration = { target = FROM value < 1.0 } }
    risk_chance = 0.1
    phases = { infiltration_border = { base = 25 } infiltration_diplomatic = { base = 25 } }
    phases = { collaboration_government_contact_military_officers = { base = 33 } }
}
operation_coordinated_strike = {
    days = 3
    network_strength = 70
    operatives = 2
    visible = {
        network_strength = { target = FROM value > 35 }
        NOT = { has_war_with = FROM }
    }
    required_tokens = { token_army }
}
operation_secret = {
    days = 10
    network_strength = 10
    operatives = 1
    visible = { has_country_flag = secret_flag }
    operation_target = { targets = { ENG } }
}
"#;
const PHASES: &str = r#"
collaboration_government_contact_military_officers = { equipment = { civilian_factories = { amount = 3 days = 30 } } }
infiltration_border = { }
"#;
const IDEOLOGIES: &str = r#"
ideologies = {
    democratic = { rules = { can_create_collaboration_government = no can_declare_war_on_same_ideology = no } }
    fascism = { rules = { can_force_government = yes } }
}
"#;

fn catalog() -> Catalog {
    let mut c = Catalog::default();
    c.add_operations(&script::parse(OPS));
    c.add_phases(&script::parse(PHASES));
    c.add_ideologies(&script::parse(IDEOLOGIES));
    c
}

#[derive(Default)]
struct F {
    ideology: Option<String>,
    war: bool,
    network: Option<f64>,
    collab: Option<f64>,
    free: Option<usize>,
    stock: HashMap<String, f64>,
}
impl Facts for F {
    fn player(&self) -> String {
        "ITA".into()
    }
    fn ideology(&self) -> Option<String> {
        self.ideology.clone()
    }
    fn subject(&self) -> Truth {
        Truth::False
    }
    fn at_war_with(&self, _: &str) -> Truth {
        Truth::from_bool(self.war)
    }
    fn at_war(&self) -> Truth {
        Truth::from_bool(self.war)
    }
    fn network(&self, _: &str) -> Option<f64> {
        self.network
    }
    fn collaboration(&self, _: &str) -> Option<f64> {
        self.collab
    }
    fn token(&self, _: &str, _: &str) -> Truth {
        Truth::False
    }
    fn dlc(&self, _: &str) -> Truth {
        Truth::True
    }
    fn operatives(&self) -> Option<usize> {
        self.free
    }
    fn free_operatives(&self) -> Option<usize> {
        self.free
    }
    fn stockpile(&self, key: &str) -> Option<f64> {
        self.stock.get(key).copied()
    }
}

fn ready_facts() -> F {
    F {
        ideology: Some("fascism".into()),
        war: true,
        network: Some(56.7),
        collab: Some(0.0),
        free: Some(2),
        stock: [("support_equipment".to_string(), 300.0), ("infantry_equipment".to_string(), 5000.0)].into(),
    }
}

#[test]
fn definitions_and_phase_costs_come_from_the_files() {
    let c = catalog();
    assert_eq!(c.operations[0].key, "operation_collaboration_government");
    let op = c.operation("operation_collaboration_government").unwrap();
    assert_eq!(op.factories, Some((10.0, 60.0)));
    assert_eq!(op.equipment.len(), 2);
    assert_eq!(op.phases.len(), 2);
    let d = describe(&c, op);
    assert_eq!(d["phase_choices"][1][0]["civilian_factories"]["days"], json!(30.0));
    assert_eq!(c.operation("operation_coordinated_strike").unwrap().required_tokens, ["token_army"]);
}

#[test]
fn collaboration_is_ready_at_war_for_a_fascist_player() {
    let c = catalog();
    let op = c.operation("operation_collaboration_government").unwrap();
    let v = verdict(&c, op, "ETH", &ready_facts());
    assert_eq!(v.ready, Truth::True, "{v:?}");
    assert!(v.reasons.is_empty());
}

#[test]
fn each_missing_piece_has_its_own_reason() {
    let c = catalog();
    let op = c.operation("operation_collaboration_government").unwrap();
    let mut f = ready_facts();
    f.network = Some(40.0);
    f.free = Some(1);
    f.stock.insert("infantry_equipment".into(), 10.0);
    let v = verdict(&c, op, "ETH", &f);
    assert_eq!(v.ready, Truth::False);
    let codes: Vec<_> = v.reasons.iter().map(|r| r.code).collect();
    assert_eq!(codes, ["network_too_weak", "not_enough_operatives", "missing_equipment"]);
    // Full collaboration ends the operation's availability.
    let mut f = ready_facts();
    f.collab = Some(1.0);
    assert_eq!(verdict(&c, op, "ETH", &f).reasons[0].code, "unavailable");
    // Democracies may not run it.
    let mut f = ready_facts();
    f.ideology = Some("democratic".into());
    assert_eq!(verdict(&c, op, "ETH", &f).reasons[0].code, "not_allowed");
    // Unknown ideology is unknown, never a refusal.
    let mut f = ready_facts();
    f.ideology = None;
    let v = verdict(&c, op, "ETH", &f);
    assert_eq!(v.ready, Truth::Unknown);
    assert_eq!(v.unknown_triggers, ["has_rule"]);
}

#[test]
fn war_tokens_targets_and_unread_triggers() {
    let c = catalog();
    let strike = c.operation("operation_coordinated_strike").unwrap();
    let v = verdict(&c, strike, "ETH", &ready_facts());
    assert_eq!(v.visible, Truth::False);
    let codes: Vec<_> = v.reasons.iter().map(|r| r.code).collect();
    assert!(codes.contains(&"not_visible") && codes.contains(&"network_too_weak") && codes.contains(&"missing_token"));
    let secret = c.operation("operation_secret").unwrap();
    let v = verdict(&c, secret, "ETH", &ready_facts());
    assert_eq!(v.reasons[0].code, "not_a_target");
    let v = verdict(&c, secret, "ENG", &ready_facts());
    assert_eq!(v.visible, Truth::Unknown);
    assert_eq!(v.unknown_triggers, ["has_country_flag"]);
}
