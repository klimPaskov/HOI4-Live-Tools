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
const ROOT: u64 = 0x490;
/// CCountryTradeView -> CTradeOfferWindow (vtable 0x3698030) -> root container (1.19.4).
const OFFER_WINDOW: u64 = 0x1ac8;
const OFFER_VTABLE: u64 = 0x3698030;
const OFFER_ROOT: u64 = 0xd60;
const GRID_ITEMS: u64 = 0x190;
const ITEM_CONTAINER: u64 = 0x10;
const CONTAINER_VTABLE: u64 = 0x36ba978;
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
    // 1.19.4: the view points at its CTradeOfferWindow (+0x1ac8), whose root container is at
    // +0xd60 (named `tradeoffer_window`). Both are checked before the root is trusted.
    let offer = r.u64(view + OFFER_WINDOW)?;
    if !(0x10000..=0x7fff_ffff_0000).contains(&offer) || offer % 8 != 0 {
        return Err(Error::new("invalid_gui", "Trade offer window not created yet"));
    }
    if r.u64(offer)? != image + OFFER_VTABLE {
        return Err(Error::new("invalid_gui", "Trade offer window class unexpected"));
    }
    let root = r.u64(offer + OFFER_ROOT)?;
    if root == 0 || r.u64(root)? != image + CONTAINER_VTABLE
        || gui_tree::name(r, root + CONTAINER_BASE, image)?.as_deref() != Some("tradeoffer_window")
    {
        return Err(Error::new("invalid_gui", "Trade offer window not created yet"));
    }
    Ok(root)
}
fn offer_open(r: &Reader<'_>, image: u64) -> Result<bool> {
    match offer_root(r, image) {
        Ok(root) => gui_tree::visible(r, root),
        Err(e) if e.code == "invalid_gui" => Ok(false),
        Err(e) => Err(e),
    }
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
    // 1.19.4 renamed `factories_used` to `factories_trade` and added `factories_consumer`.
    Ok(json!({"total":n("civ_factories")?,"used_for_trade":n("factories_trade")?,
        "consumer_goods":n("factories_consumer")?,"available":n("factories_available")?}))
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
pub(super) fn auto_import_tool() -> Value {
    json!({"name":"hoi4_auto_import","description":"HOI4 1.19.4 Automatic Importing (Trade view): without enabled/budget it reads whether it is on, its civilian factory budget and the factories available for trade (opens the view with R, changes nothing). With enabled (true/false) and/or budget (civilian factories; only while on) it clicks the checkbox and the budget slider, verified from the view's own checkbox frame and budget text. The game then starts and ends trades itself to cover resource shortages (oil also covers fuel) and blocks manual trades: hoi4_trade_set refuses with auto_import_enabled while it is on. Import priority order and accepted shortages are left at the game's settings.","inputSchema":{"type":"object","properties":{"enabled":{"type":"boolean"},"budget":{"type":"integer","minimum":0,"maximum":500},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["lease_token"],"additionalProperties":false}})
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
    if read(process, auto_import_enabled)? == Some(true) {
        return Err(Error::new(
            "auto_import_enabled",
            "Automatic Importing is on; the game blocks manual trades. Turn it off with \
             hoi4_auto_import {enabled:false} first, or let it manage imports.",
        ));
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
    fn budget_text_colour_tells_whether_auto_import_is_on() {
        assert_eq!(enabled_from_budget_text("\u{11}g5\u{11}!"), Some(false));
        assert_eq!(enabled_from_budget_text("5"), Some(true));
        assert_eq!(enabled_from_budget_text(""), None);
    }
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

// ---- 1.19.4 Automatic Importing -------------------------------------------------------------
// The Trade view's `auto_import_checkbox` (CCheckBox, renderer sprite +0xa8), the budget
// text `auto_import_budget_txt`, and the slider `auto_import_slider` (track 145 px wide, 85 px
// right of `auto_import_budget_bg`, see interface/countrytradeview.gui). The game spends up to
// the budget of civilian factories on imports of resources in shortage and blocks manual trades
// while it is on. The priority window (order, accepted shortages, oil-for-fuel) is not driven.
const AUTO_TRACK_DX: f64 = 85.0;
const AUTO_TRACK_W: f64 = 145.0;
const AUTO_TRACK_DY: f64 = 12.0;
const AUTO_KNOB: f64 = 16.0;

fn auto_checkbox(r: &Reader<'_>, image: u64) -> Result<u64> {
    let root = view_root(r, image)?;
    find(r, image, root, "auto_import_checkbox", 0)?
        .ok_or_else(|| Error::new("capability_unavailable", "This build has no Automatic Importing checkbox"))
}
/// Whether Automatic Importing is on, from what the view shows: the game greys the budget text
/// (colour code `\u{11}g`) while it is off and prints it plain while it is on (mapped live
/// 31 October 2026; the checkbox's own state word also encodes hover, so it is not used).
pub(super) fn auto_import_enabled(r: &Reader<'_>, image: u64) -> Result<Option<bool>> {
    let root = view_root(r, image)?;
    let _ = auto_checkbox(r, image)?;
    let Some(text) = find(r, image, root, "auto_import_budget_txt", 0)? else {
        return Ok(None);
    };
    Ok(enabled_from_budget_text(&r.string_bounded(text + TEXT, 64)?))
}
pub(super) fn enabled_from_budget_text(raw: &str) -> Option<bool> {
    let digits = clean(raw);
    if digits.trim().parse::<i64>().is_err() {
        return None;
    }
    Some(!raw.starts_with("\u{11}g"))
}
fn auto_budget(r: &Reader<'_>, image: u64) -> Result<Option<i64>> {
    let root = view_root(r, image)?;
    Ok(text_of(r, image, root, "auto_import_budget_txt")?.trim().parse::<i64>().ok())
}
fn available_factories(r: &Reader<'_>, image: u64) -> Result<Option<i64>> {
    let root = view_root(r, image)?;
    let t = text_of(r, image, root, "factories_available")?;
    Ok(t.rsplit(' ').next().and_then(|v| v.trim().parse::<i64>().ok()))
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct AutoImport {
    pub enabled: Option<bool>,
    pub budget: Option<i64>,
    pub available: Option<i64>,
}
fn auto_state(r: &Reader<'_>, image: u64) -> Result<AutoImport> {
    Ok(AutoImport {
        enabled: auto_import_enabled(r, image)?,
        budget: auto_budget(r, image)?,
        available: available_factories(r, image)?,
    })
}
impl AutoImport {
    pub(super) fn json(&self) -> Value {
        json!({"enabled":self.enabled,"budget_factories":self.budget,"available_factories":self.available})
    }
}
/// Open the Trade view and read the Automatic Importing state (changes nothing).
pub(super) fn auto_import_read(process: &dyn Process, keys: &MacKeys) -> Result<AutoImport> {
    clear_notices(process, keys)?;
    ensure_open(process, keys)?;
    settled(process, "automatic importing", auto_state)
}
/// Turn Automatic Importing on/off and/or set its civilian factory budget with ordinary clicks:
/// the checkbox, then clicks on the slider track until the view's own budget text reads `budget`.
pub(super) fn auto_import_set(
    process: &dyn Process,
    keys: &MacKeys,
    enabled: Option<bool>,
    budget: Option<i64>,
) -> Result<AutoImport> {
    let before = auto_import_read(process, keys)?;
    if before.enabled.is_none() {
        return Err(Error::new("invalid_gui", "Automatic Importing checkbox state unreadable"));
    }
    let want_on = enabled.unwrap_or(before.enabled == Some(true));
    if budget.is_some() && !want_on {
        return Err(Error::new(
            "invalid_arguments",
            "The budget can only be changed while Automatic Importing is on (the game disables the slider)",
        ));
    }
    if let Some(want) = enabled {
        if before.enabled != Some(want) {
            let rect = settled(process, "auto import checkbox", |r, i| {
                gui_tree::rect(r, i, auto_checkbox(r, i)?, GAME)
            })?;
            let (x, y) = rect.center();
            keys.click(GAME, x, y)?;
            wait(process, "automatic importing toggled", move |r, i| {
                Ok(auto_import_enabled(r, i)? == Some(want))
            })?;
        }
    }
    if let Some(want) = budget {
        let limit = read(process, auto_state)?;
        // The slider's maximum is the factories available for trade (20 in a 1936 Germany start).
        let max = limit.available.unwrap_or(0).max(limit.budget.unwrap_or(0));
        if want < 0 || want > max {
            return Err(Error::new(
                "amount_above_available",
                format!("Budget {want} is outside 0..={max} (civilian factories available for trade)"),
            ));
        }
        let bg = settled(process, "auto import budget", |r, i| {
            let root = view_root(r, i)?;
            let icon = find(r, i, root, "auto_import_budget_bg", 0)?
                .ok_or_else(|| Error::new("gui_target_unavailable", "Budget box not found"))?;
            gui_tree::rect(r, i, icon, GAME)
        })?;
        // The track is click-transparent: the knob is dragged along it.
        let x0 = f64::from(bg.x) + AUTO_TRACK_DX + AUTO_KNOB / 2.0;
        let span = AUTO_TRACK_W - AUTO_KNOB;
        let y = f64::from(bg.y) + AUTO_TRACK_DY;
        let at = |v: i64| x0 + span * v as f64 / max.max(1) as f64;
        let mut now = limit.budget.unwrap_or(0);
        for _ in 0..6 {
            if now == want {
                break;
            }
            keys.drag(GAME, (at(now), y), (at(want), y))?;
            std::thread::sleep(std::time::Duration::from_millis(300));
            match read(process, auto_budget)? {
                Some(v) => now = v,
                None => break,
            }
        }
    }
    read(process, auto_state)
}
