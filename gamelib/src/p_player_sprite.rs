use crate::d_thinker::ObjectBase;
use crate::info::states::State;

/// From P_PSPR
#[derive(Debug)]
#[allow(non_camel_case_types)]
pub struct PspDef {
    /// a NULL state means not active
    pub state: Option<State>,
    pub tics:  i32,
    pub sx:    f32,
    pub sy:    f32,
}

/// The player can re-fire the weapon
/// without lowering it entirely.
pub fn a_refire<'t>(actor: &'t mut ObjectBase<'t>, _pspr: &mut PspDef) {
    if let Some(actor) = actor.get_mut_player() {
        unimplemented!()
        // // check for fire
        // //  (if a weaponchange is pending, let it go through instead)
        //         if ((player -> cmd.buttons & BT_ATTACK) &&player -> pendingweapon == wp_nochange && player -> health)
        //         {
        //             player -> refire + +;
        //             P_FireWeapon(player);
        //         }
        //         else
        //         {
        //             player -> refire = 0;
        //             P_CheckAmmo(player);
        //         }
        //     }
    }
}

pub fn a_light0<'t>(actor: &'t mut ObjectBase<'t>, _pspr: &mut PspDef) {
    if let Some(actor) = actor.get_mut_player() {
        actor.extralight = 0;
    }
}

pub fn a_light1<'t>(actor: &'t mut ObjectBase<'t>, _pspr: &mut PspDef) {
    if let Some(actor) = actor.get_mut_player() {
        actor.extralight = 1;
    }
}

pub fn a_light2<'t>(actor: &'t mut ObjectBase<'t>, _pspr: &mut PspDef) {
    if let Some(actor) = actor.get_mut_player() {
        actor.extralight = 2;
    }
}
