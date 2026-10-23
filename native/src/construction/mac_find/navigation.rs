//! Passive original-scope proofs for decimal Find typing and Return readback.
//! There is no input, retry, loop, deadline or journal execution here. A proof
//! describes one finished witnessed observation; the adapter must read freshly
//! under its original Fence immediately before every ordinary key attempt.
use super::{retained, FindAnchor, Observation, RootLifecycle};
use crate::{
    construction::mac_find_focus::Control,
    error::{Error, Result},
    platform::Process,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug)]
pub(crate) struct NavigationAnchor {
    find: FindAnchor,
    window: u64,
    control: Control,
    decimal: String,
}

#[derive(Clone, Debug)]
pub(crate) struct PrefixProof {
    observation: Observation,
    expected: String,
    next: Option<char>,
}

impl PrefixProof {
    pub(crate) fn expected_prefix(&self) -> &str {
        &self.expected
    }
    pub(crate) fn next_digit(&self) -> Option<char> {
        self.next
    }
    pub(crate) fn observation(&self) -> &Observation {
        &self.observation
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ReturnProof {
    observation: Observation,
}
impl ReturnProof {
    pub(crate) fn observation(&self) -> &Observation {
        &self.observation
    }
}

#[derive(Clone, Debug)]
pub(crate) struct FinishedProof {
    observation: Observation,
}
impl FinishedProof {
    pub(crate) fn observation(&self) -> &Observation {
        &self.observation
    }
}

fn unavailable(message: &str) -> Error {
    Error::new("construction_find_unavailable", message)
}

fn query_changed() -> Error {
    Error::new(
        "construction_state_changed",
        "Find query differs from the exact expected decimal prefix",
    )
}

fn typing_control(observation: &Observation) -> Result<Control> {
    if !observation.root().visible() || !observation.root().local_transitions_idle() {
        return Err(unavailable(
            "Find must remain visible with idle local transitions during typing",
        ));
    }
    let focus = observation
        .focus()
        .ok_or_else(|| unavailable("Exact named Find edit control required"))?;
    if !focus.focused() {
        return Err(unavailable(
            "Both original Find keyboard and text receiver aliases must match",
        ));
    }
    Ok(focus.control())
}

/// Binding starts with an already-visible, idle and focused Find. It never
/// opens, focuses, clears or changes the query, and accepts only an own target.
pub(crate) fn bind(process: &dyn Process, state_id: i32) -> Result<NavigationAnchor> {
    let observation = retained::observe_with_focus(process, Some(state_id))?;
    let find = observation.anchor()?;
    if find.binding().own_state().is_none() {
        return Err(unavailable("Original owned Find state anchor required"));
    }
    let control = typing_control(&observation)?;
    let window = match observation.root() {
        RootLifecycle::Materialized { pointer, .. } => *pointer,
        RootLifecycle::Unmaterialized => {
            return Err(unavailable("Already-materialized Find required"))
        }
    };
    if observation.query() != Some("") {
        return Err(unavailable(
            "Find navigation must start with an exactly empty printable query",
        ));
    }
    Ok(NavigationAnchor {
        find,
        window,
        control,
        decimal: state_id.to_string(),
    })
}

impl NavigationAnchor {
    pub(crate) fn decimal_query(&self) -> &str {
        &self.decimal
    }
    pub(crate) fn find_anchor(&self) -> &FindAnchor {
        &self.find
    }

    fn typing_observation(&self, process: &dyn Process) -> Result<Observation> {
        let observation = self.find.observe_guarded(
            process,
            retained::ViewGuard::Typing {
                window: self.window,
                control: &self.control,
            },
        )?;
        if typing_control(&observation)? != self.control {
            return Err(Error::new(
                "construction_state_changed",
                "Original Find control membership changed",
            ));
        }
        Ok(observation)
    }

    /// Some is exact expected-prefix evidence. Only one ordinary digit behind
    /// is pending while the just-posted digit settles; other coherent edits
    /// refuse instead of adopting a new typing position. No key is retried.
    pub(crate) fn observe_prefix(
        &self,
        process: &dyn Process,
        expected_prefix: &str,
    ) -> Result<Option<PrefixProof>> {
        if !self.decimal.starts_with(expected_prefix) {
            return Err(Error::new(
                "invalid_arguments",
                "Expected Find prefix must be a prefix of the original decimal state ID",
            ));
        }
        let observation = self.typing_observation(process)?;
        let query = observation.query().ok_or_else(query_changed)?;
        if query != expected_prefix {
            if !expected_prefix.is_empty() && query == &expected_prefix[..expected_prefix.len() - 1]
            {
                return Ok(None);
            }
            return Err(query_changed());
        }
        let next = self
            .decimal
            .as_bytes()
            .get(expected_prefix.len())
            .map(|b| char::from(*b));
        Ok(Some(PrefixProof {
            observation,
            expected: expected_prefix.to_owned(),
            next,
        }))
    }

    /// Query must already be complete and exact. Only result refresh is
    /// pending: Return has no proof until the first typed row joins the exact
    /// original owned state pointer AND ID under the same observation.
    pub(crate) fn observe_return(&self, process: &dyn Process) -> Result<Option<ReturnProof>> {
        let observation = self.typing_observation(process)?;
        if observation.query() != Some(self.decimal.as_str()) {
            return Err(query_changed());
        }
        if !observation.first_row_is_requested_state() {
            return Ok(None);
        }
        Ok(Some(ReturnProof { observation }))
    }

    /// Pending until original Find is locally closed and the current camera
    /// has the original map pair. A torn/foreign/replaced scope is an error.
    /// Hidden/teardown observations deliberately never read the old edit box.
    pub(crate) fn observe_finished(&self, process: &dyn Process) -> Result<Option<FinishedProof>> {
        let observation = self.find.observe_guarded(
            process,
            retained::ViewGuard::Closing {
                window: self.window,
            },
        )?;
        let closed = match observation.root() {
            RootLifecycle::Unmaterialized => true,
            RootLifecycle::Materialized { .. } => {
                !observation.root().visible() && observation.root().local_transitions_idle()
            }
        };
        let target = self
            .find
            .binding()
            .own_state()
            .ok_or_else(|| unavailable("Original owned Find state required"))?;
        let [x, z] = target.map_position();
        let look = observation.camera().look_at();
        if !closed || look[0] != x as f32 || look[2] != z as f32 {
            return Ok(None);
        }
        Ok(Some(FinishedProof { observation }))
    }

    /// Compact private journal evidence only. Original typed identities and
    /// ordered membership remain in memory; this JSON is never proof input.
    pub(crate) fn private_baseline(&self) -> Result<Value> {
        let binding = self.find.binding();
        let state = binding
            .own_state()
            .ok_or_else(|| unavailable("Original owned Find state required"))?;
        let members =
            serde_json::to_vec(self.control.children()).map_err(|e| unavailable(&e.to_string()))?;
        let value = json!({"version":1,"game":binding.process(),"base":binding.base(),
            "context":self.find.context(),"own_tag":binding.own_tag(),
            "country":binding.country(),"production":binding.production(),
            "registry":binding.registry(),"manager":binding.manager(),"find":binding.find(),
            "camera":binding.camera(),"window_pointer":self.window,"root":self.control.root(),
            "edit_box":self.control.edit_box(),"text_receiver":self.control.edit_box()+0x180,
            "state":state.state(),"state_id":state.state_id(),"template":state.template(),
            "map_position":state.map_position(),"decimal_query":self.decimal,
            "initial_query":"","initial_both_focus_aliases_match":true,
            "control_find":self.control.find(),"control_registry":self.control.registry(),
            "member_path_count":self.control.children().len(),
            "member_count":self.control.children().iter().map(|(_,members)|members.len()).sum::<usize>(),
            "membership_sha256":format!("{:x}",Sha256::digest(members)),
            "input_authorized":false,"delivery_verified":false});
        if serde_json::to_vec(&value)
            .map_err(|e| unavailable(&e.to_string()))?
            .len()
            > 8192
        {
            return Err(unavailable("Find private baseline exceeds record budget"));
        }
        Ok(value)
    }
}

#[cfg(test)]
#[path = "../../../tests/support/mac_construction_find_navigation.rs"]
mod tests;
