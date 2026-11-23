//! The generic interface model behind `hoi4_ui_inspect` and `hoi4_ui_act`: every open window as
//! a flat list of widgets with stable ids, a lookup by id or by unique name, and the diff of two
//! readings (what an action changed). Pure: the memory reader (launch/macos/uiscan.rs) fills it,
//! the .gui index (gui_index.rs) annotates it.
//!
//! A widget id is the path of element names from the window's root, the names the game's and
//! the mod's `.gui` files define: `politics_view/ideas/idea_slot_0`. A name that repeats among
//! siblings gets `#n` (its order among them), an unnamed widget `_n` (its child index), and a
//! grid box's entries `#n` below the grid. Ids stay the same between readings as long as the
//! panel's layout does, so a learned skill can name its buttons by them.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Widget {
    pub id: String,
    pub name: String,
    /// "button", "icon", "checkbox", "container", "text", ... (memory class, else the .gui type).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Screen rectangle [x, y, w, h] in game pixels (1920x1080 space), when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<[i32; 4]>,
    pub shown: bool,
    /// Button state as the game stores it: 0 normal, 2 toggled on, 3 disabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// What the files say about it (tooltip text, scripted effect, cost): set by the index.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub script: Option<Value>,
}

impl Widget {
    pub fn enabled(&self) -> bool {
        self.state != Some(3)
    }
    /// Whether an ordinary click can reach it now.
    pub fn clickable(&self) -> bool {
        self.shown && self.at.is_some() && self.enabled()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Window {
    /// The root's element name (unique among open windows; `#n` added on a repeat).
    pub id: String,
    /// Where the window was found: "view" (interface manager), "popup", "scripted".
    pub source: String,
    pub visible: bool,
    pub widgets: Vec<Widget>,
}

/// Unique child ids under one parent: `name`, then `name#1`, `name#2`; unnamed `_index`.
#[derive(Default)]
pub struct Ids {
    seen: HashMap<(String, String), u32>,
}
impl Ids {
    pub fn child(&mut self, parent: &str, name: &str, index: usize) -> String {
        let local = if name.is_empty() {
            format!("_{index}")
        } else {
            let n = self
                .seen
                .entry((parent.to_string(), name.to_string()))
                .or_insert(0);
            *n += 1;
            if *n == 1 {
                name.to_string()
            } else {
                format!("{name}#{}", *n - 1)
            }
        };
        if parent.is_empty() {
            local
        } else {
            format!("{parent}/{local}")
        }
    }
}

/// Give repeated window names `#n` so every window id is unique.
pub fn unique_windows(windows: &mut [Window]) {
    let mut seen: HashMap<String, u32> = HashMap::new();
    for w in windows.iter_mut() {
        let n = seen.entry(w.id.clone()).or_insert(0);
        *n += 1;
        if *n > 1 {
            let old = w.id.clone();
            w.id = format!("{old}#{}", *n - 1);
            for x in &mut w.widgets {
                if let Some(rest) = x.id.strip_prefix(&old) {
                    x.id = format!("{}{rest}", w.id);
                }
            }
        }
    }
}

/// The widget an argument names: a full id (`window/a/b`), or a bare element name that only one
/// shown widget carries. Ambiguous and unknown names refuse with the candidates.
pub fn find<'a>(
    windows: &'a [Window],
    wanted: &str,
) -> std::result::Result<(&'a Window, &'a Widget), Value> {
    for w in windows {
        if let Some(x) = w.widgets.iter().find(|x| x.id == wanted) {
            return Ok((w, x));
        }
    }
    let by_name: Vec<(&Window, &Widget)> = windows
        .iter()
        .flat_map(|w| w.widgets.iter().map(move |x| (w, x)))
        .filter(|(_, x)| x.name == wanted || x.id.ends_with(&format!("/{wanted}")))
        .collect();
    let shown: Vec<&(&Window, &Widget)> = by_name.iter().filter(|(_, x)| x.shown).collect();
    match (shown.len(), by_name.len()) {
        (1, _) => Ok(*shown[0]),
        (0, 1) => Ok(by_name[0]),
        (0, 0) => Err(json!({"code": "widget_not_found",
            "message": format!("No open window has a widget {wanted:?}; read hoi4_ui_inspect for the ids"),
            "windows": windows.iter().map(|w| w.id.clone()).collect::<Vec<_>>()})),
        _ => Err(json!({"code": "widget_ambiguous",
            "message": format!("{wanted:?} names several widgets; pass one full id"),
            "candidates": by_name.iter().take(12).map(|(_, x)| x.id.clone()).collect::<Vec<_>>()})),
    }
}

/// What changed between two readings: windows opened and closed, widgets that appeared,
/// disappeared, were shown or hidden, changed text, or were enabled / disabled / toggled.
pub fn diff(before: &[Window], after: &[Window], limit: usize) -> Value {
    let ids = |ws: &[Window]| ws.iter().map(|w| w.id.clone()).collect::<Vec<_>>();
    let (b_ids, a_ids) = (ids(before), ids(after));
    let opened: Vec<&String> = a_ids.iter().filter(|i| !b_ids.contains(i)).collect();
    let closed: Vec<&String> = b_ids.iter().filter(|i| !a_ids.contains(i)).collect();
    let index = |ws: &[Window]| -> BTreeMap<String, Widget> {
        ws.iter()
            .flat_map(|w| w.widgets.iter().map(|x| (x.id.clone(), x.clone())))
            .collect()
    };
    let (b, a) = (index(before), index(after));
    let mut changes = Vec::new();
    let mut appeared = Vec::new();
    let mut disappeared = Vec::new();
    // Widgets of windows that opened or closed are summed up by the window line.
    let in_window = |id: &str, set: &[&String]| {
        set.iter()
            .any(|w| id == w.as_str() || id.starts_with(&format!("{w}/")))
    };
    for (id, x) in &a {
        match b.get(id) {
            None if !in_window(id, &opened) && x.shown => appeared.push(id.clone()),
            None => {}
            Some(old) => {
                let mut c = serde_json::Map::new();
                if old.shown != x.shown {
                    c.insert(
                        "shown".into(),
                        json!(if x.shown { "now shown" } else { "now hidden" }),
                    );
                }
                if old.text != x.text {
                    c.insert("text".into(), json!([old.text, x.text]));
                }
                if old.state != x.state {
                    c.insert(
                        "state".into(),
                        json!(match x.state {
                            Some(3) => "now disabled".to_string(),
                            Some(2) => "now toggled on".to_string(),
                            _ if old.state == Some(3) => "now enabled".to_string(),
                            _ if old.state == Some(2) => "now toggled off".to_string(),
                            other => format!("{other:?}"),
                        }),
                    );
                }
                if !c.is_empty() {
                    c.insert("id".into(), json!(id));
                    changes.push(Value::Object(c));
                }
            }
        }
    }
    for (id, x) in &b {
        if !a.contains_key(id) && !in_window(id, &closed) && x.shown {
            disappeared.push(id.clone());
        }
    }
    let total = changes.len() + appeared.len() + disappeared.len() + opened.len() + closed.len();
    changes.truncate(limit);
    appeared.truncate(limit);
    disappeared.truncate(limit);
    json!({
        "nothing_changed": total == 0,
        "windows_opened": opened,
        "windows_closed": closed,
        "widgets_changed": changes,
        "widgets_appeared": appeared,
        "widgets_disappeared": disappeared,
        "count": total,
    })
}

/// The reading as an agent sees it: per window its widgets (shown ones unless `all`), each
/// with id, kind, rect, enabled, text and what the files say; `under` keeps one window or
/// subtree; `max` bounds the rows.
pub fn view(windows: &[Window], all: bool, under: Option<&str>, max: usize) -> Value {
    let mut rows = 0usize;
    let mut out = Vec::new();
    let mut cut = false;
    for w in windows {
        if let Some(u) = under {
            if !(w.id == u
                || u.starts_with(&format!("{}/", w.id))
                || w.widgets.iter().any(|x| x.id == u))
            {
                continue;
            }
        }
        let mut list = Vec::new();
        for x in &w.widgets {
            if !all && !x.shown {
                continue;
            }
            if let Some(u) = under {
                if u != w.id && !(x.id == u || x.id.starts_with(&format!("{u}/"))) {
                    continue;
                }
            }
            if rows >= max {
                cut = true;
                break;
            }
            rows += 1;
            let mut row = serde_json::to_value(x).unwrap_or(Value::Null);
            if let Some(o) = row.as_object_mut() {
                o.remove("name");
                if !x.enabled() {
                    o.insert("enabled".into(), json!(false));
                }
                if !all {
                    o.remove("shown");
                }
            }
            list.push(row);
        }
        out.push(
            json!({"window": w.id, "found_in": w.source, "visible": w.visible, "widgets": list}),
        );
    }
    json!({"windows": out, "widget_rows": rows, "truncated": cut})
}

/// Scalars of a JSON reading by dotted path; arrays of objects are keyed by their `name`, `key`,
/// `id`, `tag` or `resource` field when they have one (else by position).
pub fn flatten(prefix: &str, v: &Value, depth: u32, out: &mut BTreeMap<String, Value>) {
    if depth > 6 || out.len() > 4000 {
        return;
    }
    let join = |k: &str| {
        if prefix.is_empty() {
            k.to_string()
        } else {
            format!("{prefix}.{k}")
        }
    };
    match v {
        Value::Object(o) => {
            for (k, x) in o {
                flatten(&join(k), x, depth + 1, out);
            }
        }
        Value::Array(a) => {
            for (i, x) in a.iter().take(200).enumerate() {
                let key = ["name", "key", "id", "tag", "resource"]
                    .iter()
                    .find_map(|f| x.get(*f).and_then(Value::as_str).map(str::to_string))
                    .unwrap_or_else(|| i.to_string());
                flatten(&join(&key), x, depth + 1, out);
            }
        }
        Value::Null => {}
        scalar => {
            out.insert(prefix.to_string(), scalar.clone());
        }
    }
}

/// What changed between two state readings: numbers with their delta, other values before and
/// after, paths that appeared or vanished; at most `limit` rows, biggest relative changes first.
pub fn state_diff(before: &Value, after: &Value, limit: usize) -> Value {
    let (mut b, mut a) = (BTreeMap::new(), BTreeMap::new());
    flatten("", before, 0, &mut b);
    flatten("", after, 0, &mut a);
    let mut rows: Vec<(f64, Value)> = Vec::new();
    for (k, x) in &a {
        match b.get(k) {
            Some(old) if old == x => {}
            Some(old) => match (old.as_f64(), x.as_f64()) {
                (Some(o), Some(n)) => {
                    let rel = (n - o).abs() / o.abs().max(1.0);
                    rows.push((
                        rel,
                        json!({"path": k, "before": o, "after": n, "delta": n - o}),
                    ));
                }
                _ => rows.push((1.0, json!({"path": k, "before": old, "after": x}))),
            },
            None => rows.push((0.5, json!({"path": k, "new": x}))),
        }
    }
    for (k, old) in &b {
        if !a.contains_key(k) {
            rows.push((0.5, json!({"path": k, "gone": old})));
        }
    }
    rows.sort_by(|x, y| y.0.partial_cmp(&x.0).unwrap_or(std::cmp::Ordering::Equal));
    let total = rows.len();
    json!({"count": total, "changes": rows.into_iter().take(limit).map(|r| r.1).collect::<Vec<_>>()})
}

/// Widgets from a recorded `widgets_under` dump (`{"path": "/root/a/b", "rect", "shown",
/// "button_state"}`), for tests and for replaying older probe logs.
pub fn from_dump(rows: &[Value], source: &str) -> Vec<Window> {
    let mut windows: Vec<Window> = Vec::new();
    for row in rows {
        let Some(path) = row["path"].as_str() else {
            continue;
        };
        let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
        let Some(root) = parts.first() else { continue };
        if !windows.iter().any(|w| w.id == *root) {
            windows.push(Window {
                id: root.to_string(),
                source: source.into(),
                visible: true,
                widgets: Vec::new(),
            });
        }
        let w = windows.iter_mut().find(|w| w.id == *root).unwrap();
        let at = row["rect"].as_array().and_then(|r| {
            let v: Vec<i32> = r
                .iter()
                .filter_map(|n| n.as_i64())
                .map(|n| n as i32)
                .collect();
            (v.len() == 4).then(|| [v[0], v[1], v[2], v[3]])
        });
        let name = parts.last().unwrap_or(&"").to_string();
        w.widgets.push(Widget {
            id: parts.join("/"),
            kind: row.get("button_state").map(|_| "button".to_string()),
            name,
            at,
            shown: row["shown"].as_bool().unwrap_or(false),
            state: row["button_state"].as_i64().map(|s| s as i32),
            text: row["text"].as_str().map(str::to_string),
            script: None,
        });
    }
    windows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recorded() -> Vec<Window> {
        let rows: Vec<Value> =
            serde_json::from_str(include_str!("fixtures/officer_corps_widgets.json")).unwrap();
        from_dump(&rows, "view")
    }

    #[test]
    fn recorded_officer_corps_dump_gives_stable_ids_and_unique_lookups() {
        let ws = recorded();
        assert_eq!(ws.len(), 1);
        assert_eq!(ws[0].id, "officer_core_view");
        let (_, x) = find(
            &ws,
            "officer_core_view/air_spirits_view/spirit_grid/#1/button",
        )
        .unwrap();
        assert_eq!(x.at, Some([272, 849, 168, 70]));
        assert!(x.clickable());
        // A bare name used by many slots is refused with candidates, never guessed.
        let e = find(&ws, "button").unwrap_err();
        assert_eq!(e["code"], "widget_ambiguous");
        assert!(e["candidates"].as_array().unwrap().len() > 1);
        assert_eq!(
            find(&ws, "no_such").unwrap_err()["code"],
            "widget_not_found"
        );
    }

    #[test]
    fn ids_number_repeated_and_unnamed_siblings() {
        let mut ids = Ids::default();
        assert_eq!(ids.child("w", "row", 0), "w/row");
        assert_eq!(ids.child("w", "row", 1), "w/row#1");
        assert_eq!(ids.child("w", "", 2), "w/_2");
        assert_eq!(ids.child("w/row", "row", 0), "w/row/row");
        let mut ws = vec![
            Window {
                id: "pop".into(),
                widgets: vec![Widget {
                    id: "pop/ok".into(),
                    ..Default::default()
                }],
                ..Default::default()
            },
            Window {
                id: "pop".into(),
                widgets: vec![Widget {
                    id: "pop/ok".into(),
                    ..Default::default()
                }],
                ..Default::default()
            },
        ];
        unique_windows(&mut ws);
        assert_eq!(ws[1].id, "pop#1");
        assert_eq!(ws[1].widgets[0].id, "pop#1/ok");
    }

    #[test]
    fn diff_reports_state_text_visibility_and_windows() {
        let before = recorded();
        let mut after = recorded();
        let w = &mut after[0].widgets;
        let i = w
            .iter()
            .position(|x| x.id.ends_with("spirit_grid/#1/button"))
            .unwrap();
        w[i].state = Some(3);
        let j = w
            .iter()
            .position(|x| x.id.ends_with("spirit_grid/#1/spirit_icon"))
            .unwrap();
        w[j].shown = true;
        after.push(Window {
            id: "confirm_popup".into(),
            source: "popup".into(),
            visible: true,
            widgets: vec![],
        });
        let d = diff(&before, &after, 10);
        assert_eq!(d["nothing_changed"], false);
        assert_eq!(d["windows_opened"], json!(["confirm_popup"]));
        let changed = d["widgets_changed"].as_array().unwrap();
        assert!(changed.iter().any(|c| c["state"] == "now disabled"));
        assert!(changed.iter().any(|c| c["shown"] == "now shown"));
        assert_eq!(diff(&before, &before, 10)["nothing_changed"], true);
    }

    #[test]
    fn state_diff_names_numbers_by_key_not_position() {
        let before = json!({"political_power": 120.0, "resources": [{"resource": "oil", "net": 4}, {"resource": "steel", "net": 10}], "alerts": []});
        let after = json!({"political_power": 95.0, "resources": [{"resource": "steel", "net": 10}, {"resource": "oil", "net": 2}], "alerts": [{"name": "decisions_available"}]});
        let d = state_diff(&before, &after, 10);
        let rows = d["changes"].as_array().unwrap();
        assert!(rows
            .iter()
            .any(|r| r["path"] == "political_power" && r["delta"] == -25.0));
        assert!(rows
            .iter()
            .any(|r| r["path"] == "resources.oil.net" && r["delta"] == -2.0));
        assert!(
            !rows.iter().any(|r| r["path"] == "resources.steel.net"),
            "a reordered list is no change"
        );
        assert!(rows
            .iter()
            .any(|r| r["path"] == "alerts.decisions_available.name"));
        assert_eq!(state_diff(&before, &before, 10)["count"], 0);
    }

    #[test]
    fn view_keeps_shown_widgets_and_one_subtree() {
        let ws = recorded();
        let all = view(&ws, true, None, 500);
        let shown = view(&ws, false, None, 500);
        assert!(all["widget_rows"].as_u64() > shown["widget_rows"].as_u64());
        let sub = view(&ws, false, Some("officer_core_view/air_spirits_view"), 500);
        for row in sub["windows"][0]["widgets"].as_array().unwrap() {
            assert!(row["id"]
                .as_str()
                .unwrap()
                .starts_with("officer_core_view/air_spirits_view"));
        }
        assert_eq!(view(&ws, true, None, 3)["truncated"], true);
    }
}
