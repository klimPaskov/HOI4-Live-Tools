//! The continuous focus through the in-process module (engineer 133, family `cfocus`), each the command the continuous
//! focus palette posts, checked by the game's own "can do" before it is posted:
//! * `cfocus.set`: CSetContinuousFocusCommand(own tag, the focus): the module takes the focus from the game's own
//!   continuous focus database by its id (exactly a CContinuousNationalFocus); the game decides whether the palette is
//!   unlocked and the focus allowed and available for us;
//! * `cfocus.drop`: CDropContinuousFocusCommand(own tag): the active continuous focus stopped.
//!
//! The palette is read from the game's own scripts (common/continuous_focus, mods included); the active focus is read
//! from memory (country +0x1108 -> +0x18 -> its id +0x18, from the drop command's own check on 1.19.4.1, 1.20.0.0 and
//! 1.20.1.0). No mouse path exists; the family is off under `HOI4_BACKEND=auto` until S34-CFOCUS passes. Served as
//! hoi4_focus_start's `continuous` argument (the focus domain's tool).
use super::inproc::{self, Channel, Routed};
use super::inproc_units::{request, routed};
use super::{journal, loc, mgmt, ui, Guard, InputAuthority};
use crate::{
    error::{Error, Result},
    memory::Reader,
    platform::Process,
    script::{self, Block},
    xlayout::{fld, Class},
};
use serde_json::{json, Map, Value};
use std::{path::Path, sync::Arc};

pub(super) const SET_OP: &str = "cfocus.set";
pub(super) const DROP_OP: &str = "cfocus.drop";
const KEY_MAX: usize = 39;
const COUNTRY: Class = Class::new("CCountry", 0x357e388);
const COUNTRY_CFOCUS: u64 = 0x1108;
const CFOCUS_STATE: Class = Class::new("CCountryContinuousFocus", 0);
const STATE_ACTIVE: u64 = 0x18;
const FOCUS_ID: u64 = 0x18;

/// One continuous focus of the game's palettes.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct CFocus {
    pub id: String,
    pub name: String,
    pub palette: String,
}

pub(super) fn palette_from(
    entries: &[script::Entry],
    names: &dyn Fn(&str) -> Option<String>,
) -> Vec<CFocus> {
    let mut out = Vec::new();
    for e in entries
        .iter()
        .filter(|e| e.key == "continuous_focus_palette")
    {
        let body = e.val.block();
        let palette = body.text("id").unwrap_or("").to_string();
        for f in body.all("focus") {
            if let Some(id) = f.block().text("id") {
                out.push(CFocus {
                    id: id.to_string(),
                    name: names(id).unwrap_or_else(|| id.to_string()),
                    palette: palette.clone(),
                });
            }
        }
    }
    out
}

fn load(dir: &Path) -> Vec<CFocus> {
    let entries = script::parse_dir(&dir.join("common/continuous_focus"));
    let ids: Vec<String> = palette_from(&entries, &|_| None)
        .into_iter()
        .map(|f| f.id)
        .collect();
    let keys: Vec<&str> = ids.iter().map(String::as_str).collect();
    let names = loc::lookup(dir, &keys);
    palette_from(&entries, &|k| names.get(k).cloned())
}

/// The game's continuous focuses (cached per install).
pub(super) fn palette(process: &dyn Process) -> Result<Arc<Vec<CFocus>>> {
    let dir = mgmt::game_dir(process)?;
    let p = script::cached(&dir, "continuous_focus", load);
    if p.is_empty() {
        return Err(Error::new(
            "catalog_unavailable",
            "The game's common/continuous_focus scripts list no continuous focus",
        ));
    }
    Ok(p)
}

/// The asked focus (script id or localised name); unknown names the closest ids.
pub(super) fn resolve<'a>(all: &'a [CFocus], asked: &str) -> Result<&'a CFocus> {
    let asked = asked.trim();
    if let Some(f) = all
        .iter()
        .find(|f| f.id == asked)
        .or_else(|| all.iter().find(|f| loc::same(&f.name, asked)))
    {
        return Ok(f);
    }
    let low = asked.to_ascii_lowercase();
    let close: Vec<&str> = all
        .iter()
        .filter(|f| f.id.contains(&low) || low.split('_').any(|w| w.len() > 3 && f.id.contains(w)))
        .map(|f| f.id.as_str())
        .take(8)
        .collect();
    Err(Error::new(
        "continuous_focus_unknown",
        format!(
            "no continuous focus {asked:?} in common/continuous_focus; close: {}",
            close.join(", ")
        ),
    ))
}

/// The module request: the focus id as character codes (set) or nothing (drop); objs = 0.
pub(super) fn set_request(id: &str) -> Result<Value> {
    if id.is_empty()
        || id.len() > KEY_MAX
        || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(Error::new(
            "invalid_arguments",
            "continuous focus id: letters, digits and _ (1..39)",
        ));
    }
    let vals: Vec<i64> = id.bytes().map(i64::from).collect();
    request(SET_OP, &[0], &[], &vals, None)
}

pub(super) fn drop_request() -> Result<Value> {
    request(DROP_OP, &[0], &[], &[], None)
}

/// The active continuous focus's id, None when none runs.
pub(super) fn active_in(r: &Reader<'_>, country: u64) -> Result<Option<String>> {
    let state = mgmt::pointer(r, country + fld(r.string_abi(), &COUNTRY, COUNTRY_CFOCUS)?)?;
    let focus = r.u64(state + fld(r.string_abi(), &CFOCUS_STATE, STATE_ACTIVE)?)?;
    if focus == 0 {
        return Ok(None);
    }
    Ok(Some(r.string(focus + FOCUS_ID)?))
}

fn active(process: &dyn Process) -> Result<Option<String>> {
    ui::read(process, |r, image| {
        let own = mgmt::own(r, image)?;
        active_in(r, own.country)
    })
}

/// What a call asks.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Ask {
    List,
    Set(String),
    Drop,
}

pub(super) fn parse(v: &Value) -> Result<Ask> {
    let bad = || {
        Error::new(
            "invalid_arguments",
            "continuous: {action: set, focus: ID} | {action: drop} | {action: list}",
        )
    };
    let o = v.as_object().ok_or_else(bad)?;
    match o.get("action").and_then(Value::as_str) {
        Some("list") => Ok(Ask::List),
        Some("drop") => Ok(Ask::Drop),
        Some("set") => Ok(Ask::Set(
            o.get("focus")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(bad)?
                .to_string(),
        )),
        _ => Err(bad()),
    }
}

/// Refusals that need no post: what the read already answers.
pub(super) fn precheck(active: Option<&str>, ask: &Ask) -> Result<()> {
    match ask {
        Ask::Set(id) if active == Some(id.as_str()) => Err(Error::new(
            "nothing_to_change",
            format!("{id} is the active continuous focus"),
        )),
        Ask::Drop if active.is_none() => Err(Error::new(
            "nothing_to_change",
            "no continuous focus is active",
        )),
        _ => Ok(()),
    }
}

/// Whether the read shows the asked change.
pub(super) fn reached(active: Option<&str>, ask: &Ask) -> bool {
    match ask {
        Ask::Set(id) => active == Some(id.as_str()),
        Ask::Drop => active.is_none(),
        Ask::List => true,
    }
}

fn post(ch: &mut Channel, req: Value, what: &str) -> Routed {
    match ch.post_raw(req) {
        Ok(reply) => routed(reply, what),
        Err(_) => Routed::Gui,
    }
}

/// hoi4_focus_start with `continuous`: list the palette (and the active one), set a continuous focus or drop the active
/// one, through the module's cfocus family. The game's own check decides; a refusal posts nothing.
pub(super) fn act(
    args: &Map<String, Value>,
    process: &dyn Process,
    guard: &mut Guard,
    authority: &InputAuthority,
    actions: &mut journal::GameJournal,
) -> Result<Value> {
    let ask = parse(&args["continuous"])?;
    if ask == Ask::List {
        super::allowed(args, &["continuous", "request_id", "lease_token"])?;
        let all = palette(process)?;
        let now = active(process);
        return Ok(json!({"continuous_focus": {
            "active": now.as_ref().ok().cloned().flatten(),
            "active_read": now.is_ok(),
            "active_error": now.err().map(|e| e.code),
            "focuses": all.iter().map(|f| json!({"id": f.id, "name": f.name, "palette": f.palette})).collect::<Vec<_>>(),
            "rule": "one continuous focus at a time, after the focus that unlocks the palette; the game's own check decides whether a focus is allowed and available for us"}}));
    }
    super::allowed(args, &["continuous", "request_id", "lease_token"])?;
    let ask = match ask {
        Ask::Set(asked) => Ask::Set(resolve(&palette(process)?, &asked)?.id.clone()),
        other => other,
    };
    let id = super::string(args, "request_id")?;
    let token = super::string(args, "lease_token")?;
    precheck(active(process)?.as_deref(), &ask)?;
    let req = match &ask {
        Ask::Set(f) => set_request(f)?,
        _ => drop_request()?,
    };
    let observed = guard.observe(process)?;
    let epoch = observed.epoch;
    authority.authorize(token, &epoch)?;
    let mut route = inproc::route("cfocus");
    if route.is_none() {
        return Err(Error::new(
            "in_process_unavailable",
            "the continuous focus runs only through the module's cfocus family (HOI4_BACKEND=inprocess with cfocus opted in, until S34-CFOCUS passes); nothing was sent",
        ));
    }
    let what = match &ask {
        Ask::Set(f) => json!({"kind":"continuous_focus","action":"set","focus":f}),
        _ => json!({"kind":"continuous_focus","action":"drop"}),
    };
    let answer = match &ask {
        Ask::Set(_) => "this continuous focus now (the palette unlocked, the focus allowed and available for us)",
        _ => "stop the active continuous focus",
    };
    let mut reply = Value::Null;
    let mut settle = || -> Result<journal::Outcome> {
        let now = active(process)?;
        Ok(if reached(now.as_deref(), &ask) {
            journal::Outcome::Reached(json!({"active": now, "verification": "memory"}))
        } else {
            journal::Outcome::Unknown
        })
    };
    let mut dispatch = || {
        authority.authorize(token, &epoch)?;
        let ch = route
            .as_mut()
            .ok_or_else(|| Error::new("in_process_unavailable", "no channel"))?;
        match post(ch, req.clone(), answer) {
            Routed::Done(body) => {
                reply = body;
                Ok(())
            }
            Routed::Refused(what) => Err(inproc::refused_error(&what)),
            Routed::Gui => Err(Error::new(
                "in_process_unavailable",
                "the module did not post it (see its log); there is no mouse path, so nothing was sent",
            )),
        }
    };
    let receipt = actions.execute(id, &epoch, what, &mut settle, &mut dispatch)?;
    Ok(json!({"receipt":receipt,"in_process":reply}))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GENERIC: &str = "continuous_focus_palette = {
        id = generic_focus
        default = yes
        focus = { id = political_power_focus icon = GFX_x modifier = { political_power_gain = 0.5 } }
        focus = { id = industrial_focus available = { always = yes } }
    }
    continuous_focus_palette = { id = other focus = { id = DEN_x } }";

    #[test]
    fn the_palette_is_read_from_the_scripts() {
        let all = palette_from(&script::parse(GENERIC), &|k| {
            (k == "industrial_focus").then(|| "Industrial Focus".to_string())
        });
        let ids: Vec<&str> = all.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(ids, ["political_power_focus", "industrial_focus", "DEN_x"]);
        assert_eq!(all[2].palette, "other");
        assert_eq!(
            resolve(&all, "Industrial Focus").unwrap().id,
            "industrial_focus"
        );
        let e = resolve(&all, "political_focus").unwrap_err();
        assert_eq!(e.code, "continuous_focus_unknown");
        assert!(e.message.contains("political_power_focus"), "{}", e.message);
    }

    #[test]
    fn requests_and_answers() {
        let r = set_request("industrial_focus").unwrap();
        assert_eq!(r["cmd"], SET_OP);
        assert_eq!(r["objs"], "0x0");
        assert!(r["vals"].as_str().unwrap().starts_with("105,110,100"));
        assert!(set_request("bad key").is_err());
        assert_eq!(drop_request().unwrap()["cmd"], DROP_OP);
        assert_eq!(
            parse(&json!({"action": "set", "focus": "x"})).unwrap(),
            Ask::Set("x".into())
        );
        assert!(parse(&json!({"action": "set"})).is_err());
        assert_eq!(parse(&json!({"action": "drop"})).unwrap(), Ask::Drop);
        let set = Ask::Set("industrial_focus".into());
        assert_eq!(
            precheck(Some("industrial_focus"), &set).unwrap_err().code,
            "nothing_to_change"
        );
        assert!(precheck(Some("political_power_focus"), &set).is_ok());
        assert_eq!(
            precheck(None, &Ask::Drop).unwrap_err().code,
            "nothing_to_change"
        );
        assert!(reached(Some("industrial_focus"), &set));
        assert!(reached(None, &Ask::Drop));
        assert!(!reached(Some("x"), &Ask::Drop));
    }
}
