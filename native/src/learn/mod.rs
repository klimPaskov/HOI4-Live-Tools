//! The learning layer: what lets an agent find out what no tool defines (a total-conversion
//! mod's panels and mechanics, a new DLC, a patch), try it, see what it did and keep what worked.
//!
//! * `uitree`: every open window as widgets with stable ids, lookups and the diff of two readings;
//! * `gui_index`: what the game's and the mods' interface files say about a widget;
//! * `skills`: the learned-skill and fact store (plain JSON files a person can read and edit).
//!
//! The memory reader that fills `uitree` and the tools live in `launch/macos/learn_tools.rs`.
pub mod gui_index;
pub mod skills;
pub mod uitree;
