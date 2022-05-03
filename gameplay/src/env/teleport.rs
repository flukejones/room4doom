use std::ptr;

use glam::Vec2;

use crate::{
    info::MapObjKind, level::map_defs::LineDef, thinker::ThinkerData, DPtr, Level, MapObject,
    Sector,
};

use crate::obj::MapObjFlag;

/// Doom function name `EV_Teleport`
pub fn teleport(
    line: DPtr<LineDef>,
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
                if let ThinkerData::MapObject(ref mobj) = thinker.data() {
                    unsafe {
                        if mobj.kind == MapObjKind::MT_TELEPORTMAN
                            && ptr::eq((*mobj.subsector).sector.as_ref(), sector)
                        {
                            return true;
                        }
                    }
                }
                false
            }) {
                let level = unsafe { &mut *thing.level };

                let old_xy = thing.xy;
                let old_z = thing.z;
                let endpoint = thinker.mobj();
                if let Some(player) = thing.player_mut() {
                    player.viewz = old_z + player.viewheight;
                }

                teleport_move(endpoint.xy, thing, level);
                thing.z = endpoint.z;

                let fog = MapObject::spawn_map_object(
                    old_xy.x,
                    old_xy.y,
                    old_z as i32,
                    MapObjKind::MT_TFOG,
                    level,
                );
                unsafe {
                    (*fog).start_sound(sound_traits::SfxEnum::Telept);
                }

                let an = endpoint.angle;
                let fog = MapObject::spawn_map_object(
                    endpoint.xy.x + 20.0 * an.cos(),
                    endpoint.xy.y + 20.0 * an.sin(),
                    endpoint.z as i32,
                    MapObjKind::MT_TFOG,
                    level,
                );
                unsafe {
                    (*fog).start_sound(sound_traits::SfxEnum::Telept);
                }

                if thing.player().is_some() {
                    thing.reactiontime = 18;
                }
                thing.angle = endpoint.angle;
                thing.momxy = Vec2::default();
                thing.momz = 0.0;

                return true;
            }
        }
    }

    false
}

/// Doom function nam `P_TeleportMove`
fn teleport_move(xy: Vec2, thing: &mut MapObject, level: &mut Level) -> bool {
    let new_subsect = unsafe { &mut *level.map_data.point_in_subsector_raw(xy) };
    let floorz = new_subsect.sector.floorheight;
    let ceilzz = new_subsect.sector.ceilingheight;

    // telefrag if needed

    unsafe {
        thing.unset_thing_position();

        telefrag_others(thing, new_subsect.sector.as_mut(), level.game_map);

        thing.xy = xy;
        thing.floorz = floorz;
        thing.ceilingz = ceilzz;
        thing.set_thing_position();
    }
    false
}

fn telefrag_others(this_thing: &mut MapObject, sector: &mut Sector, game_map: i32) {
    // monsters don't stomp things except on boss level
    if this_thing.player().is_none() && game_map != 30 {
        return;
    }

    let thing_xy = this_thing.xy;
    sector.run_func_on_thinglist(move |thing| {
        let dist = this_thing.radius + thing.radius;
        if (thing.xy.x - thing_xy.x).abs() >= dist || (thing.xy.y - thing_xy.y).abs() >= dist {
            return true;
        }

        if thing.flags & MapObjFlag::Shootable as u32 != 0 {
            thing.p_take_damage(Some(this_thing), None, false, 10000);
        }
        true
    });
}
