//! Sector light animation for the canvas. Port of `gameplay/src/env/lights.rs`.

use editor_core::EditorMap;
use math::p_random;

const MAX_LIGHT: i32 = 255;
/// `GLOWSPEED`
const GLOW_SPEED: i32 = 8;
/// `STROBEBRIGHT`
const STROBE_BRIGHT: i32 = 5;
/// `FASTDARK`
const STROBE_FAST: i32 = 15;
/// `SLOWDARK`
const STROBE_SLOW: i32 = 35;

/// Vanilla sector light kind, from `special & 0x1F`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LightEffect {
    Flicker,
    StrobeFast,
    StrobeSlow,
    StrobeFastSync,
    StrobeSlowSync,
    Glow,
    FireFlicker,
}

impl LightEffect {
    fn from_special(special: i32) -> Option<Self> {
        match special & 0x1F {
            1 => Some(Self::Flicker),
            2 | 4 => Some(Self::StrobeFast),
            3 => Some(Self::StrobeSlow),
            13 => Some(Self::StrobeFastSync),
            12 => Some(Self::StrobeSlowSync),
            8 => Some(Self::Glow),
            17 => Some(Self::FireFlicker),
            _ => None,
        }
    }
}

pub(crate) struct SectorLight {
    pub sector: usize,
    pub current: i32,
    effect: LightEffect,
    max_light: i32,
    min_light: i32,
    count: i32,
    dark_time: i32,
    direction: i32,
}

/// `P_FindMinSurroundingLight` per sector.
fn sector_neighbor_min(map: &EditorMap) -> Vec<i32> {
    let mut neighbor_min: Vec<i32> = map.sectors.iter().map(|s| s.light_level).collect();
    for line in &map.lines {
        let front = line.front.sector.map(|s| s as usize);
        let back = line
            .back
            .as_ref()
            .and_then(|b| b.sector)
            .map(|s| s as usize);
        for &(side, other) in &[(front, back), (back, front)] {
            let (Some(side), Some(o)) = (side, other) else {
                continue;
            };
            let Some(min) = neighbor_min.get_mut(side) else {
                continue;
            };
            if let Some(s) = map.sectors.get(o)
                && s.light_level < *min
            {
                *min = s.light_level;
            }
        }
    }
    neighbor_min
}

/// Build light list. Mirrors `P_Spawn*` spawn state; `p_random` in sector order.
pub(crate) fn build(map: &EditorMap) -> Vec<SectorLight> {
    let neighbor_mins = sector_neighbor_min(map);
    let mut lights = Vec::new();
    for (sector, s) in map.sectors.iter().enumerate() {
        let Some(effect) = LightEffect::from_special(s.special) else {
            continue;
        };
        let max_light = s.light_level;
        let neighbor_min = neighbor_mins[sector];
        let strobe_min = strobe_min(neighbor_min, max_light);
        let (min_light, count, dark_time, direction) = match effect {
            LightEffect::Flicker => (neighbor_min, (p_random() & 64) + 1, 0, 0),
            LightEffect::StrobeFast => (strobe_min, (p_random() & 7) + 1, STROBE_FAST, 0),
            LightEffect::StrobeSlow => (strobe_min, (p_random() & 7) + 1, STROBE_SLOW, 0),
            LightEffect::StrobeFastSync => (strobe_min, 1, STROBE_FAST, 0),
            LightEffect::StrobeSlowSync => (strobe_min, 1, STROBE_SLOW, 0),
            LightEffect::Glow => (neighbor_min, 0, 0, -1),
            LightEffect::FireFlicker => (neighbor_min + 16, 4, 0, 0),
        };
        lights.push(SectorLight {
            sector,
            current: max_light,
            effect,
            max_light,
            min_light,
            count,
            dark_time,
            direction,
        });
    }
    lights
}

/// `P_SpawnStrobeFlash`: collapse to 0 when min == max.
fn strobe_min(min: i32, max: i32) -> i32 {
    if min == max { 0 } else { min }
}

pub(crate) fn tic(lights: &mut [SectorLight]) {
    for light in lights {
        match light.effect {
            LightEffect::Flicker => tic_flicker(light),
            LightEffect::Glow => tic_glow(light),
            LightEffect::FireFlicker => tic_fire_flicker(light),
            _ => tic_strobe(light),
        }
    }
}

/// `P_LightFlash`.
fn tic_flicker(light: &mut SectorLight) {
    light.count -= 1;
    if light.count != 0 {
        return;
    }
    if light.current == light.max_light {
        light.current = light.min_light;
        light.count = (p_random() & 7) + 1;
    } else {
        light.current = light.max_light;
        light.count = (p_random() & 64) + 1;
    }
}

/// `P_StrobeFlash`.
fn tic_strobe(light: &mut SectorLight) {
    light.count -= 1;
    if light.count != 0 {
        return;
    }
    if light.current == light.min_light {
        light.current = light.max_light;
        light.count = STROBE_BRIGHT;
    } else {
        light.current = light.min_light;
        light.count = light.dark_time;
    }
}

/// `P_GlowingLight`.
fn tic_glow(light: &mut SectorLight) {
    match light.direction {
        -1 => {
            light.current -= GLOW_SPEED;
            if light.current <= light.min_light {
                light.current += GLOW_SPEED;
                light.direction = 1;
            }
        }
        1 => {
            light.current += GLOW_SPEED;
            if light.current >= light.max_light {
                light.current -= GLOW_SPEED;
                light.direction = -1;
            }
        }
        _ => {}
    }
}

/// `P_FireFlicker`.
fn tic_fire_flicker(light: &mut SectorLight) {
    light.count -= 1;
    if light.count != 0 {
        return;
    }
    let amount = (p_random() & 3) * 16;
    if light.current >= amount {
        light.current = (light.max_light - amount).clamp(0, MAX_LIGHT);
    }
    if light.current < light.min_light {
        light.current = light.min_light;
    }
    light.count = 4;
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor_core::{LineDef, LineFlags, Name8, SideDef, Vertex};

    fn side(sector: u32) -> SideDef {
        SideDef {
            x_offset: 0,
            y_offset: 0,
            top_tex: Name8::EMPTY,
            bottom_tex: Name8::EMPTY,
            middle_tex: Name8::EMPTY,
            sector: Some(sector),
        }
    }

    fn sector(light_level: i32, special: i32) -> editor_core::Sector {
        editor_core::Sector {
            floor_height: 0,
            floor_flat: Name8::EMPTY,
            ceil_height: 128,
            ceil_flat: Name8::EMPTY,
            light_level,
            special,
            tag: 0,
        }
    }

    fn two_sector_map(special: i32) -> EditorMap {
        let mut map = EditorMap {
            vertices: vec![
                Vertex {
                    x: 0.0,
                    y: 0.0,
                },
                Vertex {
                    x: 64.0,
                    y: 0.0,
                },
                Vertex {
                    x: 64.0,
                    y: 64.0,
                },
                Vertex {
                    x: 0.0,
                    y: 64.0,
                },
            ],
            sectors: vec![sector(192, special), sector(96, 0)],
            ..Default::default()
        };
        let edges = [(0u32, 1u32), (1, 2), (2, 3)];
        for (v1, v2) in edges {
            map.lines.push(LineDef {
                v1,
                v2,
                flags: LineFlags::empty(),
                special: 0,
                tag: 0,
                front: side(0),
                back: None,
            });
        }
        map.lines.push(LineDef {
            v1: 3,
            v2: 0,
            flags: LineFlags::empty(),
            special: 0,
            tag: 0,
            front: side(0),
            back: Some(side(1)),
        });
        map
    }

    #[test]
    fn build_maps_special_to_effect() {
        for (special, want) in [
            (1, Some(LightEffect::Flicker)),
            (2, Some(LightEffect::StrobeFast)),
            (3, Some(LightEffect::StrobeSlow)),
            (4, Some(LightEffect::StrobeFast)),
            (8, Some(LightEffect::Glow)),
            (12, Some(LightEffect::StrobeSlowSync)),
            (13, Some(LightEffect::StrobeFastSync)),
            (17, Some(LightEffect::FireFlicker)),
            (9, None),
            (0, None),
        ] {
            let lights = build(&two_sector_map(special));
            match want {
                Some(effect) => {
                    assert_eq!(lights.len(), 1, "special {special} spawns one light");
                    assert_eq!(lights[0].effect, effect, "special {special}");
                }
                None => assert!(lights.is_empty(), "special {special} is not a light"),
            }
        }
    }

    #[test]
    fn neighbor_min_takes_darker_adjacent_sector() {
        let lights = build(&two_sector_map(8));
        assert_eq!(lights[0].min_light, 96);
        assert_eq!(lights[0].max_light, 192);
    }

    #[test]
    fn strobe_zeroes_min_when_equal_to_max() {
        let mut map = two_sector_map(2);
        map.sectors[1].light_level = 192;
        let lights = build(&map);
        assert_eq!(lights[0].min_light, 0);
    }
}
