pub mod actions;
pub mod calendar;
pub mod error;
#[cfg(any(target_os = "linux", test))]
mod focus_ui_observation;
pub mod framing;
pub mod game;
pub mod gui;
mod gui_registry;
#[cfg(target_os = "macos")]
pub(crate) mod gui_tree;
pub mod input_lease;
pub mod launch;
pub mod memory;
pub mod own_focus;
pub mod plan_run;
pub mod platform;
pub mod protocol;
pub mod research;
pub mod research_folders;
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

pub mod construction;
pub mod construction_stage;
pub mod economy;
pub mod production_lines;
pub mod production_step;

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

pub mod focus_stage;

#[doc(hidden)]
pub mod politics_focus;

// Durable plan core; the optional Linux owner wires supported ordinary adapters.
pub mod daily_governor;

#[doc(hidden)]
pub mod daily_observation;

#[cfg(any(target_os = "linux", test))]
mod daily_worker;

#[cfg(any(target_os = "linux", all(test, unix)))]
mod daily_navigation;

mod daily_digest_lock;

#[cfg(any(target_os = "linux", test))]
mod daily_digest;

#[cfg(any(target_os = "linux", all(test, unix)))]
mod daily_focus_navigation;

// Pure generated/public topology only; no campaign integration.
pub mod graph_kernel;
