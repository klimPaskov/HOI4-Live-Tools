//! Typed production targets; untagged factory variants preserve v1 journals.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum Target {
    Factories(FactoryTarget),
    Panel(PanelTarget),
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PanelStage {
    #[serde(rename = "ensure_open")]
    EnsureOpen,
    #[serde(rename = "ensure_closed")]
    EnsureClosed,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PanelTarget {
    pub epoch: Epoch,
    pub stage: PanelStage,
}
impl PanelStage {
    pub fn name(self) -> &'static str {
        match self {
            Self::EnsureOpen => "ensure_open",
            Self::EnsureClosed => "ensure_closed",
        }
    }
}
impl PanelTarget {
    pub fn closed(epoch: Epoch) -> Self {
        Self {
            epoch,
            stage: PanelStage::EnsureClosed,
        }
    }
    pub fn new(epoch: Epoch) -> Self {
        Self {
            epoch,
            stage: PanelStage::EnsureOpen,
        }
    }
}
impl Target {
    pub fn epoch(&self) -> &Epoch {
        match self {
            Self::Factories(t) => &t.epoch,
            Self::Panel(t) => &t.epoch,
        }
    }
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Factories(t) => t.validate().map(|_| ()),
            Self::Panel(_) => Ok(()),
        }
    }
    pub fn factory(&self) -> Result<&FactoryTarget> {
        match self {
            Self::Factories(t) => Ok(t),
            _ => Err(error(
                "invalid_arguments",
                "Factory target required for this adapter",
            )),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum State {
    Factories(FactoryState),
    Panel(PanelState),
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PanelState {
    pub epoch: Epoch,
    /// Exact own country, production status, idler, registry, manager, view, root.
    /// Private journal only; never serialized in public execution receipts.
    pub panel_identity: Vec<u64>,
    pub managed_mode: i32,
    /// Literal local shown bit with both transition channels positively zero.
    pub panel_shown: bool,
}
impl State {
    pub fn factory(&self) -> Result<&FactoryState> {
        match self {
            Self::Factories(s) => Ok(s),
            _ => Err(error(
                "invalid_arguments",
                "Factory state required for this adapter",
            )),
        }
    }
    pub fn validate(&self, target: &Target) -> Result<()> {
        target.validate()?;
        match (self, target) {
            (Self::Factories(s), Target::Factories(t)) => s.validate(t),
            (Self::Panel(s), Target::Panel(t)) => {
                if s.epoch != t.epoch
                    || !(0..=23).contains(&s.managed_mode)
                    || s.panel_identity.len() != 7
                    || s.panel_identity
                        .iter()
                        .any(|p| !(0x10000..=0x7fff_ffff_0000).contains(p) || p % 8 != 0)
                    || s.panel_identity
                        .iter()
                        .collect::<std::collections::BTreeSet<_>>()
                        .len()
                        != 7
                {
                    return Err(error(
                        "ownership_changed",
                        "Exact bounded own production panel identity required",
                    ));
                }
                Ok(())
            }
            _ => Err(error(
                "ownership_changed",
                "Production state and target kinds differ",
            )),
        }
    }
    pub(super) fn step(&self, target: &Target) -> Result<()> {
        self.validate(target)?;
        match (self, target) {
            (Self::Factories(s), Target::Factories(t)) => s.step(t),
            (Self::Panel(s), Target::Panel(t)) if match t.stage {
                PanelStage::EnsureOpen => s.managed_mode == 23 && !s.panel_shown,
                PanelStage::EnsureClosed => s.managed_mode == 2 && s.panel_shown,
            } => Ok(()),
            _ => Err(error(
                "production_precondition",
                "Panel step requires settled idle mode23 for opening or shown production mode2 for closing",
            )),
        }
    }
    pub(super) fn reached(&self, target: &Target) -> bool {
        match (self, target) {
            (Self::Factories(s), Target::Factories(t)) => s.requested == t.factories,
            (Self::Panel(s), Target::Panel(t)) => match t.stage {
                PanelStage::EnsureOpen => s.managed_mode == 2 && s.panel_shown,
                PanelStage::EnsureClosed => s.managed_mode == 23 && !s.panel_shown,
            },
            _ => false,
        }
    }
    pub(super) fn same_identity(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Factories(a), Self::Factories(b)) => a.epoch == b.epoch && a.line == b.line,
            (Self::Panel(a), Self::Panel(b)) => {
                a.epoch == b.epoch && a.panel_identity == b.panel_identity
            }
            _ => false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FactoryTarget {
    pub epoch: Epoch,
    pub line_handle: String,
    pub equipment_id: String,
    pub factories: i32,
}
impl FactoryTarget {
    pub fn validate(&self) -> Result<u64> {
        if !(1..=1_000_000).contains(&self.factories) {
            return Err(error(
                "invalid_arguments",
                "One-factory controls require target allocation 1..1000000",
            ));
        }
        crate::production_lines::control_snapshot::validate_target(
            &self.line_handle,
            &self.equipment_id,
        )
    }
}
/// Private persisted own-line identity. Public receipts omit these addresses.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LineIdentity {
    pub country: u64,
    pub production: u64,
    pub line: u64,
    pub variant: u64,
    pub base_type: u64,
    pub handle: u64,
    pub equipment_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FactoryState {
    pub epoch: Epoch,
    pub line: LineIdentity,
    pub requested: i32,
}
impl FactoryState {
    pub fn validate(&self, target: &FactoryTarget) -> Result<()> {
        let handle = target.validate()?;
        if self.epoch != target.epoch
            || self.line.handle != handle
            || self.line.equipment_id != target.equipment_id
            || [
                self.line.country,
                self.line.production,
                self.line.line,
                self.line.variant,
                self.line.base_type,
            ]
            .iter()
            .any(|p| !(0x10000..=0x7fff_ffff_0000).contains(p) || p % 8 != 0)
            || !(0..=1_000_000).contains(&self.requested)
        {
            return Err(error(
                "ownership_changed",
                "Exact bounded own production identity required",
            ));
        }
        Ok(())
    }
    fn step(&self, target: &FactoryTarget) -> Result<()> {
        self.validate(target)?;
        if (i64::from(self.requested) - i64::from(target.factories)).abs() != 1 {
            return Err(error(
                "production_step_required",
                "Request exactly one factory step from the fresh allocation",
            ));
        }
        Ok(())
    }
}
