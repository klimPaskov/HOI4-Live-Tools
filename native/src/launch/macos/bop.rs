//! Balances of power (By Blood Alone and later): every active balance in the world with its
//! owner tags, value, the range the value stands in and the side it leans to. Pure memory read.
//!
//! Mapped live on 9 November 2026 (macOS 1.19.4.1 DFA875A5, 1 January 1936) and from the game
//! code: the game state +0x410 holds the `CPowerBalanceSystem` (vtable 0x358a2c8; the
//! `power_balance_value` trigger reads it there, 0x101846ce0), whose +0x8 array holds the
//! `CPowerBalance` objects inline (0x178 bytes each). A balance: +0x10 its template (id string
//! at +0x10), +0x18 / +0x20 the left and right `CPowerBalanceSide` (id string at +0x10), +0x28
//! the side it leans to, +0x30 the value (i64 1/100000), +0x38 the owner tags (i32 array, size
//! at +0x44; the `has_any_power_balance` trigger matches the scope's tag there, 0x1014217a0),
//! +0xb0 the current `CPowerBalanceRange` (+0x8 min, +0x10 max). Four were active: DEN, FIN, ETH
//! and PRC. ETH_centralization_balance read -0.25 in the range -0.35 .. -0.15 leaning to
//! ETH_mesafint_side, exactly `common/bop/ETH.txt` (initial_value -0.25,
//! ETH_mesafint_influence_range).
use crate::{
    error::{Error, Result},
    game,
    memory::Reader,
    xlayout::{fld, img, Class},
};
use serde_json::{json, Value};

const FIXED: f64 = 100_000.0;
const GAME_STATE: Class = Class::new("CCurrentGameState", 0x35044e8);
const SYSTEM: Class = Class::new("CPowerBalanceSystem", 0x358a2c8);
const BALANCE: Class = Class::new("CPowerBalance", 0x358a268);
const SIDE: Class = Class::new("CPowerBalanceSide", 0x358a750);
const RANGE: Class = Class::new("CPowerBalanceRange", 0x358a6a0);
const TEMPLATE: Class = Class::new("CPowerBalanceTemplate", 0x358a5b8);
const GS_SYSTEM: u64 = 0x410;
const SYSTEM_BALANCES: u64 = 0x8;
const BALANCE_SIZE: u64 = 0x178;
const BALANCE_TEMPLATE: u64 = 0x10;
const BALANCE_LEFT: u64 = 0x18;
const BALANCE_RIGHT: u64 = 0x20;
const BALANCE_SIDE: u64 = 0x28;
const BALANCE_VALUE: u64 = 0x30;
const BALANCE_OWNERS: u64 = 0x38;
const BALANCE_RANGE: u64 = 0xb0;
const ID: u64 = 0x10;
const RANGE_MIN: u64 = 0x8;
const RANGE_MAX: u64 = 0x10;
const LIMIT: usize = 256;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Balance {
    pub id: String,
    pub owners: Vec<String>,
    pub value: f64,
    pub range: Option<(f64, f64)>,
    pub side: Option<String>,
    pub left: Option<String>,
    pub right: Option<String>,
}
impl Balance {
    pub(super) fn json(&self, player: &str) -> Value {
        json!({"id": self.id, "owners": self.owners, "own": self.owners.iter().any(|o| o == player),
            "value": self.value,
            "range": self.range.map(|(lo, hi)| json!({"min": lo, "max": hi})),
            "side": self.side, "left_side": self.left, "right_side": self.right})
    }
}

fn is_ptr(p: u64) -> bool {
    (0x10000..=0x7fff_ffff_0000).contains(&p) && p % 8 == 0
}
fn fixed(r: &Reader<'_>, a: u64) -> Result<f64> {
    Ok((r.u64(a)? as i64 as f64 / FIXED * 10_000.0).round() / 10_000.0)
}
fn typed(r: &Reader<'_>, image: u64, p: u64, class: &Class) -> Result<bool> {
    Ok(is_ptr(p) && r.u64(p)? == image + img(r.string_abi(), class.vtable)?)
}
fn id_of(r: &Reader<'_>, image: u64, p: u64, class: &Class) -> Result<Option<String>> {
    if !typed(r, image, p, class)? {
        return Ok(None);
    }
    Ok(Some(r.string_bounded(
        p + fld(r.string_abi(), class, ID)?,
        128,
    )?))
}

/// Every active balance of power.
pub(super) fn balances(r: &Reader<'_>, image: u64) -> Result<Vec<Balance>> {
    let abi = r.string_abi();
    let g = game::layout(r);
    let gs = r.u64(image + g.gs_global)?;
    let system = r.u64(gs + fld(abi, &GAME_STATE, GS_SYSTEM)?)?;
    if !typed(r, image, system, &SYSTEM)? {
        return Err(Error::new(
            "invalid_type",
            "Balance of power system not found",
        ));
    }
    let (_, data, count) = r.array_header(system + fld(abi, &SYSTEM, SYSTEM_BALANCES)?)?;
    if count > LIMIT {
        return Err(Error::new("invalid_array", "Too many balances of power"));
    }
    let tag_db = r.u64(image + g.tag_db_global)?;
    let (_, tag_data, tag_count) = r.array_header(tag_db)?;
    let f = |mac: u64| fld(abi, &BALANCE, mac);
    let size = f(BALANCE_SIZE)?;
    let mut out = Vec::new();
    for i in 0..count as u64 {
        let b = data + i * size;
        if !typed(r, image, b, &BALANCE)? {
            return Err(Error::new("invalid_type", "Balance of power expected"));
        }
        let Some(id) = id_of(r, image, r.u64(b + f(BALANCE_TEMPLATE)?)?, &TEMPLATE)? else {
            continue;
        };
        let (_, owners_data, owners) = r.array_header(b + f(BALANCE_OWNERS)?)?;
        let mut tags = Vec::new();
        for k in 0..owners.min(16) as u64 {
            let tag = r.i32(owners_data + 4 * k)?;
            if tag > 0 && (tag as usize) < tag_count {
                tags.push(r.string(tag_data + tag as u64 * g.tag_stride)?);
            }
        }
        let range = r.u64(b + f(BALANCE_RANGE)?)?;
        let range = if typed(r, image, range, &RANGE)? {
            Some((
                fixed(r, range + fld(abi, &RANGE, RANGE_MIN)?)?,
                fixed(r, range + fld(abi, &RANGE, RANGE_MAX)?)?,
            ))
        } else {
            None
        };
        out.push(Balance {
            id,
            owners: tags,
            value: fixed(r, b + f(BALANCE_VALUE)?)?,
            range,
            side: id_of(r, image, r.u64(b + f(BALANCE_SIDE)?)?, &SIDE)?,
            left: id_of(r, image, r.u64(b + f(BALANCE_LEFT)?)?, &SIDE)?,
            right: id_of(r, image, r.u64(b + f(BALANCE_RIGHT)?)?, &SIDE)?,
        });
    }
    Ok(out)
}
