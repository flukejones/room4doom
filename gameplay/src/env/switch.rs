//! Doom source name `p_switch`

use std::ptr;

use log::debug;
use sound_common::{SfxName, SndServerTx};

use crate::pic::{Button, ButtonWhere};
use level::MapPtr;
use level::bsp3d::BSP3D;
use level::map_defs::LineDef;

const BUTTONTIME: u32 = 35;

/// Doom function name `P_StartButton`
pub fn start_button(
    line: MapPtr<LineDef>,
    bwhere: ButtonWhere,
    texture: usize,
    timer: u32,
    button_list: &mut Vec<Button>,
) {
    for b in button_list.iter() {
        if b.timer != 0 && b.line == line {
            return;
        }
    }

    for b in button_list.iter_mut() {
        // Re-use an existing one
        if b.timer == 0 {
            debug!("Re-using existing button struct for {:?}", line.as_ref());
            b.line = line;
            b.bwhere = bwhere;
            b.texture = texture;
            b.timer = timer;
            // TODO: buttonlist[i].soundorg = &line->frontsector->soundorg;
            return;
        }
    }
    debug!("Using new button struct for {:?}", line.as_ref());
    button_list.push(Button {
        line,
        bwhere,
        texture,
        timer,
    });
}

/// Start a sound using the lines front sector sound origin
pub(crate) fn start_sector_sound(line: &LineDef, sfx: SfxName, snd: &SndServerTx) {
    let sfx_origin = line.front_sidedef.sector.sound_origin;
    if let Err(e) = snd.send(sound_common::SoundAction::StartSfx {
        uid: ptr::from_ref(line) as usize,
        sfx,
        x: sfx_origin.x,
        y: sfx_origin.y,
    }) {
        log::warn!("Could not send sector sfx, sound thread gone: {e}");
    }
}

/// Doom function name `P_ChangeSwitchTexture`
pub fn change_switch_texture(
    mut line: MapPtr<LineDef>,
    use_again: bool,
    switch_list: &[usize],
    button_list: &mut Vec<Button>,
    snd: &SndServerTx,
    bsp3d: &mut BSP3D,
) {
    let sfx = if !use_again {
        line.special = None;
        SfxName::Swtchn
    } else {
        SfxName::Swtchx
    };

    for i in 0..switch_list.len() {
        let sw = switch_list[i];
        if let Some(tex_top) = line.front_sidedef.toptexture
            && sw == tex_top
        {
            start_sector_sound(&line, sfx, snd);
            let new_tex = switch_list[i ^ 1];
            line.front_sidedef.toptexture = Some(new_tex);
            bsp3d.update_wall_texture(line.num);
            if use_again {
                start_button(
                    line,
                    ButtonWhere::Top,
                    switch_list[i],
                    BUTTONTIME,
                    button_list,
                );
            }
            return;
        }
        if let Some(tex_mid) = line.front_sidedef.midtexture
            && sw == tex_mid
        {
            start_sector_sound(&line, sfx, snd);
            let new_tex = switch_list[i ^ 1];
            line.front_sidedef.midtexture = Some(new_tex);
            bsp3d.update_wall_texture(line.num);
            if use_again {
                start_button(
                    line,
                    ButtonWhere::Middle,
                    switch_list[i],
                    BUTTONTIME,
                    button_list,
                );
            }
            return;
        }
        if let Some(tex_low) = line.front_sidedef.bottomtexture
            && sw == tex_low
        {
            start_sector_sound(&line, sfx, snd);
            let new_tex = switch_list[i ^ 1];
            line.front_sidedef.bottomtexture = Some(new_tex);
            bsp3d.update_wall_texture(line.num);
            if use_again {
                start_button(
                    line,
                    ButtonWhere::Bottom,
                    switch_list[i],
                    BUTTONTIME,
                    button_list,
                );
            }
            return;
        }
    }
}
