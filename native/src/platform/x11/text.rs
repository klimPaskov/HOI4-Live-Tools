//! Bounded unshifted query input, with no clipboard or keyboard-map mutation.
use super::*;
use x11rb::protocol::xkb::{ConnectionExt as _, ID};

fn keycodes(min: u8, count: u8, stride: usize, symbols: &[u32], text: &str) -> Result<Vec<u8>> {
    crate::focus_stage::validate_query(text)?;
    if stride == 0 || symbols.len() != usize::from(count) * stride {
        return Err(xerror("Malformed text keyboard mapping"));
    }
    text.bytes()
        .map(|symbol| {
            let mut matches = symbols
                .chunks_exact(stride)
                .enumerate()
                .filter(|(_, row)| row[0] == u32::from(symbol));
            let (index, _) = matches
                .next()
                .ok_or_else(|| xerror("Unshifted query character unavailable"))?;
            if matches.next().is_some() {
                return Err(xerror("Ambiguous unshifted query character"));
            }
            min.checked_add(index as u8)
                .ok_or_else(|| xerror("Query keycode outside map"))
        })
        .collect()
}

impl X11Controls {
    fn query_codes(&self, process: &dyn Process, text: &str) -> Result<Vec<u8>> {
        crate::focus_stage::validate_query(text)?;
        // Reuse exact process/window/foreground ownership and idle-input checks.
        self.check(process, &ControlKey::Pause)?;
        let version = self
            .connection
            .xkb_use_extension(1, 0)
            .map_err(xerror)?
            .reply()
            .map_err(xerror)?;
        if !version.supported {
            return Err(xerror("XKB state verification unavailable"));
        }
        let state = self
            .connection
            .xkb_get_state(ID::USE_CORE_KBD.into())
            .map_err(xerror)?
            .reply()
            .map_err(xerror)?;
        if u8::from(state.group) != 0
            || u8::from(state.locked_group) != 0
            || state.base_group != 0
            || state.latched_group != 0
            || [
                state.mods,
                state.base_mods,
                state.latched_mods,
                state.locked_mods,
                state.compat_state,
                state.grab_mods,
                state.compat_grab_mods,
                state.lookup_mods,
                state.compat_lookup_mods,
            ]
            .iter()
            .any(|m| u16::from(*m) != 0)
            || u16::from(state.ptr_btn_state) != 0
        {
            return Err(xerror(
                "Query requires clear keyboard group, modifiers and buttons",
            ));
        }
        let setup = self.connection.setup();
        let count = setup
            .max_keycode
            .checked_sub(setup.min_keycode)
            .and_then(|v| v.checked_add(1))
            .ok_or_else(|| xerror("Query keyboard mapping bound"))?;
        let mapping = self
            .connection
            .get_keyboard_mapping(setup.min_keycode, count)
            .map_err(xerror)?
            .reply()
            .map_err(xerror)?;
        keycodes(
            setup.min_keycode,
            count,
            usize::from(mapping.keysyms_per_keycode),
            &mapping.keysyms,
            text,
        )
    }
    pub fn preflight_query(&self, process: &dyn Process, text: &str) -> Result<()> {
        self.query_codes(process, text).map(|_| ())
    }
    /// Caller persists intent, owns the input lease, and pins an empty focused
    /// own search field. Any error after `before_queue` succeeds is uncertain.
    pub fn send_query_until_checked(
        &self,
        process: &dyn Process,
        text: &str,
        deadline: Instant,
        before_queue: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        let codes = self.query_codes(process, text)?;
        before_queue()?;
        // The typed reader may block. Revalidate transport and mapping afterward.
        if self.query_codes(process, text)? != codes {
            return Err(xerror("Query keyboard mapping changed"));
        }
        process.check()?;
        self.connection.stream().verify_peer()?;
        if deadline.saturating_duration_since(Instant::now()) < Duration::from_secs(2) {
            return Err(Error::new(
                "input_lease_required",
                "Insufficient query delivery deadline",
            ));
        }
        *self.connection.stream().deadline.lock().map_err(xerror)? =
            deadline.min(Instant::now() + Duration::from_secs(2));
        let mut events = Vec::with_capacity(codes.len() * 2);
        // Pair each press/release before any acknowledgement wait. No modifiers,
        // Enter, key remapping, pointer motion, retries or corrective releases.
        for code in codes {
            events.push(
                self.connection
                    .xtest_fake_input(KEY_PRESS_EVENT, code, 0, self.root, 0, 0, 0)
                    .map_err(xerror)?,
            );
            events.push(
                self.connection
                    .xtest_fake_input(KEY_RELEASE_EVENT, code, 0, self.root, 0, 0, 0)
                    .map_err(xerror)?,
            );
        }
        self.connection.flush().map_err(xerror)?;
        for event in events {
            event.check().map_err(xerror)?;
        }
        process.check()?;
        self.connection.stream().verify_peer()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn query_requires_exact_unique_unshifted_character_and_no_control_keys() {
        let map = [
            u32::from(b'a'),
            u32::from(b'A'),
            u32::from(b'b'),
            u32::from(b'B'),
            32,
            32,
        ];
        assert_eq!(
            keycodes(8, 3, 2, &map, "a bba").unwrap(),
            vec![8, 10, 9, 9, 8]
        );
        for query in ["A", "a\n", "enter", "", " a", "a "] {
            assert!(keycodes(8, 3, 2, &map, query).is_err());
        }
        assert!(keycodes(8, 3, 0, &map, "a").is_err());
        assert!(keycodes(8, 2, 2, &map, "a").is_err());
        assert!(keycodes(8, 2, 2, &[65, 97, 98, 66], "a").is_err());
        assert!(keycodes(8, 2, 2, &[97, 65, 97, 65], "a").is_err());
    }
}
