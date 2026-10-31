//! Exact Mac building-choice observer/capture, read through the parent's own
//! catalog, complete choice membership and witness bracket. Never invokes it.
use super::{exact, invalid, pointer};
use crate::{error::Result, memory::Reader};
use serde_json::{json, Value};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Binding {
    node: u64,
    glue: u64,
    function: u64,
    flags_cc: u8,
}
impl Binding {
    pub(super) fn summary(&self) -> Value {
        json!({"binding_verified":true,"receiver_matches_requested_item":true,
            "observer_count":1,"additional_callbacks_present":false,
            "button_flags_cc_raw":self.flags_cc,"disable_requested":self.flags_cc&0x40!=0,
            "prerequisites_evaluated":false,"input_authorized":false,"delivery_verified":false})
    }
}

pub(super) fn observe(r: &Reader<'_>, base: u64, item: u64, button: u64) -> Result<Binding> {
    // Caller has proved the requested item's exact primary/template membership
    // and its named frame/button's own registry. Do not infer an item+8 owner.
    exact(r, base, button + 0x70, 0x36b9b18)?;
    if r.u64(button + 0x90)? != 0 || r.byte(button + 0x8c)? != 0 || r.u64(button + 0xc8)? != 0 {
        return Err(invalid(
            "Mac choice observer override, active notification or input handler is unverified",
        ));
    }
    // The exact constructor registers one embedded observer. Reject another
    // count before following any list or foreign observer payload.
    if r.i32(button + 0x88)? != 1 {
        return Err(invalid("Exactly one Mac choice observer required"));
    }
    let node = r.u64(button + 0x78)?;
    pointer(node)?;
    if r.u64(button + 0x80)? != node
        || r.u64(node + 8)? != 0
        || r.u64(node + 0x10)? != 0
        || r.byte(node + 0x18)? != 0
    {
        return Err(invalid("Complete active Mac choice observer list required"));
    }
    let glue = item + 0x30;
    if r.u64(node)? != glue {
        return Err(invalid(
            "Mac choice observer must belong to the requested item",
        ));
    }
    // Current Mach-O constructor 0x101ed5310, helper 0x101ee0030 and the
    // 0x1022988a0 fields copy: twenty callable slots, only the first supplied.
    exact(r, base, glue, 0x36477d8)?;
    for index in 2..=20 {
        if r.u64(glue + index * 0x30)? != 0 {
            return Err(invalid("Mac choice has an additional callback"));
        }
    }
    let function = r.u64(glue + 0x30)?;
    exact(r, base, function, 0x3647870)?;
    if r.u64(function + 8)? != base + 0x16f8600
        || r.u64(function + 0x10)? != 0
        || r.u64(function + 0x18)? != item
    {
        return Err(invalid(
            "Mac choice callback must retain the exact member and requested item capture",
        ));
    }
    // Refresh calls Standard virtual +0x2a0 (0x1027511b0), which writes this
    // byte's bit 0x40. State +0x9c alone can differ while the wrapper temporarily
    // enters state 3. This retained flag is not a fresh prerequisite evaluation.
    let flags_cc = r.byte(button + 0xd4)?;
    Ok(Binding {
        node,
        glue,
        function,
        flags_cc,
    })
}
