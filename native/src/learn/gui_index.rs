//! What the interface files say about a widget, for panels no tool knows: its element type,
//! tooltip and text keys from the `.gui` files, and for scripted interfaces (a mod's own
//! mechanics) what each button's click does according to `common/scripted_guis`: the effect,
//! whether it spends something (a negative political power, manpower, equipment, experience,
//! command power or stability amount) and whether it does something one click cannot undo
//! (a declaration, an annexation, an idea added or removed, a decision activated). The files
//! are the game's and the enabled mods' (the runtime's merged content folder), so the index
//! follows any mod. Pure parsing over text; `load` reads a content folder.
use crate::script::{self, Block, Entry, Val};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;

/// Effects whose negative amount spends something the player holds.
pub const SPEND_EFFECTS: [&str; 11] = [
    "add_political_power",
    "add_command_power",
    "add_manpower",
    "add_stability",
    "add_war_support",
    "army_experience",
    "navy_experience",
    "air_experience",
    "add_resource",
    "add_fuel",
    "add_equipment_to_stockpile",
];
/// Effects the player cannot take back with one click.
pub const IRREVERSIBLE_EFFECTS: [&str; 24] = [
    "declare_war_on",
    "annex_country",
    "create_wargoal",
    "set_politics",
    "puppet",
    "release",
    "release_puppet",
    "transfer_state",
    "set_state_owner",
    "start_civil_war",
    "leave_faction",
    "dismantle_faction",
    "create_faction",
    "add_to_faction",
    "white_peace",
    "set_capital",
    "retire_character",
    "kill_country_leader",
    "remove_ideas",
    "add_ideas",
    "add_timed_idea",
    "activate_decision",
    "complete_national_focus",
    "unlock_decision_tooltip",
];

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Element {
    /// Short kind: button, icon, text, container, grid, checkbox, edit, list, scrollbar, ...
    pub kind: String,
    pub file: String,
    pub tooltip_key: Option<String>,
    pub text_key: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScriptedButton {
    pub effect: String,
    pub spends: Vec<String>,
    pub irreversible: Vec<String>,
    pub enabled_trigger: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScriptedGui {
    pub name: String,
    pub window_name: String,
    pub context_type: Option<String>,
    pub parent: Option<String>,
    pub buttons: HashMap<String, ScriptedButton>,
}

#[derive(Clone, Debug, Default)]
pub struct GuiIndex {
    /// (root window name, element name) -> element; the root window's own entry has both names equal.
    pub by_window: HashMap<(String, String), Element>,
    /// element name -> element (first definition in load order).
    pub by_name: HashMap<String, Element>,
    /// scripted window name -> its scripted interface.
    pub scripted: HashMap<String, ScriptedGui>,
}

/// Short kind of a `.gui` element type key.
pub fn kind_of(type_key: &str) -> String {
    let k = type_key.to_ascii_lowercase();
    let short = match k.as_str() {
        "buttontype" | "guibuttontype" => "button",
        "icontype" => "icon",
        "instanttextboxtype" | "textboxtype" => "text",
        "containerwindowtype" | "windowtype" => "container",
        "gridboxtype" => "grid",
        "checkboxtype" => "checkbox",
        "editboxtype" => "edit",
        "listboxtype" | "smoothlistboxtype" => "list",
        "extendedscrollbartype" | "scrollbartype" => "scrollbar",
        "overlappingelementsboxtype" => "overlap",
        "progressbartype" => "progressbar",
        "dropdownboxtype" => "dropdown",
        _ => return k.trim_end_matches("type").to_string(),
    };
    short.to_string()
}

fn text(fields: &[Entry], key: &str) -> Option<String> {
    fields
        .text(key)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
}

impl GuiIndex {
    /// Add the elements of one `.gui` file.
    pub fn add_gui(&mut self, source: &str, file: &str) {
        for top in script::parse(source) {
            for e in top.val.block() {
                self.walk(e, None, file, 0);
            }
        }
    }
    fn walk(&mut self, e: &Entry, root: Option<&str>, file: &str, depth: u32) {
        let Val::Block(fields) = &e.val else { return };
        if depth > 40 || !e.key.to_ascii_lowercase().ends_with("type") {
            return;
        }
        let Some(name) = text(fields, "name") else {
            return;
        };
        let el = Element {
            kind: kind_of(&e.key),
            file: file.to_string(),
            tooltip_key: text(fields, "pdx_tooltip").or_else(|| text(fields, "tooltip")),
            text_key: text(fields, "text").or_else(|| text(fields, "buttonText")),
        };
        let root_name = root.unwrap_or(&name).to_string();
        self.by_window
            .entry((root_name.clone(), name.clone()))
            .or_insert_with(|| el.clone());
        self.by_name.entry(name.clone()).or_insert(el);
        for child in fields {
            self.walk(child, Some(&root_name), file, depth + 1);
        }
    }

    /// Add the scripted interfaces of one `common/scripted_guis` file.
    pub fn add_scripted(&mut self, source: &str) {
        for top in script::parse(source) {
            if top.key != "scripted_gui" {
                continue;
            }
            for g in top.val.block() {
                let Val::Block(fields) = &g.val else { continue };
                let Some(window) = text(fields, "window_name") else {
                    continue;
                };
                let triggers: Vec<String> = fields
                    .find_val("triggers")
                    .map(|t| t.block().iter().map(|e| e.key.clone()).collect())
                    .unwrap_or_default();
                let mut buttons = HashMap::new();
                for eff in fields.find_val("effects").map(Val::block).unwrap_or(&[]) {
                    let widget = button_of(&eff.key);
                    let (mut spends, mut irreversible) = (Vec::new(), Vec::new());
                    profile(eff.val.block(), 0, &mut spends, &mut irreversible);
                    buttons.entry(widget).or_insert(ScriptedButton {
                        effect: eff.key.clone(),
                        spends,
                        irreversible,
                        enabled_trigger: triggers.contains(&format!("{}_enabled", eff.key)),
                    });
                }
                self.scripted.insert(
                    window.clone(),
                    ScriptedGui {
                        name: g.key.clone(),
                        window_name: window,
                        context_type: text(fields, "context_type"),
                        parent: text(fields, "parent_window_token")
                            .or_else(|| text(fields, "parent_window_name")),
                        buttons,
                    },
                );
            }
        }
    }

    /// The element a widget is, by its window and name (falls back to the name alone).
    pub fn element(&self, window: &str, name: &str) -> Option<&Element> {
        self.by_window
            .get(&(window.to_string(), name.to_string()))
            .or_else(|| self.by_name.get(name))
    }

    /// The scripted interface and button a widget id belongs to: the nearest path segment that is
    /// a scripted window, and the widget's name among its buttons.
    pub fn scripted_button(&self, id: &str) -> Option<(&ScriptedGui, &ScriptedButton)> {
        let parts: Vec<&str> = id.split('/').map(plain).collect();
        let name = *parts.last()?;
        parts[..parts.len().saturating_sub(1)]
            .iter()
            .rev()
            .find_map(|seg| self.scripted.get(*seg))
            .and_then(|g| g.buttons.get(name).map(|b| (g, b)))
    }

    /// The `script` note of a widget: tooltip and text keys from the .gui, the scripted effect with
    /// its cost and irreversibility, all with localised text where `loc` has it.
    pub fn note(
        &self,
        window: &str,
        id: &str,
        name: &str,
        loc: &HashMap<String, String>,
    ) -> Option<Value> {
        let mut out = serde_json::Map::new();
        if let Some(el) = self.element(plain(window), name) {
            if let Some(k) = &el.tooltip_key {
                out.insert(
                    "tooltip".into(),
                    json!(loc.get(k).cloned().unwrap_or_else(|| k.clone())),
                );
            }
        }
        if let Some((g, b)) = self.scripted_button(id) {
            out.insert("scripted_gui".into(), json!(g.name));
            out.insert("effect".into(), json!(b.effect));
            if !b.spends.is_empty() {
                out.insert("spends".into(), json!(b.spends));
            }
            if !b.irreversible.is_empty() {
                out.insert("irreversible".into(), json!(b.irreversible));
            }
        }
        (!out.is_empty()).then_some(Value::Object(out))
    }

    /// Every key the notes would localise (tooltips), for one batched lookup.
    pub fn tooltip_keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self
            .by_name
            .values()
            .filter_map(|e| e.tooltip_key.clone())
            .collect();
        keys.sort();
        keys.dedup();
        keys
    }
}

/// An id segment without its `#n` repeat suffix.
pub fn plain(seg: &str) -> &str {
    seg.split('#').next().unwrap_or(seg)
}

/// The widget a scripted effect key belongs to: `buy_button_click` -> `buy_button`.
pub fn button_of(effect: &str) -> String {
    for suffix in [
        "_right_click",
        "_shift_click",
        "_control_click",
        "_alt_click",
        "_click",
    ] {
        if let Some(at) = effect.find(suffix) {
            return effect[..at].to_string();
        }
    }
    effect.to_string()
}

fn negative(v: &Val) -> bool {
    match v {
        Val::Scalar(s) => s.trim_start().starts_with('-'),
        Val::Block(b) => ["amount", "value"]
            .iter()
            .filter_map(|k| b.find_val(k))
            .any(negative),
    }
}

fn profile(block: &[Entry], depth: u32, spends: &mut Vec<String>, irreversible: &mut Vec<String>) {
    if depth > 12 {
        return;
    }
    for e in block {
        let key = e.key.as_str();
        if SPEND_EFFECTS.contains(&key) && negative(&e.val) && !spends.iter().any(|s| s == key) {
            spends.push(key.to_string());
        } else if IRREVERSIBLE_EFFECTS.contains(&key) && !irreversible.iter().any(|s| s == key) {
            irreversible.push(key.to_string());
        }
        profile(e.val.block(), depth + 1, spends, irreversible);
    }
}

fn files(dir: &Path, ext: &str, out: &mut Vec<std::path::PathBuf>, depth: u32) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth < 4 {
                files(&path, ext, out, depth + 1);
            }
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case(ext))
        {
            out.push(path);
        }
    }
}

/// The index of a content folder (the install, or the runtime's merged mod folder).
pub fn load(content: &Path) -> GuiIndex {
    let mut index = GuiIndex::default();
    let mut gui = Vec::new();
    files(&content.join("interface"), "gui", &mut gui, 0);
    gui.sort();
    for path in gui {
        if let Ok(bytes) = std::fs::read(&path) {
            let rel = path
                .strip_prefix(content)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            index.add_gui(&String::from_utf8_lossy(&bytes), &rel);
        }
    }
    let mut scripted = Vec::new();
    files(
        &content.join("common/scripted_guis"),
        "txt",
        &mut scripted,
        0,
    );
    scripted.sort();
    for path in scripted {
        if let Ok(bytes) = std::fs::read(&path) {
            index.add_scripted(&String::from_utf8_lossy(&bytes));
        }
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    const GUI: &str = r#"guiTypes = {
        containerWindowType = { name = "caps_window" position = { x = 0 y = 0 }
            buttonType = { name = "buy_button" pdx_tooltip = "CAPS_BUY_TT" spriteType = "GFX_x" }
            buttonType = { name = "raid_button" }
            instantTextBoxType = { name = "caps_value" text = "[?caps|0]" }
            gridBoxType = { name = "caps_grid" }
        }
        containerWindowType = { name = "other_window" buttonType = { name = "buy_button" } }
    }"#;
    const SCRIPTED: &str = r#"scripted_gui = {
        caps_panel = {
            context_type = player_context
            window_name = "caps_window"
            parent_window_token = politics_tab
            effects = {
                buy_button_click = { add_political_power = -25 add_resource = { type = water amount = -5 state = 1 } }
                raid_button_click = { if = { limit = { always = yes } declare_war_on = { target = XXX type = annex } } }
                raid_button_right_click = { add_political_power = 5 }
            }
            triggers = { buy_button_click_enabled = { has_political_power > 25 } }
        }
    }"#;

    fn index() -> GuiIndex {
        let mut i = GuiIndex::default();
        i.add_gui(GUI, "interface/caps.gui");
        i.add_scripted(SCRIPTED);
        i
    }

    #[test]
    fn elements_keep_kind_tooltip_and_window_scope() {
        let i = index();
        let b = i.element("caps_window", "buy_button").unwrap();
        assert_eq!(b.kind, "button");
        assert_eq!(b.tooltip_key.as_deref(), Some("CAPS_BUY_TT"));
        assert_eq!(i.element("caps_window", "caps_value").unwrap().kind, "text");
        assert_eq!(i.element("caps_window", "caps_grid").unwrap().kind, "grid");
        // Same name in another window: scoped by the window, not the first file hit.
        assert!(i
            .element("other_window", "buy_button")
            .unwrap()
            .tooltip_key
            .is_none());
        assert_eq!(kind_of("OverlappingElementsBoxType"), "overlap");
    }

    #[test]
    fn scripted_buttons_carry_costs_and_irreversible_effects() {
        let i = index();
        let (g, b) = i
            .scripted_button("politics_view/caps_window/buy_button")
            .unwrap();
        assert_eq!(g.name, "caps_panel");
        assert_eq!(g.parent.as_deref(), Some("politics_tab"));
        assert_eq!(b.spends, vec!["add_political_power", "add_resource"]);
        assert!(b.enabled_trigger);
        let (_, raid) = i.scripted_button("caps_window#1/raid_button").unwrap();
        assert_eq!(raid.irreversible, vec!["declare_war_on"]);
        // The left click's effect wins over the right click's for the same widget.
        assert_eq!(raid.effect, "raid_button_click");
        assert!(i.scripted_button("other_window/buy_button").is_none());
    }

    #[test]
    fn notes_localise_the_tooltip_and_name_the_cost() {
        let i = index();
        let loc = HashMap::from([("CAPS_BUY_TT".to_string(), "Buy water for caps".to_string())]);
        let n = i
            .note("caps_window", "caps_window/buy_button", "buy_button", &loc)
            .unwrap();
        assert_eq!(n["tooltip"], "Buy water for caps");
        assert_eq!(n["spends"], json!(["add_political_power", "add_resource"]));
        assert_eq!(n["scripted_gui"], "caps_panel");
        assert!(i
            .note("caps_window", "caps_window/caps_grid", "caps_grid", &loc)
            .is_none());
        assert_eq!(button_of("buy_button_shift_click"), "buy_button");
    }
}
