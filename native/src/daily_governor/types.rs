use crate::{
    actions::Epoch,
    calendar,
    error::{Error, Result},
};
use serde::{Deserialize, Serialize};

pub(super) fn invalid(message: &str) -> Error {
    Error::new("invalid_plan", message)
}
pub(super) fn key(s: &str) -> Result<()> {
    if s.is_empty()
        || s.len() > 128
        || !s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(invalid("Bounded explicit ID required"));
    }
    Ok(())
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    pub epoch: Epoch,
    pub anchors: [u64; 3],
    pub own_country: u64,
    pub own_status: u64,
}
impl Scope {
    pub(super) fn validate(&self) -> Result<()> {
        let e = &self.epoch;
        if e.game.pid <= 0
            || e.game.start_ticks.is_empty()
            || e.game.start_ticks.len() > 32
            || !e.game.start_ticks.bytes().all(|b| b.is_ascii_digit())
            || e.game.exe.is_empty()
            || e.game.exe.len() > 4096
            || e.campaign.is_empty()
            || e.campaign.len() > 128
        {
            return Err(invalid("Exact process/campaign identity required"));
        }
        key(&e.player)?;
        if self
            .anchors
            .iter()
            .chain([&self.own_country, &self.own_status])
            .any(|p| !(0x10000..=0x7fff_ffff_0000).contains(p))
        {
            return Err(invalid("Verified own context anchors required"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum EconomyTarget {
    Construction {
        building: String,
        state_id: u32,
        total_levels: u16,
    },
    Production {
        line_id: String,
        equipment_id: String,
        total_factories: u16,
    },
    ShortageRepair {
        equipment_id: String,
        line_id: String,
        maximum_factory_moves: u16,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum Intent {
    Research {
        technology_id: String,
        slot: u32,
    },
    Focus {
        focus_id: String,
    },
    Economy {
        target: EconomyTarget,
    },
    Army {
        division_ids: Vec<u32>,
        army_id: u32,
        order_id: String,
    },
    NavalInvasion {
        division_ids: Vec<u32>,
        army_id: u32,
        source_province: u32,
        destination_province: u32,
        sea_region_ids: Vec<u32>,
    },
}
fn ids(values: &[u32], max: usize) -> Result<()> {
    let set: std::collections::BTreeSet<_> = values.iter().collect();
    if values.is_empty() || values.len() > max || values.contains(&0) || set.len() != values.len() {
        return Err(invalid("Bounded unique positive own IDs required"));
    }
    Ok(())
}
impl Intent {
    pub(super) fn validate(&self) -> Result<()> {
        match self {
            Self::Research {
                technology_id,
                slot,
            } => {
                key(technology_id)?;
                if *slot > 31 {
                    return Err(invalid("Own research slot out of bounds"));
                }
            }
            Self::Focus { focus_id } => key(focus_id)?,
            Self::Economy { target } => match target {
                EconomyTarget::Construction {
                    building,
                    state_id,
                    total_levels,
                } => {
                    key(building)?;
                    if *state_id == 0 || !(1..=200).contains(total_levels) {
                        return Err(invalid("Explicit bounded construction target required"));
                    }
                }
                EconomyTarget::Production {
                    line_id,
                    equipment_id,
                    total_factories,
                } => {
                    key(line_id)?;
                    key(equipment_id)?;
                    if !(1..=200).contains(total_factories) {
                        return Err(invalid("Bounded factory target required"));
                    }
                }
                EconomyTarget::ShortageRepair {
                    equipment_id,
                    line_id,
                    maximum_factory_moves,
                } => {
                    key(line_id)?;
                    key(equipment_id)?;
                    if !(1..=200).contains(maximum_factory_moves) {
                        return Err(invalid("Bounded permitted repair required"));
                    }
                }
            },
            Self::Army {
                division_ids,
                army_id,
                order_id,
            } => {
                ids(division_ids, 256)?;
                key(order_id)?;
                if *army_id == 0 {
                    return Err(invalid("Explicit own army required"));
                }
            }
            Self::NavalInvasion {
                division_ids,
                army_id,
                source_province,
                destination_province,
                sea_region_ids,
            } => {
                ids(division_ids, 256)?;
                ids(sea_region_ids, 64)?;
                if *army_id == 0
                    || *source_province == 0
                    || *destination_province == 0
                    || source_province == destination_province
                {
                    return Err(invalid("Explicit invasion route required"));
                }
            }
        }
        Ok(())
    }
    pub fn capability(&self) -> &'static str {
        match self {
            Self::Research { .. } => "ordinary_research",
            Self::Focus { .. } => "ordinary_focus",
            Self::Economy { .. } => "ordinary_economy",
            Self::Army { .. } => "ordinary_army",
            Self::NavalInvasion { .. } => "ordinary_naval_invasion",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResearchNavigation {
    pub folder_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub id: String,
    pub intent: Intent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub navigation: Option<ResearchNavigation>,
}
impl Entry {
    pub(crate) fn validate(&self) -> Result<()> {
        key(&self.id)?;
        self.intent.validate()?;
        if let Some(nav) = &self.navigation {
            key(&nav.folder_id)?;
            if !matches!(self.intent, Intent::Research { .. }) {
                return Err(invalid("Navigation hints belong only to Research"));
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub version: u8,
    pub revision: u64,
    pub scope: Scope,
    pub entries: Vec<Entry>,
    pub max_actions_per_day: u8,
    pub max_observation_age_ms: u64,
    pub review_day: Option<i64>,
}
impl Plan {
    pub(super) fn validate(&self) -> Result<()> {
        self.scope.validate()?;
        if self.version != 1
            || self.revision == 0
            || self.entries.is_empty()
            || self.entries.len() > 256
            || !(1..=16).contains(&self.max_actions_per_day)
            || !(1..=5000).contains(&self.max_observation_age_ms)
            || self
                .review_day
                .is_some_and(|d| !(1936 * 365..=2001 * 365).contains(&d))
        {
            return Err(invalid("Plan version/bounds invalid"));
        }
        let mut names = std::collections::BTreeSet::new();
        let mut intents = std::collections::BTreeSet::new();
        let mut research_keys = std::collections::BTreeSet::new();
        for e in &self.entries {
            e.validate()?;
            if let Intent::Research { technology_id, .. } = &e.intent {
                if !research_keys.insert(technology_id) {
                    return Err(invalid("Duplicate research technology in refill queue"));
                }
            }
            if !names.insert(&e.id)
                || !intents.insert(
                    serde_json::to_vec(&e.intent).map_err(|_| invalid("Intent encoding failed"))?,
                )
            {
                return Err(invalid("Duplicate entry ID or intent"));
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OwnCounts {
    pub idle_research_slots: Option<u32>,
    pub idle_civilian_factories: Option<u32>,
    pub idle_military_factories: Option<u32>,
    pub scoped_equipment_deficit: Option<u32>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub scope: Scope,
    pub date_raw: i32,
    pub sequence: u64,
    pub observed_unix_ms: u64,
    pub source: String,
    pub counts: OwnCounts,
}
impl Observation {
    pub(super) fn validate(&self, plan: &Plan, now_ms: u64) -> Result<i64> {
        if self.scope != plan.scope {
            return Err(Error::new(
                "ownership_changed",
                "Daily own process/campaign/context changed",
            ));
        }
        calendar::decode(self.date_raw)?;
        if self.sequence == 0
            || self.source.is_empty()
            || self.source.len() > 96
            || self.source.chars().any(char::is_control)
            || self.observed_unix_ms > now_ms
            || now_ms - self.observed_unix_ms > plan.max_observation_age_ms
        {
            return Err(Error::new(
                "stale_observation",
                "Fresh timestamp/source/sequence required",
            ));
        }
        Ok((i64::from(self.date_raw) - 43_800_000).div_euclid(24))
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Attempt {
    pub request_id: String,
    pub entry: Entry,
    pub scope: Scope,
    pub day: i64,
    pub before_sequence: u64,
}
/// Trusted adapter result, not arbitrary MCP JSON. Adapters must translate only
/// a CURRENT verified final receipt for this exact attempt, after ordinary
/// transport and canonical own-state postcondition checks. Historical replay
/// receipts cannot be translated as `current = true`.
#[derive(Clone, Debug)]
pub struct VerifiedReceipt {
    pub(crate) attempt: Attempt,
    pub(crate) current: bool,
    pub(crate) canonical_intent: Intent,
    pub(crate) observed: Observation,
}
impl VerifiedReceipt {
    /// Narrow conversion of the existing ordinary Research stage. View/slot/
    /// panel/category selection is NOT research assignment completion. Other
    /// domains need independently reviewed owner-only converters.
    pub fn from_research(
        attempt: Attempt,
        execution: &crate::research_stage::Execution,
        observed: Observation,
    ) -> Result<Self> {
        use crate::research_stage::{Stage, Status};
        let Intent::Research {
            technology_id,
            slot,
        } = &attempt.entry.intent
        else {
            return Err(Error::new(
                "receipt_mismatch",
                "Research receipt cannot complete another domain",
            ));
        };
        let r = &execution.receipt;
        r.observed.validate(&r.target)?;
        if r.version != 1
            || r.status != Status::Verified
            || execution.historical
            || !execution.current_state_verified
            || r.request_id != attempt.request_id
            || r.target.epoch != attempt.scope.epoch
            || r.target.stage != Stage::Research
            || r.target.technology != *technology_id
            || r.target.slot != *slot
            || r.target.folder_id.is_some()
            || r.observed
                .researching
                .get(*slot as usize)
                .and_then(Option::as_ref)
                != Some(technology_id)
            || observed.scope != attempt.scope
            || observed.sequence <= attempt.before_sequence
        {
            return Err(Error::new(
                "receipt_mismatch",
                "Current exact Research assignment receipt required",
            ));
        }
        Ok(Self {
            canonical_intent: attempt.entry.intent.clone(),
            attempt,
            current: true,
            observed,
        })
    }
    /// Only a fresh exact Start receipt can complete a Focus assignment.
    /// Preparation stages and historical replay never complete an entry.
    pub fn from_focus(
        attempt: Attempt,
        execution: &crate::focus_stage::Execution,
        before: &crate::own_focus::FocusContext,
        context: &crate::own_focus::FocusContext,
        observed: Observation,
    ) -> Result<Self> {
        use crate::focus_stage::{ActiveFocus, Stage, Status};
        let Intent::Focus { focus_id } = &attempt.entry.intent else {
            return Err(Error::new(
                "receipt_mismatch",
                "Focus receipt cannot complete another domain",
            ));
        };
        let r = &execution.receipt;
        r.observed.validate(&r.target)?;
        if r.version != 1
            || r.status != Status::Verified
            || execution.historical
            || !execution.current_state_verified
            || r.request_id != attempt.request_id
            || r.target.epoch != attempt.scope.epoch
            || r.target.stage != Stage::Start
            || r.delivery == Some(crate::focus_stage::Delivery::NotQueued)
            || r.target.focus_id != *focus_id
            || !matches!(&r.observed.active_focus,ActiveFocus::Active(id) if id==focus_id)
            || observed.scope != attempt.scope
            || observed.sequence <= attempt.before_sequence
            || context.campaign.player != attempt.scope.epoch.player
            || context.campaign.anchors != attempt.scope.anchors
            || context.campaign.date_raw > observed.date_raw
            || context.anchors != before.anchors
            || context.slots != before.slots
            || context.slots.is_empty()
            || context.slots.len() > 32
            || context
                .slots
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != context.slots.len()
            || context.campaign.player != before.campaign.player
            || context.campaign.playthrough_id != before.campaign.playthrough_id
            || context.campaign.anchors != before.campaign.anchors
            || context.campaign.difficulty != before.campaign.difficulty
            || context.campaign.historical_raw != before.campaign.historical_raw
            || context.campaign.difficulty != 2
            || context.campaign.historical_raw != 1
            || context.campaign.date_raw < before.campaign.date_raw
            || context.anchors[0] != attempt.scope.own_country
            || context.anchors[6] != attempt.scope.own_status
            || context.anchors.contains(&0)
            || context.active_focus == 0
            || context.active_key.as_deref() != Some(focus_id.as_str())
        {
            return Err(Error::new(
                "receipt_mismatch",
                "Current exact Focus Start assignment receipt required",
            ));
        }
        Ok(Self {
            canonical_intent: attempt.entry.intent.clone(),
            attempt,
            current: true,
            observed,
        })
    }
}
#[derive(Clone, Debug)]
pub enum Outcome {
    Verified(Box<VerifiedReceipt>),
    NotQueued(String),
    Uncertain(String),
}
#[derive(Clone, Debug)]
pub enum Readiness {
    Ready,
    /// Fresh own slot is occupied: retain head without a day-long refusal.
    WaitingForResearchSlot,
    Deferred(String),
    Unavailable(String),
    BlockingDecision(String),
}
