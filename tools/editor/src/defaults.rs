//! Built-in thing types used when no project `things.dsp` is loaded.

use std::collections::HashMap;
use std::sync::OnceLock;

use editor_core::ThingFlags;

use crate::thing_info::THING_INFO;

/// Display name, type number, canvas colour, sprite prefix (4-char; frame A icon; empty = colour square).
pub struct ThingType {
    pub name: &'static str,
    pub kind: i32,
    pub color: [u8; 4],
    pub sprite: &'static str,
    /// Doom II only; hidden when editing a Doom 1 IWAD.
    pub doom2_only: bool,
}

const fn t(name: &'static str, kind: i32, color: [u8; 4], sprite: &'static str) -> ThingType {
    ThingType {
        name,
        kind,
        color,
        sprite,
        doom2_only: false,
    }
}

const fn t2(name: &'static str, kind: i32, color: [u8; 4], sprite: &'static str) -> ThingType {
    ThingType {
        name,
        kind,
        color,
        sprite,
        doom2_only: true,
    }
}

// Colour families by category.
const C_START: [u8; 4] = [0x00, 0xb0, 0x40, 0xff];
const C_MONSTER: [u8; 4] = [0xc0, 0x40, 0x30, 0xff];
const C_BOSS: [u8; 4] = [0x90, 0x20, 0x80, 0xff];
const C_WEAPON: [u8; 4] = [0x80, 0x70, 0x20, 0xff];
const C_AMMO: [u8; 4] = [0x50, 0x45, 0x25, 0xff];
const C_HEALTH: [u8; 4] = [0x20, 0x70, 0xc0, 0xff];
const C_ARMOR: [u8; 4] = [0x20, 0xa0, 0x40, 0xff];
const C_POWER: [u8; 4] = [0xc0, 0x40, 0xc0, 0xff];
const C_KEY: [u8; 4] = [0xd0, 0xc0, 0x20, 0xff];
const C_DECOR: [u8; 4] = [0x70, 0x70, 0x70, 0xff];
const C_HAZARD: [u8; 4] = [0xb0, 0x30, 0x20, 0xff];

pub const DEFAULT_THINGS: &[ThingType] = &[
    // Player / multiplayer starts and teleport landing.
    t("Player 1 start", 1, C_START, "PLAY"),
    t("Player 2 start", 2, C_START, "PLAY"),
    t("Player 3 start", 3, C_START, "PLAY"),
    t("Player 4 start", 4, C_START, "PLAY"),
    t("Deathmatch start", 11, [0x40, 0xb0, 0x00, 0xff], "PLAY"),
    t("Teleport exit", 14, [0x00, 0x80, 0xb0, 0xff], ""),
    // Monsters.
    t("Zombieman", 3004, C_MONSTER, "POSS"),
    t("Shotgun guy", 9, C_MONSTER, "SPOS"),
    t2("Heavy weapon dude", 65, C_MONSTER, "CPOS"),
    t("Imp", 3001, C_MONSTER, "TROO"),
    t("Demon", 3002, C_MONSTER, "SARG"),
    t("Spectre", 58, [0x90, 0x90, 0xb0, 0xff], "SARG"),
    t("Lost soul", 3006, [0xc0, 0xa0, 0x40, 0xff], "SKUL"),
    t("Cacodemon", 3005, C_MONSTER, "HEAD"),
    t2("Pain elemental", 71, C_MONSTER, "PAIN"),
    t2("Revenant", 66, C_MONSTER, "SKEL"),
    t2("Mancubus", 67, C_MONSTER, "FATT"),
    t2("Arachnotron", 68, C_MONSTER, "BSPI"),
    t2("Hell knight", 69, C_MONSTER, "BOS2"),
    t("Baron of hell", 3003, C_BOSS, "BOSS"),
    t2("Arch-vile", 64, C_BOSS, "VILE"),
    t("Spiderdemon", 7, C_BOSS, "SPID"),
    t("Cyberdemon", 16, C_BOSS, "CYBR"),
    t2("Wolfenstein SS", 84, C_MONSTER, "SSWV"),
    t2("Commander Keen", 72, C_BOSS, "KEEN"),
    t2("Boss brain", 88, C_BOSS, "BBRN"),
    t2("Boss shooter", 89, C_BOSS, "SSWV"),
    t2("Spawn spot", 87, C_BOSS, ""),
    // Weapons.
    t("Chainsaw", 2005, C_WEAPON, "CSAW"),
    t("Shotgun", 2001, C_WEAPON, "SHOT"),
    t2("Super shotgun", 82, C_WEAPON, "SGN2"),
    t("Chaingun", 2002, C_WEAPON, "MGUN"),
    t("Rocket launcher", 2003, C_WEAPON, "LAUN"),
    t("Plasma gun", 2004, C_WEAPON, "PLAS"),
    t("BFG9000", 2006, C_WEAPON, "BFUG"),
    // Ammo.
    t("Clip", 2007, C_AMMO, "CLIP"),
    t("Box of bullets", 2048, C_AMMO, "AMMO"),
    t("Shells", 2008, C_AMMO, "SHEL"),
    t("Box of shells", 2049, C_AMMO, "SBOX"),
    t("Rocket", 2010, C_AMMO, "ROCK"),
    t("Box of rockets", 2046, C_AMMO, "BROK"),
    t("Cell charge", 2047, C_AMMO, "CELL"),
    t("Cell pack", 17, C_AMMO, "CELP"),
    t("Backpack", 8, C_AMMO, "BPAK"),
    // Health and armor.
    t("Stimpack", 2011, C_HEALTH, "STIM"),
    t("Medikit", 2012, C_HEALTH, "MEDI"),
    t("Health bonus", 2014, [0x30, 0x50, 0xb0, 0xff], "BON1"),
    t("Soulsphere", 2013, C_POWER, "SOUL"),
    t2("Megasphere", 83, C_POWER, "MEGA"),
    t("Armor bonus", 2015, [0x30, 0xb0, 0x50, 0xff], "BON2"),
    t("Green armor", 2018, C_ARMOR, "ARM1"),
    t("Blue armor", 2019, [0x20, 0x20, 0xa0, 0xff], "ARM2"),
    // Powerups.
    t("Invulnerability", 2022, C_POWER, "PINV"),
    t("Berserk", 2023, C_POWER, "PSTR"),
    t("Invisibility", 2024, C_POWER, "PINS"),
    t("Radiation suit", 2025, C_POWER, "SUIT"),
    t("Computer map", 2026, C_POWER, "PMAP"),
    t("Light goggles", 2045, C_POWER, "PVIS"),
    // Keys.
    t("Blue keycard", 5, [0x30, 0x30, 0xe0, 0xff], "BKEY"),
    t("Red keycard", 13, [0xe0, 0x30, 0x30, 0xff], "RKEY"),
    t("Yellow keycard", 6, C_KEY, "YKEY"),
    t("Blue skull key", 40, [0x30, 0x30, 0xe0, 0xff], "BSKU"),
    t("Red skull key", 38, [0xe0, 0x30, 0x30, 0xff], "RSKU"),
    t("Yellow skull key", 39, C_KEY, "YSKU"),
    // Hazards / interactive.
    t("Barrel", 2035, C_HAZARD, "BAR1"),
    // Decorations and obstacles.
    t("Tech column", 48, C_DECOR, "ELEC"),
    t("Tall green pillar", 30, C_DECOR, "COL1"),
    t("Short green pillar", 31, C_DECOR, "COL2"),
    t("Tall red pillar", 32, C_DECOR, "COL3"),
    t("Short red pillar", 33, C_DECOR, "COL4"),
    t("Tall blue torch", 44, C_DECOR, "TBLU"),
    t("Tall green torch", 45, C_DECOR, "TGRN"),
    t("Tall red torch", 46, C_DECOR, "TRED"),
    t("Short blue torch", 55, C_DECOR, "SMBT"),
    t("Short green torch", 56, C_DECOR, "SMGT"),
    t("Short red torch", 57, C_DECOR, "SMRT"),
    t("Floor lamp", 2028, C_DECOR, "COLU"),
    t("Burning barrel", 70, C_HAZARD, "FCAN"),
    t("Evil eye", 41, C_DECOR, "CEYE"),
    t("Floating skull", 42, C_DECOR, "FSKU"),
    t("Gray tree", 43, C_DECOR, "TRE1"),
    t("Large brown tree", 54, C_DECOR, "TRE2"),
    t("Hanging victim, twitching", 49, C_DECOR, "GOR1"),
    t("Hanging victim, arms out", 50, C_DECOR, "GOR2"),
    t("Hanging victim, one leg", 51, C_DECOR, "GOR3"),
    t("Hanging leg", 52, C_DECOR, "GOR4"),
    t("Hanging victim, no guts", 53, C_DECOR, "GOR5"),
    t("Dead player", 15, C_DECOR, "PLAY"),
    t("Dead zombieman", 18, C_DECOR, "POSS"),
    t("Dead shotgun guy", 19, C_DECOR, "SPOS"),
    t("Dead imp", 20, C_DECOR, "TROO"),
    t("Dead demon", 21, C_DECOR, "SARG"),
    t("Dead cacodemon", 22, C_DECOR, "HEAD"),
    t("Dead lost soul", 23, C_DECOR, "SKUL"),
    t("Pool of blood and bones", 24, C_DECOR, "POL5"),
    t("Impaled human", 25, C_DECOR, "POL1"),
    t("Twitching impaled human", 26, C_DECOR, "POL6"),
    t("Skull on a pole", 27, C_DECOR, "POL4"),
    t("Five skulls shish kebab", 28, C_DECOR, "POL2"),
    t("Pile of skulls and candles", 29, C_DECOR, "POL3"),
    t("Candle", 34, C_DECOR, "CAND"),
    t("Candelabra", 35, C_DECOR, "CBRA"),
    t("Bloody mess 1", 10, C_DECOR, "PLAY"),
    t("Bloody mess 2", 12, C_DECOR, "PLAY"),
    t("Pool of blood (large)", 79, C_DECOR, "POL5"),
    t("Pool of blood (small)", 80, C_DECOR, "POL5"),
    t("Pool of brains", 81, C_DECOR, "POL5"),
];

/// Indices into [`DEFAULT_THINGS`] for the current game and available sprites.
pub fn thing_palette(doom2: bool, sprite_present: impl Fn(&str) -> bool) -> Vec<usize> {
    DEFAULT_THINGS
        .iter()
        .enumerate()
        .filter(|(_, t)| doom2 || !t.doom2_only)
        .filter(|(_, t)| t.sprite.is_empty() || sprite_present(t.sprite))
        .map(|(i, _)| i)
        .collect()
}

pub const LAUNCH_THING_KINDS: &[i32] = &[1, 2, 3, 4, 11];

pub fn launch_thing_name(kind: i32) -> &'static str {
    DEFAULT_THINGS
        .iter()
        .find(|t| t.kind == kind)
        .map_or("(unknown)", |t| t.name)
}

pub const DEFAULT_THING_RADIUS: f32 = 20.0;

/// Default flags: present on all skills.
pub const DEFAULT_THING_OPTIONS: ThingFlags = ThingFlags::EASY
    .union(ThingFlags::NORMAL)
    .union(ThingFlags::HARD);
pub const DEFAULT_THING_KIND: i32 = 1;

/// Vanilla collision radius for `kind`; falls back to [`DEFAULT_THING_RADIUS`].
pub fn thing_radius(kind: i32) -> f32 {
    static BY_KIND: OnceLock<HashMap<i32, f32>> = OnceLock::new();
    BY_KIND
        .get_or_init(|| THING_INFO.iter().map(|t| (t.doomednum, t.radius)).collect())
        .get(&kind)
        .copied()
        .unwrap_or(DEFAULT_THING_RADIUS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doom1_palette_drops_doom2_things() {
        let kinds: Vec<i32> = thing_palette(false, |_| true)
            .into_iter()
            .map(|i| DEFAULT_THINGS[i].kind)
            .collect();
        for k in [64, 66, 82, 83] {
            assert!(!kinds.contains(&k), "Doom 2 thing {k} hidden in Doom 1");
        }
        for k in [3001, 16, 1] {
            assert!(kinds.contains(&k), "shared thing {k} kept");
        }
    }

    #[test]
    fn doom2_palette_keeps_all_when_sprites_present() {
        let all = thing_palette(true, |_| true).len();
        assert_eq!(all, DEFAULT_THINGS.len());
    }

    #[test]
    fn missing_sprite_drops_thing_but_keeps_markers() {
        let kinds: Vec<i32> = thing_palette(true, |_| false)
            .into_iter()
            .map(|i| DEFAULT_THINGS[i].kind)
            .collect();
        assert!(kinds.contains(&14), "sprite-less Teleport exit kept");
        assert!(
            !kinds.contains(&3001),
            "Imp dropped when its sprite is absent"
        );
    }

    #[test]
    fn thing_radius_from_generated_table() {
        assert_eq!(thing_radius(3002), 30.0);
        assert_eq!(thing_radius(3004), 20.0);
        assert_eq!(thing_radius(-999), DEFAULT_THING_RADIUS);
    }
}
