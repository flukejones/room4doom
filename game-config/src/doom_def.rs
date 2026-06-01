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
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Default)]
pub enum WeaponType {
    Fist,
    #[default]
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

impl TryFrom<u8> for WeaponType {
    /// The raw byte that failed to map to a selectable weapon variant.
    type Error = u8;

    fn try_from(w: u8) -> Result<Self, u8> {
        match w {
            0 => Ok(WeaponType::Fist),
            1 => Ok(WeaponType::Pistol),
            2 => Ok(WeaponType::Shotgun),
            3 => Ok(WeaponType::Chaingun),
            4 => Ok(WeaponType::Missile),
            5 => Ok(WeaponType::Plasma),
            6 => Ok(WeaponType::BFG),
            7 => Ok(WeaponType::Chainsaw),
            8 => Ok(WeaponType::SuperShotgun),
            _ => Err(w),
        }
    }
}
