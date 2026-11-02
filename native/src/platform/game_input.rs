//! Platform-neutral ordinary input for one owned game window.
//!
//! The management tools drive the game through this trait only. Each platform
//! backend (CGEvent on macOS, XTEST on Linux/X11) keeps its own ownership,
//! foreground and occlusion checks; the trait adds none and removes none.
//! Coordinates are game-logical pixels inside the client area of size `game`.
//! Key codes are platform-neutral [`vk`] values (numerically the macOS ANSI
//! virtual key codes); each backend translates them to its own key codes.
use super::{key_once, Process};
use crate::error::{Error, Result};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub origin: Point,
    pub size: Point,
}

/// Platform-neutral virtual key codes (macOS ANSI numbering).
pub mod vk {
    pub const A: u16 = 0;
    pub const RETURN: u16 = 36;
    pub const TAB: u16 = 48;
    pub const SPACE: u16 = 49;
    /// Backspace.
    pub const DELETE: u16 = 51;
    pub const ESCAPE: u16 = 53;
    pub const KEYPAD_PLUS: u16 = 69;
    pub const KEYPAD_MINUS: u16 = 78;
    pub const F1: u16 = 122;
    pub const F2: u16 = 120;
    pub const F3: u16 = 99;
    pub const F4: u16 = 118;
    pub const F5: u16 = 96;
    pub const HOME: u16 = 115;
    pub const END: u16 = 119;
    pub const FORWARD_DELETE: u16 = 117;
    pub const LEFT: u16 = 123;
    pub const RIGHT: u16 = 124;
    pub const DOWN: u16 = 125;
    pub const UP: u16 = 126;
    /// ANSI letter codes indexed a..z.
    pub const LETTERS: [u16; 26] = [
        0, 11, 8, 2, 14, 3, 5, 4, 34, 38, 40, 37, 46, 45, 31, 35, 12, 15, 1, 17, 32, 9, 13, 7, 16,
        6,
    ];
    /// ANSI digit codes indexed 0..9.
    pub const DIGITS: [u16; 10] = [29, 18, 19, 20, 21, 23, 22, 26, 28, 25];
    /// Unshifted punctuation: (character, code).
    pub const PUNCTUATION: [(char, u16); 11] = [
        ('-', 27),
        ('=', 24),
        ('[', 33),
        (']', 30),
        (';', 41),
        ('\'', 39),
        (',', 43),
        ('.', 47),
        ('/', 44),
        ('\\', 42),
        ('`', 50),
    ];
    /// The code typing `c` and whether it needs Shift (US layout).
    pub fn for_char(c: char) -> Option<(u16, bool)> {
        if c.is_ascii_lowercase() {
            return Some((LETTERS[(c as u8 - b'a') as usize], false));
        }
        if c.is_ascii_uppercase() {
            return Some((
                LETTERS[(c.to_ascii_lowercase() as u8 - b'a') as usize],
                true,
            ));
        }
        if c.is_ascii_digit() {
            return Some((DIGITS[(c as u8 - b'0') as usize], false));
        }
        if c == ' ' {
            return Some((SPACE, false));
        }
        if let Some((_, code)) = PUNCTUATION.iter().find(|(p, _)| *p == c) {
            return Some((*code, false));
        }
        const SHIFTED: [(char, char); 21] = [
            ('!', '1'),
            ('@', '2'),
            ('#', '3'),
            ('$', '4'),
            ('%', '5'),
            ('^', '6'),
            ('&', '7'),
            ('*', '8'),
            ('(', '9'),
            (')', '0'),
            ('_', '-'),
            ('+', '='),
            ('{', '['),
            ('}', ']'),
            (':', ';'),
            ('"', '\''),
            ('<', ','),
            ('>', '.'),
            ('?', '/'),
            ('|', '\\'),
            ('~', '`'),
        ];
        let base = SHIFTED.iter().find(|(s, _)| *s == c)?.1;
        for_char(base).map(|(code, _)| (code, true))
    }
}

/// Stationary game keys used by the clock adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// Space toggles pause.
    Pause,
    /// Keypad plus raises game speed.
    Faster,
    /// Keypad minus lowers game speed.
    Slower,
    /// Escape closes the current panel.
    Escape,
    /// Return confirms (e.g. the Find view).
    Return,
    /// An ordinary letter hotkey, `a`..=`z`.
    Letter(char),
}
impl Key {
    pub fn code(self) -> u16 {
        match self {
            Self::Pause => vk::SPACE,
            Self::Faster => vk::KEYPAD_PLUS,
            Self::Slower => vk::KEYPAD_MINUS,
            Self::Escape => vk::ESCAPE,
            Self::Return => vk::RETURN,
            Self::Letter(c) => vk::LETTERS[(c as u8 - b'a') as usize],
        }
    }
    /// Only lowercase ASCII letters are ordinary hotkeys here.
    pub fn letter(c: char) -> Result<Self> {
        if c.is_ascii_lowercase() {
            Ok(Self::Letter(c))
        } else {
            Err(Error::new("invalid_arguments", "Unsupported hotkey"))
        }
    }
}

pub(crate) fn find_digit_code(digit: char) -> Result<u16> {
    if !digit.is_ascii_digit() {
        return Err(Error::new(
            "invalid_arguments",
            "Only an ordinary ASCII decimal digit is accepted",
        ));
    }
    Ok(vk::DIGITS[(digit as u8 - b'0') as usize])
}

/// One capture of the owned game's client area: BGRA pixels at `scale`
/// physical pixels per game pixel.
pub struct Capture {
    pub width: usize,
    pub height: usize,
    pub stride: usize,
    pub scale: f64,
    pub data: Vec<u8>,
}
impl Capture {
    /// (r, g, b) at a game-logical pixel, if inside the capture.
    pub fn rgb(&self, x: f64, y: f64) -> Option<(u8, u8, u8)> {
        let px = (x * self.scale) as usize;
        let py = (y * self.scale) as usize;
        if px >= self.width || py >= self.height {
            return None;
        }
        let o = py * self.stride + px * 4;
        Some((self.data[o + 2], self.data[o + 1], self.data[o]))
    }
}

/// Ordinary input to the owned game window. Every method checks ownership and
/// the foreground window itself; none retries an uncertain delivery.
pub(crate) trait GameInput: Send + Sync {
    /// Capture the client area.
    fn capture(&self, game: (u32, u32)) -> Result<Capture>;
    /// The owned window is ready for input (front-most, unobscured, right size).
    fn preflight(&self) -> Result<()>;
    /// No physical button or modifier is held.
    fn confirm_input_idle(&self) -> Result<()>;
    fn click(&self, game: (u32, u32), x: f64, y: f64) -> Result<Point>;
    /// Click, running `before_down` after the cursor moved and before the press.
    fn click_checked(
        &self,
        game: (u32, u32),
        x: f64,
        y: f64,
        before_down: &dyn Fn() -> Result<()>,
    ) -> Result<Point>;
    fn confirm_cursor_at(&self, game: (u32, u32), x: f64, y: f64) -> Result<()>;
    fn drag(&self, game: (u32, u32), from: (f64, f64), to: (f64, f64)) -> Result<()>;
    fn drag_checked(
        &self,
        game: (u32, u32),
        from: (f64, f64),
        to: (f64, f64),
        before_down: &dyn Fn() -> Result<()>,
    ) -> Result<()>;
    fn shift_click(&self, game: (u32, u32), x: f64, y: f64) -> Result<Point>;
    fn right_click(&self, game: (u32, u32), x: f64, y: f64) -> Result<Point>;
    fn right_drag(&self, game: (u32, u32), from: (f64, f64), to: (f64, f64)) -> Result<()>;
    /// Wheel scroll at a point; positive `pixels` scrolls content up.
    fn scroll(&self, game: (u32, u32), at: (f64, f64), pixels: i32) -> Result<()>;
    fn press_code(&self, code: u16) -> Result<()>;
    fn press_shifted(&self, code: u16) -> Result<()>;
    fn press_control(&self, code: u16) -> Result<()>;
    fn press(&self, key: Key) -> Result<()> {
        self.press_code(key.code())
    }
    fn press_once(&self, key: Key) -> key_once::Dispatch;
    fn press_digit_once(&self, digit: char) -> key_once::Dispatch;
    /// The same binding with an additional per-action fence run before every input.
    fn guarded<'a>(
        &'a self,
        process: &'a (dyn Process + Send + Sync),
        check: &'a (dyn Fn() -> Result<()> + Send + Sync),
    ) -> Result<Box<dyn GameInput + 'a>>;
}

macro_rules! delegate_game_input {
    ($($wrapper:ident),*) => {$(
        impl<T: GameInput + ?Sized> GameInput for $wrapper<T> {
            fn capture(&self, game: (u32, u32)) -> Result<Capture> {
                (**self).capture(game)
            }
            fn preflight(&self) -> Result<()> {
                (**self).preflight()
            }
            fn confirm_input_idle(&self) -> Result<()> {
                (**self).confirm_input_idle()
            }
            fn click(&self, game: (u32, u32), x: f64, y: f64) -> Result<Point> {
                (**self).click(game, x, y)
            }
            fn click_checked(
                &self,
                game: (u32, u32),
                x: f64,
                y: f64,
                before_down: &dyn Fn() -> Result<()>,
            ) -> Result<Point> {
                (**self).click_checked(game, x, y, before_down)
            }
            fn confirm_cursor_at(&self, game: (u32, u32), x: f64, y: f64) -> Result<()> {
                (**self).confirm_cursor_at(game, x, y)
            }
            fn drag(&self, game: (u32, u32), from: (f64, f64), to: (f64, f64)) -> Result<()> {
                (**self).drag(game, from, to)
            }
            fn drag_checked(
                &self,
                game: (u32, u32),
                from: (f64, f64),
                to: (f64, f64),
                before_down: &dyn Fn() -> Result<()>,
            ) -> Result<()> {
                (**self).drag_checked(game, from, to, before_down)
            }
            fn shift_click(&self, game: (u32, u32), x: f64, y: f64) -> Result<Point> {
                (**self).shift_click(game, x, y)
            }
            fn right_click(&self, game: (u32, u32), x: f64, y: f64) -> Result<Point> {
                (**self).right_click(game, x, y)
            }
            fn right_drag(&self, game: (u32, u32), from: (f64, f64), to: (f64, f64)) -> Result<()> {
                (**self).right_drag(game, from, to)
            }
            fn scroll(&self, game: (u32, u32), at: (f64, f64), pixels: i32) -> Result<()> {
                (**self).scroll(game, at, pixels)
            }
            fn press_code(&self, code: u16) -> Result<()> {
                (**self).press_code(code)
            }
            fn press_shifted(&self, code: u16) -> Result<()> {
                (**self).press_shifted(code)
            }
            fn press_control(&self, code: u16) -> Result<()> {
                (**self).press_control(code)
            }
            fn press(&self, key: Key) -> Result<()> {
                (**self).press(key)
            }
            fn press_once(&self, key: Key) -> key_once::Dispatch {
                (**self).press_once(key)
            }
            fn press_digit_once(&self, digit: char) -> key_once::Dispatch {
                (**self).press_digit_once(digit)
            }
            fn guarded<'a>(
                &'a self,
                process: &'a (dyn Process + Send + Sync),
                check: &'a (dyn Fn() -> Result<()> + Send + Sync),
            ) -> Result<Box<dyn GameInput + 'a>> {
                (**self).guarded(process, check)
            }
        }
    )*};
}
use std::sync::Arc;
delegate_game_input!(Box, Arc);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn characters_map_to_codes_and_shift() {
        assert_eq!(vk::for_char('a'), Some((0, false)));
        assert_eq!(vk::for_char('Z'), Some((6, true)));
        assert_eq!(vk::for_char('7'), Some((26, false)));
        assert_eq!(vk::for_char('-'), Some((27, false)));
        assert_eq!(vk::for_char('_'), Some((27, true)));
        assert_eq!(vk::for_char('('), Some((25, true)));
        assert_eq!(vk::for_char('é'), None);
        assert_eq!(Key::Pause.code(), 49);
        assert_eq!(Key::Letter('f').code(), 3);
        assert_eq!(find_digit_code('0').unwrap(), 29);
    }
}
