//! Special projects through the in-process module (engineer 85, family `projects`): the commands the experimental
//! facilities tab posts, each after the module's own checks (a live object of the command's class through the game's
//! own resolver, owned by the player's country) and the game's own "can do":
//! * `projects.attach`: a scientist into a facility (the roster pick; the UI's flag 1);
//! * `projects.start`: a project in a facility (the facility window's Start);
//! * `projects.stop`: a running project's stop;
//! * `projects.unattach`: the facility's scientist out.
//!
//! hoi4_special_project_assign, hoi4_special_project_stop and hoi4_special_project_unassign keep their names, schemas
//! and Verified read-backs; the mouse path stays the fallback inside the same call. Recruiting stays on the mouse path
//! (its command's authorisation also reads the candidate pool, a check the module does not repeat). Off under
//! `HOI4_BACKEND=auto` until its live session (S19-PROJECTS) passes.
use super::inproc::{self, Channel, Routed};
use super::inproc_units::{request, routed};
use super::special_projects::{Objects, Plan, Snapshot};
use crate::error::{Error, Result};
use serde_json::{json, Value};

/// One module request and what it does (for the refusal message).
pub(super) type Post = (Value, &'static str);

/// The assign plan as module requests (the scientist first, as the panel does), or None when an object is not
/// uniquely found (the mouse path serves the call).
pub(super) fn assign_posts(objs: &Objects, plan: &Plan) -> Result<Option<Vec<Post>>> {
    let Some(facility) = objs.facility(plan.facility) else {
        return Ok(None);
    };
    let mut posts = Vec::new();
    if plan.assign_scientist {
        let Some(s) = plan
            .scientist
            .as_ref()
            .and_then(|s| objs.scientist(s.character))
        else {
            return Ok(None);
        };
        posts.push((
            request("projects.attach", &[facility], &[], &[], Some(s))?,
            "this scientist in this facility now",
        ));
    }
    if let Some(p) = plan.project.as_ref() {
        let Some(o) = objs.project(p.id) else {
            return Ok(None);
        };
        posts.push((
            request("projects.start", &[facility], &[], &[], Some(o))?,
            "this project in this facility now (its scientist, the facility's specialization)",
        ));
    }
    Ok((!posts.is_empty()).then_some(posts))
}

/// The stop of the facility's running project (None: no unique object).
pub(super) fn stop_post(snap: &Snapshot, objs: &Objects, facility: i32) -> Result<Option<Post>> {
    let Some(project) = snap.facility(facility).and_then(|f| f.project) else {
        return Ok(None);
    };
    let Some(o) = objs.project(project) else {
        return Ok(None);
    };
    Ok(Some((
        request("projects.stop", &[o], &[], &[], None)?,
        "stopping this project now",
    )))
}

/// The facility's scientist out (None: no unique object).
pub(super) fn unassign_post(
    snap: &Snapshot,
    objs: &Objects,
    facility: i32,
) -> Result<Option<Post>> {
    let Some(character) = snap.facility(facility).and_then(|f| f.scientist) else {
        return Ok(None);
    };
    let (Some(s), Some(f)) = (objs.scientist(character), objs.facility(facility)) else {
        return Ok(None);
    };
    Ok(Some((
        request("projects.unattach", &[s], &[], &[], Some(f))?,
        "this scientist out of this facility now",
    )))
}

/// Post in order. Ok(None): the module refused the FIRST post on its own checks (nothing posted; the mouse path
/// runs). A game refusal of the first post is the typed error; of a later one, a note in the replies.
pub(super) fn post_all(ch: &mut Channel, posts: &[Post]) -> Result<Option<Vec<Value>>> {
    let mut replies = Vec::new();
    for (k, (req, what)) in posts.iter().enumerate() {
        let reply = match ch.post_raw(req.clone()) {
            Ok(r) => routed(r, what),
            Err(_) => Routed::Gui,
        };
        match reply {
            Routed::Done(v) => replies.push(v),
            Routed::Refused(w) if k == 0 => return Err(inproc::refused_error(&w)),
            Routed::Refused(w) => replies.push(json!({"refused_by_game_check": w})),
            Routed::Gui if k == 0 => return Ok(None),
            Routed::Gui => {
                return Err(Error::new(
                    "action_uncertain",
                    "The module refused a later command of this call (nothing of it posted); the earlier ones were posted",
                ))
            }
        }
    }
    Ok(Some(replies))
}

#[cfg(test)]
mod tests {
    use super::super::special_projects::{Facility, Project, Scientist};
    use super::*;

    fn snap() -> Snapshot {
        Snapshot {
            facilities: vec![Facility {
                id: 7,
                handle: 0,
                building: "facility_land".into(),
                spec: Some(3),
                scientist: Some(55),
                project: Some(900),
                unread_reports: 0,
            }],
            ..Snapshot::default()
        }
    }
    fn objs() -> Objects {
        Objects {
            facilities: vec![(7, 0x700)],
            projects: vec![(900, 0x900), (901, 0x901)],
            scientists: vec![(55, 0x550), (56, 0x560)],
        }
    }
    fn scientist(c: u32) -> Scientist {
        Scientist {
            character: c,
            name: "A".into(),
            key: "a".into(),
            skills: [0; 4],
            traits: Vec::new(),
        }
    }
    fn project(id: u32) -> Project {
        Project {
            id,
            key: "p".into(),
            spec: Some(3),
            days: 100,
            running: 0,
            stopping_days: None,
            progress_percent: 0,
            iterations: 0,
            gain: (0, 0),
        }
    }

    #[test]
    fn assign_posts_attach_then_start_with_the_objects() {
        let plan = Plan {
            facility: 7,
            spec: 3,
            scientist: Some(scientist(56)),
            assign_scientist: true,
            project: Some(project(901)),
        };
        let p = assign_posts(&objs(), &plan).unwrap().unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].0["cmd"], "projects.attach");
        assert_eq!(p[0].0["objs"], "0x700");
        assert_eq!(p[0].0["obj2"], "0x560");
        assert_eq!(p[1].0["cmd"], "projects.start");
        assert_eq!(p[1].0["obj2"], "0x901");
        // a scientist the object read lacks: the mouse path
        let missing = Plan {
            scientist: Some(scientist(99)),
            ..plan
        };
        assert!(assign_posts(&objs(), &missing).unwrap().is_none());
    }

    #[test]
    fn stop_and_unassign_name_the_running_objects() {
        let s = stop_post(&snap(), &objs(), 7).unwrap().unwrap();
        assert_eq!(s.0["cmd"], "projects.stop");
        assert_eq!(s.0["objs"], "0x900");
        let u = unassign_post(&snap(), &objs(), 7).unwrap().unwrap();
        assert_eq!(u.0["cmd"], "projects.unattach");
        assert_eq!(u.0["objs"], "0x550");
        assert_eq!(u.0["obj2"], "0x700");
        assert!(stop_post(&snap(), &objs(), 8).unwrap().is_none());
        // two objects of one id are never guessed
        let mut twice = objs();
        twice.projects.push((900, 0x999));
        assert!(stop_post(&snap(), &twice, 7).unwrap().is_none());
    }
}
