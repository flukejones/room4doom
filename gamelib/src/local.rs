use crate::map_object::MapObject;
use crate::thinker::ObjectBase;
use std::any::Any;

/// P_MOBJ
pub static ONFLOORZ: i32 = i32::MIN;
/// P_MOBJ
pub static ONCEILINGZ: i32 = i32::MAX;

pub static MAXHEALTH: i32 = 100;
pub static VIEWHEIGHT: i32 = 41;

// void P_RunThinkers(void)
// {
//     thinker_t *currentthinker, *nextthinker;
//
//     currentthinker = thinkercap.next;
//     while (currentthinker != &thinkercap)
//     {
//         if (currentthinker->function.acv == (actionf_v)(-1))
//         {
//             // time to remove it
//             nextthinker = currentthinker->next;
//             currentthinker->next->prev = currentthinker->prev;
//             currentthinker->prev->next = currentthinker->next;
//             Z_Free(currentthinker);
//         }
//         else
//         {
//             if (currentthinker->function.acp1)
//                 currentthinker->function.acp1(currentthinker); // WHAT??? It's casting currentthinker to mobj_t?
//             nextthinker = currentthinker->next;
//         }
//         currentthinker = nextthinker;
//     }
// }

// Need to think about this. Actions require different types. Need to maybe work out a trait
// that could be used for the objects being worked on...
//
// Example functions:
//
// Other?
// void T_VerticalDoor (vldoor_t* door)
// void T_LightFlash (lightflash_t* flash)
// void T_MoveCeiling (ceiling_t* ceiling)
// void T_FireFlicker (fireflicker_t* flick)
// void T_Glow(glow_t*	g)
//
// States:
// void A_VileTarget (mobj_t*	actor)
// void A_PlayerScream (mobj_t* mo)
// void A_Chase(mobj_t * actor)
// void A_FireCrackle (mobj_t* actor)
//
// void A_OpenShotgun2(player_t *player, pspdef_t *psp) // These are player view sprites, shown on screen
// void A_LoadShotgun2(player_t *player, pspdef_t *psp)
// void A_Lower(player_t *player, pspdef_t *psp)
// void A_Light0(player_t *player, pspdef_t *psp)

#[allow(non_camel_case_types)]
pub enum ActionType {
    A_Light0,
}

pub struct ActorAction {
    inner: ActionType,
}

impl ActorAction {
    pub fn action(object: &mut Box<dyn Any>) {
        // match anction then cast object?
    }
}

pub fn test_action(object: &mut ObjectBase) {
    //object.downcast_mut::<MapObject>();
}

pub trait Actor {
    fn actionf_p1(obj: &mut MapObject);
    fn actionf_p2(obj: &mut MapObject, obj2: &mut MapObject);
}

// BaseObject, enum to contain all object types.
