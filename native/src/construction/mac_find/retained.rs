//! Retained identity for a future same-attempt Find readback. Window lifetime,
//! query/results and camera coordinates are observations, never input authority.
use super::super::mac_find_focus::{Control, Focus};
use super::*;
use crate::{error::Error, game::ClockContext, memory::StringAbi, platform::Identity};
use serde_json::json;

/// Navigation pins a current root/control during typing. Closing permits only
/// that root or its teardown, and never follows the old edit-box payload.
pub(super) enum ViewGuard<'a> {
    Typing { window: u64, control: &'a Control },
    Closing { window: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OwnStateAnchor {
    pub(super) id: i32,
    pub(super) state: u64,
    pub(super) template: u64,
    pub(super) map_position: [i32; 2],
}

impl OwnStateAnchor {
    pub(crate) fn state_id(&self) -> i32 {
        self.id
    }
    pub(crate) fn state(&self) -> u64 {
        self.state
    }
    pub(crate) fn template(&self) -> u64 {
        self.template
    }
    pub(crate) fn map_position(&self) -> [i32; 2] {
        self.map_position
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum RequestedState {
    Foreign { id: i32, state: u64 },
    Own(OwnStateAnchor),
}

impl RequestedState {
    fn state_id(&self) -> i32 {
        match self {
            Self::Foreign { id, .. } => *id,
            Self::Own(state) => state.id,
        }
    }

    fn value(&self, look: [f32; 3]) -> Value {
        match self {
            Self::Foreign { id, .. } => json!({"state_id":id,"own_owned_and_controlled":false,
                "anchor":null,"camera_centered":false}),
            Self::Own(state) => json!({"state_id":state.id,"own_owned_and_controlled":true,
                "anchor":{"kind":"state_template_map_position","map_position":state.map_position},
                "camera_centered":look[0]==state.map_position[0] as f32
                    && look[2]==state.map_position[1] as f32,
                "comparison":"exact_template_position_without_wrap_adjustment"}),
        }
    }
}

/// This is the current root, not a promise that an earlier root survived. The
/// Find destructor/rebuilder may clear or replace it while retaining Find.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RootLifecycle {
    Unmaterialized,
    Materialized {
        pointer: u64,
        object: u64,
        flags: u8,
        transitions: [i32; 2],
    },
}

impl RootLifecycle {
    pub(crate) fn visible(&self) -> bool {
        matches!(self, Self::Materialized { flags, .. } if flags & 8 != 0)
    }
    pub(crate) fn local_transitions_idle(&self) -> bool {
        matches!(
            self,
            Self::Materialized {
                transitions: [0, 0],
                ..
            }
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Query {
    Printable(String),
    Unsupported { length: usize },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FirstResult {
    pub(super) kind: i32,
    pub(super) object: Option<u64>,
    pub(super) state_id: Option<i32>,
}

impl FirstResult {
    pub(crate) fn kind(&self) -> i32 {
        self.kind
    }
    pub(crate) fn state_id(&self) -> Option<i32> {
        self.state_id
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Results {
    pub(super) count: usize,
    pub(super) first: Option<FirstResult>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct View {
    pub(super) root: RootLifecycle,
    pub(super) query: Option<Query>,
    pub(super) results: Option<Results>,
    pub(super) focus_requested: bool,
    pub(super) focus: Option<super::super::mac_find_focus::Focus>,
}

impl View {
    fn value(&self) -> Value {
        let window = match self.root {
            RootLifecycle::Unmaterialized => Value::Null,
            RootLifecycle::Materialized { .. } => json!({"exact_container":true,
                "own_registry_verified":true,"visible_flag":self.root.visible(),
                "local_transitions_idle":self.root.local_transitions_idle(),
                "settled_visibility_verified":false}),
        };
        let query = match &self.query {
            None => Value::Null,
            Some(Query::Printable(text)) => json!(text),
            Some(Query::Unsupported { length }) => {
                json!({"unsupported":"non_printable_ascii","length":length})
            }
        };
        let results = self.results.as_ref().map(|results| {
            let first = results.first.as_ref().map(|first| {
                json!({"kind_raw":first.kind,"state_id":first.state_id,"supported":first.kind==2})
            });
            json!({"result_count":results.count,"first_row":first})
        });
        let mut value = json!({"materialized":!matches!(self.root,RootLifecycle::Unmaterialized),
            "window":window,"query":query,"text_box_focused":null,"results":results});
        if self.focus_requested {
            value["focus"] = self.focus.as_ref().map(|f| f.summary()).into();
            value["text_box_focused"] = self.focus.as_ref().map(|f| f.focused()).into();
        }
        value
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CameraState {
    pub(super) object: u64,
    pub(super) eye: [f32; 3],
    pub(super) look_at: [f32; 3],
}

impl CameraState {
    pub(crate) fn eye(&self) -> [f32; 3] {
        self.eye
    }
    pub(crate) fn look_at(&self) -> [f32; 3] {
        self.look_at
    }
}

/// Exact identities retained in memory only. No public JSON is parsed back
/// into this binding, and no window or current camera coordinates are pinned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FindBinding {
    process: Identity,
    base: u64,
    own_tag: i32,
    country: u64,
    production: u64,
    registry: u64,
    manager: u64,
    find: u64,
    camera: u64,
    requested: Option<RequestedState>,
}

impl FindBinding {
    pub(crate) fn process(&self) -> &Identity {
        &self.process
    }
    pub(crate) fn base(&self) -> u64 {
        self.base
    }
    pub(crate) fn own_tag(&self) -> i32 {
        self.own_tag
    }
    pub(crate) fn country(&self) -> u64 {
        self.country
    }
    pub(crate) fn production(&self) -> u64 {
        self.production
    }
    pub(crate) fn registry(&self) -> u64 {
        self.registry
    }
    pub(crate) fn manager(&self) -> u64 {
        self.manager
    }
    pub(crate) fn find(&self) -> u64 {
        self.find
    }
    pub(crate) fn camera(&self) -> u64 {
        self.camera
    }
    pub(crate) fn own_state(&self) -> Option<&OwnStateAnchor> {
        match &self.requested {
            Some(RequestedState::Own(state)) => Some(state),
            _ => None,
        }
    }
}

/// One finished witnessed observation. Root lifecycle and changing facts stay
/// separate from the original campaign/object identity used by FindAnchor.
#[derive(Clone, Debug)]
pub(crate) struct Observation {
    context: ClockContext,
    binding: FindBinding,
    view: View,
    camera: CameraState,
    read_calls: u64,
    read_bytes: usize,
    elapsed_ms: f64,
    pub(super) opening: Option<super::opener::button::Binding>,
    pub(super) characters: Option<super::characters::Binding>,
}

impl Observation {
    pub(crate) fn context(&self) -> &ClockContext {
        &self.context
    }
    pub(crate) fn binding(&self) -> &FindBinding {
        &self.binding
    }
    pub(super) fn view(&self) -> &View {
        &self.view
    }
    pub(crate) fn root(&self) -> &RootLifecycle {
        &self.view.root
    }
    pub(crate) fn camera(&self) -> &CameraState {
        &self.camera
    }
    pub(super) fn focus(&self) -> Option<&Focus> {
        self.view.focus.as_ref()
    }
    pub(crate) fn query(&self) -> Option<&str> {
        match &self.view.query {
            Some(Query::Printable(text)) => Some(text),
            _ => None,
        }
    }
    pub(crate) fn result_count(&self) -> Option<usize> {
        self.view.results.as_ref().map(|results| results.count)
    }
    pub(crate) fn first_result(&self) -> Option<&FirstResult> {
        self.view.results.as_ref()?.first.as_ref()
    }
    pub(crate) fn first_row_is_requested_state(&self) -> bool {
        match (self.first_result(), self.binding.own_state()) {
            (Some(first), Some(state)) => {
                first.kind == 2
                    && first.object == Some(state.state)
                    && first.state_id == Some(state.id)
            }
            _ => false,
        }
    }
    pub(crate) fn anchor(&self) -> Result<FindAnchor> {
        if matches!(self.binding.requested, Some(RequestedState::Foreign { .. })) {
            return Err(Error::new(
                "construction_target_unavailable",
                "Retained Find state must be owned and controlled by the current player",
            ));
        }
        Ok(FindAnchor {
            context: self.context.clone(),
            binding: self.binding.clone(),
        })
    }

    pub(super) fn value(&self) -> Result<Value> {
        Ok(
            json!({"source":"native own Mac construction find view","build_id":game::MACOS.build_id,
            "scope":"own_find_view_camera_and_requested_own_state","player":self.context.player,
            "date_raw":self.context.date_raw,"paused":self.context.paused,"find_view":self.view.value(),
            "camera":{"eye":self.camera.eye,"look_at":self.camera.look_at},
            "target":self.binding.requested.as_ref().map(|target|target.value(self.camera.look_at)),
            "input_authorized":false,"placement_eligibility_verified":false,
            "effective_routing_verified":false,"current_frame_verified":false,
            "live_validation":"pending","observation_paused_game":false,
            "freshness":{"observed_unix_ms":crate::daily_observation::unix_ms()?,
                "elapsed_ms":self.elapsed_ms,"read_calls_kind":"logical_read_requests",
                "read_calls":self.read_calls,"read_bytes":self.read_bytes}}),
        )
    }
}

#[derive(Clone, Debug)]
pub(crate) struct FindAnchor {
    context: ClockContext,
    binding: FindBinding,
}

impl FindAnchor {
    pub(crate) fn context(&self) -> &ClockContext {
        &self.context
    }
    pub(crate) fn binding(&self) -> &FindBinding {
        &self.binding
    }
    pub(crate) fn observe(&self, process: &dyn Process) -> Result<Observation> {
        observe_in(
            process,
            self.binding
                .requested
                .as_ref()
                .map(RequestedState::state_id),
            Some(self),
            false,
            None,
            None,
            None,
        )
    }
    pub(crate) fn observe_with_focus(&self, process: &dyn Process) -> Result<Observation> {
        observe_in(
            process,
            self.binding
                .requested
                .as_ref()
                .map(RequestedState::state_id),
            Some(self),
            true,
            None,
            None,
            None,
        )
    }
    pub(super) fn observe_guarded(
        &self,
        process: &dyn Process,
        guard: ViewGuard<'_>,
    ) -> Result<Observation> {
        let include_focus = matches!(guard, ViewGuard::Typing { .. });
        observe_in(
            process,
            self.binding
                .requested
                .as_ref()
                .map(RequestedState::state_id),
            Some(self),
            include_focus,
            Some(guard),
            None,
            None,
        )
    }
    pub(super) fn observe_characters(
        &self,
        process: &dyn Process,
        guard: ViewGuard<'_>,
        characters: &super::characters::Binding,
    ) -> Result<Observation> {
        observe_in(
            process,
            self.binding
                .requested
                .as_ref()
                .map(RequestedState::state_id),
            Some(self),
            true,
            Some(guard),
            None,
            Some(super::characters::ReadGuard::Original(characters)),
        )
    }
    pub(crate) fn require_same_scope(&self, observation: &Observation) -> Result<()> {
        same_context(&self.context, &observation.context)?;
        if self.binding != observation.binding {
            return Err(changed(
                "Original Find ownership or requested state identity changed",
            ));
        }
        Ok(())
    }
}

fn changed(message: &str) -> Error {
    Error::new("construction_state_changed", message)
}

fn same_context(before: &ClockContext, now: &ClockContext) -> Result<()> {
    if before.anchors != now.anchors
        || before.player != now.player
        || before.playthrough_id != now.playthrough_id
        || before.difficulty != now.difficulty
        || before.historical_raw != now.historical_raw
        || now.date_raw < before.date_raw
    {
        return Err(changed(
            "Find campaign, player, roots, setup or chronology changed",
        ));
    }
    Ok(())
}

fn own_entry(r: &Reader<'_>, header: u64, index: usize, stride: usize) -> Result<Vec<u8>> {
    let (bytes, data, _) = r.array_header(header)?;
    if i32::from_le_bytes(bytes[8..12].try_into().unwrap()) > 4096 || data % stride as u64 != 0 {
        return Err(changed(
            "Find own-country map exceeds capacity/alignment policy",
        ));
    }
    r.array_entry(header, index, stride)
}

/// Compare links before any replacement object payload. A repeated edge change
/// inside this same Witnesses bracket is a tear, never an adopted identity.
fn original_edges(r: &Reader<'_>, anchor: &FindAnchor) -> Result<()> {
    let b = &anchor.binding;
    let g = &game::MACOS;
    let roots = [
        r.u64(b.base + g.gs_global)?,
        r.u64(b.base + g.idler_global)?,
        r.u64(b.base + g.technology_db_global)?,
    ];
    if roots != anchor.context.anchors {
        return Err(changed("Find campaign roots changed before payload"));
    }
    let [gs, idler, _] = roots;
    mac_ui::exact(r, b.base, gs, g.gs_vtable)?;
    mac_ui::exact(r, b.base, idler, g.idler_vtable)?;
    let tag = r.i32(gs + g.player_tag)?;
    if tag != b.own_tag || !(1..4096).contains(&tag) {
        return Err(changed("Find player changed before own payload"));
    }
    let index = i32::from_le_bytes(
        own_entry(r, gs + g.tag_country_index, tag as usize, 4)?
            .try_into()
            .unwrap(),
    );
    if index < 0
        || u64::from_le_bytes(
            own_entry(r, gs + g.countries, index as usize, 8)?
                .try_into()
                .unwrap(),
        ) != b.country
    {
        return Err(changed("Find own-country mapping changed before payload"));
    }
    mac_ui::exact(r, b.base, b.country, g.country_vtable)?;
    if r.i32(b.country + 8)? != tag || r.u64(b.country + g.country_production)? != b.production {
        return Err(changed(
            "Find own country/production changed before payload",
        ));
    }
    mac_ui::exact(r, b.base, b.production, g.production_vtable)?;
    if r.u64(b.production + crate::game::layout(r).production(0x290))? != b.country {
        return Err(changed("Find production owner changed before payload"));
    }
    if r.u64(idler + 0x4c0)? != b.registry || r.u64(idler + 0x670)? != b.manager {
        return Err(changed("Find own interface changed before payload"));
    }
    mac_ui::exact(r, b.base, b.registry, 0x36a5810)?;
    if r.u64(b.manager + 0x180)? != idler {
        return Err(changed("Find manager owner changed before payload"));
    }
    if r.u64(b.manager + 0x190)? != b.find {
        return Err(changed("Original Find object changed before payload"));
    }
    mac_ui::exact(r, b.base, b.find, FIND)?;
    if r.u64(b.find + 0xd50)? != idler {
        return Err(changed("Find idler backlink changed before payload"));
    }
    if r.u64(idler + 0x718)? != b.camera {
        return Err(changed("Find camera object changed before payload"));
    }
    if let Some(state) = b.own_state() {
        if location::catalog_pointer(r, gs + super::super::layout(r).gs_states, state.id, 4096)?
            != state.state
        {
            return Err(changed(
                "Find requested state mapping changed before payload",
            ));
        }
        if !location::own_state(r, b.base, state.state, tag)? {
            return Err(Error::new(
                "construction_target_unavailable",
                "Original Find state lost own ownership/control",
            ));
        }
        if r.i32(state.state + super::super::layout(r).state_id)? != state.id
            || r.u64(state.state + 0x20)? != state.template
        {
            return Err(changed(
                "Find requested state/template changed before payload",
            ));
        }
        mac_ui::exact(r, b.base, state.template, STATE_TEMPLATE)?;
        if [
            r.i32(state.template + 0x120)?,
            r.i32(state.template + 0x124)?,
        ] != state.map_position
        {
            return Err(changed("Find requested state map anchor changed"));
        }
    }
    Ok(())
}

pub(crate) fn observe(process: &dyn Process, state_id: Option<i32>) -> Result<Observation> {
    observe_in(process, state_id, None, false, None, None, None)
}

pub(super) fn observe_with_focus(
    process: &dyn Process,
    state_id: Option<i32>,
) -> Result<Observation> {
    observe_in(process, state_id, None, true, None, None, None)
}

pub(super) fn observe_for_characters(process: &dyn Process, state_id: i32) -> Result<Observation> {
    observe_in(
        process,
        Some(state_id),
        None,
        true,
        None,
        None,
        Some(super::characters::ReadGuard::Bind),
    )
}

pub(super) fn observe_opener(
    process: &dyn Process,
    state_id: i32,
    expected: Option<&FindAnchor>,
    guard: super::opener::ReadGuard<'_>,
    view: Option<ViewGuard<'_>>,
) -> Result<Observation> {
    observe_in(
        process,
        Some(state_id),
        expected,
        true,
        view,
        Some(guard),
        None,
    )
}

fn observe_in(
    process: &dyn Process,
    state_id: Option<i32>,
    expected: Option<&FindAnchor>,
    include_focus: bool,
    view_guard: Option<ViewGuard<'_>>,
    opening_guard: Option<super::opener::ReadGuard<'_>>,
    character_guard: Option<super::characters::ReadGuard<'_>>,
) -> Result<Observation> {
    if let Some(id) = state_id {
        validate_state_id(i64::from(id))?;
    }
    if process.string_abi() != StringAbi::LibCxx {
        return Err(Error::new(
            "unsupported_platform",
            "Retained Find observations require the verified Mac layout",
        ));
    }
    let start = Instant::now();
    process.begin_snapshot()?;
    let identity = process
        .identity()
        .cloned()
        .ok_or_else(|| invalid("Owned Mac Find process required"))?;
    let base = process.image_base();
    if expected
        .is_some_and(|anchor| anchor.binding.process != identity || anchor.binding.base != base)
    {
        return Err(Error::new(
            "ownership_changed",
            "Original Find process or image changed",
        ));
    }
    let w = Witnesses::new(process);
    let r = Reader::new(&w);
    w.section("mac_find_open_original_edges");
    if let Some(guard) = &opening_guard {
        guard.before_context(&r, base, &identity)?;
    }
    w.section("mac_find_character_original_edges");
    if let Some(guard) = &character_guard {
        guard.before_context(&r, base)?;
    }
    w.section("mac_find_original_edges");
    if let Some(anchor) = expected {
        original_edges(&r, anchor)?;
    }
    w.section("mac_find_own_scope");
    let (country, production) = economy::own_production(&r, base)?;
    let own_tag = r.i32(country + 8)?;
    let context = game::context_from_reader(&r, base)?;
    if let Some(guard) = &opening_guard {
        same_context(guard.context(), &context)?;
    }
    if let Some(anchor) = expected {
        same_context(&anchor.context, &context)?;
    }
    let [gs, idler, _] = context.anchors;
    let registry = r.u64(idler + 0x4c0)?;
    mac_ui::exact(&r, base, registry, 0x36a5810)?;
    w.section("mac_find_characters");
    let characters = character_guard
        .as_ref()
        .map(|guard| guard.observe(&r, base, idler, registry))
        .transpose()?;
    let manager = r.u64(idler + 0x670)?;
    pointer(manager)?;
    if r.u64(manager + 0x180)? != idler {
        return Err(invalid("Mac Find manager must retain the own idler"));
    }
    w.section("mac_find_view");
    let find = r.u64(manager + 0x190)?;
    mac_ui::exact(&r, base, find, FIND)?;
    if r.u64(find + 0xd50)? != idler {
        return Err(invalid("Mac Find view must retain the own idler"));
    }
    if let Some(guard) = view_guard {
        w.section("mac_find_original_control");
        if r.u64(base + 0x37eda08)? != registry {
            return Err(changed(
                "Original Find GUI singleton changed before payload",
            ));
        }
        let current = r.u64(find + 0xd58)?;
        match guard {
            ViewGuard::Typing { window, control } => {
                if current != window {
                    return Err(changed("Original Find window changed during typing"));
                }
                control.original_edges(&r, base, find, registry)?;
            }
            ViewGuard::Closing { window } => {
                if current != 0 && current != window {
                    return Err(changed("Original Find window was replaced after Return"));
                }
            }
        }
    }
    let mut view = view(&r, base, gs, registry, find)?;
    if include_focus {
        w.section("mac_find_focus");
        view.focus_requested = true;
        let root = match view.root {
            RootLifecycle::Unmaterialized => 0,
            RootLifecycle::Materialized { object, .. } => object,
        };
        view.focus = super::super::mac_find_focus::observe(&r, base, find, root, registry)?;
    }
    w.section("mac_find_open_control");
    let opening = opening_guard
        .as_ref()
        .map(|guard| {
            guard.control(
                &r,
                base,
                [
                    context.anchors[0],
                    context.anchors[1],
                    registry,
                    manager,
                    find,
                ],
                &view,
            )
        })
        .transpose()?
        .flatten();
    w.section("mac_find_camera");
    let camera = camera(&r, idler)?;
    let requested = state_id
        .map(|id| target(&r, base, gs, own_tag, id))
        .transpose()?;
    let binding = FindBinding {
        process: identity,
        base,
        own_tag,
        country,
        production,
        registry,
        manager,
        find,
        camera: camera.object,
        requested,
    };
    let (calls, bytes) = w.finish()?;
    process.check()?;
    if process.identity() != Some(&binding.process) || process.image_base() != binding.base {
        return Err(invalid("Mac Find process identity changed"));
    }
    let observation = Observation {
        context,
        binding,
        view,
        camera,
        read_calls: r.calls.get() + calls as u64,
        read_bytes: r.bytes.get() + bytes,
        elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
        opening,
        characters,
    };
    if let Some(anchor) = expected {
        anchor.require_same_scope(&observation)?;
    }
    Ok(observation)
}
