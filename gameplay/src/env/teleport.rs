use std::ptr;

use math::FixedT;

use crate::info::MapObjKind;
use crate::thinker::ThinkerData;
use crate::{LevelState, MapObject};
use level::MapPtr;
use level::map_defs::LineDef;

use crate::thing::MapObjFlag;

/// Doom function name `EV_Teleport`
pub fn teleport(
    line: MapPtr<LineDef>,
    side: usize,
    thing: &mut MapObject,
    level: &mut LevelState,
) -> bool {
    // Don't teleport missiles... this could be interesting to muck with.
    if thing.flags.contains(MapObjFlag::Missile) {
        return false;
    }

    if side == 1 {
        return false;
    }

    let tag = line.tag;
    for sector in level.level_data.sectors().iter() {
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

                let fog =
                    MapObject::spawn_map_object(old_x, old_y, old_z, MapObjKind::MT_TFOG, level);
                unsafe {
                    (*fog).start_sound(sound_common::SfxName::Telept);
                }

                let bam = endpoint.angle.to_bam();
                let fog_x = endpoint.x + FixedT::from(20).fixed_mul(FixedT::cos_bam(bam));
                let fog_y = endpoint.y + FixedT::from(20).fixed_mul(FixedT::sin_bam(bam));
                let fog = MapObject::spawn_map_object(
                    fog_x,
                    fog_y,
                    endpoint.z,
                    MapObjKind::MT_TFOG,
                    level,
                );
                unsafe {
                    (*fog).start_sound(sound_common::SfxName::Telept);
                }

                thing.angle = endpoint.angle;
                thing.momx = FixedT::ZERO;
                thing.momy = FixedT::ZERO;
                thing.momz = FixedT::ZERO;

                // Snap prev position so interpolation doesn't slide from old location
                thing.prev_x = thing.x;
                thing.prev_y = thing.y;
                thing.prev_z = thing.z;

                if thing.player().is_some() {
                    thing.reactiontime = 18;
                }
                if let Some(player) = thing.player_mut() {
                    player.save_prev_render();
                }

                return true;
            }
        }
    }

    false
}

/// OG Doom MAPBLOCKSHIFT = FRACBITS + 7 = 23
const MAPBLOCKSHIFT: i32 = 23;
/// OG Doom MAXRADIUS in 16.16 fixed-point = 32 << 16
const MAXRADIUS_FIXED: i32 = 32 << 16;

/// Doom function name `P_TeleportMove`
pub fn teleport_move(x: FixedT, y: FixedT, thing: &mut MapObject, level: &mut LevelState) -> bool {
    let new_subsect = &mut *level.level_data.point_in_subsector(x, y);
    let floorz = new_subsect.sector.floorheight;
    let ceilzz = new_subsect.sector.ceilingheight;

    // OG P_TeleportMove: iterate blockmap cells with MAXRADIUS extension
    let bm = level.level_data.blockmap();
    let orgx = bm.x_origin;
    let orgy = bm.y_origin;
    let bmw = bm.columns;
    let bmh = bm.rows;
    let tx = x.to_fixed_raw();
    let ty = y.to_fixed_raw();
    let rad = thing.radius.to_fixed_raw();
    let game_map = level.options.map;

    let xl = (tx - rad - orgx - MAXRADIUS_FIXED) >> MAPBLOCKSHIFT;
    let xh = (tx + rad - orgx + MAXRADIUS_FIXED) >> MAPBLOCKSHIFT;
    let yl = (ty - rad - orgy - MAXRADIUS_FIXED) >> MAPBLOCKSHIFT;
    let yh = (ty + rad - orgy + MAXRADIUS_FIXED) >> MAPBLOCKSHIFT;

    for by in yl..=yh {
        for bx in xl..=xh {
            if bx < 0 || by < 0 || bx >= bmw || by >= bmh {
                continue;
            }
            let idx = (by * bmw + bx) as usize;
            let mut mobj_ptr = level.blocklinks[idx];
            while let Some(ptr) = mobj_ptr {
                let other = unsafe { &mut *ptr };
                mobj_ptr = other.b_next;
                if !pit_stomp_thing(thing, other, x, y, game_map) {
                    return false;
                }
            }
        }
    }

    unsafe {
        thing.unset_thing_position();
        thing.x = x;
        thing.y = y;
        thing.floorz = FixedT::from_fixed(floorz.to_fixed_raw());
        thing.ceilingz = FixedT::from_fixed(ceilzz.to_fixed_raw());
        thing.set_thing_position();
    }
    true
}

/// OG Doom `PIT_StompThing` — telefrag check for blockmap iteration
fn pit_stomp_thing(
    this_thing: &mut MapObject,
    other: &mut MapObject,
    new_x: FixedT,
    new_y: FixedT,
    game_map: usize,
) -> bool {
    if !other.flags.contains(MapObjFlag::Shootable) {
        return true;
    }

    let dist = this_thing.radius + other.radius;
    if (other.x - new_x).doom_abs() >= dist || (other.y - new_y).doom_abs() >= dist {
        return true;
    }

    if this_thing.thinker == other.thinker {
        return true;
    }

    // monsters don't telefrag things except on boss level
    if this_thing.player().is_none() && game_map != 30 {
        return false;
    }

    // OG: P_DamageMobj(thing, tmthing, tmthing, 10000)
    if other.flags.contains(MapObjFlag::Shootable) {
        other.p_take_damage(
            Some((this_thing.x, this_thing.y, this_thing.z)),
            Some(this_thing),
            10000,
        );
    }
    true
}
