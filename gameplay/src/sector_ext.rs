//! Extension trait for Sector that provides Thinker-dependent methods.
//! These methods use gameplay types (Thinker, MapObject) that can't live
//! in the map-data crate.

use log::error;
use map_data::Sector;

use crate::thing::MapObject;
use crate::thinker::{Thinker, ThinkerData};

/// Extension trait adding gameplay-specific Thinker methods to Sector.
pub trait SectorExt {
    /// Returns false if `func` returns false
    fn run_mut_func_on_thinglist(&mut self, func: impl FnMut(&mut MapObject) -> bool) -> bool;

    fn run_func_on_thinglist(&self, func: impl FnMut(&MapObject) -> bool) -> bool;

    /// Add this thing to the sectors thing list
    ///
    /// # Safety
    /// The `Thinker` pointer *must* be valid, and the `Thinker` must not be
    /// `Free` or `Remove`
    unsafe fn add_to_thinglist(&mut self, thing: *mut Thinker);

    /// Remove this thing from this sectors thinglist
    ///
    /// # Safety
    /// Must be called if a thing is ever removed
    unsafe fn remove_from_thinglist(&mut self, thing: &mut Thinker);

    fn sound_target(&self) -> Option<&mut MapObject>;

    fn sound_target_raw(&mut self) -> Option<*mut Thinker>;

    fn set_sound_target_thinker(&mut self, target: *mut Thinker);

    /// Mark this sector as having an active mover (floor, ceiling, platform,
    /// door).
    fn set_sector_mover(&mut self, thinker: *mut Thinker);
}

impl SectorExt for Sector {
    fn run_mut_func_on_thinglist(&mut self, mut func: impl FnMut(&mut MapObject) -> bool) -> bool {
        if let Some(thing_ptr) = self.thinglist {
            let thing = thing_ptr as *mut Thinker;
            #[cfg(feature = "null_check")]
            if thing.is_null() {
                std::panic!("thinglist is null when it shouldn't be");
            }
            unsafe {
                if (*thing).should_remove() {
                    return true;
                }
                let mut thing = (*thing).mobj_mut();

                loop {
                    let next = thing.s_next;
                    if !func(thing) {
                        return false;
                    }

                    if let Some(next) = next {
                        #[cfg(feature = "null_check")]
                        if next.is_null() {
                            std::panic!("thinglist thing.s_next is null when it shouldn't be");
                        }
                        if (*next).should_remove() {
                            continue;
                        }
                        thing = (*next).mobj_mut()
                    } else {
                        break;
                    }
                }
            }
        }
        true
    }

    fn run_func_on_thinglist(&self, mut func: impl FnMut(&MapObject) -> bool) -> bool {
        if let Some(thing_ptr) = self.thinglist {
            let thing = thing_ptr as *mut Thinker;
            #[cfg(feature = "null_check")]
            if thing.is_null() {
                std::panic!("thinglist is null when it shouldn't be");
            }
            unsafe {
                if (*thing).should_remove() {
                    return true;
                }
                let mut thing = (*thing).mobj();

                loop {
                    let next = thing.s_next;
                    if !func(thing) {
                        return false;
                    }

                    if let Some(next) = next {
                        #[cfg(feature = "null_check")]
                        if next.is_null() {
                            std::panic!("thinglist thing.s_next is null when it shouldn't be");
                        }
                        if (*next).should_remove() {
                            continue;
                        }
                        thing = (*next).mobj()
                    } else {
                        break;
                    }
                }
            }
        }
        true
    }

    unsafe fn add_to_thinglist(&mut self, thing: *mut Thinker) {
        if matches!(
            (unsafe { &*thing }).data(),
            ThinkerData::Free | ThinkerData::Remove
        ) {
            error!("add_to_thinglist() tried to add a Thinker that was Free or Remove");
            return;
        }
        unsafe { &mut *thing }.mobj_mut().s_prev = None;
        unsafe { &mut *thing }.mobj_mut().s_next = self.thinglist.map(|p| p as *mut Thinker);

        if let Some(other_ptr) = self.thinglist {
            let other = other_ptr as *mut Thinker;
            unsafe { &mut *other }.mobj_mut().s_prev = Some(thing);
        }

        self.thinglist = Some(thing as *mut ());
    }

    unsafe fn remove_from_thinglist(&mut self, thing: &mut Thinker) {
        if thing.mobj().s_next.is_none() && thing.mobj().s_prev.is_none() {
            self.thinglist = None;
        }

        if let Some(next) = thing.mobj().s_next {
            unsafe { &mut *next }.mobj_mut().s_prev = (*thing).mobj_mut().s_prev;
        }

        if let Some(prev) = thing.mobj().s_prev {
            unsafe { &mut *prev }.mobj_mut().s_next = thing.mobj_mut().s_next;
        } else {
            let mut ss = thing.mobj().subsector.clone();
            ss.sector.thinglist = thing.mobj().s_next.map(|p| p as *mut ());
        }
    }

    fn sound_target(&self) -> Option<&mut MapObject> {
        self.sound_target
            .map(|p| unsafe { &mut *(p as *mut Thinker) }.mobj_mut())
    }

    fn sound_target_raw(&mut self) -> Option<*mut Thinker> {
        self.sound_target.map(|p| p as *mut Thinker)
    }

    fn set_sound_target_thinker(&mut self, target: *mut Thinker) {
        self.sound_target = Some(target as *mut ());
    }

    fn set_sector_mover(&mut self, thinker: *mut Thinker) {
        self.specialdata = Some(thinker as *mut ());
    }
}
