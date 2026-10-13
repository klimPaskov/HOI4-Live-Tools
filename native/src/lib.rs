pub mod actions;
pub mod calendar;
pub mod error;
pub mod framing;
pub mod game;
pub mod gui;
pub mod input_lease;
pub mod launch;
pub mod memory;
pub mod own_focus;
pub mod platform;
pub mod protocol;
pub mod research;
pub mod research_ui;
mod target_node;
pub mod technology;

pub(crate) mod technology_nodes;

mod node_binding;

mod node_reuse;

mod node_detail;

mod node_hierarchy;

mod node_unit_cache;

mod info_research_button;

pub mod economy;

// Ordinary research candidate building block; no protocol input registration.
pub mod research_attempt;

pub mod army;

mod focus_start_binding;

pub mod research_candidate_observation;

pub mod research_stage;

mod focus_node_binding;
mod slot_control;

mod category_ui;

mod focus_geometry;

mod focus_ui_facts;

mod focus_candidate_observation;
