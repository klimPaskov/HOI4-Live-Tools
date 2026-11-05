//! `hoi4_occupation`: resistance and compliance of occupied states (La
//! Résistance), from the state objects in memory. Pure read: no input, no
//! lease, no cursor, no pause.
//!
//! Provenance (macOS 1.19.4.1, found 5 Nov 2026 by static analysis of the
//! game's own state triggers and live reads on an Italy 1936-37 run):
//! - `CState+0x258` is an embedded `CResistance` (vtable `+0x3518868`; its
//!   `+0x8` points back to the state). Self checking: both are verified.
//! - The evaluators of the script triggers read the state directly:
//!   `resistance` (`CResistanceTrigger`, image `+0x185ce00`) `CState+0x268`,
//!   `resistance_speed` (`+0x185d270`) `+0x270`, `resistance_target`
//!   (`+0x185d040`) `+0x280`, `compliance` (`+0x185d4b0`) `+0x298`,
//!   `compliance_speed` (`+0x185d6e0`) `+0x2a0`; that is `CResistance` `+0x10`,
//!   `+0x18`, `+0x28`, `+0x40`, `+0x48`. All i64 fixed point, 100000 = 1 %.
//! - Live behaviour (Italy occupying Ethiopian states, Jan 1937, four samples
//!   over 12 game days): Wello's resistance rose 12.4 -> 14.8 % at its speed
//!   0.2 %/day (= NResistance.RESISTANCE_GROWTH_BASE), Dire Dawa's fell at its
//!   speed -0.075 %/day while above its target, compliance rose by exactly its
//!   speed (0.0283 %/day) in all four states; every value is 0 in own core
//!   and unoccupied states.
//! - Unconfirmed (reported with `validated:false`): `CResistance+0x20` (a
//!   target before additions; equals the target when nothing is added),
//!   `+0x30`/`+0x38` (a source state and an amount added to the target, seen
//!   only on states next to the named state: El Agheila from Tripoli, Sirte
//!   from Benghasi, Cyrenaica from Derna), `+0x50` (a tag index: ETH for the
//!   occupied Ethiopian states, and for Italy's own non-core states a
//!   country holding a core in the history files: Zadar CRO (of YUG, CRO,
//!   LBV), Dodecanese
//!   GRE, Libya LBA, Eritrea ERI, Somalia SOM).
//! - Aug 1937 also showed own non-core states with resistance (Libya,
//!   Eritrea, Somalia, Zadar, Dodecanese), and Dodecanese at 0 % with a 6 %
//!   target and speed 0, as NResistance.RESISTANCE_TARGET_TO_REENABLE_RESISTANCE
//!   (10) predicts.
//! - Seen once and not explained: compliance of all four occupied Ethiopian
//!   states dropped to 0 between 5 and 9 Jan 1937 and then grew again.
//! - The player's occupation law: `CCountry+0xd78` -> `CCountryOccupationStatus`
//!   (vtable `+0x34bd248`) `+0x108` -> `COccupationLaw` (vtable `+0x35105d0`)
//!   with its libc++ key at `+0x10` (`military_governor_occupation` seen).
//!   Which law slot this is (default or a per-country override) is not proven.
use super::{
    diplomacy_state,
    states::{names, tag_name},
    ui::read,
};
use crate::{
    error::{Error, Result},
    game,
    memory::Reader,
    platform::Process,
    xlayout::{fld, img, Class},
};
use serde_json::{json, Map, Value};
use std::{collections::HashMap, path::Path, time::Instant};

const STATE: Class = Class::new("CState", 0x357f410);
const RESISTANCE: Class = Class::new("CResistance", 0x3518868);
const COUNTRY: Class = Class::new("CCountry", 0x357e388);
const OCCUPATION_STATUS: Class = Class::new("CCountryOccupationStatus", 0x34bd248);
const OCCUPATION_LAW: Class = Class::new("COccupationLaw", 0x35105d0);
const STATE_RESISTANCE: u64 = 0x258;
const R_STATE: u64 = 0x8;
const R_VALUE: u64 = 0x10;
const R_SPEED: u64 = 0x18;
const R_BASE_TARGET: u64 = 0x20;
const R_TARGET: u64 = 0x28;
const R_ADDED_FROM: u64 = 0x30;
const R_ADDED: u64 = 0x38;
const R_COMPLIANCE: u64 = 0x40;
const R_COMPLIANCE_SPEED: u64 = 0x48;
const R_COUNTRY: u64 = 0x50;
const BLOCK: usize = 0x58;
const COUNTRY_OCCUPATION: u64 = 0xd78;
const OS_LAW: u64 = 0x108;
const LAW_KEY: u64 = 0x10;
/// 100000 = 1 %.
const SCALE: f64 = 100_000.0;
const STATE_BUDGET: usize = 4096;
const DEFAULT_LIMIT: usize = 200;

/// Field offsets inside the `CResistance` block for this build.
#[derive(Clone, Copy, Debug)]
struct Offsets {
    state: usize,
    value: usize,
    speed: usize,
    base_target: usize,
    target: usize,
    added_from: usize,
    added: usize,
    compliance: usize,
    compliance_speed: usize,
    country: usize,
}

impl Offsets {
    #[cfg(test)]
    const MACOS: Offsets = Offsets {
        state: R_STATE as usize,
        value: R_VALUE as usize,
        speed: R_SPEED as usize,
        base_target: R_BASE_TARGET as usize,
        target: R_TARGET as usize,
        added_from: R_ADDED_FROM as usize,
        added: R_ADDED as usize,
        compliance: R_COMPLIANCE as usize,
        compliance_speed: R_COMPLIANCE_SPEED as usize,
        country: R_COUNTRY as usize,
    };
    fn resolve(r: &Reader<'_>) -> Result<Self> {
        let abi = r.string_abi();
        let f = |off: u64| -> Result<usize> { Ok(fld(abi, &RESISTANCE, off)? as usize) };
        Ok(Self {
            state: f(R_STATE)?,
            value: f(R_VALUE)?,
            speed: f(R_SPEED)?,
            base_target: f(R_BASE_TARGET)?,
            target: f(R_TARGET)?,
            added_from: f(R_ADDED_FROM)?,
            added: f(R_ADDED)?,
            compliance: f(R_COMPLIANCE)?,
            compliance_speed: f(R_COMPLIANCE_SPEED)?,
            country: f(R_COUNTRY)?,
        })
    }
    fn span(&self) -> usize {
        [
            self.state,
            self.value,
            self.speed,
            self.base_target,
            self.target,
            self.added_from,
            self.added,
            self.compliance,
            self.compliance_speed,
            self.country,
        ]
        .into_iter()
        .max()
        .unwrap_or(0)
            + 8
    }
}

/// The resistance block of one state, raw fixed-point values.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
struct Block {
    back_pointer: u64,
    value: i64,
    speed: i64,
    base_target: i64,
    target: i64,
    added_from: u64,
    added: i64,
    compliance: i64,
    compliance_speed: i64,
    country: i32,
}

fn parse_block(bytes: &[u8], o: &Offsets) -> Option<Block> {
    let q = |at: usize| -> Option<u64> {
        Some(u64::from_le_bytes(bytes.get(at..at + 8)?.try_into().ok()?))
    };
    let d = |at: usize| -> Option<i32> {
        Some(i32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
    };
    Some(Block {
        back_pointer: q(o.state)?,
        value: q(o.value)? as i64,
        speed: q(o.speed)? as i64,
        base_target: q(o.base_target)? as i64,
        target: q(o.target)? as i64,
        added_from: q(o.added_from)?,
        added: q(o.added)? as i64,
        compliance: q(o.compliance)? as i64,
        compliance_speed: q(o.compliance_speed)? as i64,
        country: d(o.country)?,
    })
}

impl Block {
    fn active(&self) -> bool {
        self.value != 0
            || self.target != 0
            || self.compliance != 0
            || self.speed != 0
            || self.compliance_speed != 0
    }
    /// Percentages in 0..=100 (speeds in -100..=100 per day); anything else
    /// means the block is not what the offsets say.
    fn plausible(&self) -> bool {
        let pct = |v: i64| (0..=100 * SCALE as i64).contains(&v);
        let rate = |v: i64| (-100 * SCALE as i64..=100 * SCALE as i64).contains(&v);
        pct(self.value)
            && pct(self.target)
            && pct(self.compliance)
            && rate(self.speed)
            && rate(self.compliance_speed)
            && (0..1024).contains(&self.country)
    }
}

fn pct(v: i64) -> f64 {
    (v as f64 / SCALE * 10_000.0).round() / 10_000.0
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scope {
    /// states the player controls with resistance, and own states others hold
    Player,
    /// every state with an owner other than its controller or a resistance
    All,
}

/// Whether a state belongs in the listing, and the player's role in it.
fn role(
    scope: Scope,
    player: i32,
    owner: i32,
    controller: i32,
    active: bool,
) -> Option<&'static str> {
    let occupied = owner != controller;
    if controller == player && (occupied || active) {
        return Some(if occupied {
            "occupied_by_player"
        } else {
            "owned_non_core_or_resisting"
        });
    }
    if owner == player && occupied {
        return Some("own_state_held_by_other");
    }
    if scope == Scope::All && (occupied || active) {
        return Some("other");
    }
    None
}

struct Row {
    id: i32,
    owner: i32,
    controller: i32,
    role: &'static str,
    block: Block,
    added_from_state: Option<i32>,
}

fn occupation_law(r: &Reader<'_>, image: u64, country: u64) -> Result<Option<String>> {
    let abi = r.string_abi();
    let status = r.u64(country + fld(abi, &COUNTRY, COUNTRY_OCCUPATION)?)?;
    if status == 0 || r.u64(status)? != image + img(abi, OCCUPATION_STATUS.vtable)? {
        return Ok(None);
    }
    let law = r.u64(status + fld(abi, &OCCUPATION_STATUS, OS_LAW)?)?;
    if law == 0 || r.u64(law)? != image + img(abi, OCCUPATION_LAW.vtable)? {
        return Ok(None);
    }
    Ok(
        r.string_bounded(law + fld(abi, &OCCUPATION_LAW, LAW_KEY)?, 96)
            .ok(),
    )
}

pub(super) fn tool() -> Value {
    json!({"name":"hoi4_occupation","description":"Read-only (memory, no input, no pause): resistance and compliance of occupied states (La Résistance). Per state: state_id, name, owner, controller, role (occupied_by_player, own_state_held_by_other, owned_non_core_or_resisting, other), resistance %, resistance target %, resistance speed %/day, compliance %, compliance speed %/day, plus unconfirmed extras (target before additions, target added from a neighbouring state). Also the player's occupation law key (unconfirmed slot). Default scope: states the player controls that have resistance or another owner, and own states others hold; scope all lists every occupied or resisting state in the world. Garrison strength and suppression are not read.","inputSchema":{"type":"object","properties":{"scope":{"type":"string","enum":["player","all"]},"limit":{"type":"integer","minimum":1,"maximum":1100}},"additionalProperties":false}})
}

pub(super) fn occupation(
    process: &dyn Process,
    dir: &Path,
    args: &Map<String, Value>,
) -> Result<Value> {
    let started = Instant::now();
    let scope = match args
        .get("scope")
        .and_then(Value::as_str)
        .unwrap_or("player")
    {
        "player" => Scope::Player,
        "all" => Scope::All,
        _ => {
            return Err(Error::new(
                "invalid_arguments",
                "scope must be player or all",
            ))
        }
    };
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|v| v.clamp(1, 1100) as usize)
        .unwrap_or(DEFAULT_LIMIT);
    let state_names: HashMap<i32, String> = names(dir)
        .map(|v| v.into_iter().collect())
        .unwrap_or_default();
    let mut out = read(process, |r, image| {
        let abi = r.string_abi();
        let g = game::layout(r);
        let l = crate::construction::layout(r);
        let offsets = Offsets::resolve(r)?;
        let span = offsets.span().max(BLOCK);
        let w = diplomacy_state::world(r, image)?;
        let state_vtable = image + img(abi, STATE.vtable)?;
        let resistance_vtable = image + img(abi, RESISTANCE.vtable)?;
        let block_at = fld(abi, &STATE, STATE_RESISTANCE)?;
        let list = r.pointer_array(w.gs + l.gs_states)?;
        if list.len() > STATE_BUDGET {
            return Err(Error::new("invalid_array", "State list too long"));
        }
        let mut id_of: HashMap<u64, i32> = HashMap::new();
        let mut rows = Vec::new();
        let mut unreadable = Vec::new();
        let mut total = 0usize;
        for &state in &list {
            if state == 0 || r.u64(state)? != state_vtable {
                continue;
            }
            let id = r.i32(state + l.state_id)?;
            id_of.insert(state, id);
            let owner = r.i32(state + l.state_owner)?;
            let controller = r.i32(state + l.state_controller)?;
            let base = state + block_at;
            if r.u64(base)? != resistance_vtable {
                unreadable
                    .push(json!({"state_id":id,"reason":"no CResistance at the expected offset"}));
                continue;
            }
            let bytes = r.read(base, span)?;
            let Some(block) = parse_block(&bytes, &offsets) else {
                continue;
            };
            if block.back_pointer != state || !block.plausible() {
                unreadable
                    .push(json!({"state_id":id,"reason":"resistance block failed its checks"}));
                continue;
            }
            let Some(role) = role(scope, w.player_tag, owner, controller, block.active()) else {
                continue;
            };
            total += 1;
            if rows.len() < limit {
                rows.push(Row {
                    id,
                    owner,
                    controller,
                    role,
                    block,
                    added_from_state: None,
                });
            }
        }
        for row in &mut rows {
            if row.block.added_from != 0 {
                row.added_from_state = id_of.get(&row.block.added_from).copied();
            }
        }
        let tag = |t: i32| tag_name(r, image, t).unwrap_or_else(|_| format!("#{t}"));
        let states: Vec<Value> = rows
            .iter()
            .map(|row| {
                let b = &row.block;
                json!({
                    "state_id":row.id,
                    "name":state_names.get(&row.id),
                    "owner":tag(row.owner),
                    "controller":tag(row.controller),
                    "role":row.role,
                    "resistance_active":b.active(),
                    "resistance":pct(b.value),
                    "resistance_target":pct(b.target),
                    "resistance_speed_per_day":pct(b.speed),
                    "compliance":pct(b.compliance),
                    "compliance_speed_per_day":pct(b.compliance_speed),
                    "unconfirmed":{
                        "validated":false,
                        "resistance_target_before_additions":pct(b.base_target),
                        "resistance_target_added":pct(b.added),
                        "resistance_target_added_from_state":row.added_from_state,
                        "resistance_country":if b.country > 0 { json!(tag(b.country)) } else { Value::Null },
                    },
                })
            })
            .collect();
        let date = r.i32(w.gs + g.date)?;
        let law = occupation_law(r, image, w.player_country)?;
        Ok(json!({
            "source":"native state resistance blocks (memory)",
            "player":tag(w.player_tag),
            "date":diplomacy_state::date_json(date),
            "scope":if scope == Scope::All { "all" } else { "player" },
            "count":total,
            "truncated":total > rows.len(),
            "states":states,
            "unreadable":unreadable,
            "units":"percent (resistance 14.8 means 14.8 %); speeds in percent per day",
            "player_occupation_law":{"key":law,"validated":false,
                "note":"CCountryOccupationStatus+0x108; a real law key, not yet proven to be the default law shown on the occupation screen"},
            "garrison":null,"garrison_reason":"offset_not_validated",
            "suppression":null,"suppression_reason":"offset_not_validated",
            "validated":true,
            "validation":{
                "offsets":"resistance, target, speed, compliance and compliance speed are the fields the game's own triggers read (static)",
                "behaviour":"live Jan 1937 (Italy in Ethiopia): resistance and compliance moved by exactly their speeds per game day; resistance decayed above target; 0 in own core and unoccupied states",
                "screen":"not yet compared with the state view's resistance/compliance text"},
            "observation_paused_game":false,
            "input_authorized":false,
        }))
    })?;
    out["freshness"] = json!({"elapsed_ms": started.elapsed().as_secs_f64() * 1000.0});
    Ok(out)
}

// ---------------------------------------------------------------------------
// Probe-only helpers (macprobe), for mapping fields.

fn parse_addr(process: &dyn Process, text: &str) -> Result<u64> {
    let a = u64::from_str_radix(text.trim().trim_start_matches("0x"), 16)
        .map_err(|_| Error::new("invalid_arguments", "hex address"))?;
    Ok(if a < 0x1_0000_0000 {
        process.image_base() + a
    } else {
        a
    })
}

/// Probe-only: `ADDR:LEN` as one hex string.
pub(super) fn debug_hex(process: &dyn Process, spec: &str) -> Result<Value> {
    let (a, n) = spec
        .split_once(':')
        .ok_or_else(|| Error::new("invalid_arguments", "ADDR:LEN"))?;
    let addr = parse_addr(process, a)?;
    let n: usize = n
        .parse()
        .ok()
        .filter(|n| (1..=0x10000).contains(n))
        .ok_or_else(|| Error::new("invalid_arguments", "LEN"))?;
    let bytes = process.read(addr, n)?;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    Ok(json!({"addr":format!("{addr:#x}"),"image":format!("{:#x}",process.image_base()),"hex":hex}))
}

/// Probe-only: every state with its pointer, owner and controller.
pub(super) fn debug_states(process: &dyn Process) -> Result<Value> {
    read(process, |r, image| {
        let g = game::layout(r);
        let l = crate::construction::layout(r);
        let gs = r.u64(image + g.gs_global)?;
        let mut rows = Vec::new();
        for state in r.pointer_array(gs + l.gs_states)? {
            if state == 0 || r.u64(state)? != image + l.state_vtable {
                continue;
            }
            let id = r.i32(state + l.state_id)?;
            let o = r.i32(state + l.state_owner)?;
            let c = r.i32(state + l.state_controller)?;
            rows.push(json!({"id":id,"ptr":format!("{state:#x}"),
                "owner":tag_name(r, image, o).unwrap_or_default(),
                "controller":tag_name(r, image, c).unwrap_or_default()}));
        }
        Ok(json!({"gs":format!("{gs:#x}"),"states":rows}))
    })
}

/// Probe-only: country pointer and relation pointers of `TAG`.
pub(super) fn debug_country(process: &dyn Process, tag: &str) -> Result<Value> {
    read(process, |r, image| {
        let w = diplomacy_state::world(r, image)?;
        let t = diplomacy_state::tag_index_of(r, image, tag)?;
        let c = *w
            .country_of
            .get(&t)
            .ok_or_else(|| Error::new("unknown_country", "no country"))?;
        let ds = r.u64(c + 0xd30)?;
        let mut rels = Vec::new();
        for p in r.pointer_array(ds + 0x20)? {
            let vt = r.u64(p)?;
            rels.push(
                json!({"ptr":format!("{p:#x}"),"vt":format!("{:#x}",vt.wrapping_sub(image)),
                "a":r.i32(p+0xc)?,"b":r.i32(p+0x10)?}),
            );
        }
        Ok(
            json!({"tag":t,"country":format!("{c:#x}"),"ds":format!("{ds:#x}"),
            "player_tag":w.player_tag,"gs":format!("{:#x}",w.gs),"rels":rels}),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block_bytes(
        state: u64,
        value: i64,
        speed: i64,
        target: i64,
        compliance: i64,
        cspeed: i64,
        country: i32,
    ) -> Vec<u8> {
        let mut b = vec![0u8; BLOCK];
        b[0..8].copy_from_slice(&0x3518868u64.to_le_bytes());
        b[8..16].copy_from_slice(&state.to_le_bytes());
        b[0x10..0x18].copy_from_slice(&value.to_le_bytes());
        b[0x18..0x20].copy_from_slice(&speed.to_le_bytes());
        b[0x20..0x28].copy_from_slice(&target.to_le_bytes());
        b[0x28..0x30].copy_from_slice(&target.to_le_bytes());
        b[0x40..0x48].copy_from_slice(&compliance.to_le_bytes());
        b[0x48..0x50].copy_from_slice(&cspeed.to_le_bytes());
        b[0x50..0x54].copy_from_slice(&country.to_le_bytes());
        b
    }

    #[test]
    fn parses_live_wello_block() {
        // Wello (843) on 1937-01-21: resistance 14.8 %, +0.2 %/day, target 41.317 %,
        // compliance 0.34062 %, +0.02826 %/day, resistance of ETH (44)
        let bytes = block_bytes(
            0x7fec2ac31200,
            1_480_000,
            20_000,
            4_131_700,
            34_062,
            2_826,
            44,
        );
        let b = parse_block(&bytes, &Offsets::MACOS).unwrap();
        assert_eq!(b.back_pointer, 0x7fec2ac31200);
        assert!(b.active() && b.plausible());
        assert_eq!(pct(b.value), 14.8);
        assert_eq!(pct(b.speed), 0.2);
        assert_eq!(pct(b.target), 41.317);
        assert_eq!(pct(b.compliance), 0.3406);
        assert_eq!(pct(b.compliance_speed), 0.0283);
        assert_eq!(b.country, 44);
        // decay above target is a negative speed
        let decay = parse_block(
            &block_bytes(1, 1_894_100, -7_500, 1_854_600, 0, 0, 44),
            &Offsets::MACOS,
        )
        .unwrap();
        assert_eq!(pct(decay.speed), -0.075);
        // own core state: all zero
        let core = parse_block(&block_bytes(1, 0, 0, 0, 0, 0, 0), &Offsets::MACOS).unwrap();
        assert!(!core.active() && core.plausible());
        // garbage is refused
        let bad = parse_block(
            &block_bytes(1, 500 * 100_000, 0, 0, 0, 0, 0),
            &Offsets::MACOS,
        )
        .unwrap();
        assert!(!bad.plausible());
        assert!(parse_block(&bytes[..0x40], &Offsets::MACOS).is_none());
    }

    #[test]
    fn roles() {
        let (ita, eth, eng) = (16, 44, 7);
        assert_eq!(
            role(Scope::Player, ita, eth, ita, true),
            Some("occupied_by_player")
        );
        assert_eq!(
            role(Scope::Player, ita, ita, eth, false),
            Some("own_state_held_by_other")
        );
        assert_eq!(
            role(Scope::Player, ita, ita, ita, true),
            Some("owned_non_core_or_resisting")
        );
        assert_eq!(role(Scope::Player, ita, ita, ita, false), None);
        assert_eq!(role(Scope::Player, ita, eth, eng, true), None);
        assert_eq!(role(Scope::All, ita, eth, eng, false), Some("other"));
        assert_eq!(role(Scope::All, ita, eng, eng, false), None);
    }
}
