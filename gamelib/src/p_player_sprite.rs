use crate::{info::states::State, player::Player};

/// From P_PSPR
#[derive(Debug)]
#[allow(non_camel_case_types)]
pub(crate) struct PspDef {
    /// a NULL state means not active
    pub state: Option<State>,
    pub tics:  i32,
    pub sx:    f32,
    pub sy:    f32,
}

/// The player can re-fire the weapon
/// without lowering it entirely.
pub(crate) fn a_refire(actor: &mut Player, _pspr: &mut PspDef) {
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

pub(crate) fn a_weaponready(actor: &mut Player, _pspr: &mut PspDef) {
    unimplemented!()
}

pub(crate) fn a_lower(actor: &mut Player, _pspr: &mut PspDef) {
    unimplemented!()
}

pub(crate) fn a_raise(actor: &mut Player, _pspr: &mut PspDef) {
    unimplemented!()
}

pub(crate) fn a_firepistol(actor: &mut Player, _pspr: &mut PspDef) {
    unimplemented!()
}

pub(crate) fn a_fireshotgun(actor: &mut Player, _pspr: &mut PspDef) {
    unimplemented!()
}

pub(crate) fn a_fireshotgun2(actor: &mut Player, _pspr: &mut PspDef) {
    unimplemented!()
}

pub(crate) fn a_firecgun(actor: &mut Player, _pspr: &mut PspDef) {
    unimplemented!()
}

pub(crate) fn a_fireplasma(actor: &mut Player, _pspr: &mut PspDef) {
    unimplemented!()
}

pub(crate) fn a_firemissile(actor: &mut Player, _pspr: &mut PspDef) {
    unimplemented!()
}

pub(crate) fn a_firebfg(actor: &mut Player, _pspr: &mut PspDef) {
    unimplemented!()
}

pub(crate) fn a_bfgsound(actor: &mut Player, _pspr: &mut PspDef) {
    unimplemented!()
}

pub(crate) fn a_gunflash(actor: &mut Player, _pspr: &mut PspDef) {
    unimplemented!()
}

pub(crate) fn a_punch(actor: &mut Player, _pspr: &mut PspDef) {
    unimplemented!()
}

pub(crate) fn a_checkreload(actor: &mut Player, _pspr: &mut PspDef) {
    unimplemented!()
}

pub(crate) fn a_openshotgun2(actor: &mut Player, _pspr: &mut PspDef) {
    unimplemented!()
}

pub(crate) fn a_loadshotgun2(actor: &mut Player, _pspr: &mut PspDef) {
    unimplemented!()
}

pub(crate) fn a_closeshotgun2(actor: &mut Player, _pspr: &mut PspDef) {
    unimplemented!()
}

pub(crate) fn a_saw(actor: &mut Player, _pspr: &mut PspDef) { unimplemented!() }

pub(crate) fn a_light0(actor: &mut Player, _pspr: &mut PspDef) {
    actor.extralight = 0;
}

pub(crate) fn a_light1(actor: &mut Player, _pspr: &mut PspDef) {
    actor.extralight = 1;
}

pub(crate) fn a_light2(actor: &mut Player, _pspr: &mut PspDef) {
    actor.extralight = 2;
}
