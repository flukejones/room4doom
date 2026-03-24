//! winit input helpers — conversion from winit key/mouse types to
//! backend-agnostic `KeyCode`/`MouseBtn`.

use gamestate_traits::{KeyCode, MouseBtn};
use winit::keyboard::KeyCode as WinitKey;

/// Convert a winit `KeyCode` to the backend-agnostic `KeyCode`.
pub fn winit_keycode_to_keycode(wk: WinitKey) -> Option<KeyCode> {
    Some(match wk {
        WinitKey::KeyA => KeyCode::A,
        WinitKey::KeyB => KeyCode::B,
        WinitKey::KeyC => KeyCode::C,
        WinitKey::KeyD => KeyCode::D,
        WinitKey::KeyE => KeyCode::E,
        WinitKey::KeyF => KeyCode::F,
        WinitKey::KeyG => KeyCode::G,
        WinitKey::KeyH => KeyCode::H,
        WinitKey::KeyI => KeyCode::I,
        WinitKey::KeyJ => KeyCode::J,
        WinitKey::KeyK => KeyCode::K,
        WinitKey::KeyL => KeyCode::L,
        WinitKey::KeyM => KeyCode::M,
        WinitKey::KeyN => KeyCode::N,
        WinitKey::KeyO => KeyCode::O,
        WinitKey::KeyP => KeyCode::P,
        WinitKey::KeyQ => KeyCode::Q,
        WinitKey::KeyR => KeyCode::R,
        WinitKey::KeyS => KeyCode::S,
        WinitKey::KeyT => KeyCode::T,
        WinitKey::KeyU => KeyCode::U,
        WinitKey::KeyV => KeyCode::V,
        WinitKey::KeyW => KeyCode::W,
        WinitKey::KeyX => KeyCode::X,
        WinitKey::KeyY => KeyCode::Y,
        WinitKey::KeyZ => KeyCode::Z,
        WinitKey::Digit1 => KeyCode::Num1,
        WinitKey::Digit2 => KeyCode::Num2,
        WinitKey::Digit3 => KeyCode::Num3,
        WinitKey::Digit4 => KeyCode::Num4,
        WinitKey::Digit5 => KeyCode::Num5,
        WinitKey::Digit6 => KeyCode::Num6,
        WinitKey::Digit7 => KeyCode::Num7,
        WinitKey::Digit8 => KeyCode::Num8,
        WinitKey::Digit9 => KeyCode::Num9,
        WinitKey::Digit0 => KeyCode::Num0,
        WinitKey::Enter => KeyCode::Return,
        WinitKey::Escape => KeyCode::Escape,
        WinitKey::Backspace => KeyCode::Backspace,
        WinitKey::Tab => KeyCode::Tab,
        WinitKey::Space => KeyCode::Space,
        WinitKey::Minus => KeyCode::Minus,
        WinitKey::Equal => KeyCode::Equals,
        WinitKey::CapsLock => KeyCode::CapsLock,
        WinitKey::F1 => KeyCode::F1,
        WinitKey::F2 => KeyCode::F2,
        WinitKey::F3 => KeyCode::F3,
        WinitKey::F4 => KeyCode::F4,
        WinitKey::F5 => KeyCode::F5,
        WinitKey::F6 => KeyCode::F6,
        WinitKey::F7 => KeyCode::F7,
        WinitKey::F8 => KeyCode::F8,
        WinitKey::F9 => KeyCode::F9,
        WinitKey::F10 => KeyCode::F10,
        WinitKey::F11 => KeyCode::F11,
        WinitKey::F12 => KeyCode::F12,
        WinitKey::Pause => KeyCode::Pause,
        WinitKey::ArrowRight => KeyCode::Right,
        WinitKey::ArrowLeft => KeyCode::Left,
        WinitKey::ArrowDown => KeyCode::Down,
        WinitKey::ArrowUp => KeyCode::Up,
        WinitKey::ControlLeft => KeyCode::LCtrl,
        WinitKey::ShiftLeft => KeyCode::LShift,
        WinitKey::AltLeft => KeyCode::LAlt,
        WinitKey::ControlRight => KeyCode::RCtrl,
        WinitKey::ShiftRight => KeyCode::RShift,
        WinitKey::AltRight => KeyCode::RAlt,
        _ => return None,
    })
}

/// Convert a winit `MouseButton` to the backend-agnostic `MouseBtn`.
pub fn winit_mousebutton_to_mousebtn(mb: winit::event::MouseButton) -> Option<MouseBtn> {
    match mb {
        winit::event::MouseButton::Left => Some(MouseBtn::Left),
        winit::event::MouseButton::Middle => Some(MouseBtn::Middle),
        winit::event::MouseButton::Right => Some(MouseBtn::Right),
        _ => None,
    }
}
