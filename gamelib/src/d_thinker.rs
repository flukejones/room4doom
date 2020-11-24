use crate::{
    p_map_object::MapObject, p_player_sprite::PspDef, p_spec::*, player::Player,
};
use std::ptr::null_mut;
use std::{any::Any, fmt};

// TODO: Split thinkers and things in to MapObject, Lights, Movers, Player,
//  where Movers contains all level structure changing things like doors/platforms

/// Thinkers *must* be contained in a structure that has **stable** memory locations.
/// In Doom this is managed by Doom's custom allocator, where each location in memory
/// also has a pointer to the locations 'owner'. When Doom does a defrag or any op
/// that moves memory locations it also runs through the owners and updates their
/// pointers. This isn't done in the Rust version.
///
/// Another way to manager Thinkers in a volatile container like a Vec is to use `self.function`
/// to mark for removal (same as Doom), then iterate over the container and only
/// run thinkers not marked for removal, then remove marked thinkers after cycle.
/// This method would have a big impact on iter speed though as there may be many
/// 'dead' thinkers.
///
/// Inserting the `Thinker` in to the game is done in p_tick.c with `P_RunThinkers`.
///
/// On Drop the thinker will unlink itself from whereever it is in the link chain.
///
///  State should live in MapObject. State.action and Think.function are two
///  different functions
///
/// The LinkedList style serves to give the Objects a way to find the next/prev of
/// its neighbours and more, without having to pass in a ref to the Thinker container,
/// or iterate over possible blank spots in memory.
#[derive(Debug)]
pub struct Thinker<T: Any + Think> {
    pub prev:     *mut Thinker<T>,
    pub next:     *mut Thinker<T>,
    pub obj:      T,
    /// The `Thinker` function to run, this function typically also runs a `State`
    /// function on the Object. The `State` function may then require access to
    /// the `Thinker` to change/remove the thinker funciton.
    // TODO: maybe make this take the Thinker as arg. Easily done if the Thinker then contains
    //  only one struct type for things
    pub function: ActionFunc,
}

impl<T: Any + Think> Thinker<T> {
    pub fn new(obj: T) -> Thinker<T> {
        Thinker {
            prev: null_mut(),
            next: null_mut(),
            obj,
            function: ActionFunc::None,
        }
    }

    pub fn unlink(&mut self) {
        if !self.prev.is_null() && !self.next.is_null() {
            let prev = unsafe { &mut *self.prev };
            let next = unsafe { &mut *self.next };
            prev.next = next;
            next.prev = prev;
        } else if !self.prev.is_null() && self.next.is_null() {
            // Only linked to previous, so unlink prev to this thinker
            unsafe {
                (*self.prev).next = null_mut();
            }
        } else if self.prev.is_null() && !self.next.is_null() {
            // Only linked to next, so unlink next to this thinker
            unsafe {
                (*self.next).prev = null_mut();
            }
        }
    }

    /// If returns true then the thinker + objects should be removed
    pub fn think(&mut self) -> bool {
        self.obj.think()
        // let func = self.state.action.mobj_func();
        // unsafe { (*func)(self) }
    }
}

impl<T: Any + Think> Drop for Thinker<T> {
    fn drop(&mut self) {
        // if this thinker has links in both directions then the thinkers at those
        // ends must be linked to this thinker, so we need to unlink those from
        // this thinker, and link them together
        self.unlink();
    }
}

pub trait Think {
    /// impl of this trait should return true *if* the thinker + object are to be removed
    fn think(&mut self) -> bool;
}

/// Enum of function callbacks
///
/// Similar to `actionf_t` in d_think.h. `ObjectBase` is required because we need to wrap the
/// various different args *because* unlike C we can't rely on function arg casts. Use of `Any`
/// could be done, but it introduces overhead at runtime.
#[derive(Clone)]
pub enum ActionFunc {
    /// NULL thinker, used to tell the thinker runner to remove the thinker from list
    None,
    /// Called in the Thinker runner and State
    MapObject(*const dyn Fn(&mut MapObject)),
    /// Called in the Thinker runner and State
    Player(*const dyn Fn(&mut Player, &mut PspDef)), // P_SetPsprite runs this
    // Lights
    FireFlicker(*const dyn Fn(&mut FireFlicker)),
    LightFlash(*const dyn Fn(&mut LightFlash)),
    Strobe(*const dyn Fn(&mut Strobe)),
    Glow(*const dyn Fn(&mut Glow)),
    // Map movers
    Platform(*const dyn Fn(&mut Platform)),
    Floor(*const dyn Fn(&mut FloorMove)),
    Ceiling(*const dyn Fn(&mut CeilingMove)),
}

impl ActionFunc {
    pub fn mobj_func(&self) -> *const dyn Fn(&mut MapObject) {
        match self {
            ActionFunc::MapObject(f) => *f,
            _ => panic!("Incorrect object for function"),
        }
    }

    pub fn player_func(&self) -> *const dyn Fn(&mut Player, &mut PspDef) {
        match self {
            ActionFunc::Player(f) => *f,
            _ => panic!("Incorrect object for function"),
        }
    }

    pub fn fire_flicker_func(&self) -> *const dyn Fn(&mut FireFlicker) {
        match self {
            ActionFunc::FireFlicker(f) => *f,
            _ => panic!("Incorrect object for function"),
        }
    }

    pub fn light_flash_func(&self) -> *const dyn Fn(&mut LightFlash) {
        match self {
            ActionFunc::LightFlash(f) => *f,
            _ => panic!("Incorrect object for function"),
        }
    }

    pub fn strobe_func(&self) -> *const dyn Fn(&mut Strobe) {
        match self {
            ActionFunc::Strobe(f) => *f,
            _ => panic!("Incorrect object for function"),
        }
    }

    pub fn glow_func(&self) -> *const dyn Fn(&mut Glow) {
        match self {
            ActionFunc::Glow(f) => *f,
            _ => panic!("Incorrect object for function"),
        }
    }

    pub fn platform_func(&self) -> *const dyn Fn(&mut Platform) {
        match self {
            ActionFunc::Platform(f) => *f,
            _ => panic!("Incorrect object for function"),
        }
    }

    pub fn floor_func(&self) -> *const dyn Fn(&mut FloorMove) {
        match self {
            ActionFunc::Floor(f) => *f,
            _ => panic!("Incorrect object for function"),
        }
    }

    pub fn ceiling_func(&self) -> *const dyn Fn(&mut CeilingMove) {
        match self {
            ActionFunc::Ceiling(f) => *f,
            _ => panic!("Incorrect object for function"),
        }
    }
}

impl fmt::Debug for ActionFunc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Action").finish()
    }
}
