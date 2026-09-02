//! The headset options page.
//!
//! Panel geometry has to be dialled in while wearing the headset, which is what
//! an edit-and-restart config file cannot do.

use gameplay::english as lang;
use gamestate_traits::ConfigKey;

use super::{MenuAction, MenuIndex, MenuItem, MenuSet};

const SNAP_TURN: &[&str] = &[lang::OPT_VR_SNAP_30, lang::OPT_VR_SNAP_45];
const DISPLAY: &[&str] = &[lang::OPT_VR_DISPLAY_WINDOW, lang::OPT_VR_DISPLAY_HEADSET];
const LOCOMOTION: &[&str] = &[lang::OPT_VR_LOCO_TELEPORT, lang::OPT_VR_LOCO_SMOOTH];
const WARP_STYLE: &[&str] = &[lang::OPT_VR_WARP_AIM, lang::OPT_VR_WARP_DASH];
const WARP_ACTIVATION: &[&str] = &[
    lang::OPT_VR_WARP_LEFT_TRIGGER,
    lang::OPT_VR_WARP_LEFT_STICK,
    lang::OPT_VR_WARP_RIGHT_STICK,
];
const DASH_DISTANCE: &[&str] = &[lang::OPT_VR_DASH_WALK, lang::OPT_VR_DASH_RUN];

pub(super) fn menu_set() -> MenuSet {
    MenuSet::new(
        MenuIndex::OptVr,
        MenuIndex::Options,
        vec![],
        32,
        40,
        vec![
            MenuItem::cycle(lang::OPT_VR_DISPLAY, ConfigKey::VrDisplay, DISPLAY),
            MenuItem::label(lang::OPT_VR_MOVEMENT, MenuAction::OpenOptVrMovement, 'M'),
            MenuItem::cycle(lang::OPT_VR_SNAP_TURN, ConfigKey::VrSnapTurn, SNAP_TURN),
            MenuItem::toggle(lang::OPT_VR_HIDE_HUD, ConfigKey::VrHideHud),
            MenuItem::toggle(lang::OPT_VR_SHOW_HANDS, ConfigKey::VrShowHands),
            MenuItem::slider(
                lang::OPT_VR_HUD_DIST,
                ConfigKey::VrHudDistance,
                150,
                200,
                10,
            ),
            MenuItem::slider(lang::OPT_VR_HUD_WIDTH, ConfigKey::VrHudWidth, 65, 85, 5),
            MenuItem::slider(lang::OPT_VR_HUD_PITCH, ConfigKey::VrHudPitch, -10, 10, 5),
            MenuItem::slider(
                lang::OPT_VR_WEAPON_SCALE,
                ConfigKey::VrWeaponScale,
                65,
                85,
                5,
            ),
        ],
    )
}

pub(super) fn movement_menu_set() -> MenuSet {
    MenuSet::new(
        MenuIndex::OptVrMovement,
        MenuIndex::OptVr,
        vec![],
        32,
        40,
        vec![
            MenuItem::cycle(lang::OPT_VR_LOCOMOTION, ConfigKey::VrLocomotion, LOCOMOTION),
            MenuItem::cycle(lang::OPT_VR_WARP_STYLE, ConfigKey::VrWarpStyle, WARP_STYLE),
            MenuItem::cycle(
                lang::OPT_VR_WARP_ACTIVATION,
                ConfigKey::VrWarpActivation,
                WARP_ACTIVATION,
            ),
            MenuItem::cycle(
                lang::OPT_VR_DASH_DISTANCE,
                ConfigKey::VrDashDistance,
                DASH_DISTANCE,
            ),
        ],
    )
}
