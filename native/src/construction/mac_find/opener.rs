//! Original-scope proof for one ordinary mouse attempt. No input, retries, deadline,
//! permission or journal live in this passive module.
use super::{retained, FindAnchor, RootLifecycle, View};
use crate::{
    construction::{mac_find_focus::Control, mac_ui::SelectionAnchor},
    error::{Error, Result},
    memory::Reader,
    platform::{Identity, Process},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
pub(super) mod button;
mod geometry;
fn changed(s: &str) -> Error {
    Error::new("construction_find_open_changed", s)
}
#[derive(Clone, Debug)]
pub(crate) struct ReadyProof {
    window: u64,
    control: Control,
}
#[derive(Clone, Debug)]
pub(crate) struct OpenAnchor {
    find: FindAnchor,
    selection: SelectionAnchor,
    state_id: i32,
    button: Option<button::Binding>,
    ready: Option<ReadyProof>,
}
pub(super) enum ReadGuard<'a> {
    Prepare {
        selection: &'a SelectionAnchor,
        button: Option<&'a button::Binding>,
        route: bool,
    },
    After {
        selection: &'a SelectionAnchor,
    },
}
impl ReadGuard<'_> {
    pub(super) fn context(&self) -> &crate::game::ClockContext {
        match self {
            Self::Prepare { selection, .. } | Self::After { selection } => selection.context(),
        }
    }
    pub(super) fn before_context(
        &self,
        r: &Reader<'_>,
        base: u64,
        identity: &Identity,
    ) -> Result<()> {
        let selection = match self {
            Self::Prepare { selection, .. } | Self::After { selection } => selection,
        };
        selection.require_original_edges(r, base, identity)?;
        if let Self::Prepare {
            button: Some(original),
            ..
        } = self
        {
            original.original_edges(r)?;
        }
        Ok(())
    }
    pub(super) fn control(
        &self,
        r: &Reader<'_>,
        base: u64,
        owners: [u64; 5],
        view: &View,
    ) -> Result<Option<button::Binding>> {
        let Self::Prepare { button, route, .. } = self else {
            return Ok(None);
        };
        if view.root.visible() {
            if button.is_some() {
                return Err(changed("Find opened before the original click attempt"));
            }
            ready(view)?.ok_or_else(|| {
                changed("Visible Find must already be idle, empty and exactly focused")
            })?;
            return Ok(None);
        }
        if !view.root.local_transitions_idle() {
            return Err(changed(
                "Ordinary click requires a materialized hidden idle Find",
            ));
        }
        button::observe(r, base, owners, *button, *route).map(Some)
    }
}
fn ready(view: &View) -> Result<Option<ReadyProof>> {
    if !view.root.visible() {
        return Ok(None);
    }
    if !matches!(&view.query,Some(retained::Query::Printable(s)) if s.is_empty()) {
        return Err(changed(
            "Find opening cannot adopt or clear an unexpected query",
        ));
    }
    if !view.root.local_transitions_idle() {
        return Ok(None);
    }
    let Some(focus) = view.focus.as_ref().filter(|f| f.focused()) else {
        return Ok(None);
    };
    let RootLifecycle::Materialized { pointer, .. } = view.root else {
        return Ok(None);
    };
    Ok(Some(ReadyProof {
        window: pointer,
        control: focus.control(),
    }))
}
impl OpenAnchor {
    pub(crate) fn bind(
        process: &dyn Process,
        selection: &SelectionAnchor,
        state_id: i32,
    ) -> Result<Self> {
        selection.require_inactive()?;
        let observed = retained::observe_opener(
            process,
            state_id,
            None,
            ReadGuard::Prepare {
                selection,
                button: None,
                route: false,
            },
            None,
        )?;
        let find = observed.anchor()?;
        if find.binding().own_state().is_none() {
            return Err(changed("Original owned Find state required"));
        }
        let reached = ready(observed.view())?;
        if reached.is_none() && observed.opening.is_none() {
            return Err(changed("Typed current click control required"));
        }
        Ok(Self {
            find,
            selection: selection.clone(),
            state_id,
            button: observed.opening,
            ready: reached,
        })
    }
    pub(crate) fn initially_ready(&self) -> bool {
        self.ready.is_some()
    }
    fn require_control(&self, process: &dyn Process, route: bool) -> Result<()> {
        let control = self
            .button
            .as_ref()
            .ok_or_else(|| changed("Originally ready Find must receive no click"))?;
        let observed = retained::observe_opener(
            process,
            self.state_id,
            Some(&self.find),
            ReadGuard::Prepare {
                selection: &self.selection,
                button: Some(control),
                route,
            },
            None,
        )?;
        if observed
            .opening
            .as_ref()
            .is_none_or(|now| !control.same_target(now))
        {
            return Err(changed("Original click binding disappeared before click"));
        }
        Ok(())
    }
    pub(crate) fn control_point(&self) -> Result<(f64, f64)> {
        self.button
            .as_ref()
            .map(button::Binding::point)
            .ok_or_else(|| changed("Originally ready Find must receive no click"))
    }
    pub(crate) fn require_before_move(&self, process: &dyn Process) -> Result<()> {
        self.require_control(process, false)
    }
    pub(crate) fn require_before_down(&self, process: &dyn Process) -> Result<()> {
        self.require_control(process, true)
    }
    pub(crate) fn observe_ready(&self, process: &dyn Process) -> Result<Option<ReadyProof>> {
        if let Some(proof) = &self.ready {
            proof.reobserve(process, self)?;
            return Ok(Some(proof.clone()));
        }
        let observed = retained::observe_opener(
            process,
            self.state_id,
            Some(&self.find),
            ReadGuard::After {
                selection: &self.selection,
            },
            None,
        )?;
        ready(observed.view())
    }
    pub(crate) fn private_baseline(&self) -> Result<Value> {
        let b = self.find.binding();
        let bytes = format!("{:?}", (&self.button, &self.ready)).into_bytes();
        let value = json!({"version":1,"selection":self.selection.private_baseline()?,"process":b.process(),"base":b.base(),"context":self.find.context(),"registry":b.registry(),"manager":b.manager(),"find":b.find(),"camera":b.camera(),"state_id":self.state_id,"initially_ready":self.initially_ready(),"control_bytes":bytes.len(),"control_sha256":format!("{:x}",Sha256::digest(&bytes))});
        if serde_json::to_vec(&value)
            .map_err(|e| changed(&e.to_string()))?
            .len()
            > 16 * 1024
        {
            return Err(changed(
                "Find opener private baseline exceeds record budget",
            ));
        }
        Ok(value)
    }
}
impl ReadyProof {
    pub(crate) fn reobserve(&self, process: &dyn Process, anchor: &OpenAnchor) -> Result<()> {
        let observed = retained::observe_opener(
            process,
            anchor.state_id,
            Some(&anchor.find),
            ReadGuard::After {
                selection: &anchor.selection,
            },
            Some(retained::ViewGuard::Typing {
                window: self.window,
                control: &self.control,
            }),
        )?;
        let now = ready(observed.view())?
            .ok_or_else(|| changed("Reached Find lost readiness after platform check"))?;
        if now.window != self.window || now.control != self.control {
            return Err(changed("Reached Find control replaced"));
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests;
