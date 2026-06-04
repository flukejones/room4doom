//! Backend-agnostic key and mouse button enums.
//!
//! Discriminant values match SDL2 scancodes so existing `user.toml` config
//! files remain valid without migration.

use std::fmt;

/// Keyboard key codes. `#[repr(i32)]` with SDL2-matching discriminants for
/// config file compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum KeyCode {
    A = 4,
    B = 5,
    C = 6,
    D = 7,
    E = 8,
    F = 9,
    G = 10,
    H = 11,
    I = 12,
    J = 13,
    K = 14,
    L = 15,
    M = 16,
    N = 17,
    O = 18,
    P = 19,
    Q = 20,
    R = 21,
    S = 22,
    T = 23,
    U = 24,
    V = 25,
    W = 26,
    X = 27,
    Y = 28,
    Z = 29,
    Num1 = 30,
    Num2 = 31,
    Num3 = 32,
    Num4 = 33,
    Num5 = 34,
    Num6 = 35,
    Num7 = 36,
    Num8 = 37,
    Num9 = 38,
    Num0 = 39,
    Return = 40,
    Escape = 41,
    Backspace = 42,
    Tab = 43,
    Space = 44,
    Minus = 45,
    Equals = 46,
    CapsLock = 57,
    F1 = 58,
    F2 = 59,
    F3 = 60,
    F4 = 61,
    F5 = 62,
    F6 = 63,
    F7 = 64,
    F8 = 65,
    F9 = 66,
    F10 = 67,
    F11 = 68,
    F12 = 69,
    Pause = 72,
    Right = 79,
    Left = 80,
    Down = 81,
    Up = 82,
    LCtrl = 224,
    LShift = 225,
    LAlt = 226,
    RCtrl = 228,
    RShift = 229,
    RAlt = 230,
}

/// All `KeyCode` variants in discriminant order, for `from_i32` lookup.
const ALL_KEYS: &[KeyCode] = &[
    KeyCode::A,
    KeyCode::B,
    KeyCode::C,
    KeyCode::D,
    KeyCode::E,
    KeyCode::F,
    KeyCode::G,
    KeyCode::H,
    KeyCode::I,
    KeyCode::J,
    KeyCode::K,
    KeyCode::L,
    KeyCode::M,
    KeyCode::N,
    KeyCode::O,
    KeyCode::P,
    KeyCode::Q,
    KeyCode::R,
    KeyCode::S,
    KeyCode::T,
    KeyCode::U,
    KeyCode::V,
    KeyCode::W,
    KeyCode::X,
    KeyCode::Y,
    KeyCode::Z,
    KeyCode::Num1,
    KeyCode::Num2,
    KeyCode::Num3,
    KeyCode::Num4,
    KeyCode::Num5,
    KeyCode::Num6,
    KeyCode::Num7,
    KeyCode::Num8,
    KeyCode::Num9,
    KeyCode::Num0,
    KeyCode::Return,
    KeyCode::Escape,
    KeyCode::Backspace,
    KeyCode::Tab,
    KeyCode::Space,
    KeyCode::Minus,
    KeyCode::Equals,
    KeyCode::CapsLock,
    KeyCode::F1,
    KeyCode::F2,
    KeyCode::F3,
    KeyCode::F4,
    KeyCode::F5,
    KeyCode::F6,
    KeyCode::F7,
    KeyCode::F8,
    KeyCode::F9,
    KeyCode::F10,
    KeyCode::F11,
    KeyCode::F12,
    KeyCode::Pause,
    KeyCode::Right,
    KeyCode::Left,
    KeyCode::Down,
    KeyCode::Up,
    KeyCode::LCtrl,
    KeyCode::LShift,
    KeyCode::LAlt,
    KeyCode::RCtrl,
    KeyCode::RShift,
    KeyCode::RAlt,
];

impl fmt::Display for KeyCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(c) = self.to_char() {
            write!(f, "{}", c.to_uppercase())
        } else {
            write!(f, "{self:?}")
        }
    }
}

impl KeyCode {
    /// Convert an i32 discriminant to a `KeyCode`. Returns `None` for
    /// unknown values. Used for config deserialization and weapon key lookup.
    pub fn from_i32(val: i32) -> Option<Self> {
        ALL_KEYS.iter().find(|k| **k as i32 == val).copied()
    }

    /// Convert this key to the corresponding ASCII character, if any.
    /// Used by the cheat code system (replaces `Keycode::from_scancode`).
    pub const fn to_char(self) -> Option<char> {
        match self {
            Self::A => Some('a'),
            Self::B => Some('b'),
            Self::C => Some('c'),
            Self::D => Some('d'),
            Self::E => Some('e'),
            Self::F => Some('f'),
            Self::G => Some('g'),
            Self::H => Some('h'),
            Self::I => Some('i'),
            Self::J => Some('j'),
            Self::K => Some('k'),
            Self::L => Some('l'),
            Self::M => Some('m'),
            Self::N => Some('n'),
            Self::O => Some('o'),
            Self::P => Some('p'),
            Self::Q => Some('q'),
            Self::R => Some('r'),
            Self::S => Some('s'),
            Self::T => Some('t'),
            Self::U => Some('u'),
            Self::V => Some('v'),
            Self::W => Some('w'),
            Self::X => Some('x'),
            Self::Y => Some('y'),
            Self::Z => Some('z'),
            Self::Num1 => Some('1'),
            Self::Num2 => Some('2'),
            Self::Num3 => Some('3'),
            Self::Num4 => Some('4'),
            Self::Num5 => Some('5'),
            Self::Num6 => Some('6'),
            Self::Num7 => Some('7'),
            Self::Num8 => Some('8'),
            Self::Num9 => Some('9'),
            Self::Num0 => Some('0'),
            Self::Space => Some(' '),
            Self::Minus => Some('-'),
            Self::Equals => Some('='),
            _ => None,
        }
    }
}

/// Mouse button identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum MouseBtn {
    Left = 1,
    Middle = 2,
    Right = 3,
}

impl MouseBtn {
    /// Convert a raw button index to `MouseBtn`.
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            1 => Some(Self::Left),
            2 => Some(Self::Middle),
            3 => Some(Self::Right),
            _ => None,
        }
    }
}
