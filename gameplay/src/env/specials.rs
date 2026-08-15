//! Implements special effects:
//! Texture animation, height or lighting changes according to adjacent sectors,
//! respective utility functions. Line Tag handling. Line and Sector triggers.
//!
//! Doom source name `p_spec`

use std::ptr;

use crate::doom_def::{ONCEILINGZ, ONFLOORZ};
use crate::env::effects::light_presets::{
    LightPreset, ev_start_light_strobing, ev_turn_light_on, ev_turn_tag_lights_off, spawn_light,
};
use crate::env::effects::mover_presets::{
    ev_stop_platform, spawn_door_close_in_30, spawn_door_raise_in_5_mins,
};
use crate::env::generalized;
use crate::env::switch::start_sector_sound;
use crate::env::teleport::teleport;
use crate::info::{MOBJINFO, MapObjKind};
use crate::level::LevelState;
use crate::pic::ButtonWhere;
use crate::thing::MapObject;
use crate::{MapObjFlag, TICRATE};
use level::map_defs::{LineDef, Sector, SectorHeight};
use level::{MapPtr, MovementType};
use log::{debug, error, trace};
use math::{Angle, FixedT};
use pic_data::PicData;
use sound_common::SfxName;

// BOOM generalized sector type bit masks
#[allow(dead_code)]
const BOOM_DAMAGE_MASK: u32 = 0x60;
#[allow(dead_code)]
const BOOM_DAMAGE_SHIFT: u32 = 5;
const BOOM_SECRET_MASK: u32 = 0x80;

level::byte_enum! {
    /// Vanilla sector-special types (bits 0-4 of `Sector::special`). Damage
    /// types act during play (player think); the rest spawn something at load.
    pub enum SectorSpecial {
        Flicker = 1,
        StrobeFast = 2,
        StrobeSlow = 3,
        StrobeFastDamage = 4,
        Damage10 = 5,
        Damage5 = 7,
        Glow = 8,
        Secret = 9,
        DoorCloseIn30 = 10,
        DamageEnd = 11,
        StrobeSlowSync = 12,
        StrobeFastSync = 13,
        DoorRaiseIn5Mins = 14,
        Damage20 = 16,
        FireFlicker = 17,
    }
}

impl SectorSpecial {
    /// The light this sector type spawns at level load, if any.
    fn light_preset(self) -> Option<LightPreset> {
        match self {
            Self::Flicker => Some(LightPreset::Flash),
            Self::StrobeFast | Self::StrobeFastDamage => Some(LightPreset::StrobeFast),
            Self::StrobeSlow => Some(LightPreset::StrobeSlow),
            Self::Glow => Some(LightPreset::Glow),
            Self::StrobeSlowSync => Some(LightPreset::StrobeSlowSync),
            Self::StrobeFastSync => Some(LightPreset::StrobeFastSync),
            Self::FireFlicker => Some(LightPreset::Fire),
            _ => None,
        }
    }
}

/// OG `P_ChangeSector` -- iterate blockmap cells in sector's bounding box.
fn change_sector(sector: &Sector, crunch: bool, level: &LevelState) -> bool {
    let mut no_fit = false;

    let bm = level.level_data.blockmap();
    let bmw = bm.columns;
    let bmh = bm.rows;

    // OG: sector->blockbox is [top, bottom, left, right]
    for bx in sector.blockbox[2]..=sector.blockbox[3] {
        for by in sector.blockbox[1]..=sector.blockbox[0] {
            if bx < 0 || by < 0 || bx >= bmw || by >= bmh {
                continue;
            }
            let idx = (by * bmw + bx) as usize;
            let mut mobj_ptr = level.blocklinks[idx];
            while let Some(ptr) = mobj_ptr {
                let thing = unsafe { &mut *ptr };
                mobj_ptr = thing.b_next;
                thing.pit_change_sector(&mut no_fit, crunch);
            }
        }
    }

    no_fit
}

/// The result of raising a plane. `PastDest` = stop, `Crushed` = should crush
/// all in sector
#[derive(Debug, Clone, Copy)]
pub enum PlaneResult {
    Ok,
    Crushed,
    PastDest,
}

pub fn move_plane(
    mut sector: MapPtr<Sector>,
    speed: SectorHeight,
    dest: SectorHeight,
    crush: bool,
    floor_or_ceiling: i32,
    direction: i32,
    level: &mut LevelState,
) -> PlaneResult {
    // Split borrow: bsp3d from level_data, blocklinks from level
    let bsp3d = unsafe { &mut *ptr::from_mut(&mut level.level_data.bsp_3d) };
    let sector_num = sector.num as usize;
    match floor_or_ceiling {
        0 => {
            // FLOOR
            match direction {
                -1 => {
                    // DOWN
                    trace!(
                        "move_plane: floor: down: {} to {} at speed {}",
                        sector.floorheight, dest, speed
                    );
                    let last_pos = sector.floorheight;
                    if sector.floorheight - speed < dest {
                        sector.floorheight = dest;
                        bsp3d.move_surface(sector_num, MovementType::Floor, dest.to_f32());

                        if change_sector(&sector, crush, level) {
                            sector.floorheight = last_pos;
                            bsp3d.move_surface(sector_num, MovementType::Floor, last_pos.to_f32());
                            change_sector(&sector, crush, level);
                        }
                        return PlaneResult::PastDest;
                    } else {
                        // COULD GET CRUSHED
                        sector.floorheight -= speed;
                        bsp3d.move_surface(
                            sector_num,
                            MovementType::Floor,
                            sector.floorheight.to_f32(),
                        );
                        // OG: floor-down always reverts on collision (no crush check)
                        if change_sector(&sector, crush, level) {
                            sector.floorheight = last_pos;
                            bsp3d.move_surface(sector_num, MovementType::Floor, last_pos.to_f32());
                            change_sector(&sector, crush, level);
                            return PlaneResult::Crushed;
                        }
                    }
                }
                1 => {
                    // UP
                    trace!(
                        "move_plane: floor: up: {} to {} at speed {}",
                        sector.floorheight, dest, speed
                    );
                    let last_pos = sector.floorheight;
                    if sector.floorheight + speed > dest {
                        sector.floorheight = dest;
                        bsp3d.move_surface(sector_num, MovementType::Floor, dest.to_f32());

                        if change_sector(&sector, crush, level) {
                            sector.floorheight = last_pos;
                            bsp3d.move_surface(sector_num, MovementType::Floor, last_pos.to_f32());
                            change_sector(&sector, crush, level);
                        }
                        return PlaneResult::PastDest;
                    } else {
                        sector.floorheight += speed;
                        bsp3d.move_surface(
                            sector_num,
                            MovementType::Floor,
                            sector.floorheight.to_f32(),
                        );
                        if change_sector(&sector, crush, level) {
                            if crush {
                                return PlaneResult::Crushed;
                            }
                            sector.floorheight = last_pos;
                            bsp3d.move_surface(sector_num, MovementType::Floor, last_pos.to_f32());
                            change_sector(&sector, crush, level);
                            return PlaneResult::Crushed;
                        }
                    }
                }
                _ => error!("Invalid floor direction: {direction}"),
            }
        }
        1 => {
            // CEILING
            match direction {
                -1 => {
                    // DOWN
                    trace!(
                        "move_plane: ceiling: down: {} to {} at speed {}",
                        sector.ceilingheight, dest, speed
                    );
                    let last_pos = sector.ceilingheight;
                    if sector.ceilingheight - speed < dest {
                        sector.ceilingheight = dest;
                        bsp3d.move_surface(sector_num, MovementType::Ceiling, dest.to_f32());

                        if change_sector(&sector, crush, level) {
                            sector.ceilingheight = last_pos;
                            bsp3d.move_surface(
                                sector_num,
                                MovementType::Ceiling,
                                last_pos.to_f32(),
                            );
                            change_sector(&sector, crush, level);
                        }
                        return PlaneResult::PastDest;
                    } else {
                        // COULD GET CRUSHED
                        sector.ceilingheight -= speed;
                        bsp3d.move_surface(
                            sector_num,
                            MovementType::Ceiling,
                            sector.ceilingheight.to_f32(),
                        );

                        if change_sector(&sector, crush, level) {
                            if crush {
                                return PlaneResult::Crushed;
                            }
                            sector.ceilingheight = last_pos;
                            bsp3d.move_surface(
                                sector_num,
                                MovementType::Ceiling,
                                last_pos.to_f32(),
                            );
                            change_sector(&sector, crush, level);
                            return PlaneResult::Crushed;
                        }
                    }
                }
                1 => {
                    // UP
                    trace!(
                        "move_plane: ceiling: up: {} to {} at speed {}",
                        sector.ceilingheight, dest, speed
                    );
                    // OG: strictly greater (not >=)
                    if sector.ceilingheight + speed > dest {
                        let last_pos = sector.ceilingheight;
                        sector.ceilingheight = dest;
                        bsp3d.move_surface(sector_num, MovementType::Ceiling, dest.to_f32());

                        if change_sector(&sector, crush, level) {
                            sector.ceilingheight = last_pos;
                            bsp3d.move_surface(
                                sector_num,
                                MovementType::Ceiling,
                                last_pos.to_f32(),
                            );
                            change_sector(&sector, crush, level);
                        }
                        return PlaneResult::PastDest;
                    } else {
                        sector.ceilingheight += speed;
                        bsp3d.move_surface(
                            sector_num,
                            MovementType::Ceiling,
                            sector.ceilingheight.to_f32(),
                        );
                        change_sector(&sector, crush, level);
                    }
                }
                _ => error!("Invalid ceiling direction: {direction}"),
            }
        }
        _ => error!("Invalid floor_or_ceiling: {floor_or_ceiling}"),
    }

    PlaneResult::Ok
}

/// Trigger various actions when a line is crossed which has a non-zero special
/// attached
///
/// Doom function name is `P_CrossSpecialLine`
pub fn cross_special_line(side: usize, mut line: MapPtr<LineDef>, thing: &mut MapObject) {
    let mut ok = false;

    //  Triggers that other things can activate
    if thing.player().is_none() {
        // Things that should NOT trigger specials...
        match thing.kind {
            MapObjKind::MT_ROCKET
            | MapObjKind::MT_PLASMA
            | MapObjKind::MT_BFG
            | MapObjKind::MT_TROOPSHOT
            | MapObjKind::MT_HEADSHOT
            | MapObjKind::MT_BRUISERSHOT => return,
            _ => {}
        }

        // Teleports stay vanilla; the monster-openable door/lift movers
        // (vanilla 4/10/88) are generalized and carry the encoded monster bit,
        // checked in handle_generalized_cross.
        if matches!(
            line.special,
            39    // TELEPORT TRIGGER
            | 97  // TELEPORT RETRIGGER
            | 125 // TELEPORT MONSTERONLY TRIGGER
            | 126 // TELEPORT MONSTERONLY RETRIGGER
        ) || generalized::is_generalized(line.special)
        {
            ok = true;
        }

        if !ok {
            return;
        }
    }

    assert!(!thing.level.is_null(), "Thing had a bad level pointer");
    let level: &mut LevelState = unsafe { &mut *thing.level };

    // BOOM generalized linedef types
    if generalized::is_generalized(line.special) {
        let is_monster = thing.player().is_none();
        generalized::handle_generalized_cross(line, thing, level, is_monster);
        return;
    }

    match line.special {
        52 => {
            level.do_exit_level();
        }
        124 => {
            level.do_secret_exit_level();
        }
        12 => {
            debug!(
                "line-special #{}: turn light on nearest bright!",
                line.special
            );
            ev_turn_light_on(line.clone(), 0, level);
            line.special = 0;
        }
        13 => {
            debug!("line-special #{}: turn light on 255!", line.special);
            ev_turn_light_on(line.clone(), 255, level);
            line.special = 0;
        }
        35 => {
            debug!("line-special #{}: turn light off!", line.special);
            ev_turn_light_on(line.clone(), 35, level);
            line.special = 0;
        }
        79 => {
            debug!("line-special #{}: turn light off!", line.special);
            ev_turn_light_on(line.clone(), 35, level);
        }
        80 => {
            debug!(
                "line-special #{}: turn light on nearest bright!",
                line.special
            );
            ev_turn_light_on(line, 0, level);
        }
        81 => {
            debug!("line-special #{}: turn light on 255!", line.special);
            ev_turn_light_on(line, 255, level);
        }
        17 => {
            debug!("line-special #{}: start light strobe!", line.special);
            ev_start_light_strobing(line.clone(), level);
            line.special = 0;
        }
        104 => {
            debug!(
                "line-special #{}: turn lights off in sector tag!",
                line.special
            );
            ev_turn_tag_lights_off(line.clone(), level);
            line.special = 0;
        }
        125
            // TELEPORT MonsterONLY
            if thing.player().is_none() => {
                teleport(line.clone(), side, thing, level);
                line.special = 0;
            }
        39 => {
            teleport(line.clone(), side, thing, level);
            line.special = 0;
        }
        54 => {
            ev_stop_platform(line.clone(), level);
            line.special = 0;
        }
        89 => {
            ev_stop_platform(line.clone(), level);
        }
        97 => {
            teleport(line, side, thing, level);
        }
        126
            // TELEPORT MonsterONLY
            if thing.player().is_none() => {
                teleport(line.clone(), side, thing, level);
            }
        _ => {}
    }
}

/// Actions for when a thing shoots a special line
///
/// Doom function name `P_ShootSpecialLine`
pub fn shoot_special_line(line: MapPtr<LineDef>, thing: &mut MapObject) {
    assert!(!thing.level.is_null(), "Thing had a bad level pointer");
    let level: &mut LevelState = unsafe { &mut *thing.level };

    // Every shootable vanilla special is a mover, so all arrive in generalized
    // form; monster permission is the encoded monster bit (vanilla 46 only).
    if generalized::is_generalized(line.special) {
        generalized::handle_generalized_shoot(line, thing, level);
    }
}

pub fn spawn_specials(level: &mut LevelState) {
    // TODO: deathmatch -timer never seeds level_timer, so timed matches never end; seed from GameOptions here.

    let level_iter = unsafe { &mut *ptr::from_mut(level) };
    for sector in level_iter
        .level_data
        .sectors
        .iter_mut()
        .filter(|s| s.special != 0)
    {
        // BOOM: bits 5-11 encode generalized sector properties.
        // Vanilla type is in bits 0-4.
        let vanilla_type = (sector.special & 0x1F) as u8;

        // BOOM secret (bit 7)
        if sector.special & BOOM_SECRET_MASK != 0 {
            level.total_level_secrets += 1;
        }

        let Ok(special) = SectorSpecial::try_from(vanilla_type) else {
            continue;
        };
        if let Some(preset) = special.light_preset() {
            debug!("sector-special #{}: {special:?}", sector.special);
            spawn_light(sector, preset, level);
            // The light spawn cleared the special; type 4 keeps its damage.
            if matches!(special, SectorSpecial::StrobeFastDamage) {
                sector.special = SectorSpecial::StrobeFastDamage as u32;
            }
            continue;
        }
        match special {
            SectorSpecial::Secret => {
                // Only count vanilla secret if BOOM secret bit is not set
                // (BOOM secret already counted above)
                if sector.special & BOOM_SECRET_MASK == 0 {
                    level.total_level_secrets += 1;
                }
            }
            SectorSpecial::DoorCloseIn30 => spawn_door_close_in_30(sector, level),
            SectorSpecial::DoorRaiseIn5Mins => spawn_door_raise_in_5_mins(sector, level),
            _ => {}
        }
    }

    for line in level_iter.level_data.linedefs.iter_mut() {
        // Scrolling wall
        if line.special == 48 {
            level.line_special_list.push(MapPtr::new(line));
        }
    }
}

/// Doom function name `P_UpdateSpecials`
pub fn update_specials(level: &mut LevelState, pic_data: &mut PicData) {
    // Used mostly for deathmatch as far as I know
    if level.level_timer && level.level_time == 0 {
        // exit
        level.do_exit_level();
    }

    // Flats and wall texture animations (switching between series)
    for anim in &mut level.animations {
        anim.update(pic_data, level.level_time as usize);
    }

    // Animate switches
    for b in &mut level.button_list {
        if b.timer != 0 {
            b.timer -= 1;
            if b.timer == 0 {
                debug!("Button {:?} is switching after countdown", b.line.as_ref());
                match b.bwhere {
                    ButtonWhere::Top => {
                        if let Some(t) = b.line.front_sidedef.toptexture.as_mut() {
                            *t = b.texture;
                        }
                    }
                    ButtonWhere::Middle => {
                        if let Some(t) = b.line.front_sidedef.midtexture.as_mut() {
                            *t = b.texture;
                        }
                    }
                    ButtonWhere::Bottom => {
                        if let Some(t) = b.line.front_sidedef.bottomtexture.as_mut() {
                            *t = b.texture;
                        }
                    }
                }
                level.level_data.bsp_3d.update_wall_texture(b.line.num);
                start_sector_sound(&b.line, SfxName::Swtchn, &level.snd_command);
            }
        }
    }
    for i in 0..level.line_special_list.len() {
        let line = &mut level.line_special_list[i];
        line.front_sidedef.textureoffset += 1;
        if line.front_sidedef.textureoffset == FixedT::MAX {
            line.front_sidedef.textureoffset = FixedT::ZERO;
        }
        // Special-48 scrollers start at offset 0 (the build-baked UV), so the
        // live offset is the scroll delta (matches software25d).
        let num = line.num;
        let scroll = f32::from(line.front_sidedef.textureoffset);
        level.level_data.bsp_3d.set_wall_scroll(num, scroll);
    }
}

/// P_RespawnSpecials
pub fn respawn_specials(level: &mut LevelState) {
    // only respawn items in deathmatch
    if level.options.deathmatch == 0 && !level.options.respawn_monsters {
        return;
    }

    if let Some(mthing) = level.respawn_queue.back() {
        // wait at least 30 seconds
        if (level.level_time - mthing.0) / (TICRATE as u32) < 30 {
            return;
        }
    } else {
        return;
    }

    if let Some(mthing) = level.respawn_queue.pop_back() {
        // spawn a teleport fog at the new spot
        // OG: mthing->x/y are i16 map units, << 16 for fixed-point
        let ss = level.level_data.point_in_subsector(
            FixedT::from(mthing.1.x as i32),
            FixedT::from(mthing.1.y as i32),
        );
        let floor = ss.sector.floorheight.to_i32();
        let fog = unsafe {
            &mut *MapObject::spawn_map_object(
                FixedT::from(mthing.1.x as i32),
                FixedT::from(mthing.1.y as i32),
                floor.into(),
                MapObjKind::MT_TFOG,
                level,
            )
        };
        fog.start_sound(SfxName::Itmbk);

        let mut i = 0;
        for n in 0..MapObjKind::Count as u16 {
            if mthing.1.kind == MOBJINFO[n as usize].doomednum as i16 {
                i = n;
                break;
            }
        }

        if i == MapObjKind::Count as u16 {
            error!(
                "P_SpawnMapThing: Unknown type {} at ({}, {})",
                mthing.1.kind, mthing.1.x, mthing.1.y
            );
        }

        let kind = MapObjKind::from(i);

        let z = if MOBJINFO[i as usize]
            .flags
            .contains(MapObjFlag::Spawnceiling)
        {
            ONCEILINGZ
        } else {
            ONFLOORZ
        };

        // spawn it
        let thing = unsafe {
            &mut *MapObject::spawn_map_object(
                FixedT::from(mthing.1.x as i32),
                FixedT::from(mthing.1.y as i32),
                z.into(),
                kind,
                level,
            )
        };
        // OG: mobj->angle = ANG45 * (mthing->angle/45)
        thing.angle = Angle::from_bam(math::ANG45.wrapping_mul((mthing.1.angle as u32) / 45));
        thing.spawnpoint = mthing.1;
    }
}

#[cfg(test)]
mod env_tests {
    use crate::MapObjKind;
    use crate::env::specials::cross_special_line;
    use crate::test_support::{TestLevel, rng_guard};
    use level::MapPtr;
    use math::get_prndindex;

    fn tag_sector(level: &TestLevel, tag: i16) -> usize {
        level
            .level_data()
            .sectors
            .iter()
            .position(|s| s.tag == tag)
            .expect("sector with tag")
    }

    /// E1M1 ld195: walk-triggered lift (tag 2). Pins the lower/hold/return cycle.
    #[test]
    fn e1m1_lift_lowers_then_returns() {
        let _g = rng_guard();
        let make = || {
            let mut level = TestLevel::load("E1M1");
            let player = level.spawn_ptr(1056, -3616, MapObjKind::MT_PLAYER);
            let ld = MapPtr::new(&mut level.level_data_mut().linedefs[195]);
            cross_special_line(0, ld, unsafe { &mut *player });
            level
        };
        let sec = tag_sector(&make(), 2);

        let mut a = make();
        a.run_level_tics(16);
        assert_eq!(
            a.level_data().sectors[sec].floorheight.to_f32(),
            40.0,
            "descending"
        );

        let mut b = make();
        b.run_level_tics(70);
        assert_eq!(
            b.level_data().sectors[sec].floorheight.to_f32(),
            -48.0,
            "bottom"
        );

        let mut c = make();
        c.run_level_tics(245);
        assert_eq!(
            c.level_data().sectors[sec].floorheight.to_f32(),
            104.0,
            "returned"
        );
    }

    /// E1M1 sector 44 glow (special 8): deterministic light ramp, 8/tic.
    #[test]
    fn e1m1_glow_light_ramps_deterministically() {
        let _g = rng_guard();
        let mut level = TestLevel::load("E1M1");
        level.spawn_level_specials();
        assert_eq!(level.level_data().sectors[44].lightlevel, 255);
        level.run_level_tics(1);
        assert_eq!(level.level_data().sectors[44].lightlevel, 247);
        level.run_level_tics(1);
        assert_eq!(level.level_data().sectors[44].lightlevel, 239);
    }

    /// E1M1 sector 40 flicker (special 1): RNG-driven. Pins light + RNG index.
    #[test]
    fn e1m1_flicker_light_rng_fingerprint() {
        let _g = rng_guard();
        let mut level = TestLevel::load("E1M1");
        level.spawn_level_specials();
        level.run_level_tics(4);
        assert_eq!(level.level_data().sectors[40].lightlevel, 144);
        assert_eq!(get_prndindex(), 2);
    }
}
