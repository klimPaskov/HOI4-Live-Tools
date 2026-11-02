//! Dormant Linux-only diagnostics for the original retained Research root.
//! No Snapshot, input proof, executable route, polling or deadline renewal.
//! The independently decoded container updaters advance these fields; their
//! units, caller cadence and runtime cause remain unknown.
use super::*;

#[derive(Debug, PartialEq)]
pub(crate) struct AnimationSample {
    pub(crate) mode_raw: i32,
    pub(crate) literal_shown: bool,
    pub(crate) gui_flags_raw: u8,
    pub(crate) update_registration: Registration,
    pub(crate) scalar: Scalar,
    pub(crate) position: Position,
}
#[derive(Debug, PartialEq)]
pub(crate) struct Registration {
    pub(crate) entries: usize,
    pub(crate) original_root_matches: usize,
}
#[derive(Debug, PartialEq)]
pub(crate) struct Scalar {
    pub(crate) state: u8,
    pub(crate) effect_raw: i32,
    pub(crate) duration: f32,
    pub(crate) delay: f32,
    pub(crate) elapsed: f32,
    pub(crate) value: f32,
}
#[derive(Debug, PartialEq)]
pub(crate) struct Position {
    pub(crate) state: u8,
    pub(crate) effect_raw: i32,
    pub(crate) active_duration: f32,
    pub(crate) elapsed: f32,
    pub(crate) completed: bool,
    pub(crate) setter_flags: [bool; 2],
}
fn unknown() -> Error {
    Error::new(
        "research_animation_unknown",
        "Unknown own animation diagnostic value",
    )
}
fn state(bytes: &[u8]) -> Result<u8> {
    let value = i32::from_le_bytes(bytes.try_into().unwrap());
    if (0..=2).contains(&value) {
        Ok(value as u8)
    } else {
        Err(unknown())
    }
}
fn finite(bytes: &[u8]) -> Result<f32> {
    let value = f32::from_le_bytes(bytes.try_into().unwrap());
    if value.is_finite() {
        Ok(value)
    } else {
        Err(unknown())
    }
}
fn boolean(value: u8) -> Result<bool> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(unknown()),
    }
}
impl Anchor {
    pub(crate) fn animation_diagnostic(&self, process: &dyn Process) -> Result<AnimationSample> {
        self.animation_with_clock(process, &Instant::now)
    }
    pub(super) fn animation_with_clock(
        &self,
        process: &dyn Process,
        now: &dyn Fn() -> Instant,
    ) -> Result<AnimationSample> {
        boundary(process, &self.scope, Some(self.base), self.until, now)?;
        let timed = Timed {
            inner: process,
            until: self.until,
            now,
        };
        timed.begin_snapshot()?;
        let witnesses = Witnesses::new(&timed);
        let r = Reader::new(&witnesses);
        let base = self.base;
        let scope = &self.scope;
        crate::daily_observation::own_scope(&r, base, scope)?;
        // The canonical clock context also reads own Research. Keep all of
        // the retained model's original membership/owner-before-payload gates.
        members(&r, scope.own_status + 0xa0, 32, Some(&self.original.slots))?;
        members(
            &r,
            scope.anchors[2] + research::layout(&r).database_templates,
            8192,
            Some(&self.original.templates),
        )?;
        members(
            &r,
            scope.own_status + 0x88,
            8192,
            Some(&self.original.technologies),
        )?;
        model_links(&r, base, scope, &self.original)?;
        let context = crate::game::context_from_reader(&r, base)?;
        if context.anchors != scope.anchors
            || context.player != scope.epoch.player
            || context.difficulty != 2
            || context.historical_raw != 1
            || context.date_raw < self.last_date
            || !scope
                .epoch
                .campaign
                .ends_with(&format!(":{}", context.playthrough_id))
        {
            return Err(changed("Original animation campaign/setup changed"));
        }
        let idler = scope.anchors[1];
        let original = &self.original;
        let registry = link(&r, idler + 0x4f8, Some(original.registry))?;
        exact(&r, registry, base + 0x42736f8)?;
        if r.u64(base + 0x45778e0)? != registry {
            return Err(changed("Original animation input registry changed"));
        }
        let manager = link(&r, idler + 0x6b8, Some(original.manager))?;
        if r.u64(manager + 0x180)? != idler {
            return Err(changed("Original animation manager owner changed"));
        }
        let mode_raw = r.i32(manager + 0x3d8)?;
        if !matches!(mode_raw, 5 | 23) {
            return Err(refused(
                "Own animation diagnostic requires Idle23/Research5",
            ));
        }
        let view = link(&r, manager + 0xf0, Some(original.view))?;
        let tree = link(&r, manager + 0xf8, Some(original.tree))?;
        exact(&r, view, base + 0x424dc38)?;
        exact(&r, tree, base + 0x424de40)?;
        if r.u64(view + 0x2f0)? != idler
            || r.u64(tree + 0x2f0)? != idler
            || r.i32(view + 0x300)? != 0
            || r.i32(tree + 0xe24)? != 6
        {
            return Err(changed("Original animation views/tab changed"));
        }
        let displayed = r.i32(tree + 0xe20)?;
        if displayed != 0 && displayed != r.i32(scope.own_status + 0x138)? {
            return Err(changed("Foreign displayed animation country"));
        }
        let root = link(&r, view + 0x2f8, Some(original.roots[0]))?;
        exact(&r, root, base + 0x4347d58)?;
        if r.u64(root + 0x98)? != registry {
            return Err(changed("Original animation root registry changed"));
        }
        // CGameGui is a same-base CGui. Its constructor/destructor and the two
        // update loops independently bind this current pointer-only vector.
        // A matching secondary address is admission, never execution/cadence.
        let (header, data, count) = r.array_header(registry + 0x188)?;
        let capacity = i32::from_le_bytes(header[8..12].try_into().unwrap());
        if !(0..=4096).contains(&capacity) {
            return Err(refused("Own GUI update census exceeds diagnostic bound"));
        }
        let entries = if count == 0 {
            Vec::new()
        } else {
            pointer(data)?;
            r.read(data, count * 8)?
        };
        let mut original_root_matches = 0;
        for entry in entries.chunks_exact(8) {
            let entry = u64::from_le_bytes(entry.try_into().unwrap());
            pointer(entry)?;
            original_root_matches += usize::from(entry == root + 0x30);
        }
        let update_registration = Registration {
            entries: count,
            original_root_matches,
        };
        // Exact Linux object offsets from the separately retained static slice.
        // Beyond guarded canonical own context, no GUI child, foreign country
        // or event receiver is traversed.
        let gui_flags_raw = r.byte(root + 0xa5)?;
        let literal_shown = gui_flags_raw & 8 != 0;
        let scalar = r.read(root + 0xe58, 4)?;
        let fields = r.read(root + 0xeb0, 0x80)?;
        let sample = AnimationSample {
            mode_raw,
            literal_shown,
            gui_flags_raw,
            update_registration,
            scalar: Scalar {
                state: state(&fields[0..4])?,
                duration: finite(&fields[4..8])?,
                delay: finite(&fields[8..12])?,
                effect_raw: i32::from_le_bytes(fields[12..16].try_into().unwrap()),
                elapsed: finite(&fields[16..20])?,
                value: finite(&scalar)?,
            },
            position: Position {
                active_duration: finite(&fields[20..24])?,
                elapsed: finite(&fields[32..36])?,
                state: state(&fields[72..76])?,
                effect_raw: i32::from_le_bytes(fields[76..80].try_into().unwrap()),
                completed: boolean(fields[124])?,
                setter_flags: [boolean(fields[125])?, boolean(fields[126])?],
            },
        };
        witnesses.finish()?;
        timed.check()?;
        boundary(process, scope, Some(base), self.until, now)?;
        Ok(sample)
    }
}
