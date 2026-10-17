//! Bounded canonical search order, separate from text entry and assignment.
use super::{key, search_error};
use crate::error::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

pub const MAX_MATCHES: usize = 64;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SearchMatches {
    pub ordered: Vec<String>,
    pub selected: String,
}
impl SearchMatches {
    pub(super) fn from_observation(s: &Value) -> Result<Option<Self>> {
        if s["complete_current_item_coverage"] != true
            || s["ordered_matches_available"] != true
            || s["selected_in_matches"] != true
        {
            return Ok(None);
        }
        let ordered: Vec<String> = serde_json::from_value(s["ordered_match_keys"].clone())
            .map_err(|_| search_error("Canonical ordered search matches required"))?;
        if s["match_count"].as_u64() != Some(ordered.len() as u64) {
            return Err(search_error(
                "Search match count differs from complete order",
            ));
        }
        let selected = s["selected_key"]
            .as_str()
            .ok_or_else(|| search_error("Canonical selected search key required"))?
            .to_owned();
        let matches = Self { ordered, selected };
        matches.validate()?;
        Ok(Some(matches))
    }
    pub(super) fn validate(&self) -> Result<()> {
        if self.ordered.is_empty() || self.ordered.len() > MAX_MATCHES {
            return Err(search_error(
                "Search navigation needs 1–64 complete matches",
            ));
        }
        let mut seen = BTreeSet::new();
        for id in &self.ordered {
            key(id)?;
            if id.len() > 128 {
                return Err(search_error("Search canonical key exceeds producer bound"));
            }
            if !seen.insert(id) {
                return Err(search_error(
                    "Search navigation needs unique canonical matches",
                ));
            }
        }
        if !seen.contains(&self.selected) {
            return Err(search_error(
                "Selected search key absent from ordered matches",
            ));
        }
        Ok(())
    }
    pub fn next_key(&self) -> Option<&str> {
        let index = self.ordered.iter().position(|id| id == &self.selected)?;
        self.ordered
            .get((index + 1) % self.ordered.len())
            .map(String::as_str)
    }
}
