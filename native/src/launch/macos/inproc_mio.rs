//! Military-industrial organisations through the in-process module (engineer 84, family `mio`), as the organisation
//! views post them: the trait tree's click (CUnlockIndustrialOrganisationTrait when a point is free and the game's
//! can_unlock accepts the trait, else CAddIndustrialOrgTraitToQueueCommand), the policy window's item behind its own
//! gate (CAttachPolicyToIndustrialOrgCommand; the attached policy's item detaches) and a production line's roster row
//! (CSetIndustrialManufacturerCommand). The state comes from the module's `mios` read, a pure read of the player's own
//! organisation list with the game's own checks (no opt-in needed): each organisation's key, points, funds, size,
//! attached policy, queue and its trait tree in the game's order with each trait's state. hoi4_mio_trait,
//! hoi4_mio_policy and hoi4_mio_line keep their names, schemas and read-backs; hoi4_mios adds the runtime state.
//! Off under `HOI4_BACKEND=auto` until S16-MIO passes.
use super::inproc::{self, Channel, Mode};
use super::inproc_units::request;
use crate::error::{Error, Result};
use serde_json::{json, Value};

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Trait {
    pub token: i64,
    pub key: String,
    /// u unlocked, q queued, a can be unlocked now (the game's can_unlock), l locked
    pub state: char,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Org {
    pub address: u64,
    pub key: String,
    pub points: i64,
    pub size: i64,
    pub policy: i64,
    pub policy_key: Option<String>,
    /// policy tokens the policy window's gate allows for this organisation now
    pub allowed: Vec<i64>,
    pub queue: Vec<i64>,
    pub traits: Vec<Trait>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Mios {
    pub orgs: Vec<Org>,
    /// the country's policies: (token, key, base cost)
    pub policies: Vec<(i64, String, i64)>,
    pub empty_token: i64,
}

fn hex(v: &Value) -> Option<u64> {
    u64::from_str_radix(v.as_str()?.strip_prefix("0x")?, 16).ok()
}

/// The module's `mios` reply as typed state; anything not as the module writes it is an error (nothing guessed).
pub(super) fn parse(reply: &Value) -> Result<Mios> {
    let bad = || {
        Error::new(
            "inproc_channel",
            "The organisations read is not in the module's shape",
        )
    };
    if reply["ok"].as_bool() != Some(true) {
        return Err(Error::new(
            "capability_unavailable",
            format!(
                "The module's organisations read failed: {}",
                reply["reason"]
                    .as_str()
                    .or(reply["error"].as_str())
                    .unwrap_or("?")
            ),
        ));
    }
    let mut policies = Vec::new();
    for p in reply["policies"].as_array().ok_or_else(bad)? {
        policies.push((
            p["token"].as_i64().ok_or_else(bad)?,
            p["key"].as_str().unwrap_or_default().to_string(),
            p["cost"].as_i64().unwrap_or_default(),
        ));
    }
    let mut orgs = Vec::new();
    for o in reply["organisations"].as_array().ok_or_else(bad)? {
        let ints = |k: &str| -> Result<Vec<i64>> {
            o[k].as_array()
                .ok_or_else(bad)?
                .iter()
                .map(|x| x.as_i64().ok_or_else(bad))
                .collect()
        };
        let mut traits = Vec::new();
        for t in o["traits"].as_array().ok_or_else(bad)? {
            let state = t[2]
                .as_str()
                .and_then(|s| s.chars().next())
                .ok_or_else(bad)?;
            if !"uqal".contains(state) {
                return Err(bad());
            }
            traits.push(Trait {
                token: t[0].as_i64().ok_or_else(bad)?,
                key: t[1].as_str().unwrap_or_default().to_string(),
                state,
            });
        }
        orgs.push(Org {
            address: hex(&o["org"]).ok_or_else(bad)?,
            key: o["key"].as_str().ok_or_else(bad)?.to_string(),
            points: o["trait_points"].as_i64().ok_or_else(bad)?,
            size: o["size"].as_i64().unwrap_or_default(),
            policy: o["policy"].as_i64().ok_or_else(bad)?,
            policy_key: o["policy_key"].as_str().map(str::to_string),
            allowed: ints("policies_allowed")?,
            queue: ints("queue")?,
            traits,
        });
    }
    Ok(Mios {
        orgs,
        policies,
        empty_token: reply["empty_token"].as_i64().ok_or_else(bad)?,
    })
}

pub(super) fn read(ch: &mut Channel) -> Result<(Mios, Value)> {
    let reply = ch.call_raw(json!({"op":"mios"}))?;
    Ok((parse(&reply)?, reply))
}

/// The state read on its own (hoi4_mios): any channel, in any mode but `gui` (a pure read, no family needed).
pub(super) fn read_only() -> Option<Value> {
    if inproc::mode() == Mode::Gui {
        return None;
    }
    let mut ch = Channel::connect()?;
    let (_, reply) = read(&mut ch).ok()?;
    Some(reply)
}

impl Mios {
    pub(super) fn org(&self, key: &str) -> Option<&Org> {
        self.orgs.iter().find(|o| o.key == key)
    }
    pub(super) fn policy(&self, key: &str) -> Option<i64> {
        self.policies.iter().find(|p| p.1 == key).map(|p| p.0)
    }
}

/// What the tree's click does for one trait: unlock now (a point is free and the game's can_unlock accepts it) or
/// queue it (the game unlocks queued traits as points come).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Step {
    Unlock(i64),
    Queue(i64),
}

/// The trait at `index` of the tree (the game's order; list:true in process returns the same order). Typed refusals
/// as the mouse path names them: trait_unknown, trait_already_queued, trait_unavailable (unlocked already).
pub(super) fn step(
    org: &Org,
    index: usize,
    points_left: i64,
) -> std::result::Result<Step, (&'static str, String)> {
    let Some(t) = org.traits.get(index) else {
        return Err((
            "trait_unknown",
            format!(
                "{} has {} traits (0..{})",
                org.key,
                org.traits.len(),
                org.traits.len().saturating_sub(1)
            ),
        ));
    };
    match t.state {
        'u' => Err((
            "trait_unavailable",
            format!("{} is unlocked already", t.key),
        )),
        'q' => Err((
            "trait_already_queued",
            format!("{} is queued already", t.key),
        )),
        'a' if points_left > 0 => Ok(Step::Unlock(t.token)),
        _ => Ok(Step::Queue(t.token)),
    }
}

/// Trait keys (the scripts' `token`, as the module's read names each trait) to their indices in the tree, in the
/// order asked; an index given as a number passes through. A key the organisation does not hold is refused
/// trait_unknown with the keys it does hold, before any input.
pub(super) fn indices_of(
    org: &Org,
    asked: &[Value],
) -> std::result::Result<Vec<usize>, (&'static str, String)> {
    let mut out = Vec::new();
    for a in asked {
        let i = match a {
            Value::Number(n) => n.as_u64().map(|n| n as usize),
            Value::String(s) => match s.trim().parse::<usize>() {
                Ok(n) => Some(n),
                Err(_) => {
                    let key = s.trim();
                    let Some(i) = org.traits.iter().position(|t| t.key == key) else {
                        let held: Vec<&str> = org.traits.iter().map(|t| t.key.as_str()).collect();
                        return Err((
                            "trait_unknown",
                            format!(
                                "{} has no trait {key:?}; its traits: {}",
                                org.key,
                                held.join(", ")
                            ),
                        ));
                    };
                    Some(i)
                }
            },
            _ => None,
        };
        let i = i.ok_or_else(|| {
            (
                "invalid_arguments",
                "a trait is a key (string) or a 0-based index".to_string(),
            )
        })?;
        if !out.contains(&i) {
            out.push(i);
        }
    }
    Ok(out)
}

/// `auto`: the first traits the game would unlock now (state a), in tree order, until `want` traits wait in the queue
/// or are unlocked by this call; never one already held.
pub(super) fn auto_picks(org: &Org, want: usize, skip: &[usize]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut waiting = org.queue.len();
    for (i, t) in org.traits.iter().enumerate() {
        if waiting >= want {
            break;
        }
        if t.state == 'a' && !skip.contains(&i) {
            out.push(i);
            waiting += 1;
        }
    }
    out
}

pub(super) fn trait_request(org: &Org, step: Step) -> Result<Value> {
    match step {
        Step::Unlock(t) => request("mio.unlock_trait", &[org.address], &[], &[t], None),
        Step::Queue(t) => request("mio.queue_trait", &[org.address], &[], &[t], None),
    }
}

/// The policy item's click: the item of the attached policy detaches it (the module sends the empty token then).
pub(super) fn policy_request(org: &Org, token: i64) -> Result<Value> {
    request("mio.policy", &[org.address], &[], &[token], None)
}

/// A production line's roster row: the organisation, or none (produce without one).
pub(super) fn line_request(line: u64, org: Option<u64>) -> Result<Value> {
    request("mio.line", &[line], &[], &[], org)
}

/// Read-back: the trait is queued or unlocked now.
pub(super) fn trait_held(m: &Mios, org_key: &str, token: i64) -> Option<bool> {
    let t = m.org(org_key)?.traits.iter().find(|t| t.token == token)?;
    Some(t.state == 'q' || t.state == 'u')
}

/// Read-back: the organisation's attached policy is the one asked (attach) or none (detach of `token`).
pub(super) fn policy_held(m: &Mios, org_key: &str, token: i64, detach: bool) -> Option<bool> {
    let o = m.org(org_key)?;
    Some(if detach {
        o.policy == m.empty_token
    } else {
        o.policy == token
    })
}

/// The in-process listing of list:true: the tree in the game's order with each trait's state.
pub(super) fn listing(org: &Org) -> Value {
    json!({
        "listing_only": true,
        "backend": "inprocess",
        "organization": org.key,
        "trait_points": org.points,
        "queue": org.queue,
        "entries": org.traits.iter().enumerate().map(|(i, t)| json!({
            "index": i, "trait": t.key, "token": t.token,
            "unlocked": t.state == 'u', "queued": t.state == 'q', "unlockable_now": t.state == 'a',
        })).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply() -> Value {
        json!({"ok":true,"policies":[{"token":140,"key":"mio_policy_cost_reduction","cost":150},
                {"token":141,"key":"mio_policy_quality","cost":100}],
            "empty_token":19479,
            "organisations":[{"org":"0x6000f60","handle":"0x7700000001","key":"ITA_ansaldo_organization","funds_raw":2500000,
                "size":3,"trait_points":1,"policy":141,"policy_key":"mio_policy_quality","policies_allowed":[140],
                "queue":[112],"unlocked":1,"traits":[[111,"a_trait","u"],[112,"b_trait","q"],[113,"c_trait","a"],[114,"d_trait","l"],[115,"e_trait","a"]]}],
            "organisations_read":1,"truncated":false})
    }

    #[test]
    fn mio_stays_off_in_auto_until_its_live_pass() {
        use super::super::inproc::{family_usable, Mode};
        let hello =
            json!({"post":{"enabled":true},"families":{"mio":{"on":true,"live_passed":false}}});
        assert!(!family_usable(&hello, "mio", Mode::Auto));
        assert!(family_usable(&hello, "mio", Mode::InProcess));
        assert!(!family_usable(&hello, "mio", Mode::Gui));
        let off =
            json!({"post":{"enabled":true},"families":{"mio":{"on":false,"live_passed":false}}});
        assert!(!family_usable(&off, "mio", Mode::InProcess));
    }

    #[test]
    fn the_read_parses_into_typed_state_and_refuses_other_shapes() {
        let m = parse(&reply()).unwrap();
        let o = m.org("ITA_ansaldo_organization").unwrap();
        assert_eq!(o.address, 0x6000f60);
        assert_eq!((o.points, o.policy, o.allowed.clone()), (1, 141, vec![140]));
        assert_eq!(
            o.traits[2],
            Trait {
                token: 113,
                key: "c_trait".into(),
                state: 'a'
            }
        );
        assert_eq!(m.policy("mio_policy_cost_reduction"), Some(140));
        let mut bad = reply();
        bad["organisations"][0]["traits"][0][2] = json!("x");
        assert!(parse(&bad).is_err());
        assert!(
            parse(&json!({"ok":false,"reason":"organisation facts unresolved on this build"}))
                .is_err()
        );
    }

    #[test]
    fn the_click_unlocks_with_a_free_point_and_queues_otherwise() {
        let m = parse(&reply()).unwrap();
        let o = &m.orgs[0];
        assert_eq!(step(o, 2, 1), Ok(Step::Unlock(113)));
        assert_eq!(step(o, 2, 0), Ok(Step::Queue(113)));
        assert_eq!(step(o, 3, 1), Ok(Step::Queue(114)));
        assert_eq!(step(o, 0, 1).unwrap_err().0, "trait_unavailable");
        assert_eq!(step(o, 1, 1).unwrap_err().0, "trait_already_queued");
        assert_eq!(step(o, 9, 1).unwrap_err().0, "trait_unknown");
        assert_eq!(auto_picks(o, 3, &[]), vec![2, 4]);
        assert_eq!(auto_picks(o, 2, &[2]), vec![4]);
        assert!(auto_picks(o, 1, &[]).is_empty());
    }

    #[test]
    fn requests_carry_the_organisation_and_the_token() {
        let m = parse(&reply()).unwrap();
        let o = &m.orgs[0];
        let r = trait_request(o, Step::Unlock(113)).unwrap();
        assert_eq!(
            (r["cmd"].as_str(), r["objs"].as_str(), r["vals"].as_str()),
            (Some("mio.unlock_trait"), Some("0x6000f60"), Some("113"))
        );
        assert_eq!(
            trait_request(o, Step::Queue(114)).unwrap()["cmd"],
            "mio.queue_trait"
        );
        assert_eq!(policy_request(o, 140).unwrap()["vals"], "140");
        let r = line_request(0x7000_0010, Some(0x6000f60)).unwrap();
        assert_eq!(
            (r["objs"].as_str(), r["obj2"].as_str()),
            (Some("0x70000010"), Some("0x6000f60"))
        );
        assert!(line_request(0x7000_0010, None)
            .unwrap()
            .get("obj2")
            .is_none());
    }

    #[test]
    fn read_backs_follow_the_organisation_state() {
        let m = parse(&reply()).unwrap();
        let k = "ITA_ansaldo_organization";
        assert_eq!(trait_held(&m, k, 112), Some(true));
        assert_eq!(trait_held(&m, k, 113), Some(false));
        assert_eq!(policy_held(&m, k, 141, false), Some(true));
        assert_eq!(policy_held(&m, k, 141, true), Some(false));
        assert_eq!(trait_held(&m, "nope", 1), None);
        assert_eq!(listing(&m.orgs[0])["entries"][2]["unlockable_now"], true);
    }

    #[test]
    fn trait_keys_resolve_to_the_tree_order_and_unknown_keys_are_refused() {
        let m = parse(&reply()).unwrap();
        let o = &m.orgs[0];
        assert_eq!(
            indices_of(
                o,
                &[json!("e_trait"), json!("c_trait"), json!(1), json!("3")]
            ),
            Ok(vec![4, 2, 1, 3])
        );
        // the same trait twice is asked once
        assert_eq!(indices_of(o, &[json!("c_trait"), json!(2)]), Ok(vec![2]));
        let (code, msg) = indices_of(o, &[json!("z_trait")]).unwrap_err();
        assert_eq!(code, "trait_unknown");
        assert!(msg.contains("a_trait") && msg.contains("e_trait"));
        assert_eq!(
            indices_of(o, &[json!(true)]).unwrap_err().0,
            "invalid_arguments"
        );
    }
}
