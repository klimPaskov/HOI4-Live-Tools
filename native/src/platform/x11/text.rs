//! Bounded unshifted query input, with no clipboard or keyboard-map mutation.
use super::*;
use x11rb::protocol::xkb::{ConnectionExt as _, ID};

/// Bounded decimal own state ID text: 1..4095, no leading zero.
pub(crate) fn validate_digits(text: &str) -> Result<()> {
    if text.is_empty()
        || text.len() > 4
        || text.starts_with('0')
        || !text.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(xerror("Bounded decimal state digits required"));
    }
    Ok(())
}
fn keycodes(min: u8, count: u8, stride: usize, symbols: &[u32], text: &str) -> Result<Vec<u8>> {
    if crate::focus_stage::validate_query(text).is_err() {
        validate_digits(text)?;
    }
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

// None is the single internally chosen Tab operation, never arbitrary keys.
fn search_codes(
    min: u8,
    count: u8,
    stride: usize,
    symbols: &[u32],
    text: Option<&str>,
) -> Result<Vec<u8>> {
    if let Some(text) = text {
        return keycodes(min, count, stride, symbols, text);
    }
    if stride == 0 || symbols.len() != usize::from(count) * stride {
        return Err(xerror("Malformed search keyboard mapping"));
    }
    let mut found = symbols
        .chunks_exact(stride)
        .enumerate()
        .filter(|(_, row)| row[0] == 0xff09);
    let (index, _) = found
        .next()
        .ok_or_else(|| xerror("Unshifted Tab unavailable"))?;
    if found.next().is_some() {
        return Err(xerror("Ambiguous unshifted Tab"));
    }
    Ok(vec![min
        .checked_add(index as u8)
        .ok_or_else(|| xerror("Search keycode outside map"))?])
}

impl X11Controls {
    fn search_codes(&self, process: &dyn Process, text: Option<&str>) -> Result<Vec<u8>> {
        if let Some(text) = text {
            if crate::focus_stage::validate_query(text).is_err() {
                validate_digits(text)?;
            }
        }
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
        search_codes(
            setup.min_keycode,
            count,
            usize::from(mapping.keysyms_per_keycode),
            &mapping.keysyms,
            text,
        )
    }
    pub fn preflight_query(&self, process: &dyn Process, text: &str) -> Result<()> {
        self.search_codes(process, Some(text)).map(|_| ())
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
        self.send_search_until_checked(process, Some(text), deadline, before_queue)
    }
    pub fn preflight_digits(&self, process: &dyn Process, digits: &str) -> Result<()> {
        validate_digits(digits)?;
        self.search_codes(process, Some(digits)).map(|_| ())
    }
    /// Caller persists intent, owns the lease and pins the focused empty Find
    /// query. Unshifted digit keys only; any error after `before_queue` is uncertain.
    pub fn send_digits_until_checked(
        &self,
        process: &dyn Process,
        digits: &str,
        deadline: Instant,
        before_queue: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        validate_digits(digits)?;
        self.send_search_until_checked(process, Some(digits), deadline, before_queue)
    }
    pub fn preflight_search_next(&self, process: &dyn Process) -> Result<()> {
        self.search_codes(process, None).map(|_| ())
    }
    /// One fixed unshifted Tab; the caller pins the next canonical search match.
    pub fn send_search_next_until_checked(
        &self,
        process: &dyn Process,
        deadline: Instant,
        before_queue: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        self.send_search_until_checked(process, None, deadline, before_queue)
    }
    fn send_search_until_checked(
        &self,
        process: &dyn Process,
        text: Option<&str>,
        deadline: Instant,
        before_queue: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        let codes = self.search_codes(process, text)?;
        before_queue()?;
        // The typed reader may block. Revalidate transport and mapping afterward.
        if self.search_codes(process, text)? != codes {
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
    fn next_search_match_uses_only_one_unique_unshifted_tab() {
        assert_eq!(
            search_codes(8, 2, 2, &[97, 65, 0xff09, 0xfe20], None).unwrap(),
            vec![9]
        );
        for map in [[97, 0xff09, 98, 66], [0xff09, 0xfe20, 0xff09, 0xfe20]] {
            assert!(search_codes(8, 2, 2, &map, None).is_err());
        }
        assert!(search_codes(8, 2, 0, &[], None).is_err());
        assert!(search_codes(8, 1, 2, &[0xff09, 0xfe20], Some("\t")).is_err());
    }
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
    #[test]
    fn state_digits_are_bounded_unique_unshifted_decimal_keys_only() {
        let map = [
            u32::from(b'6'),
            u32::from(b'^'),
            u32::from(b'4'),
            u32::from(b'$'),
            u32::from(b'0'),
            u32::from(b')'),
        ];
        assert_eq!(keycodes(8, 3, 2, &map, "64").unwrap(), vec![8, 9]);
        assert_eq!(keycodes(8, 3, 2, &map, "4060").unwrap(), vec![9, 10, 8, 10]);
        for text in ["", "0", "064", "40604", "-6", "6 4", "6\n", "6a", "+64"] {
            assert!(validate_digits(text).is_err(), "{text}");
        }
        assert!(keycodes(8, 3, 2, &map, "65").is_err());
        assert!(keycodes(8, 2, 2, &[54, 94, 54, 36], "6").is_err());
    }
}
