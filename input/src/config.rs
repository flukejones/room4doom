use sdl2::{keyboard::Scancode as Sc, mouse::MouseButton as Mb};
use serde::{de, Deserialize, Serialize, Serializer};

fn serialize_scancode<S>(sc: &Sc, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    s.serialize_i32(*sc as i32)
}

fn deserialize_scancode<'de, D>(deserializer: D) -> Result<Sc, D::Error>
where
    D: de::Deserializer<'de>,
{
    let sc: i32 = de::Deserialize::deserialize(deserializer)?;
    let sc = Sc::from_i32(sc).unwrap_or_else(|| panic!("Could not deserialise key config"));
    Ok(sc)
}

fn serialize_mb<S>(sc: &Mb, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    s.serialize_u8(*sc as u8)
}

fn deserialize_mb<'de, D>(deserializer: D) -> Result<Mb, D::Error>
where
    D: de::Deserializer<'de>,
{
    let sc: u8 = de::Deserialize::deserialize(deserializer)?;
    let sc = Mb::from_ll(sc);
    Ok(sc)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputConfig {
    #[serde(serialize_with = "serialize_scancode")]
    #[serde(deserialize_with = "deserialize_scancode")]
    pub(crate) key_right: Sc,
    #[serde(serialize_with = "serialize_scancode")]
    #[serde(deserialize_with = "deserialize_scancode")]
    pub(crate) key_left: Sc,
    #[serde(serialize_with = "serialize_scancode")]
    #[serde(deserialize_with = "deserialize_scancode")]
    pub(crate) key_up: Sc,
    #[serde(serialize_with = "serialize_scancode")]
    #[serde(deserialize_with = "deserialize_scancode")]
    pub(crate) key_down: Sc,
    #[serde(serialize_with = "serialize_scancode")]
    #[serde(deserialize_with = "deserialize_scancode")]
    pub(crate) key_strafeleft: Sc,
    #[serde(serialize_with = "serialize_scancode")]
    #[serde(deserialize_with = "deserialize_scancode")]
    pub(crate) key_straferight: Sc,
    #[serde(serialize_with = "serialize_scancode")]
    #[serde(deserialize_with = "deserialize_scancode")]
    pub(crate) key_fire: Sc,
    #[serde(serialize_with = "serialize_scancode")]
    #[serde(deserialize_with = "deserialize_scancode")]
    pub(crate) key_use: Sc,
    #[serde(serialize_with = "serialize_scancode")]
    #[serde(deserialize_with = "deserialize_scancode")]
    pub(crate) key_strafe: Sc,
    #[serde(serialize_with = "serialize_scancode")]
    #[serde(deserialize_with = "deserialize_scancode")]
    pub(crate) key_speed: Sc,

    #[serde(serialize_with = "serialize_mb")]
    #[serde(deserialize_with = "deserialize_mb")]
    pub(crate) mousebfire: Mb,
    #[serde(serialize_with = "serialize_mb")]
    #[serde(deserialize_with = "deserialize_mb")]
    pub(crate) mousebstrafe: Mb,
    #[serde(serialize_with = "serialize_mb")]
    #[serde(deserialize_with = "deserialize_mb")]
    pub(crate) mousebforward: Mb,
}

impl Default for InputConfig {
    fn default() -> Self {
        InputConfig {
            key_right: Sc::Right,
            key_left: Sc::Left,

            key_up: Sc::W,
            key_down: Sc::S,
            key_strafeleft: Sc::A,
            key_straferight: Sc::D,
            key_fire: Sc::RCtrl,
            key_use: Sc::Space,
            key_strafe: Sc::RAlt,
            key_speed: Sc::LShift,

            mousebfire: Mb::Left,
            mousebstrafe: Mb::Middle,
            mousebforward: Mb::Right,
        }
    }
}
