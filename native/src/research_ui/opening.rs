//! Strict own shown Research root readiness for the reviewed Linux build.
//! A known pending snapshot is identity evidence only, never input authority.
//! The ordinary OpenPanel journal controls the original deadline and any wait.
use super::{exact, UiContext, Witnesses};
use crate::{
    error::{Error, Result},
    memory::{Reader, StringAbi},
    platform::Process,
    research::ResearchContext,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Binding {
    pub anchors: [u64; 6],
    pub slots: Vec<u64>,
    pub root: u64,
}
pub(crate) struct Observation {
    pub context: UiContext,
    pub binding: Binding,
    pub shown: bool,
    pub transitions: [i32; 2],
}
impl Observation {
    pub(crate) fn require_settled_shown(&self) -> Result<()> {
        if !self.shown || self.transitions != [0, 0] {
            return Err(Error::new("research_open_transition", format!(
                "Own Research opening readiness pending; root=research_panel; transitions={:?}; literal_shown={}",
                self.transitions, self.shown)));
        }
        Ok(())
    }
}
fn refuse(reason: &str) -> Error {
    Error::new("research_precondition", reason)
}
fn pointer(p: u64) -> Result<()> {
    if !(0x10000..=0x7fff_ffff_0000).contains(&p) || p % 8 != 0 {
        return Err(refuse("Bounded own Research opening identity required"));
    }
    Ok(())
}

pub(crate) fn observe(process: &dyn Process, expected: &UiContext) -> Result<Observation> {
    if process.string_abi() != StringAbi::LibStdCxx {
        return Err(Error::new(
            "capability_unavailable",
            "Research opening readiness requires reviewed Linux build",
        ));
    }
    process.begin_snapshot()?;
    let witnesses = Witnesses::new(process);
    let r = Reader::new(&witnesses);
    let base = process.image_base();
    witnesses.section("open_settlement_own");
    let (_, own) = crate::research::observe_from_reader(&r, base, 0, 1, true)?;
    let own = own.ok_or_else(|| refuse("Own opening campaign context unavailable"))?;
    witnesses.section("open_settlement_root");
    let observation = from_reader(&r, base, own, expected)?;
    witnesses.finish()?;
    process.check()?;
    Ok(observation)
}

fn from_reader(
    r: &Reader<'_>,
    base: u64,
    own: ResearchContext,
    expected: &UiContext,
) -> Result<Observation> {
    let before = &expected.campaign;
    let current = &own.campaign;
    if current.anchors != before.anchors
        || current.playthrough_id != before.playthrough_id
        || current.player != before.player
        || current.difficulty != before.difficulty
        || current.historical_raw != before.historical_raw
        || current.date_raw < before.date_raw
        || own.own != expected.anchors[..2]
        || own.slots != expected.slots
    {
        return Err(Error::new(
            "ownership_changed",
            "Original own Research opening context changed",
        ));
    }
    let [country, status] = own.own;
    let [_, idler, _] = current.anchors;
    let registry = r.u64(idler + 0x4f8)?;
    let manager = r.u64(idler + 0x6b8)?;
    if registry != expected.anchors[2] || manager != expected.anchors[3] {
        return Err(Error::new(
            "ownership_changed",
            "Original opening registry/manager changed",
        ));
    }
    pointer(registry)?;
    pointer(manager)?;
    exact(r, registry, base + 0x42736f8)?;
    if r.u64(manager + 0x180)? != idler || r.u64(base + 0x45778e0)? != registry {
        return Err(refuse("Opening manager/input registry owner changed"));
    }
    let view = r.u64(manager + 0xf0)?;
    let tree = r.u64(manager + 0xf8)?;
    if [view, tree] != expected.anchors[4..] {
        return Err(Error::new(
            "ownership_changed",
            "Original own opening views changed",
        ));
    }
    pointer(view)?;
    pointer(tree)?;
    exact(r, view, base + 0x424dc38)?;
    exact(r, tree, base + 0x424de40)?;
    if r.u64(view + 0x2f0)? != idler
        || r.u64(tree + 0x2f0)? != idler
        || r.i32(tree + 0xe24)? != 6
        || r.i32(manager + 0x3d8)? != 5
    {
        return Err(refuse("Own Research opening view owner/mode changed"));
    }
    let displayed = r.i32(tree + 0xe20)?;
    if displayed != 0 && displayed != r.i32(status + 0x138)? {
        return Err(refuse("Foreign displayed Research opening country"));
    }
    let root = r.u64(view + 0x2f8)?;
    pointer(root)?;
    exact(r, root, base + 0x4347d58)?;
    if r.u64(root + 0x98)? != registry {
        return Err(refuse("Own Research opening root registry changed"));
    }
    let shown = r.byte(root + 0xa5)? & 8 != 0;
    let transitions = [r.i32(root + 0xeb0)?, r.i32(root + 0xef8)?];
    if !transitions.iter().all(|state| matches!(state, 0..=2)) {
        return Err(refuse(&format!("Unknown Research opening root animation; root=research_panel; transitions={transitions:?}; literal_shown={shown}")));
    }
    let anchors = [country, status, registry, manager, view, tree];
    let binding = Binding {
        anchors,
        slots: own.slots.clone(),
        root,
    };
    let mut identities = std::collections::BTreeSet::new();
    for p in anchors.iter().chain(own.slots.iter()).chain([&root]) {
        pointer(*p)?;
        if !identities.insert(*p) {
            return Err(refuse(
                "Distinct original Research opening identities required",
            ));
        }
    }
    if !(1..=32).contains(&own.slots.len()) {
        return Err(refuse("Complete own opening slot identities required"));
    }
    Ok(Observation {
        context: UiContext {
            campaign: own.campaign,
            anchors,
            slots: own.slots,
        },
        binding,
        shown,
        transitions,
    })
}

#[cfg(test)]
mod tests;
