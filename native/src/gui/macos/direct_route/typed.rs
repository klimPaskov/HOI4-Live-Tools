//! Private typed observation only. Caller owns original context/deadline/final
//! witnesses. No process opener, event, input or atomic-cache authority here.
use super::{envelope as e, model};
use crate::{
    error::{Error, Result},
    memory::Reader,
};
use e::{basis, Common, Decision, ProgressTrig};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
const TYPES: &[(u64, &str)] = &[
    (0x36bea88, "C2dPositionObject"),
    (0x36cc528, "CSprite"),
    (0x36be530, "CFrameAnimatedSprite"),
    (0x36bb6a0, "CCorneredTileSprite"),
    (0x36c9210, "CProgressbarSprite"),
    (0x36c04b8, "CInstantTextObject"),
    (0x35edb70, "CShieldObject"),
    (0x36cee08, "CTextSprite"),
    (0x36c8c18, "C2dPieChartTemplate<SPieVertex>"),
    (0x36c0840, "CLineChart"),
];
const CANVAS: [i32; 2] = [1920, 1080];
const POINT: [f32; 2] = [960.0, 540.0];
fn invalid(s: impl Into<String>) -> Error {
    Error::new("direct_gui_observation_unavailable", s)
}
fn ptr(a: u64) -> Result<()> {
    if !(0x10000..=0x7fff_fffe_f000).contains(&a) || a % 8 != 0 {
        return Err(invalid("Bounded aligned original pointer required"));
    }
    Ok(())
}
fn i(b: &[u8], at: usize) -> i32 {
    i32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}
fn u(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().unwrap())
}
fn f(b: &[u8], at: usize) -> Result<f32> {
    let x = f32::from_le_bytes(b[at..at + 4].try_into().unwrap());
    if !x.is_finite() {
        return Err(invalid("Nonfinite consumed geometry"));
    }
    Ok(x)
}
fn pair(r: &Reader<'_>, a: u64) -> Result<[f32; 2]> {
    let b = r.read(a, 8)?;
    Ok([f(&b, 0)?, f(&b, 4)?])
}
fn ints(r: &Reader<'_>, a: u64) -> Result<[i32; 2]> {
    let b = r.read(a, 8)?;
    Ok([i(&b, 0), i(&b, 4)])
}
#[derive(Clone, Debug)]
enum Asset {
    Sprite(basis::SpriteType),
    Frame(e::FrameResource),
    Masked(e::MaskedResource),
    Progress(e::ProgressResource),
    Unsupported,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceIdentity {
    field_offset: u64,
    object: u64,
    primary: u64,
}
impl ResourceIdentity {
    pub fn field_offset(&self) -> u64 {
        self.field_offset
    }
    pub fn object(&self) -> u64 {
        self.object
    }
    pub fn primary(&self) -> u64 {
        self.primary
    }
}
fn asset(
    r: &Reader<'_>,
    base: u64,
    owner: u64,
    offset: u64,
    identities: &mut Vec<ResourceIdentity>,
) -> Result<Asset> {
    let a = r.u64(owner + offset)?;
    if a == 0 {
        identities.push(ResourceIdentity {
            field_offset: offset,
            object: 0,
            primary: 0,
        });
        return Ok(Asset::Unsupported);
    }
    ptr(a)?;
    let primary = r.u64(a)?; // unknown exact primary never permits resource payload
    identities.push(ResourceIdentity {
        field_offset: offset,
        object: a,
        primary,
    });
    Ok(match primary.checked_sub(base) {
        Some(0x36cc880) => Asset::Sprite(basis::SpriteType {
            primary_rva: 0x36cc880,
            texture: ints(r, a + 0x8c)?,
            tiles: ints(r, a + 0x78)?,
            orientation: r.i32(a + 0xfc)?,
            loaded_index: r.i32(a + 0x188)?,
        }),
        Some(0x36be828) => Asset::Frame(e::FrameResource {
            primary_rva: 0x36be828,
            dimensions: ints(r, a + 0x8c)?,
            tiles: ints(r, a + 0x78)?,
            orientation: r.i32(a + 0xfc)?,
            loaded_index: r.i32(a + 0x188)?,
        }),
        Some(0x35ec9d8) => Asset::Masked(e::MaskedResource {
            primary_rva: 0x35ec9d8,
            dimensions: ints(r, a + 0x210)?,
            tiles: ints(r, a + 0x78)?,
            orientation: r.i32(a + 0xfc)?,
        }),
        Some(0x36c9130) => Asset::Progress(e::ProgressResource {
            primary_rva: 0x36c9130,
            dimensions: ints(r, a + 0xdc)?,
        }),
        _ => Asset::Unsupported,
    })
}
#[derive(Clone, Debug)]
enum Shape {
    None,
    Sprite(Asset, Asset),
    Frame(Asset, Asset),
    Masked(Asset, Asset),
    Progress(Asset, f32),
    Corner([i32; 2]),
    Instant {
        offset: [i32; 2],
        delta: [i32; 2],
        dims: [i32; 2],
        align: i32,
    },
    Unsupported,
}
#[derive(Clone, Debug)]
struct Meta {
    local: [f32; 2],
    rs: [f32; 2],
    anchor: bool,
    target: u64,
    resource_bindings: Vec<ResourceIdentity>,
    unconditional: u8,
    clip: model::Clip,
    clip_mode: i32,
    clip145: bool,
    canvas: Option<[i32; 2]>,
    cached: Option<([f32; 3], [f32; 2])>,
    shape: Shape,
}
#[derive(Clone, Debug)]
pub struct Node {
    object: u64,
    parent: u64,
    primary: u64,
    name: &'static str,
    depth: usize,
    children: Vec<u64>,
    child_data: u64,
    child_capacity: i32,
    self_candidate: bool,
    metadata: Option<Meta>,
}
impl Node {
    pub fn resource_identities(&self) -> &[ResourceIdentity] {
        self.metadata
            .as_ref()
            .map_or(&[], |m| m.resource_bindings.as_slice())
    }
    pub fn target(&self) -> Option<u64> {
        self.metadata
            .as_ref()
            .and_then(|m| (m.target != 0).then_some(m.target))
    }
    pub fn child_header(&self) -> (u64, i32, usize) {
        (self.child_data, self.child_capacity, self.children.len())
    }
    pub fn object(&self) -> u64 {
        self.object
    }
    pub fn children(&self) -> &[u64] {
        &self.children
    }
    pub fn parent(&self) -> u64 {
        self.parent
    }
    pub fn primary(&self) -> u64 {
        self.primary
    }
    pub fn depth(&self) -> usize {
        self.depth
    }
}
#[derive(Clone, Debug)]
pub struct Guards {
    complete: bool,
    up_byte: u8,
    singleton: Option<(u64, u8)>,
}
fn guards(r: &Reader<'_>, base: u64, gui: u64, gfx: u64) -> Result<Guards> {
    let current = pair(r, gfx + 0x40)?;
    let processed = pair(r, gfx + 0x48)?;
    let tg = pair(r, gfx + 0x1c0)?;
    let te = pair(r, gfx + 0x1c8)?;
    let canvas = ints(r, gfx + 0x170)?;
    let (_, _, source) = r.array_header(gfx + 0x50)?;
    let (_, _, route) = r.array_header(gfx + 0x28)?;
    let skip = r.byte(gui + 0x130)?;
    let conditional = r.byte(gui + 0x12)?;
    let focus = r.u64(gui + 0x138)?;
    let up = r.byte(base + 0x37ed958)?;
    let up_object = r.u64(base + 0x37ed9b0)?;
    let post = r.u64(base + 0x37ed910)?;
    let singleton = if up == 1 {
        let a = r.u64(base + 0x37ed950)?;
        if a == 0 {
            return Err(invalid("Up invalidation singleton absent"));
        }
        ptr(a)?;
        if r.u64(a)? != base + 0x36b9360 {
            return Err(invalid(
                "Exact final up singleton primary required before411",
            ));
        }
        let b = r.byte(a + 0x411)?;
        if b > 1 {
            return Err(invalid("Unsupported invalidation byte411"));
        }
        Some((a, b))
    } else {
        None
    };
    Ok(Guards {
        complete: current == POINT
            && processed == POINT
            && tg == [1.0, 1.0]
            && te == [1.0, 1.0]
            && canvas == CANVAS
            && source == 0
            && route == 0
            && skip == 0
            && conditional == 0
            && focus == 0
            && up <= 1
            && up_object == 0
            && post == 0,
        up_byte: up,
        singleton,
    })
}
#[derive(Clone, Debug, Default)]
pub struct Counts {
    nodes: usize,
    self_objects: usize,
    discrepancy_examples: Vec<(u64, f32, [f32; 2])>,
    candidates: usize,
    clip_rejected: usize,
    geometry_rejected: usize,
    possible: usize,
    unsupported: usize,
    xy_mismatch: usize,
    scale_mismatch: usize,
    reasons: BTreeMap<String, usize>,
}
impl Counts {
    fn reason(&mut self, r: impl Into<String>) {
        *self.reasons.entry(r.into()).or_default() += 1;
    }
    fn summary(&self) -> Value {
        json!({"nodes":self.nodes,"self_objects":self.self_objects,"candidate_targets":self.candidates,"scale_discrepancy_examples":self.discrepancy_examples,"clip_rejected":self.clip_rejected,"geometry_rejected":self.geometry_rejected,"possible":self.possible,"unsupported":self.unsupported,"xy_cache_mismatches":self.xy_mismatch,"scale_cache_mismatches":self.scale_mismatch,"reasons":self.reasons})
    }
    fn rejected(&self) -> bool {
        self.candidates == self.clip_rejected + self.geometry_rejected
            && self.possible == 0
            && self.unsupported == 0
    }
}
pub struct TypedObservation {
    base: u64,
    gui: u64,
    gfx: u64,
    root: u64,
    guards: Guards,
    nodes: Vec<Node>,
    fresh: Counts,
    strict: Counts,
    unqualified: Counts,
    calls: u64,
    bytes: usize,
}
impl TypedObservation {
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }
    /// Narrow, provisional data condition only. Caller MUST finish its original
    /// witnesses/context/deadline first. This cannot establish cache completion,
    /// absence of GUI effects, full picker semantics or input/delivery authority.
    pub fn require_no_direct_gui_recipient(&self) -> Result<()> {
        if !self.guards.complete || !self.fresh.rejected() {
            return Err(invalid(
                "Direct GUI recipient absence unsupported by complete typed observation",
            ));
        }
        Ok(())
    }
    /// Bounded tool diagnostics omit private pointer identities and examples.
    /// This renders existing typed facts and is never consumed as authority.
    pub fn compact_summary(&self) -> Value {
        let counts = |c: &Counts| {
            json!({"nodes":c.nodes,"self_objects":c.self_objects,
                "candidate_targets":c.candidates,"clip_rejected":c.clip_rejected,
                "geometry_rejected":c.geometry_rejected,"possible":c.possible,
                "unsupported":c.unsupported,"xy_cache_mismatches":c.xy_mismatch,
                "scale_cache_mismatches":c.scale_mismatch})
        };
        json!({"status":"typed_direct_gui_observation","complete_graph":true,
            "guard_conditions":self.guards.complete,
            "fresh_normal_c99_zero":counts(&self.fresh),
            "strict_cache_normal_c99_zero":counts(&self.strict),
            "fresh_unqualified_trig":counts(&self.unqualified),
            "logical_requests":self.calls,"bytes":self.bytes,
            "cache_completion_verified":false,"full_picker_verified":false,
            "no_gui_effects_verified":false,"input_authorized":false,"delivery_verified":false})
    }
    pub fn summary(&self) -> Value {
        json!({"status":"typed_direct_gui_observation","anchors":{"base":self.base,"gui":self.gui,"graphics":self.gfx,"root":self.root},"complete_graph":true,"guard_conditions":self.guards.complete,"up_global_byte":self.guards.up_byte,"up_invalidation_singleton_present":self.guards.singleton.is_some(),"up_invalidation_flag411":self.guards.singleton.map(|x|x.1),"fresh_normal_c99_zero":self.fresh.summary(),"strict_cache_normal_c99_zero":self.strict.summary(),"fresh_unqualified_trig":self.unqualified.summary(),"logical_requests":self.calls,"bytes":self.bytes,"cache_completion_verified":false,"full_picker_verified":false,"no_gui_effects_verified":false,"input_authorized":false,"delivery_verified":false})
    }
}
pub fn observe(
    r: &Reader<'_>,
    base: u64,
    gui: u64,
    gfx: u64,
    root: u64,
) -> Result<TypedObservation> {
    for a in [base, gui, gfx, root] {
        ptr(a)?
    }
    if base
        .checked_add(0x37ed9b8)
        .is_none_or(|x| x > 0x7fff_ffff_0000)
    {
        return Err(invalid("Bounded image base required"));
    }
    if root
        != gfx
            .checked_add(0x1e0)
            .ok_or_else(|| invalid("Root overflow"))?
    {
        return Err(invalid("Original embedded root association required"));
    }
    if r.u64(gui)? != base + 0x36a5810
        || r.u64(gui + 0x98)? != gfx
        || r.u64(gfx)? != base + 0x35e5b58
    {
        return Err(invalid("Original exact GUI/graphics join required"));
    }
    let guard = guards(r, base, gui, gfx)?;
    let mut stack = vec![(root, 0, 0)];
    let mut seen = BTreeSet::new();
    let mut nodes = Vec::new();
    while let Some((object, parent, depth)) = stack.pop() {
        if depth > 128 || seen.len() >= 65536 || r.calls.get() > 180000 {
            return Err(invalid("Complete tree policy exceeded"));
        }
        ptr(object)?;
        if !seen.insert(object) {
            return Err(invalid("Duplicate/cycle graphical membership"));
        }
        let primary = r.u64(object)?;
        let Some((_, name)) = TYPES
            .iter()
            .find(|(rv, _)| base.checked_add(*rv) == Some(primary))
        else {
            return Err(invalid("Unknown node primary before payload"));
        };
        if parent == 0 && primary != base + 0x36bea88 {
            return Err(invalid("Exact root primary required"));
        }
        if r.u64(object + 0x108)? != parent {
            return Err(invalid(
                "Original reciprocal parent required before payload",
            ));
        }
        let p = r.read(object, 0x15c)?;
        if u(&p, 0) != primary || u(&p, 0x108) != parent {
            return Err(invalid("Original primary/parent changed before payload"));
        }
        let traversal = p[0x128] != 0;
        let self_candidate = p[0x144] != 0;
        let children = if traversal {
            let count = i(&p, 0xcc);
            let capacity = i(&p, 0xc8);
            if count < 0 || capacity < 0 || count > capacity || capacity > 8192 {
                return Err(invalid("Complete child bounds required"));
            }
            let data = u(&p, 0xc0);
            if capacity > 0 {
                ptr(data)?
            }
            if count == 0 {
                Vec::new()
            } else {
                r.read(data, count as usize * 8)?
                    .chunks_exact(8)
                    .map(|x| u(x, 0))
                    .collect::<Vec<_>>()
            }
        } else {
            Vec::new()
        };
        if seen.len() + stack.len() + children.len() > 65536 {
            return Err(invalid("Prospective total graph bound"));
        }
        let metadata = if traversal || self_candidate {
            let target = u(&p, 0x138);
            let mut m = Meta {
                local: [f(&p, 0x70)?, f(&p, 0x74)?],
                rs: [f(&p, 0x110)?, f(&p, 0x114)?],
                anchor: p[0x134] != 0,
                target,
                resource_bindings: Vec::new(),
                unconditional: p[0x31],
                clip: [i(&p, 0x148), i(&p, 0x14c), i(&p, 0x150), i(&p, 0x154)],
                clip_mode: i(&p, 0x158),
                clip145: p[0x145] != 0,
                canvas: None,
                cached: None,
                shape: Shape::None,
            };
            if self_candidate {
                m.canvas = Some([i(&p, 0x120), i(&p, 0x124)]);
                m.cached = Some((
                    [f(&p, 0xb0)?, f(&p, 0xb4)?, f(&p, 0xb8)?],
                    [f(&p, 0x80)?, f(&p, 0x94)?],
                ));
                if target != 0 {
                    m.shape = match *name {
                        "CSprite" => {
                            let a = asset(r, base, object, 0x168, &mut m.resource_bindings)?;
                            Shape::Sprite(a.clone(), a)
                        }
                        "CFrameAnimatedSprite" => Shape::Frame(
                            asset(r, base, object, 0x168, &mut m.resource_bindings)?,
                            asset(r, base, object, 0x1b0, &mut m.resource_bindings)?,
                        ),
                        "CShieldObject" => Shape::Masked(
                            asset(r, base, object, 0x168, &mut m.resource_bindings)?,
                            asset(r, base, object, 0x1b0, &mut m.resource_bindings)?,
                        ),
                        "CProgressbarSprite" => Shape::Progress(
                            asset(r, base, object, 0x168, &mut m.resource_bindings)?,
                            f(&r.read(object + 0x160, 4)?, 0)?,
                        ),
                        "CCorneredTileSprite" => Shape::Corner(ints(r, object + 0x16c)?),
                        "CInstantTextObject" => Shape::Instant {
                            offset: ints(r, object + 0x1e0)?,
                            delta: ints(r, object + 0x1f8)?,
                            dims: ints(r, object + 0x200)?,
                            align: r.i32(object + 0x208)?,
                        },
                        _ => Shape::Unsupported,
                    }
                }
            }
            Some(m)
        } else {
            None
        };
        for &child in children.iter().rev() {
            stack.push((child, object, depth + 1))
        }
        nodes.push(Node {
            object,
            parent,
            primary,
            name,
            depth,
            child_data: if traversal { u(&p, 0xc0) } else { 0 },
            child_capacity: if traversal { i(&p, 0xc8) } else { 0 },
            children,
            self_candidate,
            metadata,
        });
    }
    let fresh = evaluate(&nodes, ProgressTrig::NormalSincosZero, false)?;
    let strict = evaluate(&nodes, ProgressTrig::NormalSincosZero, true)?;
    let unqualified = evaluate(&nodes, ProgressTrig::Unqualified, false)?;
    Ok(TypedObservation {
        base,
        gui,
        gfx,
        root,
        guards: guard,
        nodes,
        fresh,
        strict,
        unqualified,
        calls: r.calls.get(),
        bytes: r.bytes.get(),
    })
}
fn converted(d: basis::Decision) -> Decision {
    match d {
        basis::Decision::Rejected => Decision::Rejected,
        basis::Decision::Possible => Decision::Possible,
        basis::Decision::Unsupported(x) => Decision::Unsupported(e::Unsupported::Basis(x)),
    }
}
fn evaluate(nodes: &[Node], trig: ProgressTrig, strict: bool) -> Result<Counts> {
    let mut states = BTreeMap::new();
    let mut out = Counts {
        nodes: nodes.len(),
        ..Default::default()
    };
    for n in nodes {
        let (parent, clip, mut consistent) = if n.parent == 0 {
            (model::Transform::SEED, None, true)
        } else {
            *states
                .get(&n.parent)
                .ok_or_else(|| invalid("Ordered typed parent absent"))?
        };
        if !n.self_candidate && n.children.is_empty() {
            states.insert(n.object, (parent, clip, consistent));
            continue;
        }
        let m = n
            .metadata
            .as_ref()
            .ok_or_else(|| invalid("Consumed typed metadata missing"))?;
        let t = model::transform(
            parent,
            m.local,
            m.rs[1],
            m.anchor,
            n.self_candidate,
            !n.children.is_empty(),
            CANVAS,
        )
        .map_err(|x| invalid(format!("Transform {x:?}")))?
        .ok_or_else(|| invalid("Unexpected early transform"))?;
        let clips = model::clips(clip, m.clip_mode, m.clip, m.clip145, n.self_candidate);
        if let Some((xy, scale)) = m.cached {
            out.self_objects += 1;
            let a = t.xy[0].to_bits() == xy[0].to_bits()
                && t.xy[1].to_bits() == xy[1].to_bits()
                && xy[2].to_bits() == 0;
            let b = scale.iter().all(|s| s.to_bits() == t.scale.to_bits());
            out.xy_mismatch += usize::from(!a);
            out.scale_mismatch += usize::from(!b);
            if !b && out.discrepancy_examples.len() < 8 {
                out.discrepancy_examples.push((n.object, t.scale, scale));
            }
            consistent &= a && b;
        }
        states.insert(n.object, (t, clips.children, consistent));
        if !n.self_candidate || m.target == 0 {
            continue;
        }
        out.candidates += 1;
        if m.unconditional != 0 {
            out.unsupported += 1;
            out.reason("source31");
            continue;
        }
        if strict && !consistent {
            out.unsupported += 1;
            out.reason("captured_transform_discrepancy");
            continue;
        }
        if !model::clip_contains(clips.self_clip, POINT) {
            out.clip_rejected += 1;
            continue;
        }
        if !t.unit_path {
            out.unsupported += 1;
            out.reason("nonunit_ancestor");
            continue;
        }
        let canvas = m
            .canvas
            .ok_or_else(|| invalid("Typed self canvas missing"))?;
        if canvas != CANVAS {
            return Err(invalid("Qualified canvas required"));
        }
        let c = Common {
            canvas,
            translation: t.xy,
            inherited_scale: t.scale,
            rotation: m.rs[0],
        };
        let d = match &m.shape {
            Shape::Sprite(Asset::Sprite(a), Asset::Sprite(b)) => basis::sprite(
                POINT,
                c,
                m.rs[1],
                basis::SpriteResource::SpriteType(*a),
                basis::SpriteResource::SpriteType(*b),
            )
            .map(converted),
            Shape::Frame(Asset::Frame(a), Asset::Frame(b)) => e::frame(
                POINT,
                e::FrameRenderer {
                    primary_rva: 0x36be530,
                    common: c,
                    local_scale: m.rs[1],
                },
                *a,
                *b,
            ),
            Shape::Masked(Asset::Masked(a), Asset::Masked(b)) => e::masked(
                POINT,
                e::MaskedRenderer {
                    primary_rva: 0x35edb70,
                    common: c,
                    local_scale: m.rs[1],
                },
                *a,
                *b,
            ),
            Shape::Progress(Asset::Progress(a), angle) => e::progress(
                POINT,
                e::ProgressRenderer {
                    primary_rva: 0x36c9210,
                    common: c,
                    rotation: *angle,
                },
                *a,
                trig,
            ),
            Shape::Corner(d) => basis::cornered(POINT, c, *d).map(converted),
            Shape::Instant {
                offset,
                delta,
                dims,
                align,
            } => model::instant_union(POINT, c, *offset, *delta, *dims, *align),
            _ => Ok(Decision::Unsupported(e::Unsupported::ResourcePrimary)),
        }
        .map_err(|x| invalid(format!("Malformed class geometry: {x:?}")))?;
        match d {
            Decision::Rejected => out.geometry_rejected += 1,
            Decision::Possible => {
                out.possible += 1;
                out.reason(n.name)
            }
            Decision::Unsupported(r) => {
                out.unsupported += 1;
                out.reason(format!("{r:?}"))
            }
        }
    }
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::ReadMemory;
    use std::cell::RefCell;
    const B: u64 = 0x100000000;
    const G: u64 = 0x200000;
    const X: u64 = 0x300000;
    const R: u64 = X + 0x1e0;
    const D: u64 = 0x400000;
    const C: u64 = 0x500000;
    struct Fake {
        bytes: BTreeMap<u64, u8>,
        reads: RefCell<Vec<(u64, usize)>>,
        tear: bool,
    }
    impl Fake {
        fn span(&mut self, a: u64, n: usize) {
            for x in 0..n {
                self.bytes.insert(a + x as u64, 0);
            }
        }
        fn put(&mut self, a: u64, b: &[u8]) {
            for (i, v) in b.iter().enumerate() {
                self.bytes.insert(a + i as u64, *v);
            }
        }
        fn u(&mut self, a: u64, v: u64) {
            self.put(a, &v.to_le_bytes())
        }
        fn i(&mut self, a: u64, v: i32) {
            self.put(a, &v.to_le_bytes())
        }
        fn f(&mut self, a: u64, v: f32) {
            self.put(a, &v.to_le_bytes())
        }
    }
    impl ReadMemory for Fake {
        fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
            self.reads.borrow_mut().push((a, n));
            let mut b = (0..n)
                .map(|i| {
                    self.bytes
                        .get(&(a + i as u64))
                        .copied()
                        .ok_or_else(|| Error::new("short_read", "fixture unmapped"))
                })
                .collect::<Result<Vec<_>>>()?;
            if self.tear && a == C && n == 0x15c {
                b[0] ^= 8
            }
            Ok(b)
        }
    }
    fn fixture() -> Fake {
        let mut f = Fake {
            bytes: BTreeMap::new(),
            reads: RefCell::new(Vec::new()),
            tear: false,
        };
        f.span(G, 0x220);
        f.span(X, 0x400);
        f.span(C, 0x200);
        f.span(B + 0x37ed910, 0xb0);
        f.span(D, 8);
        f.u(G, B + 0x36a5810);
        f.u(G + 0x98, X);
        f.u(X, B + 0x35e5b58);
        for at in [X + 0x40, X + 0x48] {
            f.f(at, 960.0);
            f.f(at + 4, 540.0)
        }
        for at in [X + 0x1c0, X + 0x1c8] {
            f.f(at, 1.0);
            f.f(at + 4, 1.0)
        }
        f.i(X + 0x170, 1920);
        f.i(X + 0x174, 1080);
        f.u(R, B + 0x36bea88);
        f.u(R + 0xc0, D);
        f.i(R + 0xc8, 1);
        f.i(R + 0xcc, 1);
        f.put(R + 0x128, &[1]);
        f.f(R + 0x114, 1.0);
        f.u(D, C);
        f.u(C, B + 0x36bb6a0);
        f.u(C + 0x108, R);
        f.put(C + 0x144, &[1]);
        f.u(C + 0x138, 0x600000);
        f.f(C + 0x70, -960.0);
        f.f(C + 0x74, 540.0);
        f.f(C + 0x114, 1.0);
        f.i(C + 0x120, 1920);
        f.i(C + 0x124, 1080);
        f.f(C + 0xb0, -960.0);
        f.f(C + 0xb4, 540.0);
        f.f(C + 0x80, 1.0);
        f.f(C + 0x94, 1.0);
        f.i(C + 0x16c, 1);
        f.i(C + 0x170, 1);
        f
    }
    #[test]
    fn complete_typed_graph_and_guards_feed_narrow_condition() {
        let f = fixture();
        let r = Reader::new(&f);
        let o = observe(&r, B, G, X, R).unwrap();
        assert_eq!(o.nodes().len(), 2);
        assert_eq!(o.nodes()[0].children(), &[C]);
        o.require_no_direct_gui_recipient().unwrap();
        assert_eq!(o.summary()["input_authorized"], false);
        let compact = o.compact_summary();
        assert_eq!(compact["fresh_normal_c99_zero"]["nodes"], 2);
        assert!(compact.get("anchors").is_none());
        for branch in [
            "fresh_normal_c99_zero",
            "strict_cache_normal_c99_zero",
            "fresh_unqualified_trig",
        ] {
            assert!(compact[branch].get("scale_discrepancy_examples").is_none());
            assert!(compact[branch]
                .as_object()
                .unwrap()
                .values()
                .all(Value::is_u64));
        }
    }
    #[test]
    fn unknown_primary_stops_before_parent_or_payload() {
        let mut f = fixture();
        f.u(C, B + 1);
        assert!(observe(&Reader::new(&f), B, G, X, R).is_err());
        assert!(!f.reads.borrow().contains(&(C + 0x108, 8)));
        assert!(!f.reads.borrow().contains(&(C, 0x15c)));
    }
    #[test]
    fn foreign_parent_stops_before_common_or_child_payload() {
        let mut f = fixture();
        f.u(C + 0x108, 0x700000);
        assert!(observe(&Reader::new(&f), B, G, X, R).is_err());
        assert!(!f.reads.borrow().contains(&(C, 0x15c)));
    }
    #[test]
    fn prefix_replacement_and_child_bounds_refuse() {
        let mut f = fixture();
        f.tear = true;
        assert!(observe(&Reader::new(&f), B, G, X, R).is_err());
        let mut f = fixture();
        f.i(R + 0xc8, 8193);
        assert!(observe(&Reader::new(&f), B, G, X, R).is_err());
        assert!(!f.reads.borrow().contains(&(D, 8)));
    }
    #[test]
    fn unknown_resource_payload_is_not_followed_and_remains_unsupported() {
        let mut f = fixture();
        f.u(C, B + 0x36cc528);
        f.u(C + 0x168, 0x700000);
        f.u(0x700000, B + 2);
        let o = observe(&Reader::new(&f), B, G, X, R).unwrap();
        assert!(o.require_no_direct_gui_recipient().is_err());
        assert_eq!(o.summary()["fresh_normal_c99_zero"]["unsupported"], 1);
        assert_eq!(
            f.reads
                .borrow()
                .iter()
                .filter(|(a, _)| *a >= 0x700000 && *a < 0x710000)
                .copied()
                .collect::<Vec<_>>(),
            vec![(0x700000, 8)]
        );
    }
    #[test]
    fn typed_up_invalidation_accepts_only_exact_singleton_before_flag() {
        let mut f = fixture();
        f.put(B + 0x37ed958, &[1]);
        f.u(B + 0x37ed950, 0x700000);
        f.u(0x700000, B + 0x36b9360);
        f.put(0x700411, &[1]);
        observe(&Reader::new(&f), B, G, X, R)
            .unwrap()
            .require_no_direct_gui_recipient()
            .unwrap();
        f.reads.borrow_mut().clear();
        f.u(0x700000, B + 0x3644900);
        assert!(observe(&Reader::new(&f), B, G, X, R).is_err());
        assert!(!f.reads.borrow().contains(&(0x700411, 1)));
    }
    #[test]
    fn nonempty_cache_does_not_qualify_despite_outer_misses() {
        let mut f = fixture();
        f.i(X + 0x58, 1);
        f.i(X + 0x5c, 1);
        f.u(X + 0x50, 0x800000);
        let o = observe(&Reader::new(&f), B, G, X, R).unwrap();
        assert!(o.require_no_direct_gui_recipient().is_err());
        assert!(!f.reads.borrow().iter().any(|(a, _)| *a == 0x800000));
    }
    #[test]
    fn base_and_root_mismatch_refuse_before_reads() {
        let f = fixture();
        assert!(observe(&Reader::new(&f), 0, G, X, R).is_err());
        assert!(f.reads.borrow().is_empty());
        assert!(observe(&Reader::new(&f), B, G, X, R + 8).is_err());
        assert!(f.reads.borrow().is_empty());
    }
}
#[cfg(test)]
mod cache_difference_test {
    use super::*;
    #[test]
    fn original_typed_data_not_json_can_retain_discrepancy_without_adopting_it() {
        let m = Meta {
            local: [-960.0, 540.0],
            rs: [0.0, 1.0],
            anchor: false,
            target: 0x700000,
            resource_bindings: Vec::new(),
            unconditional: 0,
            clip: [0; 4],
            clip_mode: 0,
            clip145: false,
            canvas: Some(CANVAS),
            cached: Some(([-960.0, 540.0, 0.0], [0.8, 0.8])),
            shape: Shape::Corner([1, 1]),
        };
        let n = Node {
            object: 0x10000,
            parent: 0,
            primary: 0,
            name: "CCorneredTileSprite",
            depth: 0,
            children: Vec::new(),
            child_data: 0,
            child_capacity: 0,
            self_candidate: true,
            metadata: Some(m),
        };
        let fresh = evaluate(
            std::slice::from_ref(&n),
            ProgressTrig::NormalSincosZero,
            false,
        )
        .unwrap();
        let strict = evaluate(&[n], ProgressTrig::NormalSincosZero, true).unwrap();
        assert_eq!(fresh.geometry_rejected, 1);
        assert_eq!(fresh.scale_mismatch, 1);
        assert_eq!(fresh.self_objects, 1);
        assert_eq!(fresh.discrepancy_examples, vec![(0x10000, 1.0, [0.8, 0.8])]);
        assert_eq!(strict.unsupported, 1);
    }
}
