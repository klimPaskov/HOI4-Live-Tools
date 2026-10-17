//! Read-only conjunction over the existing witnessed own-country snapshot.
//! A target already present in the first sample must not wait for another change.
use crate::error::{Error, Result};
use serde_json::Value;
use std::collections::BTreeSet;

pub(super) struct Target {
    focus: Option<String>,
    research: Vec<(u64, String)>,
}
fn invalid() -> Error {
    Error::new(
        "invalid_arguments",
        "Expected bounded canonical focus/research IDs and unique slots in until_assignments",
    )
}
fn key(value: &Value, limit: usize) -> Result<String> {
    value
        .as_str()
        .filter(|s| {
            !s.is_empty()
                && s.len() <= limit
                && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        })
        .map(str::to_owned)
        .ok_or_else(invalid)
}
impl Target {
    pub(super) fn parse(value: &Value) -> Result<Self> {
        let object = value.as_object().ok_or_else(invalid)?;
        if object
            .keys()
            .any(|k| !["focus_id", "research"].contains(&k.as_str()))
        {
            return Err(invalid());
        }
        let focus = object.get("focus_id").map(|v| key(v, 128)).transpose()?;
        let mut research = Vec::new();
        let mut slots = BTreeSet::new();
        if let Some(value) = object.get("research") {
            let rows = value
                .as_array()
                .filter(|v| v.len() <= 32)
                .ok_or_else(invalid)?;
            for row in rows {
                let row = row
                    .as_object()
                    .filter(|r| r.len() == 2)
                    .ok_or_else(invalid)?;
                let slot = row
                    .get("slot")
                    .and_then(Value::as_u64)
                    .filter(|v| *v < 32)
                    .ok_or_else(invalid)?;
                let technology = key(row.get("technology_id").ok_or_else(invalid)?, 256)?;
                if !slots.insert(slot) {
                    return Err(invalid());
                }
                research.push((slot, technology));
            }
        }
        if focus.is_none() && research.is_empty() {
            return Err(invalid());
        }
        Ok(Self { focus, research })
    }
    pub(super) fn reached(&self, sample: &Value) -> bool {
        sample["owned"] == true
            && self
                .focus
                .as_ref()
                .is_none_or(|key| sample["focus"]["key"].as_str() == Some(key))
            && self.research.iter().all(|(slot, technology)| {
                sample["research"].as_array().is_some_and(|rows| {
                    let mut matches = rows
                        .iter()
                        .filter(|row| row["slot"].as_u64() == Some(*slot));
                    matches
                        .next()
                        .is_some_and(|row| row["researching"].as_str() == Some(technology))
                        && matches.next().is_none()
                })
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn only_the_full_exact_own_conjunction_matches() {
        let target = Target::parse(&json!({"focus_id":"GER_rhineland","research":[{"slot":0,"technology_id":"support_weapons"},{"slot":2,"technology_id":"tools1"}]})).unwrap();
        let sample = json!({"owned":true,"focus":{"key":"GER_rhineland"},"research":[{"slot":2,"researching":"tools1"},{"slot":0,"researching":"support_weapons"}]});
        assert!(target.reached(&sample));
        for (pointer, replacement) in [
            ("/owned", json!(false)),
            ("/focus/key", json!("other")),
            ("/focus", Value::Null),
            ("/research/0/slot", json!(1)),
            ("/research/0/researching", Value::Null),
            ("/research/1/researching", json!("other")),
        ] {
            let mut changed = sample.clone();
            *changed.pointer_mut(pointer).unwrap() = replacement;
            assert!(!target.reached(&changed), "{pointer}");
        }
        let mut duplicate = sample.clone();
        duplicate["research"]
            .as_array_mut()
            .unwrap()
            .push(sample["research"][0].clone());
        assert!(!target.reached(&duplicate));
    }
    #[test]
    fn malformed_empty_unbounded_and_duplicate_targets_refuse() {
        let mut cases = vec![
            Value::Null,
            json!({}),
            json!({"research":[]}),
            json!({"focus_id":null}),
            json!({"focus_id":""}),
            json!({"focus_id":"a b"}),
            json!({"focus_id":"a","unknown":1}),
            json!({"research":null}),
            json!({"research":[{"slot":0}]}),
            json!({"research":[{"slot":32,"technology_id":"a"}]}),
            json!({"research":[{"slot":-1,"technology_id":"a"}]}),
            json!({"research":[{"slot":0.5,"technology_id":"a"}]}),
            json!({"research":[{"slot":0,"technology_id":"a","extra":1}]}),
            json!({"research":[{"slot":0,"technology_id":"a"},{"slot":0,"technology_id":"b"}]}),
        ];
        cases.push(json!({"focus_id":"a".repeat(129)}));
        cases.push(json!({"research":[{"slot":0,"technology_id":"a".repeat(257)}]}));
        cases.push(json!({"research":vec![json!({"slot":0,"technology_id":"a"});33]}));
        for case in cases {
            assert_eq!(
                Target::parse(&case).err().unwrap().code,
                "invalid_arguments",
                "{case}"
            );
        }
    }
    #[test]
    fn each_domain_can_be_targeted_independently() {
        assert!(Target::parse(&json!({"focus_id":"GER_rhineland"}))
            .unwrap()
            .reached(&json!({"owned":true,"focus":{"key":"GER_rhineland"}})));
        assert!(
            Target::parse(&json!({"research":[{"slot":31,"technology_id":"tools1"}]}))
                .unwrap()
                .reached(&json!({"owned":true,"research":[{"slot":31,"researching":"tools1"}]}))
        );
    }
}
