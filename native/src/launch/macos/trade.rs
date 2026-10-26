//! Resource trade through the ordinary Trade view (key R): the view lists the
//! seven resources with the player's balance, one resource's trade partners
//! with what they can export, and a click on a partner opens the trade offer
//! window (a buy slider, the civilian factories and convoys it needs, Send).
//!
//! macOS 089EA519, mapped live on 26 October 2026 (Germany, February 1936):
//! interface manager (`idler+0x670`) slot `+0x140` is `CCountryTradeView`;
//! its root container is at `+0x430` and the trade offer window's root
//! container at `+0x2440`. Grids use the shared `CStandardGridBox` layout
//! (items array at `+0x190`, item container at `item+0x10`). The resource
//! order of the table and the filter buttons is the order of
//! `common/resources` (oil, aluminium, rubber, tungsten, steel, chromium,
//! coal). Only what the view itself shows is read: the player's own balances
//! and the partners' public export offers.
use super::diplomacy::{clean, find};
use super::ui::{accept_command, clear_notices, read, settled, wait};
use crate::{
    error::{Error, Result},
    gui_tree::{self, Rect},
    memory::{Reader, StringAbi},
    platform::{
        macos_input::{Key, MacKeys},
        Process,
    },
};
use serde_json::{json, Value};

const MANAGER: u64 = 0x670;
const VIEW: u64 = 0x140;
const ROOT: u64 = 0x430;
const OFFER_ROOT: u64 = 0x2440;
const GRID_ITEMS: u64 = 0x190;
const ITEM_CONTAINER: u64 = 0x10;
const CONTAINER_VTABLE: u64 = 0x34ac0b0;
const CONTAINER_BASE: u64 = 0x30;
const TEXT: u64 = 0x100;
const GAME: (u32, u32) = (1920, 1080);
const RESOURCES: [&str; 7] = [
    "oil",
    "aluminium",
    "rubber",
    "tungsten",
    "steel",
    "chromium",
    "coal",
];
/// Offer window layout (countrytradeview.gui): the slider sits 234 px left
/// of the Send button's left edge, is 300 px long and 53 px above its top.
const SLIDER_LEFT_OF_SEND: f64 = 234.0;
const SLIDER_ABOVE_SEND: f64 = 53.0;
const SLIDER_LENGTH: f64 = 300.0;
const SLIDER_ARROW: f64 = 14.0;

fn view_root(r: &Reader<'_>, image: u64) -> Result<u64> {
    if r.string_abi() != StringAbi::LibCxx {
        return Err(Error::new(
            "capability_unavailable",
            "The trade view is not reviewed for this build",
        ));
    }
    let idler = r.u64(image + crate::game::layout(r).idler_global)?;
    let view = r.u64(r.u64(idler + MANAGER)? + VIEW)?;
    gui_tree::container(r, r.u64(view + ROOT)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Trade view root unavailable"))
}
fn open(r: &Reader<'_>, image: u64) -> Result<bool> {
    gui_tree::visible(r, view_root(r, image)?)
}
fn offer_root(r: &Reader<'_>, image: u64) -> Result<u64> {
    let idler = r.u64(image + crate::game::layout(r).idler_global)?;
    let view = r.u64(r.u64(idler + MANAGER)? + VIEW)?;
    gui_tree::container(r, r.u64(view + OFFER_ROOT)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Trade offer window unavailable"))
}
fn offer_open(r: &Reader<'_>, image: u64) -> Result<bool> {
    gui_tree::visible(r, offer_root(r, image)?)
}
fn text_of(r: &Reader<'_>, image: u64, parent: u64, name: &str) -> Result<String> {
    Ok(match find(r, image, parent, name, 0)? {
        Some(t) => clean(&r.string_bounded(t + TEXT, 512)?),
        None => String::new(),
    })
}
/// Entry containers of the grid `grid` inside the window named `window`.
fn grid_entries(r: &Reader<'_>, image: u64, window: &str, grid: &str) -> Result<Vec<u64>> {
    let root = view_root(r, image)?;
    let holder = find(r, image, root, window, 0)?
        .ok_or_else(|| Error::new("gui_target_unavailable", format!("Trade view part {window} not found")))?;
    let holder = gui_tree::container(r, holder, image)?
        .ok_or_else(|| Error::new("invalid_gui", format!("Trade view part {window} is not a window")))?;
    let grid = find(r, image, holder, grid, 0)?
        .ok_or_else(|| Error::new("gui_target_unavailable", format!("Trade list {grid} not found")))?;
    let (_, data, count) = r.array_header(grid + GRID_ITEMS)?;
    let mut out = Vec::new();
    for index in 0..count.min(128) as u64 {
        let item = r.u64(data + index * 8)?;
        let entry = r.u64(item + ITEM_CONTAINER)?;
        let base = if r.u64(entry)? == image + CONTAINER_VTABLE {
            entry + CONTAINER_BASE
        } else {
            entry
        };
        if let Some(frame) = gui_tree::container(r, base, image)? {
            out.push(frame);
        }
    }
    Ok(out)
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Partner {
    pub country: String,
    /// units bought from this partner now
    pub bought: String,
    /// "current/available" as the list shows it
    pub available: String,
    pub convoys: String,
    flag: Option<Rect>,
}
fn partners(r: &Reader<'_>, image: u64) -> Result<Vec<Partner>> {
    let mut out = Vec::new();
    for frame in grid_entries(r, image, "trade", "trade_grid")? {
        let mut flag = None;
        for part in ["country_flag_frame", "country_flag", "available_export_background", "exported_background", "convoys_background"] {
            if let Some(w) = gui_tree::child(r, image, frame, part)? {
                if let Ok(rect) = gui_tree::rect_unclipped(r, image, w, GAME) {
                    flag = Some(rect);
                    break;
                }
            }
        }
        out.push(Partner {
            country: text_of(r, image, frame, "country")?,
            bought: text_of(r, image, frame, "exported")?,
            available: text_of(r, image, frame, "available_export")?,
            convoys: text_of(r, image, frame, "convoys")?,
            flag,
        });
    }
    Ok(out)
}
fn filter_rect(r: &Reader<'_>, image: u64, index: usize) -> Result<Rect> {
    let frame = *grid_entries(r, image, "filters", "resources_grid")?
        .get(index)
        .ok_or_else(|| Error::new("gui_target_unavailable", "Resource filter not found"))?;
    let button = gui_tree::child(r, image, frame, "background")?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Resource filter button not found"))?;
    gui_tree::rect(r, image, button, GAME)
}
fn resource_rows(r: &Reader<'_>, image: u64) -> Result<Vec<Value>> {
    let mut out = Vec::new();
    for (index, frame) in grid_entries(r, image, "resources", "resources_grid")?
        .into_iter()
        .enumerate()
    {
        let number = |name: &str| -> Result<Value> {
            let t = text_of(r, image, frame, name)?;
            Ok(t.trim().parse::<i64>().map(|v| json!(v)).unwrap_or(json!(t)))
        };
        out.push(json!({
            "resource":RESOURCES.get(index).copied().unwrap_or("unknown"),
            "index":index,
            "balance":number("balance")?,"produced":number("produced")?,
            "imported":number("imported")?,"exported":number("exported")?,
            "special_projects":number("special_project")?,"production_lines":number("production_line")?,
            "needed":text_of(r, image, frame, "needed")?,
        }));
    }
    Ok(out)
}
fn civilian_factories(r: &Reader<'_>, image: u64) -> Result<Value> {
    let root = view_root(r, image)?;
    let n = |name: &str| -> Result<Value> {
        let t = text_of(r, image, root, name)?;
        Ok(t.trim().parse::<i64>().map(|v| json!(v)).unwrap_or(json!(t)))
    };
    Ok(json!({"total":n("civ_factories")?,"used":n("factories_used")?,"available":n("factories_available")?}))
}

fn ensure_open(process: &dyn Process, keys: &MacKeys) -> Result<()> {
    if !read(process, open)? {
        keys.press(Key::letter('r')?)?;
        wait(process, "open trade view", open)?;
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    Ok(())
}
/// Click the resource's filter button. The filter is a radio selection (one
/// resource is always shown); the toggled look is not readable from the
/// button state (hover also sets it), so the choice is confirmed from the
/// offer window's own text ("units of Rubber") before anything is sent.
fn select_resource(process: &dyn Process, keys: &MacKeys, index: usize) -> Result<()> {
    let rect = settled(process, "resource filter", |r, i| filter_rect(r, i, index))?;
    let (x, y) = rect.center();
    keys.click(GAME, x, y)?;
    std::thread::sleep(std::time::Duration::from_millis(500));
    Ok(())
}
fn resource_index(name: &str) -> Result<usize> {
    let lower = name.trim().to_lowercase();
    if let Ok(i) = lower.parse::<usize>() {
        if i < RESOURCES.len() {
            return Ok(i);
        }
    }
    RESOURCES
        .iter()
        .position(|r| *r == lower || (lower == "aluminum" && *r == "aluminium"))
        .ok_or_else(|| {
            Error::new(
                "invalid_arguments",
                format!("resource must be one of: {}", RESOURCES.join(", ")),
            )
        })
}

pub(super) fn tool() -> Value {
    json!({"name":"hoi4_trade","description":"Read the Trade view (opens it with the R key, changes nothing): the player's civilian factories and, for the seven resources (oil, aluminium, rubber, tungsten, steel, chromium, coal), balance, produced, imported, exported, special projects, production lines and needed; with `resource` it also selects that resource's filter and lists its trade partners (country, units bought now, units available as 'current/available', convoys used). Selecting a filter is an ordinary click on the view. Set a purchase with hoi4_trade_set.","inputSchema":{"type":"object","properties":{"resource":{"type":"string","maxLength":24},"limit":{"type":"integer","minimum":1,"maximum":60},"lease_token":{"type":"string"}},"required":["lease_token"],"additionalProperties":false}})
}
pub(super) fn set_tool() -> Value {
    json!({"name":"hoi4_trade_set","description":"Buy `amount` units of `resource` from partner `country` (exact name as hoi4_trade lists it) through the Trade view: select the resource filter, click the partner's row, set the offer window's buy slider (a click on its track, then its arrows until the window's own 'Buy amount' reads the number) and press Send. amount: an integer (0 cancels the purchase), \"max\" or \"needed\" (the window's button that buys what the country needs). Typed refusals before anything is sent: partner_not_listed (lists the partners), amount_above_available (the window's maximum is named), trade_unavailable (the game offers no slider). Verified from the partner's contracted units in the view's Delivered column after Send (the game moves the goods with convoys over the following days; imports show in the balance table once they arrive).","inputSchema":{"type":"object","properties":{"resource":{"type":"string","maxLength":24},"country":{"type":"string","maxLength":64},"amount":{"anyOf":[{"type":"integer","minimum":0,"maximum":100000},{"type":"string","enum":["max","needed"]}]},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["resource","country","amount","request_id","lease_token"],"additionalProperties":false}})
}

/// The Trade view's tables (the caller has checked the lease).
pub(super) fn view(
    process: &dyn Process,
    keys: &MacKeys,
    resource: Option<&str>,
    limit: usize,
) -> Result<Value> {
    clear_notices(process, keys)?;
    ensure_open(process, keys)?;
    if let Some(name) = resource {
        select_resource(process, keys, resource_index(name)?)?;
    }
    let chosen = match resource {
        Some(name) => Some(resource_index(name)?),
        None => None,
    };
    let (table, factories, list) = settled(process, "trade view", |r, i| {
        Ok((
            resource_rows(r, i)?.iter().map(Value::to_string).collect::<Vec<_>>(),
            civilian_factories(r, i)?.to_string(),
            partners(r, i)?,
        ))
    })?;
    let rows: Vec<Value> = table
        .iter()
        .filter_map(|v| serde_json::from_str(v).ok())
        .collect();
    let mut out = json!({"civilian_factories":serde_json::from_str::<Value>(&factories).unwrap_or(Value::Null),
        "resources":rows,
        "selected_resource":chosen.and_then(|i| RESOURCES.get(i)),"selected_note":"the partner list belongs to the resource filter last clicked (the view always shows one resource; without `resource` it is whichever was last chosen)",
        "validation":{"resource_order":"common/resources order; the table columns match the economy (oil 3, aluminium 95, ... coal 195 at the 1936 start)",
            "partners":"names, bought and available read from the list; the offer window's slider is set by hoi4_trade_set"},
        "observation_paused_game":false});
    {
        out["partners"] = json!(list
            .iter()
            .take(limit)
            .map(|p| json!({"country":p.country,"delivered_over_contracted":p.bought,"units_contracted":contracted(&p.bought),"available":p.available,"convoys":p.convoys}))
            .collect::<Vec<_>>());
        out["partner_count"] = json!(list.len());
    }
    Ok(out)
}

/// Texts of the open trade offer window.
fn offer_texts(r: &Reader<'_>, image: u64) -> Result<(String, String, i64, i64)> {
    let root = offer_root(r, image)?;
    let country = text_of(r, image, root, "country")?;
    let factories = text_of(r, image, root, "factories_description")?;
    let min = text_of(r, image, root, "min_amount")?.trim().parse().unwrap_or(0);
    let max = text_of(r, image, root, "max_amount")?
        .trim()
        .parse()
        .unwrap_or(0);
    Ok((country, factories, min, max))
}
/// "Buy amount: N" of the offer window.
fn buy_amount(text: &str) -> Option<i64> {
    let after = text.split("Buy amount:").nth(1)?;
    after.split_whitespace().next()?.parse().ok()
}
fn offer_button(r: &Reader<'_>, image: u64, name: &str) -> Result<Rect> {
    let root = offer_root(r, image)?;
    let button = gui_tree::child(r, image, root, name)?
        .ok_or_else(|| Error::new("gui_target_unavailable", format!("Offer button {name} not found")))?;
    gui_tree::rect(r, image, button, GAME)
}
fn current_amount(process: &dyn Process) -> Result<Option<i64>> {
    Ok(buy_amount(&read(process, offer_texts)?.1))
}

/// What a planned purchase will do (checked before any input that sends).
pub(super) struct Plan {
    pub resource: usize,
    pub country: String,
    pub partner: Partner,
}

/// Select the resource, find the partner; refusals need no more input.
pub(super) fn plan(
    process: &dyn Process,
    keys: &MacKeys,
    resource: &str,
    country: &str,
) -> Result<Plan> {
    let index = resource_index(resource)?;
    clear_notices(process, keys)?;
    ensure_open(process, keys)?;
    if read(process, offer_open)? {
        close_offer(process, keys);
    }
    select_resource(process, keys, index)?;
    let list = settled(process, "trade partners", partners)?;
    let found = list
        .iter()
        .find(|p| p.country.eq_ignore_ascii_case(country.trim()))
        .cloned()
        .ok_or_else(|| {
            Error::new(
                "partner_not_listed",
                format!(
                    "{country} does not offer {}. Partners listed: {}",
                    RESOURCES[index],
                    list.iter()
                        .take(40)
                        .map(|p| format!("{} ({})", p.country, p.available))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )
        })?;
    Ok(Plan {
        resource: index,
        country: found.country.clone(),
        partner: found,
    })
}

fn close_offer(process: &dyn Process, keys: &MacKeys) {
    if let Ok(rect) = read(process, |r, i| offer_button(r, i, "cancel_button")) {
        let (x, y) = rect.center();
        let _ = keys.click(GAME, x, y);
        let _ = wait(process, "offer closed", |r, i| Ok(!offer_open(r, i)?));
    }
}

/// Open the partner's offer window, set the amount and Send. `sent` is set
/// just before the Send click.
pub(super) fn execute(
    process: &dyn Process,
    keys: &MacKeys,
    plan: &Plan,
    amount: &Amount,
    sent: &std::cell::Cell<bool>,
) -> Result<Value> {
    // the list may have scrolled or been re-sorted since the plan
    let rect = settled(process, "partner row", |r, i| {
        partners(r, i)?
            .into_iter()
            .find(|p| p.country == plan.country)
            .and_then(|p| p.flag)
            .ok_or_else(|| Error::new("gui_target_unavailable", "Partner row not on screen"))
    })
    .map_err(|e| {
        Error::new(
            e.code,
            format!("{} (scroll the partner list so that {} is visible, or sort it)", e.message, plan.country),
        )
    })?;
    let (x, y) = rect.center();
    keys.click(GAME, x, y)?;
    wait(process, "offer window", offer_open)?;
    let flow = (|| -> Result<Value> {
        let head = read(process, |r, i| text_of(r, i, offer_root(r, i)?, "trade"))?;
        let wanted = format!("units of {}", RESOURCES[plan.resource]).to_lowercase();
        if !head.to_lowercase().contains(&wanted) {
            return Err(Error::new(
                "resource_mismatch",
                format!(
                    "The offer window is for another resource ({head}); wanted {}; nothing was sent",
                    RESOURCES[plan.resource]
                ),
            ));
        }
        let (_, _, min, max) = settled(process, "offer window", offer_texts)?;
        let target = match amount {
            Amount::Exact(n) => *n,
            Amount::Max => max,
            Amount::Needed => -1,
        };
        if target > max {
            return Err(Error::new(
                "amount_above_available",
                format!("The window's maximum is {max} units"),
            ));
        }
        let send = settled(process, "send button", |r, i| offer_button(r, i, "send_button"))?;
        let left = f64::from(send.x) - SLIDER_LEFT_OF_SEND;
        let mid = f64::from(send.y) - SLIDER_ABOVE_SEND;
        if max <= min {
            return Err(Error::new("trade_unavailable", "The game offers no amount to set"));
        }
        if matches!(amount, Amount::Needed) {
            let (bx, by) = read(process, |r, i| offer_button(r, i, "buy_needed_amount_button"))?.center();
            keys.click(GAME, bx, by)?;
            std::thread::sleep(std::time::Duration::from_millis(300));
        } else {
            // a click on the track, then arrow clicks until the window reads the amount
            let span = SLIDER_LENGTH - 2.0 * SLIDER_ARROW;
            let fraction = (target - min) as f64 / (max - min) as f64;
            let at = left + SLIDER_ARROW + span * fraction.clamp(0.0, 1.0);
            keys.click(GAME, at, mid)?;
            std::thread::sleep(std::time::Duration::from_millis(250));
            // the window sells whole blocks (one civilian factory per block,
            // e.g. 8 units of rubber): the arrows move one block, so walk
            // towards the target and stop when it is reached or passed
            let mut last = None;
            for _ in 0..80 {
                let now = current_amount(process)?.ok_or_else(|| {
                    Error::new("input_readback_failed", "The offer window shows no 'Buy amount'")
                })?;
                if now == target || last == Some(now) {
                    break;
                }
                last = Some(now);
                let up = now < target;
                let (ax, ay) = if up {
                    (left + SLIDER_LENGTH - 6.0, mid)
                } else {
                    (left + 6.0, mid)
                };
                keys.click(GAME, ax, ay)?;
                std::thread::sleep(std::time::Duration::from_millis(150));
                let after = current_amount(process)?.unwrap_or(now);
                if (up && after > target) || (!up && after < target) {
                    return Err(Error::new(
                        "amount_not_offered",
                        format!(
                            "The window sells in blocks: it offers {} and {} units, not {target}; nothing was sent",
                            now.min(after),
                            now.max(after)
                        ),
                    ));
                }
            }
        }
        let chosen = current_amount(process)?.unwrap_or(-1);
        if !matches!(amount, Amount::Needed) && chosen != target {
            return Err(Error::new(
                "input_readback_failed",
                format!("The slider reads {chosen}, not {target}; nothing was sent"),
            ));
        }
        let texts = read(process, offer_texts)?;
        let (sx, sy) = settled(process, "send button", |r, i| offer_button(r, i, "send_button"))?.center();
        sent.set(true);
        keys.click(GAME, sx, sy)?;
        wait(process, "offer sent", |r, i| Ok(!offer_open(r, i)?))?;
        Ok(json!({"offer_text":texts.1,"max":max,"chosen":chosen}))
    })();
    match flow {
        Ok(v) => {
            let _ = accept_command(process, keys);
            Ok(v)
        }
        Err(e) => {
            close_offer(process, keys);
            Err(e)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Amount {
    Exact(i64),
    Max,
    Needed,
}
/// "units bought now" of partner `country` in the view as a number.
pub(super) fn bought_now(process: &dyn Process, country: &str) -> Result<Option<i64>> {
    let list = read(process, partners)?;
    Ok(list
        .iter()
        .find(|p| p.country == country)
        .and_then(|p| contracted(&p.bought)))
}
/// Units contracted from the "Delivered" column text: "16/16" (delivered /
/// contracted, read live after a purchase of 16 rubber) or a plain "0".
pub(super) fn contracted(text: &str) -> Option<i64> {
    let last = text.rsplit('/').next()?;
    last.trim().parse().ok()
}
pub(super) fn resource_name(index: usize) -> &'static str {
    RESOURCES.get(index).copied().unwrap_or("unknown")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resources_are_named_by_index_or_name() {
        assert_eq!(resource_index("oil").unwrap(), 0);
        assert_eq!(resource_index("Aluminum").unwrap(), 1);
        assert_eq!(resource_index("6").unwrap(), 6);
        assert_eq!(resource_index("coal").unwrap(), 6);
        assert_eq!(resource_index("gold").unwrap_err().code, "invalid_arguments");
        assert_eq!(resource_index("7").unwrap_err().code, "invalid_arguments");
    }
    #[test]
    fn the_delivered_column_gives_the_contracted_units() {
        assert_eq!(contracted("16/16"), Some(16));
        assert_eq!(contracted("0/16"), Some(16));
        assert_eq!(contracted("0"), Some(0));
        assert_eq!(contracted("-"), None);
    }
    #[test]
    fn buy_amount_is_read_from_the_window_text() {
        assert_eq!(buy_amount("Buy amount: 0\nFor civilian factories: 0"), Some(0));
        assert_eq!(buy_amount("Buy amount: 168 For civilian factories: 3"), Some(168));
        assert_eq!(buy_amount("nothing"), None);
    }
}
