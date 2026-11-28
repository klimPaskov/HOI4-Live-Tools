//! The agency's cipher work through the in-process module (engineer 133, family `decrypt`), each the command the
//! cryptology view posts, checked by the game's own "can do" before it is posted:
//! * `decrypt.start`: CStartStopDecryptionCommand(own tag, target tag, start 1 / stop 0): start decrypting a country (a
//!   valid target whose cipher is not broken yet) or stop decrypting it;
//! * `decrypt.activate`: CActivateActiveDecryptionBonuses(own tag, target tag): a broken cipher's bonuses for the game's
//!   own number of days (the decryption of that country stops).
//!
//! The module takes the target country itself from its tag; the verdict is read back from the agency's decryption
//! records (hoi4_intelligence `decryption`). No mouse path exists; the family is off under `HOI4_BACKEND=auto` until its
//! live session (S33-DECRYPT) passes. Served as hoi4_agency_upgrade's `decryption` argument (the agency domain's tool).
use super::inproc::{self, Channel, Routed};
use super::inproc_units::{request, routed};
use super::{diplomacy_state, intel_live, journal, mgmt, ui, Guard, InputAuthority};
use crate::{
    error::{Error, Result},
    platform::Process,
    xlayout::{fld, Class},
};
use serde_json::{json, Map, Value};

const COUNTRY: Class = Class::new("CCountry", 0x357e388);

pub(super) const START_OP: &str = "decrypt.start";
pub(super) const ACTIVATE_OP: &str = "decrypt.activate";

/// What a call asks of the agency.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Ask {
    Start,
    Stop,
    Activate,
}

impl Ask {
    pub(super) fn parse(s: &str) -> Result<Self> {
        match s {
            "start" => Ok(Self::Start),
            "stop" => Ok(Self::Stop),
            "activate" => Ok(Self::Activate),
            _ => Err(Error::new(
                "invalid_arguments",
                "decryption.action: start, stop or activate",
            )),
        }
    }
    fn key(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Activate => "activate",
        }
    }
}

/// The module request: no object (objs = 0), the target's tag index, and for start/stop 1 or 0.
pub(super) fn request_for(ask: Ask, tag_index: i32) -> Result<Value> {
    if tag_index <= 0 {
        return Err(Error::new(
            "invalid_arguments",
            "the target country has no tag index",
        ));
    }
    let t = i64::from(tag_index);
    match ask {
        Ask::Start => request(START_OP, &[0], &[], &[t, 1], None),
        Ask::Stop => request(START_OP, &[0], &[], &[t, 0], None),
        Ask::Activate => request(ACTIVATE_OP, &[0], &[], &[t], None),
    }
}

/// Whether the agency's records show the asked change for `target`: start = decrypting it, stop = not decrypting it,
/// activate = its bonus active. None when the target has no record yet (nothing to read).
pub(super) fn reached(ciphers: &[intel_live::Cipher], target: &str, ask: Ask) -> Option<bool> {
    let c = ciphers.iter().find(|c| c.target == target);
    match ask {
        Ask::Start => c.map(|c| c.decrypting),
        Ask::Stop => Some(c.is_none_or(|c| !c.decrypting)),
        Ask::Activate => c.map(|c| c.bonus_active),
    }
}

/// Refusals that need no post: what the records already answer.
pub(super) fn precheck(
    ciphers: &[intel_live::Cipher],
    own: &str,
    target: &str,
    ask: Ask,
) -> Result<()> {
    if target == own {
        return Err(Error::new(
            "invalid_target",
            "the agency does not decrypt its own country",
        ));
    }
    let c = ciphers.iter().find(|c| c.target == target);
    match ask {
        Ask::Start if c.is_some_and(|c| c.decrypting) => Err(Error::new(
            "nothing_to_change",
            format!("the agency already decrypts {target}"),
        )),
        Ask::Stop if !c.is_some_and(|c| c.decrypting) => Err(Error::new(
            "nothing_to_change",
            format!("the agency does not decrypt {target}"),
        )),
        Ask::Activate if c.is_some_and(|c| c.bonus_active) => Err(Error::new(
            "nothing_to_change",
            format!(
                "the bonuses of {target}'s cipher are active ({} days left)",
                c.map_or(0, |c| c.bonus_days)
            ),
        )),
        Ask::Activate if c.is_none() => Err(Error::new(
            "cipher_not_broken",
            format!("the agency never worked on {target}'s cipher: start decrypting it first"),
        )),
        _ => Ok(()),
    }
}

fn post(ch: &mut Channel, req: Value, what: &str) -> Routed {
    match ch.post_raw(req) {
        Ok(reply) => routed(reply, what),
        Err(_) => Routed::Gui,
    }
}

fn read(process: &dyn Process) -> Result<(String, Vec<intel_live::Cipher>)> {
    ui::read(process, |r, image| {
        let own = mgmt::own(r, image)?;
        let index = r.i32(own.country + fld(r.string_abi(), &COUNTRY, 8)?)?;
        let tag = super::states::tag_name(r, image, index)?;
        Ok((tag, intel_live::decryption(r, image, own.country)?))
    })
}

/// hoi4_agency_upgrade with `decryption: {target, action}`: start or stop decrypting a country, or activate a broken
/// cipher's bonuses, through the module's decrypt family. The game's own check decides; a refusal posts nothing.
pub(super) fn act(
    args: &Map<String, Value>,
    process: &dyn Process,
    guard: &mut Guard,
    authority: &InputAuthority,
    actions: &mut journal::GameJournal,
) -> Result<Value> {
    super::allowed(args, &["decryption", "request_id", "lease_token"])?;
    let d = args
        .get("decryption")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            Error::new(
                "invalid_arguments",
                "decryption: {target: TAG, action: start|stop|activate}",
            )
        })?;
    let target = d
        .get("target")
        .and_then(Value::as_str)
        .filter(|t| t.len() == 3 && t.bytes().all(|b| b.is_ascii_alphanumeric()))
        .ok_or_else(|| Error::new("invalid_arguments", "decryption.target: a country tag"))?
        .to_string();
    let ask = Ask::parse(d.get("action").and_then(Value::as_str).unwrap_or(""))?;
    let id = super::string(args, "request_id")?;
    let token = super::string(args, "lease_token")?;
    let (own, ciphers) = read(process)?;
    precheck(&ciphers, &own, &target, ask)?;
    let tag_index = ui::read(process, |r, i| diplomacy_state::tag_index_of(r, i, &target))?;
    let req = request_for(ask, tag_index)?;
    let observed = guard.observe(process)?;
    let epoch = observed.epoch;
    authority.authorize(token, &epoch)?;
    let mut route = inproc::route("decrypt");
    if route.is_none() {
        return Err(Error::new(
            "in_process_unavailable",
            "decryption runs only through the module's decrypt family (HOI4_BACKEND=inprocess with decrypt opted in, until S33-DECRYPT passes); nothing was sent",
        ));
    }
    let what = json!({"kind":"agency_decryption","target":target,"action":ask.key()});
    let answer = match ask {
        Ask::Start => {
            "start decrypting this country now (a valid target whose cipher is not broken)"
        }
        Ask::Stop => "stop decrypting this country (it is being decrypted)",
        Ask::Activate => {
            "activate this cipher's bonuses now (the cipher broken, its bonus not active)"
        }
    };
    let refused = std::cell::Cell::new(false);
    let mut reply = Value::Null;
    let mut settle = || -> Result<journal::Outcome> {
        if refused.get() {
            return Ok(journal::Outcome::Unchanged(
                json!({"refused_by_game_check": true}),
            ));
        }
        let (_, now) = read(process)?;
        let row = now.iter().find(|c| c.target == target).map(|c| {
            json!({"target":c.target,"decrypting":c.decrypting,"bonus_active":c.bonus_active,
                   "bonus_days_left":c.bonus_days,"verification":"memory"})
        });
        Ok(match reached(&now, &target, ask) {
            Some(true) => journal::Outcome::Reached(row.unwrap_or(Value::Null)),
            _ => journal::Outcome::Unknown,
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
            Routed::Refused(what) => {
                refused.set(true);
                Err(inproc::refused_error(&what))
            }
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

    fn c(t: &str, decrypting: bool, active: bool) -> intel_live::Cipher {
        intel_live::Cipher {
            target: t.into(),
            decrypting,
            bonus_active: active,
            bonus_days: if active { 20 } else { 0 },
            progress_raw: 0,
        }
    }

    #[test]
    fn requests_carry_the_target_tag_and_start_or_stop() {
        let r = request_for(Ask::Start, 17).unwrap();
        assert_eq!(r["cmd"], START_OP);
        assert_eq!(r["objs"], "0x0");
        assert_eq!(r["vals"], "17,1");
        assert_eq!(request_for(Ask::Stop, 17).unwrap()["vals"], "17,0");
        let a = request_for(Ask::Activate, 17).unwrap();
        assert_eq!(a["cmd"], ACTIVATE_OP);
        assert_eq!(a["vals"], "17");
        assert!(request_for(Ask::Start, 0).is_err());
        assert!(Ask::parse("crack").is_err());
    }

    #[test]
    fn the_records_answer_what_needs_no_post() {
        let rec = [
            c("FRA", true, false),
            c("ENG", false, true),
            c("POL", false, false),
        ];
        assert_eq!(
            precheck(&rec, "GER", "GER", Ask::Start).unwrap_err().code,
            "invalid_target"
        );
        assert_eq!(
            precheck(&rec, "GER", "FRA", Ask::Start).unwrap_err().code,
            "nothing_to_change"
        );
        assert!(precheck(&rec, "GER", "POL", Ask::Start).is_ok());
        assert!(precheck(&rec, "GER", "SOV", Ask::Start).is_ok());
        assert_eq!(
            precheck(&rec, "GER", "POL", Ask::Stop).unwrap_err().code,
            "nothing_to_change"
        );
        assert!(precheck(&rec, "GER", "FRA", Ask::Stop).is_ok());
        assert_eq!(
            precheck(&rec, "GER", "ENG", Ask::Activate)
                .unwrap_err()
                .code,
            "nothing_to_change"
        );
        assert_eq!(
            precheck(&rec, "GER", "SOV", Ask::Activate)
                .unwrap_err()
                .code,
            "cipher_not_broken"
        );
        // a record whose progress the game judges: the game's own check decides the activation
        assert!(precheck(&rec, "GER", "POL", Ask::Activate).is_ok());
    }

    #[test]
    fn the_verdict_is_read_from_the_records() {
        let rec = [c("FRA", true, false), c("ENG", false, true)];
        assert_eq!(reached(&rec, "FRA", Ask::Start), Some(true));
        assert_eq!(reached(&rec, "ENG", Ask::Start), Some(false));
        assert_eq!(reached(&rec, "POL", Ask::Start), None);
        assert_eq!(reached(&rec, "POL", Ask::Stop), Some(true));
        assert_eq!(reached(&rec, "FRA", Ask::Stop), Some(false));
        assert_eq!(reached(&rec, "ENG", Ask::Activate), Some(true));
    }
}
