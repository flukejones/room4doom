use std::ptr;

use crate::info::MapObjKind;
use crate::level::map_defs::LineDef;
use crate::thinker::ThinkerData;
use crate::{Level, MapObject, MapPtr, Sector};

use crate::thing::MapObjFlag;

/// Doom function name `EV_Teleport`
pub fn teleport(
    line: MapPtr<LineDef>,
    side: usize,
    thing: &mut MapObject,
    level: &mut Level,
) -> bool {
    // Don't teleport missiles... this could be interesting to muck with.
    if thing.flags & MapObjFlag::Missile as u32 != 0 {
        return false;
    }

    if side == 1 {
        return false;
    }

    let tag = line.tag;
    for sector in level.map_data.sectors().iter() {
        if sector.tag == tag {
            // TODO: check teleport move P_TeleportMove
            if let Some(thinker) = level.thinkers.find_thinker(|thinker| {
                // Find the right thinker
                if let &ThinkerData::MapObject(ref mobj) = thinker.data() {
                    if mobj.kind == MapObjKind::MT_TELEPORTMAN
                        && ptr::eq(mobj.subsector.sector.as_ref(), sector)
                    {
                        return true;
                    }
                }
                false
            }) {
                let level = unsafe { &mut *thing.level };

                let old_x = thing.x;
                let old_y = thing.y;
                let old_z = thing.z;
                let endpoint = thinker.mobj();
                if let Some(player) = thing.player_mut() {
                    player.viewz = old_z + player.viewheight;
                }

                if !teleport_move(endpoint.x, endpoint.y, thing, level) {
                    return false;
                }
                thing.z = endpoint.z;

                let fog = MapObject::spawn_map_object(
                    old_x,
                    old_y,
                    old_z as i32,
                    MapObjKind::MT_TFOG,
                    level,
                );
                unsafe {
                    (*fog).start_sound(sound_traits::SfxName::Telept);
                }

                let an = endpoint.angle;
                let fog = MapObject::spawn_map_object(
                    endpoint.x + 20.0 * an.cos(),
                    endpoint.y + 20.0 * an.sin(),
                    endpoint.z as i32,
                    MapObjKind::MT_TFOG,
                    level,
                );
                unsafe {
                    (*fog).start_sound(sound_traits::SfxName::Telept);
                }

                if thing.player().is_some() {
                    thing.reactiontime = 18;
                }
                thing.angle = endpoint.angle;
                thing.momx = 0.0;
                thing.momy = 0.0;
                thing.momz = 0.0;

                return true;
            }
        }
    }

    false
}

/// Doom function nam `P_TeleportMove`
pub fn teleport_move(x: f32, y: f32, thing: &mut MapObject, level: &mut Level) -> bool {
    let new_subsect = &mut *level.map_data.point_in_subsector_raw_xy(x, y);
    let floorz = new_subsect.sector.floorheight;
    let ceilzz = new_subsect.sector.ceilingheight;

    // telefrag if needed
    if !telefrag(thing, x, y, new_subsect.sector.as_mut(), level.options.map) {
        return false;
    }
    unsafe {
        thing.unset_thing_position();
        thing.x = x;
        thing.y = y;
        thing.floorz = floorz;
        thing.ceilingz = ceilzz;
        thing.set_thing_position();
    }
    true
}

fn telefrag(
    this_thing: &mut MapObject,
    new_x: f32,
    new_y: f32,
    sector: &mut Sector,
    game_map: usize,
) -> bool {
    sector.run_mut_func_on_thinglist(move |thing| {
        if thing.flags & MapObjFlag::Shootable as u32 == 0 {
            return true;
        }

        let dist = this_thing.radius + thing.radius;
        if (thing.x - new_x).abs() >= dist || (thing.y - new_y).abs() >= dist {
            return true;
        }

        if this_thing.thinker == thing.thinker {
            return true;
        }

        // monsters don't telefrag things except on boss level
        if this_thing.player().is_none() && game_map != 30 {
            return false;
        }

        if thing.flags & MapObjFlag::Shootable as u32 != 0 {
            thing.p_take_damage(Some(this_thing), None, false, 10000);
        }
        true
    })
}
