//! Doctrines through the in-process module (engineer 85, family `doctrine`): the officer corps view's doctrine
//! selection (1.20 0x101051db0) posts one of two commands, each after the module's own checks (a live template of the
//! exact class, the player's own tag) and the game's own "can do":
//! * `doctrine.grand`: UnlockGrandDoctrine(own tag, the grand doctrine template); the check is the doctrine status's
//!   can-unlock (the folder, the experience and its cost);
//! * `doctrine.sub`: UnlockSubDoctrine(own tag, the track's place, the subdoctrine template, the folder template); the
//!   check also takes the track at that place of the folder's grand doctrine and the subdoctrine's own track list.
//!
//! The templates come from the game's own databases (`TGameItemDatabase<CGrandDoctrineDatabase>` and
//! `<CSubDoctrineDatabase>`: the token map the commands' own readers look the templates up in), matched by the name key
//! the doctrine read shows; the folder template from the own doctrine status. hoi4_doctrine_adopt keeps its name,
//! schema and Verified read-back; the mouse path stays the fallback in the same call. Off under `HOI4_BACKEND=auto`
//! until its live session (S18-DOCTRINE) passes.
use super::doctrines;
use super::inproc::{Channel, Routed};
use super::inproc_units::{request, routed};
use super::mgmt;
use crate::{
    error::{Error, Result},
    memory::Reader,
    xlayout::img,
};

/// `TGameItemDatabase<NDoctrines::CGrandDoctrineDatabase>::_pInstance` (macOS 1.19.4.1 key; 1.20 by its symbol).
const GRAND_DB: u64 = 0x3703a00;
/// `TGameItemDatabase<NDoctrines::CSubDoctrineDatabase>::_pInstance` (macOS 1.19.4.1 key; 1.20 by its symbol).
const SUB_DB: u64 = 0x3703a90;
/// The database's token map, as the commands' readers walk it (1.20 0x10103a440, 1.19.4.1 0x1010370c8): buckets
/// `+0x30`, mask `+0x3c`, the longest probe `+0x40` (u8); a bucket is 0x18 bytes: `+0x4` its probe distance (0 =
/// empty), `+0x8` the token, `+0x10` the template.
const MAP_BUCKETS: u64 = 0x30;
const MAP_MASK: u64 = 0x3c;
const MAP_PROBE: u64 = 0x40;
const BUCKET: usize = 0x18;
const BUCKET_USED: usize = 0x4;
const BUCKET_VALUE: usize = 0x10;
const MAP_LIMIT: usize = 1 << 14;
pub(super) const GRAND_TYPE: &str = "N10NDoctrines22CGrandDoctrineTemplateE";
pub(super) const SUB_TYPE: &str = "N10NDoctrines20CSubDoctrineTemplateE";
const FOLDER_TYPE: &str = "N10NDoctrines15CFolderTemplateE";

/// The templates of one doctrine database (pointer, name key), each of the exact class `mangled`.
fn database(r: &Reader<'_>, image: u64, key: u64, mangled: &str) -> Result<Vec<(u64, String)>> {
    let abi = r.string_abi();
    let db = mgmt::pointer(r, image + img(abi, key)?)?;
    let head = r.read(db + MAP_BUCKETS, (MAP_PROBE - MAP_BUCKETS) as usize + 1)?;
    let buckets = mgmt::u64_at(&head, 0);
    let mask = mgmt::i32_at(&head, (MAP_MASK - MAP_BUCKETS) as usize);
    let probe = head[(MAP_PROBE - MAP_BUCKETS) as usize] as usize;
    if mask < 0 || !mgmt::is_pointer(buckets) {
        return Err(Error::new(
            "invalid_array",
            "Doctrine database map unreadable",
        ));
    }
    let count = mask as usize + 1 + probe;
    if count > MAP_LIMIT {
        return Err(Error::new("invalid_array", "Implausible doctrine database"));
    }
    let bytes = r.read(buckets, count * BUCKET)?;
    let mut out = Vec::new();
    for b in 0..count {
        let at = b * BUCKET;
        if bytes[at + BUCKET_USED] == 0 {
            continue;
        }
        let t = mgmt::u64_at(&bytes, at + BUCKET_VALUE);
        if !mgmt::is_pointer(t) {
            continue;
        }
        out.push((t, doctrines::template(r, t, mangled)?.name_key));
    }
    Ok(out)
}

/// The template whose name key is `name_key` (exactly one).
fn pick(templates: &[(u64, String)], name_key: &str) -> Option<u64> {
    let hits: Vec<u64> = templates
        .iter()
        .filter(|(_, k)| k == name_key)
        .map(|(t, _)| *t)
        .collect();
    (hits.len() == 1).then(|| hits[0])
}

/// What the module posts for one adoption.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Post {
    Grand {
        template: u64,
    },
    Sub {
        template: u64,
        folder: u64,
        track: usize,
    },
}

/// The objects of a grand doctrine (by name key) or a subdoctrine (by name key, the folder template of the doctrine
/// read, the track's place in the folder's grand doctrine). None: no single template of that key (the mouse path).
pub(super) fn objects(
    r: &Reader<'_>,
    image: u64,
    name_key: &str,
    sub: Option<(u64, usize)>,
) -> Result<Option<Post>> {
    Ok(match sub {
        None => pick(&database(r, image, GRAND_DB, GRAND_TYPE)?, name_key)
            .map(|template| Post::Grand { template }),
        Some((folder, track)) => {
            mgmt::expect_type(r, folder, FOLDER_TYPE)?;
            pick(&database(r, image, SUB_DB, SUB_TYPE)?, name_key).map(|template| Post::Sub {
                template,
                folder,
                track,
            })
        }
    })
}

/// The module request of `post`.
pub(super) fn post_request(post: Post) -> Result<serde_json::Value> {
    match post {
        Post::Grand { template } => request("doctrine.grand", &[template], &[], &[], None),
        Post::Sub {
            template,
            folder,
            track,
        } => {
            if track > 31 {
                return Err(Error::new("invalid_target", "Track place out of range"));
            }
            request(
                "doctrine.sub",
                &[template],
                &[],
                &[track as i64],
                Some(folder),
            )
        }
    }
}

/// Post the adoption: Done, the game's refusal (its own "can do"), or Gui (the module refused on its own checks).
pub(super) fn adopt(ch: &mut Channel, post: Post) -> Routed {
    match post_request(post).and_then(|r| ch.post_raw(r)) {
        Ok(reply) => routed(
            reply,
            "this doctrine now (the experience, its cost, the folder's grand doctrine, the track)",
        ),
        Err(_) => Routed::Gui,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_template_is_picked_only_when_its_key_is_unique() {
        let t = vec![
            (0x10, "mobile_warfare".to_string()),
            (0x20, "superior_firepower".to_string()),
            (0x30, "superior_firepower".to_string()),
        ];
        assert_eq!(pick(&t, "mobile_warfare"), Some(0x10));
        assert_eq!(pick(&t, "superior_firepower"), None);
        assert_eq!(pick(&t, "grand_battleplan"), None);
    }

    #[test]
    fn requests_name_the_module_ops() {
        let g = post_request(Post::Grand { template: 0x10 }).unwrap();
        assert_eq!(g["cmd"], "doctrine.grand");
        assert_eq!(g["objs"], "0x10");
        assert!(g.get("obj2").is_none() || g["obj2"].is_null());
        let s = post_request(Post::Sub {
            template: 0x20,
            folder: 0x30,
            track: 2,
        })
        .unwrap();
        assert_eq!(s["cmd"], "doctrine.sub");
        assert_eq!(s["objs"], "0x20");
        assert_eq!(s["obj2"], "0x30");
        assert_eq!(s["vals"], "2");
        assert!(post_request(Post::Sub {
            template: 0x20,
            folder: 0x30,
            track: 40,
        })
        .is_err());
    }
}
