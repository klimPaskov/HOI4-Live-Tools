//! Original passive point and current local UI gates. These facts deliberately
//! do not claim that a map click passes every modal/capture/GUI receiver.
use super::super::action_fence::Fence;
use crate::{
    construction::{mac_map_point, mac_panel, mac_scene_scope},
    error::{Error, Result},
    game::{self, ClockContext},
    gui::macos::{self, source_point},
    gui_tree,
    memory::{ReadMemory, Reader, StringAbi},
    platform::{Identity, Process},
    research_ui::Witnesses,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::cell::RefCell;

fn changed(message: &str) -> Error {
    Error::new("construction_placement_changed", message)
}
fn source_pointer(value: u64) -> Result<u64> {
    if !(0x10000..=0x7fff_ffff_0000).contains(&value) || value % 8 != 0 {
        return Err(changed(
            "Bounded aligned original placement pointer required",
        ));
    }
    Ok(value)
}
pub(super) fn require_map_route(
    observation: Option<&macos::direct_route::TypedObservation>,
) -> Result<()> {
    observation
        .ok_or_else(|| {
            Error::new(
                "construction_map_route_unqualified",
                "An encompassing typed direct-GUI observation is required",
            )
        })?
        .require_no_direct_gui_recipient()
}
fn scope(a: &ClockContext, b: &ClockContext) -> Result<()> {
    if a.anchors != b.anchors
        || a.player != b.player
        || a.playthrough_id != b.playthrough_id
        || a.difficulty != b.difficulty
        || a.historical_raw != b.historical_raw
        || b.date_raw < a.date_raw
    {
        return Err(changed("Original placement campaign changed"));
    }
    Ok(())
}
fn edges(r: &Reader<'_>, expected: &[(u64, u64)]) -> Result<()> {
    for &(a, v) in expected {
        if r.u64(a)? != v {
            return Err(changed(
                "Original placement edge changed before replacement payload",
            ));
        }
    }
    Ok(())
}
fn single_window(r: &Reader<'_>, base: u64) -> Result<()> {
    let video = source_pointer(r.u64(base + 0x3639800)?)?;
    for (o, f) in [(8, 0x263fea0), (0x48, 0x2643a50), (0x128, 0x2644350)] {
        if r.u64(video + o)? != base + f {
            return Err(changed(
                "Exact Cocoa backend required before window payload",
            ));
        }
    }
    let window = source_pointer(r.u64(video + 0x2c8)?)?;
    if r.u64(window)? != video + 0x2d8 || r.u64(window + 0xe8)? != 0 {
        return Err(changed(
            "Only one exact owned SDL window is supported; no next-window payload followed",
        ));
    }
    Ok(())
}
fn geometry_edges(r: &Reader<'_>, base: u64, g: &macos::Geometry) -> Result<Vec<(u64, u64)>> {
    let [i, gui, gfx, video, backend, window, data, nswindow, view, listener] = g.objects;
    let vi = r.u64(base + 0x34ce008)?;
    let li = r.u64(base + 0x34cdfa8)?;
    if [vi, li]
        .iter()
        .any(|v| !(8..=4096).contains(v) || v % 8 != 0)
    {
        return Err(changed("Bounded original Cocoa ivar offsets required"));
    }
    Ok(vec![
        (base + 0x35196c0, i),
        (i + 0x4c0, gui),
        (gui + 0x98, gfx),
        (base + 0x3639800, video),
        (video + 0x5a8, backend),
        (video + 0x2c8, window),
        (window + 0xe8, 0),
        (window + 0xd8, data),
        (data, window),
        (data + 8, nswindow),
        (data + 0x10, view),
        (data + 0x30, listener),
        (data + 0x38, backend),
        (base + 0x34ce008, vi),
        (base + 0x34cdfa8, li),
        (view + vi, window),
        (listener + li, data),
    ])
}

// Record only reads consumed by the existing typed own-scene reader. Later
// checks address those OLD locations in original order: they never follow a
// replacement array or receiver and do not reconstruct a complete scene walk.
struct Recorder<'a> {
    inner: &'a dyn ReadMemory,
    rows: RefCell<Vec<(u64, Vec<u8>)>>,
}
impl ReadMemory for Recorder<'_> {
    fn string_abi(&self) -> StringAbi {
        self.inner.string_abi()
    }
    fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
        let v = self.inner.read(a, n)?;
        if !self
            .rows
            .borrow()
            .iter()
            .any(|(at, old)| *at == a && old.len() == n)
        {
            self.rows.borrow_mut().push((a, v.clone()));
        }
        Ok(v)
    }
}
struct Scene {
    binding: mac_scene_scope::Binding,
    rows: Vec<(u64, Vec<u8>)>,
}
impl Scene {
    fn bind(memory: &dyn ReadMemory, base: u64) -> Result<Self> {
        let record = Recorder {
            inner: memory,
            rows: RefCell::new(vec![]),
        };
        let binding = mac_scene_scope::from_reader(&Reader::new(&record), base, None)?;
        Ok(Self {
            binding,
            rows: record.rows.into_inner(),
        })
    }
    fn check(&self, r: &Reader<'_>) -> Result<()> {
        for (a, bytes) in &self.rows {
            if r.read(*a, bytes.len())? != *bytes {
                return Err(changed(
                    "Original own scene source changed before new candidate payload",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct Ui {
    manager: u64,
    registry: u64,
    panel: u64,
    find_root: u64,
    edges: Vec<(u64, u64)>,
}
impl Ui {
    fn bind(p: &dyn Process, base: u64, context: &ClockContext) -> Result<Self> {
        let observation = mac_panel::observe_placement(p)?;
        let mut e = observation.placement_edges(base)?;
        let value = |a| {
            e.iter()
                .find(|(at, _)| *at == a)
                .map(|(_, v)| *v)
                .ok_or_else(|| changed("Missing typed panel binding"))
        };
        let idler = context.anchors[1];
        if value(base + game::MACOS.gs_global)? != context.anchors[0]
            || value(base + game::MACOS.idler_global)? != idler
            || value(base + game::MACOS.technology_db_global)? != context.anchors[2]
        {
            return Err(changed("Panel and point roots differ"));
        }
        let manager = value(idler + 0x670)?;
        let registry = value(idler + 0x4c0)?;
        let panel = observation
            .placement_window_object()
            .ok_or_else(|| changed("Materialized panel required"))?;
        p.begin_snapshot()?;
        let w = Witnesses::new(p);
        let r = Reader::new(&w);
        edges(&r, &e)?;
        let find = source_pointer(r.u64(manager + 0x190)?)?;
        if r.u64(find)? != base + 0x3457c38 || r.u64(find + 0xc30)? != idler {
            return Err(changed("Own exact Find required before root payload"));
        }
        let find_root = r.u64(find + 0xc38)?;
        e.extend([
            (manager + 0x190, find),
            (find + 0xc30, idler),
            (find + 0xc38, find_root),
            (find, base + 0x3457c38),
        ]);
        let ui = Self {
            manager,
            registry,
            panel,
            find_root,
            edges: e,
        };
        ui.check(&r, base)?;
        w.finish()?;
        p.check()?;
        Ok(ui)
    }
    fn check(&self, r: &Reader<'_>, base: u64) -> Result<()> {
        edges(r, &self.edges)?;
        if r.i32(self.manager + 0x3a8)? != 17
            || r.i32(self.manager + 0x3c0)? != 1
            || r.byte(self.panel + 0x95)? & 8 == 0
            || r.i32(self.panel + 0x12b8)? != 0
            || r.i32(self.panel + 0x1270)? != 0
        {
            return Err(changed(
                "Current selected construction panel must remain open and idle",
            ));
        }
        // No query, edit box, retained rows or result payload is needed.
        if self.find_root != 0 {
            source_pointer(self.find_root)?;
            let root = gui_tree::container(r, self.find_root, base)?
                .ok_or_else(|| changed("Exact Find root required"))?;
            if r.u64(root + 0x88)? != self.registry
                || r.byte(root + 0x95)? & 8 != 0
                || r.i32(root + 0x12b8)? != 0
                || r.i32(root + 0x1270)? != 0
            {
                return Err(changed(
                    "Find must remain hidden and idle before map placement",
                ));
            }
        }
        Ok(())
    }
}

pub(super) struct Anchor {
    identity: Identity,
    base: u64,
    state_id: i32,
    context: ClockContext,
    map: mac_map_point::Binding,
    geometry: macos::Geometry,
    source: source_point::Binding,
    world: [u32; 3],
    province: i32,
    pins: Vec<(u64, u64)>,
    ui: Ui,
    scene: Scene,
}
impl Anchor {
    pub(super) fn bind(p: &dyn Process, state_id: i32, expected: &ClockContext) -> Result<Self> {
        let identity = p
            .identity()
            .cloned()
            .ok_or_else(|| changed("Exact placement process required"))?;
        let base = p.image_base();
        p.begin_snapshot()?;
        let w = Witnesses::new(p);
        let r = Reader::new(&w);
        let context = game::context_from_reader(&r, base)?;
        scope(expected, &context)?;
        let map = mac_map_point::from_reader(&r, base, state_id, None)?;
        if map.mode() != 27 {
            return Err(changed(
                "Construction placement requires observed decimal map mode 27",
            ));
        }
        single_window(&r, base)?;
        let geometry = macos::from_reader(&r, base)?;
        geometry.require_profile()?;
        let pins = geometry_edges(&r, base, &geometry)?;
        edges(&r, &pins)?;
        let source = source_point::from_reader(&r, base, &geometry, None)?;
        let scene = Scene::bind(&w, base)?;
        w.finish()?;
        p.check()?;
        let ui = Ui::bind(p, base, &context)?;
        let mut a = Self {
            identity,
            base,
            state_id,
            context,
            map: map.binding().clone(),
            geometry,
            source: source.binding().clone(),
            world: map.world_point().map(f32::to_bits),
            province: map.province_id(),
            pins,
            ui,
            scene,
        };
        a.pins.extend_from_slice(&a.ui.edges);
        a.observe(p, true)?;
        Ok(a)
    }
    pub(super) fn observe(&self, p: &dyn Process, check_scene: bool) -> Result<[i32; 2]> {
        if p.identity() != Some(&self.identity) || p.image_base() != self.base {
            return Err(changed(
                "Original placement process/base changed before reads",
            ));
        }
        p.begin_snapshot()?;
        let w = Witnesses::new(p);
        let r = Reader::new(&w);
        edges(&r, &self.pins)?;
        let context = game::context_from_reader(&r, self.base)?;
        scope(&self.context, &context)?;
        let map = mac_map_point::from_reader(&r, self.base, self.state_id, Some(&self.map))?;
        if map.mode() != 27 || map.world_point().map(f32::to_bits) != self.world {
            return Err(changed("Original center terrain point changed"));
        }
        single_window(&r, self.base)?;
        // Geometry sees those exact previously-witnessed edges inside this
        // SAME bracket, so a late replacement refuses before new payload.
        let geometry = macos::from_reader(&r, self.base)?;
        geometry.require_profile()?;
        if !geometry.same_binding(&self.geometry) {
            return Err(changed("Original native mapping changed"));
        }
        self.ui.check(&r, self.base)?;
        let source = source_point::from_reader(&r, self.base, &geometry, Some(&self.source))?;
        if check_scene {
            self.scene.check(&r)?;
        }
        w.finish()?;
        p.check()?;
        if p.identity() != Some(&self.identity) || p.image_base() != self.base {
            return Err(changed("Original placement identity changed after witness"));
        }
        Ok(source.point())
    }
    pub(super) fn pinned<'a, 'b>(&'a self, fence: &'a Fence<'b>) -> Pinned<'a, 'b> {
        Pinned {
            fence,
            pins: &self.pins,
        }
    }
    /// Rebuild necessary bounds and separately retain current cached-route/global
    /// state. No engine call, full-picker or atomic-event claim. Every consumed
    /// field is witnessed under the SAME original action Fence before admission.
    pub(super) fn verify_route(&self, p: &dyn Process) -> Result<Value> {
        if p.identity() != Some(&self.identity) || p.image_base() != self.base {
            return Err(changed("Original route process/base changed"));
        }
        p.check()?;
        let before = game::clock_context(p)?;
        scope(&self.context, &before)?;
        p.begin_snapshot()?;
        let w = macos::direct_route::Witness::new(
            p,
            std::time::Instant::now() + std::time::Duration::from_secs(8),
        );
        let r = Reader::new(&w);
        edges(&r, &self.pins)?;
        let gui = self.geometry.objects[1];
        let gfx = self.geometry.objects[2];
        let observation = macos::direct_route::observe(&r, self.base, gui, gfx, gfx + 0x1e0)?;
        w.finish()?;
        let after = game::clock_context(p)?;
        scope(&before, &after)?;
        p.check()?;
        require_map_route(Some(&observation))?;
        Ok(observation.compact_summary())
    }
    pub(super) fn private_baseline(&self) -> Value {
        let map = format!("{:?}", self.map);
        let mut hash = Sha256::new();
        let mut bytes = 0;
        for (a, v) in &self.scene.rows {
            hash.update(a.to_le_bytes());
            hash.update((v.len() as u64).to_le_bytes());
            hash.update(v);
            bytes += v.len();
        }
        json!({"version":1,"center":[960,540],"state_id":self.state_id,"province_id":self.province,
            "world_bits":self.world,"mode":27,"original_geometry_objects":self.geometry.objects,
            "map_binding_sha256":format!("{:x}",Sha256::digest(map.as_bytes())),
            "source_binding_sha256":format!("{:x}",Sha256::digest(format!("{:?}",self.source).as_bytes())),
            "ui_edges_sha256":format!("{:x}",Sha256::digest(format!("{:?}",self.ui.edges).as_bytes())),
            "scene_sources":self.scene.binding.entries().len(),"scene_bytes":bytes,"scene_sha256":format!("{:x}",hash.finalize()),
            "full_picker_verified":false,"model_hit_verified":false,"event_authority":false,"modal_route_qualified":false})
    }
}

/// Read-only fixed-edge interception also covers MacKeys' internal geometry
/// readers. A changed edge is rejected BEFORE it is returned to any caller.
pub(super) struct Pinned<'a, 'b> {
    fence: &'a Fence<'b>,
    pins: &'a [(u64, u64)],
}
fn inspect(pins: &[(u64, u64)], address: u64, bytes: &[u8]) -> Result<()> {
    let end = address
        .checked_add(bytes.len() as u64)
        .ok_or_else(|| changed("Pinned read overflow"))?;
    for &(a, v) in pins {
        let lo = address.max(a);
        let hi = end.min(a + 8);
        if lo < hi
            && bytes[(lo - address) as usize..(hi - address) as usize]
                != v.to_le_bytes()[(lo - a) as usize..(hi - a) as usize]
        {
            return Err(changed(
                "Original input edge changed before replacement payload",
            ));
        }
    }
    Ok(())
}
impl ReadMemory for Pinned<'_, '_> {
    fn string_abi(&self) -> StringAbi {
        self.fence.string_abi()
    }
    fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
        let bytes = self.fence.read(a, n)?;
        inspect(self.pins, a, &bytes)?;
        Ok(bytes)
    }
}
impl Process for Pinned<'_, '_> {
    fn check(&self) -> Result<()> {
        self.fence.check()
    }
    fn begin_snapshot(&self) -> Result<()> {
        self.fence.begin_snapshot()
    }
    fn image_base(&self) -> u64 {
        self.fence.image_base()
    }
    fn identity(&self) -> Option<&Identity> {
        self.fence.identity()
    }
}
#[cfg(test)]
mod tests;
