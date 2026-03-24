/// Game mode handling - identify IWAD version to handle IWAD dependend
/// animations etc.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum GameMode {
    /// DOOM 1 shareware, E1, M9
    Shareware,
    /// DOOM 1 registered, E3, M27
    Registered,
    /// DOOM 2 retail, E1 M34
    Commercial,
    /// DOOM 1 retail, E4, M36
    Retail,
    Indetermined, // Well, no IWAD found.
}

// Mission packs - might be useful for TC stuff?
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum GameMission {
    /// Doom (shareware, registered)
    Doom,
    /// Doom II
    Doom2,
    /// TNT mission pack
    PackTnt,
    /// Plutonia mission pack
    PackPlut,
    None,
}

/// The defined weapons, including a marker indicating user has not changed
/// weapon.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub enum WeaponType {
    Fist,
    Pistol,
    Shotgun,
    Chaingun,
    Missile,
    Plasma,
    BFG,
    Chainsaw,
    SuperShotgun,
    /// Used as a marker to count total available weapons
    NumWeapons,
    /// No pending weapon change.
    NoChange,
}

impl From<WeaponType> for usize {
    fn from(w: WeaponType) -> Self {
        match w {
            WeaponType::Fist => 0,
            WeaponType::Pistol => 1,
            WeaponType::Shotgun => 2,
            WeaponType::Chaingun => 3,
            WeaponType::Missile => 4,
            WeaponType::Plasma => 5,
            WeaponType::BFG => 6,
            WeaponType::Chainsaw => 7,
            WeaponType::SuperShotgun => 8,
            _ => 0,
        }
    }
}

impl Default for WeaponType {
    fn default() -> Self {
        Self::Pistol
    }
}

impl From<u8> for WeaponType {
    fn from(w: u8) -> Self {
        if w >= WeaponType::NumWeapons as u8 {
            panic!("{} is not a variant of WeaponType", w);
        }
        unsafe { std::mem::transmute(w) }
    }
}
