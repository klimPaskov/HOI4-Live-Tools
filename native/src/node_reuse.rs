//! Request-local shared proofs only. All reads flow through caller Witnesses.
//! Caller must finish those witnesses before publishing the model. Reuse never
//! survives a request and does not establish an atomic snapshot or input authority.
use crate::{
    error::{Error, Result},
    memory::Reader,
    research_ui::primary_type_encoding,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
pub(crate) struct Reuse {
    base: u64,
    tree: u64,
    types: BTreeMap<u64, Value>,
    callbacks: [Option<bool>; 2],
}
impl Reuse {
    pub(crate) fn new(base: u64, tree: u64) -> Self {
        Self {
            base,
            tree,
            types: BTreeMap::new(),
            callbacks: [None, None],
        }
    }
    pub(crate) fn base(&self) -> u64 {
        self.base
    }
    pub(crate) fn tree(&self) -> u64 {
        self.tree
    }
    pub(crate) fn nullable_type(&mut self, r: &Reader<'_>, object: u64) -> Result<Value> {
        if object == 0 {
            return Ok(Value::Null);
        }
        // Never reuse object identity/vptr: only validated immutable ELF metadata.
        let table = r.u64(object)?;
        if let Some(value) = self.types.get(&table) {
            return Ok(value.clone());
        }
        let value = json!(primary_type_encoding(r, self.base, table)?);
        self.types.insert(table, value.clone());
        Ok(value)
    }
    pub(crate) fn callback(&mut self, r: &Reader<'_>, target: u64) -> Result<bool> {
        let index = if target == self.tree + 0x5e0 {
            0
        } else if target == self.tree + 0x868 {
            1
        } else {
            return Ok(false);
        };
        // Caller always freshly reads the target's exact primary before this helper.
        if let Some(value) = self.callbacks[index] {
            return Ok(value);
        }
        let mut value = false;
        if r.u64(target + 0x38)? == self.base + 0x15a5710
            && r.u64(target + 0x40)? == self.base + 0x15a56e0
        {
            let capture = r.u64(target + 0x28)?;
            if capture == 0 || capture % 8 != 0 {
                return Err(Error::new(
                    "invalid_gui_binding",
                    "Malformed callback capture",
                ));
            }
            let method = if index == 0 { 0x1599290 } else { 0x15994c0 };
            value = r.u64(capture)? == self.base + method
                && r.u64(capture + 8)? == 0
                && r.u64(capture + 0x10)? == self.tree;
        }
        self.callbacks[index] = Some(value);
        Ok(value)
    }
}
