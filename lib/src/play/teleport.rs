use std::ptr;

use glam::Vec2;
use log::{error, trace};

use crate::{
    info::MapObjectType, level_data::map_defs::LineDef, play::d_thinker::ObjectType, DPtr, Level,
    MapObject, Sector,
};

use super::map_object::MobjFlag;

/// Doom function name `EV_Teleport`
pub fn teleport(
    line: DPtr<LineDef>,
    side: usize,
    thing: &mut MapObject,
    level: &mut Level,
) -> bool {
    // Don't teleport missiles... this could be interesting to muck with.
    if thing.flags & MobjFlag::MISSILE as u32 != 0 {
        return false;
    }

    if side == 1 {
        return false;
    }

    let tag = line.tag;
    for sector in level.map_data.sectors.iter() {
        if sector.tag == tag {
            // TODO: check teleport move P_TeleportMove
            if let Some(thinker) = level.thinkers.find_thinker(|thinker| {
                // Find the right thinker
                if let &ObjectType::Mobj(ref mobj) = thinker.obj_type() {
                    unsafe {
                        if (*mobj.subsector).sector.as_ptr()
                            == sector as *const Sector as *mut Sector
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
                let endpoint = thinker.obj_mut::<MapObject>();
                if let Some(ref mut player) = thing.player {
                    unsafe {
                        player.as_mut().viewz = thing.z + player.as_ref().viewheight;
                    }
                }

                teleport_move(endpoint.xy, thing, level);
                thing.z = endpoint.z;

                let fog = MapObject::spawn_map_object(
                    old_xy.x(),
                    old_xy.y(),
                    old_z as i32,
                    MapObjectType::MT_TFOG,
                    level,
                );
                // TODO: S_StartSound(fog, sfx_telept);

                let an = endpoint.angle;
                let fog = MapObject::spawn_map_object(
                    endpoint.xy.x() + 20.0 * an.cos(),
                    endpoint.xy.y() + 20.0 * an.sin(),
                    endpoint.z as i32,
                    MapObjectType::MT_TFOG,
                    level,
                );
                // TODO: S_StartSound(fog, sfx_telept);

                if thing.player.is_some() {
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
    let new_subsect = unsafe { &*level.map_data.point_in_subsector_ref(xy) };
    let floorz = new_subsect.sector.floorheight;
    let ceilzz = new_subsect.sector.ceilingheight;

    // telefrag if needed

    unsafe {
        thing.unset_thing_position();

        telefrag_others(thing, &new_subsect.sector, level.game_map);

        thing.xy = xy;
        thing.floorz = floorz;
        thing.ceilingz = ceilzz;
        thing.set_thing_position();
    }
    false
}

fn telefrag_others(this_thing: &mut MapObject, sector: &Sector, game_map: u32) {
    if !sector.thinglist.is_null() {
        let mut thing = sector.thinglist;
        unsafe {
            while !(thing == (*thing).s_next) && !(*thing).s_next.is_null() {
                trace!("Thing type {:?} is getting telefragged", (*thing).kind);
                let other_thing = &mut *thing;
                if other_thing.flags & MobjFlag::SHOOTABLE as u32 == 0 {
                    thing = (*thing).s_next;
                    continue;
                }

                // Monsters can't telefrag things unless it's the boss level
                if this_thing.player.is_none() && game_map != 30 {
                    break;
                }

                other_thing.p_take_damage(Some(this_thing), None, 10000);

                thing = (*thing).s_next;
            }
        }
    }
}
