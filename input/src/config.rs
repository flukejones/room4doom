use gamestate_traits::{KeyCode, MouseBtn};
use nanoserde::{DeRon, SerRon};

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
            key_right: KeyCode::Right as i32,
            key_left: KeyCode::Left as i32,

            key_up: KeyCode::W as i32,
            key_down: KeyCode::S as i32,
            key_strafeleft: KeyCode::A as i32,
            key_straferight: KeyCode::D as i32,
            key_fire: KeyCode::RCtrl as i32,
            key_use: KeyCode::Space as i32,
            key_strafe: KeyCode::RAlt as i32,
            key_speed: KeyCode::LShift as i32,

            mousebfire: MouseBtn::Left as u8,
            mousebstrafe: MouseBtn::Middle as u8,
            mousebforward: MouseBtn::Right as u8,
        }
    }
}

pub struct InputConfigResolved {
    pub(crate) key_right: KeyCode,
    pub(crate) key_left: KeyCode,
    pub(crate) key_up: KeyCode,
    pub(crate) key_down: KeyCode,
    pub(crate) key_strafeleft: KeyCode,
    pub(crate) key_straferight: KeyCode,
    pub(crate) key_fire: KeyCode,
    pub(crate) key_use: KeyCode,
    pub(crate) key_strafe: KeyCode,
    pub(crate) key_speed: KeyCode,
    pub(crate) mousebfire: MouseBtn,
    pub(crate) mousebstrafe: MouseBtn,
    pub(crate) mousebforward: MouseBtn,
}

impl From<&InputConfig> for InputConfigResolved {
    fn from(i: &InputConfig) -> Self {
        Self {
            key_right: KeyCode::from_i32(i.key_right).expect("invalid keycode in config"),
            key_left: KeyCode::from_i32(i.key_left).expect("invalid keycode in config"),
            key_up: KeyCode::from_i32(i.key_up).expect("invalid keycode in config"),
            key_down: KeyCode::from_i32(i.key_down).expect("invalid keycode in config"),
            key_strafeleft: KeyCode::from_i32(i.key_strafeleft).expect("invalid keycode in config"),
            key_straferight: KeyCode::from_i32(i.key_straferight)
                .expect("invalid keycode in config"),
            key_fire: KeyCode::from_i32(i.key_fire).expect("invalid keycode in config"),
            key_use: KeyCode::from_i32(i.key_use).expect("invalid keycode in config"),
            key_strafe: KeyCode::from_i32(i.key_strafe).expect("invalid keycode in config"),
            key_speed: KeyCode::from_i32(i.key_speed).expect("invalid keycode in config"),
            mousebfire: MouseBtn::from_u8(i.mousebfire).expect("invalid mouse button in config"),
            mousebstrafe: MouseBtn::from_u8(i.mousebstrafe)
                .expect("invalid mouse button in config"),
            mousebforward: MouseBtn::from_u8(i.mousebforward)
                .expect("invalid mouse button in config"),
        }
    }
}
