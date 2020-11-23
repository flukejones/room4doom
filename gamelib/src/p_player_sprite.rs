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

pub fn a_weaponready<'t>(actor: &'t mut ObjectBase<'t>, _pspr: &mut PspDef) {
    if let Some(actor) = actor.get_mut_player() {
        unimplemented!()
    }
}

pub fn a_lower<'t>(actor: &'t mut ObjectBase<'t>, _pspr: &mut PspDef) {
    if let Some(actor) = actor.get_mut_player() {
        unimplemented!()
    }
}

pub fn a_raise<'t>(actor: &'t mut ObjectBase<'t>, _pspr: &mut PspDef) {
    if let Some(actor) = actor.get_mut_player() {
        unimplemented!()
    }
}

pub fn a_firepistol<'t>(actor: &'t mut ObjectBase<'t>, _pspr: &mut PspDef) {
    if let Some(actor) = actor.get_mut_player() {
        unimplemented!()
    }
}

pub fn a_fireshotgun<'t>(actor: &'t mut ObjectBase<'t>, _pspr: &mut PspDef) {
    if let Some(actor) = actor.get_mut_player() {
        unimplemented!()
    }
}

pub fn a_fireshotgun2<'t>(actor: &'t mut ObjectBase<'t>, _pspr: &mut PspDef) {
    if let Some(actor) = actor.get_mut_player() {
        unimplemented!()
    }
}

pub fn a_firecgun<'t>(actor: &'t mut ObjectBase<'t>, _pspr: &mut PspDef) {
    if let Some(actor) = actor.get_mut_player() {
        unimplemented!()
    }
}

pub fn a_fireplasma<'t>(actor: &'t mut ObjectBase<'t>, _pspr: &mut PspDef) {
    if let Some(actor) = actor.get_mut_player() {
        unimplemented!()
    }
}

pub fn a_firemissile<'t>(actor: &'t mut ObjectBase<'t>, _pspr: &mut PspDef) {
    if let Some(actor) = actor.get_mut_player() {
        unimplemented!()
    }
}

pub fn a_firebfg<'t>(actor: &'t mut ObjectBase<'t>, _pspr: &mut PspDef) {
    if let Some(actor) = actor.get_mut_player() {
        unimplemented!()
    }
}

pub fn a_bfgsound<'t>(actor: &'t mut ObjectBase<'t>, _pspr: &mut PspDef) {
    if let Some(actor) = actor.get_mut_player() {
        unimplemented!()
    }
}

pub fn a_gunflash<'t>(actor: &'t mut ObjectBase<'t>, _pspr: &mut PspDef) {
    if let Some(actor) = actor.get_mut_player() {
        unimplemented!()
    }
}

pub fn a_punch<'t>(actor: &'t mut ObjectBase<'t>, _pspr: &mut PspDef) {
    if let Some(actor) = actor.get_mut_player() {
        unimplemented!()
    }
}

pub fn a_checkreload<'t>(actor: &'t mut ObjectBase<'t>, _pspr: &mut PspDef) {
    if let Some(actor) = actor.get_mut_player() {
        unimplemented!()
    }
}

pub fn a_openshotgun2<'t>(actor: &'t mut ObjectBase<'t>, _pspr: &mut PspDef) {
    if let Some(actor) = actor.get_mut_player() {
        unimplemented!()
    }
}

pub fn a_loadshotgun2<'t>(actor: &'t mut ObjectBase<'t>, _pspr: &mut PspDef) {
    if let Some(actor) = actor.get_mut_player() {
        unimplemented!()
    }
}

pub fn a_closeshotgun2<'t>(actor: &'t mut ObjectBase<'t>, _pspr: &mut PspDef) {
    if let Some(actor) = actor.get_mut_player() {
        unimplemented!()
    }
}

pub fn a_saw<'t>(actor: &'t mut ObjectBase<'t>, _pspr: &mut PspDef) {
    if let Some(actor) = actor.get_mut_player() {
        unimplemented!()
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
