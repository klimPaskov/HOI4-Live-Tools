//! Private identity projection of the existing witnessed controls observation.
//! No process read is added, no pointer is serialized and no input is enabled.
use crate::{
    construction_stage::Stage,
    error::{Error, Result},
    memory::ReadMemory,
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};

fn missing(message: &str) -> Error {
    Error::new("construction_scope_unavailable", message)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Scope {
    common: Common,
    target: Target,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Common {
    base: u64,
    campaign_anchors: [u64; 3],
    playthrough_id: String,
    player: String,
    country: u64,
    production: u64,
    registry: u64,
    manager: u64,
    panel: u64,
    panel_root: u64,
    blockers: Vec<OwnerRoot>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Target {
    Opener(Opener),
    Choice(Choice),
    Find(crate::construction::find_view::FindIdentity),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Callback {
    pub(super) glue: u64,
    pub(super) capture: u64,
    pub(super) listener: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Optional {
    Absent,
    OwnPosition(u64),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Control {
    pub(super) owner: u64,
    pub(super) window: u64,
    pub(super) button: u64,
    pub(super) callback: Callback,
    pub(super) optional: Optional,
    geometry: Geometry,
}
impl Control {
    pub(super) fn new(
        owner: u64,
        window: u64,
        button: u64,
        callback: Callback,
        optional: Optional,
    ) -> Self {
        Self {
            owner,
            window,
            button,
            callback,
            optional,
            geometry: Geometry::NotRequested,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Geometry {
    NotRequested,
    Observed {
        renderer: u64,
        primary: u64,
        resource: u64,
        graphics: u64,
        positions: [u64; 2],
        parents: Vec<Node>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Node {
    object: u64,
    primary: u64,
    parent: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct OwnerRoot {
    owner: u64,
    root: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Opener {
    pub(super) topbar: u64,
    pub(super) named_path: Vec<u64>,
    pub(super) control: Control,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Choice {
    pub(super) key: String,
    pub(super) database: u64,
    pub(super) ordered_catalog: Vec<u64>,
    /// Every supported current choice after full canonical/ordinal validation.
    pub(super) joins: Vec<(u64, u64, usize)>,
    pub(super) requested_item: u64,
    pub(super) requested_template: u64,
    pub(super) control: Control,
}

/// Only declared pointer/vptr sites are retained. Scalar words, flags, modes,
/// coordinates, dimensions, visibility and the general read footprint are not.
/// Registration never reads memory; the existing producer decides whether a
/// declared site is safe to read, including unknown/foreign refusal ordering.
#[derive(Clone, Copy)]
enum Follow {
    None,
    Primary,
    Parent(usize),
    Owner,
    Graphics,
}
pub(super) struct Recorder<'a> {
    inner: &'a dyn ReadMemory,
    sites: RefCell<BTreeMap<u64, Follow>>,
    words: RefCell<BTreeMap<u64, u64>>,
}
impl<'a> Recorder<'a> {
    pub(super) fn new(inner: &'a dyn ReadMemory) -> Self {
        Self {
            inner,
            sites: RefCell::new(BTreeMap::new()),
            words: RefCell::new(BTreeMap::new()),
        }
    }
    fn watch(&self, address: u64, follow: Follow) {
        // Registration adds no reads and must not interpret an unvalidated raw
        // pointer. Preserve its bytes for the producer's own bounded refusal.
        if (0x10000..=0x7fff_ffff_0000).contains(&address) {
            self.sites.borrow_mut().insert(address, follow);
        }
    }
    fn watch_offset(&self, object: u64, offset: u64, follow: Follow) {
        if let Some(address) = object.checked_add(offset) {
            self.watch(address, follow);
        }
    }
    fn node(&self, object: u64, depth: usize) {
        if object != 0 && depth <= 16 {
            self.watch(object, Follow::None);
            self.watch_offset(object, 0x108, Follow::Parent(depth));
        }
    }
    pub(super) fn control(&self, base: u64, window: u64, button: u64) {
        self.watch_offset(button, 0xe8, Follow::None);
        self.watch_offset(window, 0xe70, Follow::Primary);
        self.watch_offset(window, 0xe78, Follow::Primary);
        self.watch_offset(base, 0x4577890, Follow::Graphics);
    }
    pub(super) fn renderer(&self, render: u64, primary: u64, base: u64) {
        // Already read and exact supported by the construction control producer.
        self.words.borrow_mut().insert(render, primary);
        self.node(render, 0);
        match primary.checked_sub(base) {
            Some(0x4356e08 | 0x43593d8) => self.watch_offset(render, 0x168, Follow::Primary),
            Some(0x434b370) => {
                self.watch_offset(render, 0x168, Follow::Primary);
                self.watch_offset(render, 0x1b0, Follow::Primary);
            }
            _ => {}
        }
    }
    pub(super) fn blockers(&self, manager: u64) {
        for offset in [0x130, 0x128, 0xf0, 0xf8] {
            self.watch_offset(manager, offset, Follow::Owner);
        }
    }
    pub(super) fn owner(&self, object: u64) {
        if object != 0 {
            self.watch(object, Follow::None);
            self.watch_offset(object, 0x2f0, Follow::None);
            self.watch_offset(object, 0x2f8, Follow::Primary);
        }
    }
    pub(super) fn finish(&self) -> Words {
        Words(self.words.borrow().clone())
    }
}
impl ReadMemory for Recorder<'_> {
    fn read(&self, address: u64, size: usize) -> Result<Vec<u8>> {
        let bytes = self.inner.read(address, size)?;
        let follow = self.sites.borrow().get(&address).copied();
        if let Some(follow) = follow.filter(|_| size == 8 && bytes.len() == 8) {
            let word = u64::from_le_bytes(bytes[..].try_into().unwrap());
            self.words.borrow_mut().insert(address, word);
            match follow {
                Follow::None => {}
                Follow::Primary => {
                    if word != 0 {
                        self.watch(word, Follow::None);
                    }
                }
                Follow::Parent(depth) => self.node(word, depth + 1),
                Follow::Owner => self.owner(word),
                Follow::Graphics => {
                    if word != 0 {
                        self.watch(word, Follow::None);
                        for offset in [0x1f0, 0x350] {
                            if let Some(root) = word.checked_add(offset) {
                                self.node(root, 16);
                            }
                        }
                    }
                }
            }
        }
        Ok(bytes)
    }
}
pub(super) struct Words(BTreeMap<u64, u64>);
impl Words {
    fn word(&self, address: u64) -> Result<u64> {
        self.0
            .get(&address)
            .copied()
            .ok_or_else(|| missing("Requested identity field was not witnessed"))
    }
    fn nonnull(&self, address: u64) -> Result<u64> {
        let value = self.word(address)?;
        if value == 0 {
            return Err(missing("Requested identity is unmaterialized"));
        }
        Ok(value)
    }
    fn blockers(&self, base: u64, manager: u64, idler: u64) -> Result<Vec<OwnerRoot>> {
        let mut result = Vec::new();
        for (offset, primary) in [
            (0x130, 0x4249c70),
            (0x128, 0x42546e8),
            (0xf0, 0x424dc38),
            (0xf8, 0x424de40),
        ] {
            let owner = self.nonnull(manager + offset)?;
            if self.word(owner)? != base + primary || self.word(owner + 0x2f0)? != idler {
                return Err(missing("Complete exact owned blocker roots required"));
            }
            let root = self.nonnull(owner + 0x2f8)?;
            if self.word(root)? != base + 0x4347d58 {
                return Err(missing("Exact materialized blocker container required"));
            }
            result.push(OwnerRoot { owner, root });
        }
        Ok(result)
    }
    fn geometry(&self, base: u64, control: &Control) -> Result<Geometry> {
        let renderer = self.nonnull(control.button + 0xe8)?;
        let primary = self.word(renderer)?;
        let resource = match primary.checked_sub(base) {
            Some(0x4356e08) => {
                let p = self.nonnull(renderer + 0x168)?;
                if self.word(p)? != base + 0x4357160 {
                    return Err(missing("Exact Sprite resource identity required"));
                }
                p
            }
            Some(0x43593d8) => {
                let p = self.nonnull(renderer + 0x168)?;
                if self.word(p)? != base + 0x4359740 {
                    return Err(missing("Exact Text resource identity required"));
                }
                p
            }
            Some(0x434b370) => {
                let p = self.nonnull(renderer + 0x1b0)?;
                if self.word(renderer + 0x168)? != p || self.word(p)? != base + 0x434b668 {
                    return Err(missing("Exact aliased Frame resource identity required"));
                }
                p
            }
            _ => return Err(missing("Exact supported renderer identity required")),
        };
        let graphics = self.nonnull(base + 0x4577890)?;
        if self.word(graphics)? != base + 0x42417f8 {
            return Err(missing("Exact witnessed graphics identity required"));
        }
        let positions = [
            self.nonnull(control.window + 0xe70)?,
            self.nonnull(control.window + 0xe78)?,
        ];
        let mut parents = Vec::new();
        let mut seen = BTreeSet::new();
        let mut object = renderer;
        for _ in 0..=16 {
            if !seen.insert(object) {
                return Err(missing("Repeated witnessed renderer ancestor"));
            }
            let primary = self.word(object)?;
            if object != renderer
                && ![0x43484f0, 0x4356e08, 0x434b370, 0x434c5b0]
                    .iter()
                    .any(|p| primary == base + p)
            {
                return Err(missing(
                    "Exact witnessed renderer ancestor identity required",
                ));
            }
            let parent = self.0.get(&(object + 0x108)).copied();
            parents.push(Node {
                object,
                primary,
                parent,
            });
            if [graphics + 0x1f0, graphics + 0x350].contains(&object) {
                break;
            }
            if parent.is_none() {
                return Err(missing(
                    "Renderer ancestor parent identity was not witnessed",
                ));
            }
            match parent {
                Some(p) if p != 0 => object = p,
                _ => break,
            }
        }
        if parents
            .last()
            .is_some_and(|n| n.parent.is_some_and(|p| p != 0))
            && ![graphics + 0x1f0, graphics + 0x350].contains(&parents.last().unwrap().object)
        {
            return Err(missing(
                "Complete bounded renderer ancestor identities required",
            ));
        }
        Ok(Geometry::Observed {
            renderer,
            primary,
            resource,
            graphics,
            positions,
            parents,
        })
    }
}

pub(super) struct Scopes {
    pub(super) base: u64,
    pub(super) anchors: [u64; 3],
    pub(super) playthrough_id: String,
    pub(super) player: String,
    pub(super) country: u64,
    pub(super) production: u64,
    pub(super) registry: u64,
    pub(super) manager: u64,
    pub(super) panel: u64,
    pub(super) panel_root: Option<u64>,
    pub(super) opener: Option<Opener>,
    pub(super) choice: Option<Choice>,
    pub(super) include_geometry: bool,
    pub(super) find: Option<crate::construction::find_view::FindIdentity>,
    pub(super) words: Words,
}
impl Scopes {
    pub(super) fn scope(&self, stage: Stage) -> Result<Scope> {
        // Find stages pin the Find view instead; the panel may be unmaterialized.
        let root = match self.panel_root.filter(|p| *p != 0) {
            Some(root) => root,
            None if stage.is_find() => 0,
            None => return Err(missing("Materialized own construction panel/root required")),
        };
        let common = Common {
            base: self.base,
            campaign_anchors: self.anchors,
            playthrough_id: self.playthrough_id.clone(),
            player: self.player.clone(),
            country: self.country,
            production: self.production,
            registry: self.registry,
            manager: self.manager,
            panel: self.panel,
            panel_root: root,
            blockers: self
                .words
                .blockers(self.base, self.manager, self.anchors[1])?,
        };
        let target = match stage {
            Stage::EnsureOpen => {
                let mut target = self.opener.clone().ok_or_else(|| {
                    missing("Exact own named opener/callback/optional identity required")
                })?;
                if self.include_geometry {
                    target.control.geometry = self.words.geometry(self.base, &target.control)?;
                }
                Target::Opener(target)
            }
            Stage::SelectBuilding => {
                let mut target = self.choice.clone().ok_or_else(|| {
                    missing("Complete exact requested canonical choice joins required")
                })?;
                if self.include_geometry {
                    target.control.geometry = self.words.geometry(self.base, &target.control)?;
                }
                Target::Choice(target)
            }
            Stage::FindOpen | Stage::FindQuery | Stage::FindActivate => {
                let find = self
                    .find
                    .clone()
                    .ok_or_else(|| missing("Exact own Find view identity required"))?;
                if find.window == 0
                    || find.text_box == 0
                    || (stage != Stage::FindOpen
                        && (find.state.is_none() || find.template.is_none()))
                {
                    return Err(missing(
                        "Materialized Find view, text box and own state template required",
                    ));
                }
                Target::Find(find)
            }
        };
        Ok(Scope { common, target })
    }
}

#[cfg(test)]
mod tests;
