use nanoserde::{DeRon, SerRon};
use sdl2::keyboard::Scancode;
use sdl2::mouse::MouseButton;

#[derive(Debug, Clone, DeRon, SerRon)]
pub struct InputConfig {
    pub(crate) key_right: i32,
    pub(crate) key_left: i32,
    pub(crate) key_up: i32,
    pub(crate) key_down: i32,
    pub(crate) key_strafeleft: i32,
    pub(crate) key_straferight: i32,
    pub(crate) key_fire: i32,
    pub(crate) key_use: i32,
    pub(crate) key_strafe: i32,
    pub(crate) key_speed: i32,
    pub(crate) mousebfire: u8,
    pub(crate) mousebstrafe: u8,
    pub(crate) mousebforward: u8,
}

impl Default for InputConfig {
    fn default() -> Self {
        InputConfig {
            key_right: Scancode::Right as i32,
            key_left: Scancode::Left as i32,

            key_up: Scancode::W as i32,
            key_down: Scancode::S as i32,
            key_strafeleft: Scancode::A as i32,
            key_straferight: Scancode::D as i32,
            key_fire: Scancode::RCtrl as i32,
            key_use: Scancode::Space as i32,
            key_strafe: Scancode::RAlt as i32,
            key_speed: Scancode::LShift as i32,

            mousebfire: MouseButton::Left as u8,
            mousebstrafe: MouseButton::Middle as u8,
            mousebforward: MouseButton::Right as u8,
        }
    }
}

pub struct InputConfigSdl {
    pub(crate) key_right: Scancode,
    pub(crate) key_left: Scancode,
    pub(crate) key_up: Scancode,
    pub(crate) key_down: Scancode,
    pub(crate) key_strafeleft: Scancode,
    pub(crate) key_straferight: Scancode,
    pub(crate) key_fire: Scancode,
    pub(crate) key_use: Scancode,
    pub(crate) key_strafe: Scancode,
    pub(crate) key_speed: Scancode,
    pub(crate) mousebfire: MouseButton,
    pub(crate) mousebstrafe: MouseButton,
    pub(crate) mousebforward: MouseButton,
}

impl From<&InputConfig> for InputConfigSdl {
    fn from(i: &InputConfig) -> Self {
        Self {
            key_right: Scancode::from_i32(i.key_right).unwrap(),
            key_left: Scancode::from_i32(i.key_left).unwrap(),
            key_up: Scancode::from_i32(i.key_up).unwrap(),
            key_down: Scancode::from_i32(i.key_down).unwrap(),
            key_strafeleft: Scancode::from_i32(i.key_strafeleft).unwrap(),
            key_straferight: Scancode::from_i32(i.key_straferight).unwrap(),
            key_fire: Scancode::from_i32(i.key_fire).unwrap(),
            key_use: Scancode::from_i32(i.key_use).unwrap(),
            key_strafe: Scancode::from_i32(i.key_strafe).unwrap(),
            key_speed: Scancode::from_i32(i.key_speed).unwrap(),
            mousebfire: MouseButton::from_ll(i.mousebfire),
            mousebstrafe: MouseButton::from_ll(i.mousebstrafe),
            mousebforward: MouseButton::from_ll(i.mousebforward),
        }
    }
}
