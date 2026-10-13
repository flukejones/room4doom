//! Implements special effects:
//! Texture animation, height or lighting changes according to adjacent sectors,
//! respective utility functions. Line Tag handling. Line and Sector triggers.
//!
//! Doom source name `p_spec`

use std::ptr;

use crate::doom_def::{ONCEILINGZ, ONFLOORZ};
use crate::env::effects::light_presets::{LightPreset, spawn_light};
use crate::env::effects::mover_presets::{spawn_door_close_in_30, spawn_door_raise_in_5_mins};
use crate::env::switch::start_sector_sound;
use crate::info::{MOBJINFO, MapObjKind};
use crate::level::LevelState;
use crate::pic::ButtonWhere;
use crate::thing::MapObject;
use crate::{MapObjFlag, TICRATE};
use level::map_defs::{Sector, SectorHeight};
use level::special_encode::{LineEffect, LineSpecial, SectorKind, SectorSpecial};
use level::{MapPtr, MovementType};
use log::{debug, error, trace};
use math::{Angle, FixedT};
use pic_data::PicData;
use sound_common::SfxName;

/// The light a sector type spawns at level load, if any.
fn light_preset(kind: SectorKind) -> Option<LightPreset> {
    match kind {
        SectorKind::Flicker => Some(LightPreset::Flash),
        SectorKind::StrobeFast | SectorKind::StrobeFastDamage => Some(LightPreset::StrobeFast),
        SectorKind::StrobeSlow => Some(LightPreset::StrobeSlow),
        SectorKind::Glow => Some(LightPreset::Glow),
        SectorKind::StrobeSlowSync => Some(LightPreset::StrobeSlowSync),
        SectorKind::StrobeFastSync => Some(LightPreset::StrobeFastSync),
        SectorKind::FireFlicker => Some(LightPreset::Fire),
        _ => None,
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

pub fn spawn_specials(level: &mut LevelState) {
    // TODO: deathmatch -timer never seeds level_timer, so timed matches never end; seed from GameOptions here.

    let level_iter = unsafe { &mut *ptr::from_mut(level) };
    for sector in level_iter
        .level_data
        .sectors
        .iter_mut()
        .filter(|s| !s.special.is_inert())
    {
        if sector.special.secret {
            level.total_level_secrets += 1;
        }
        let Some(kind) = sector.special.kind else {
            continue;
        };
        if let Some(preset) = light_preset(kind) {
            debug!("sector-special: {kind:?}");
            spawn_light(sector, preset, level);
            // The light spawn cleared the special; type 4 keeps its damage,
            // and OG restores exactly 4 (dropping any BOOM bits with it).
            if matches!(kind, SectorKind::StrobeFastDamage) {
                sector.special = SectorSpecial {
                    kind: Some(SectorKind::StrobeFastDamage),
                    ..SectorSpecial::default()
                };
            }
            continue;
        }
        match kind {
            // The BOOM secret bit already counted this one.
            SectorKind::Secret => {
                if !sector.special.secret {
                    level.total_level_secrets += 1;
                }
            }
            SectorKind::DoorCloseIn30 => spawn_door_close_in_30(sector, level),
            SectorKind::DoorRaiseIn5Mins => spawn_door_raise_in_5_mins(sector, level),
            _ => {}
        }
    }

    for line in level_iter.level_data.linedefs.iter_mut() {
        if line.special == Some(LineSpecial::Effect(LineEffect::Scroll)) {
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
    use crate::env::activate::activate_line;
    use crate::test_support::{TestLevel, rng_guard};
    use level::MapPtr;
    use level::special_encode::Activation;
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
            activate_line(ld, unsafe { &mut *player }, 0, Activation::Cross);
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
