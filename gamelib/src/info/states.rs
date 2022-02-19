use crate::info::{SpriteNum, StateNum};
use crate::p_enemy::a_chase;
use crate::{p_enemy::*, p_player_sprite::*};
use std::fmt;

use super::ActionF;

pub struct State {
    /// Sprite to use
    pub sprite: SpriteNum,
    /// The frame within this sprite to show for the state
    pub frame: i32,
    /// How many tics this state takes. On nightmare it is shifted >> 1
    pub tics: i32,
    // void (*action) (): i32,
    /// An action callback to run on this state
    pub action: ActionF,
    /// The state that should come after this. Can be looped.
    pub next_state: StateNum,
    /// Don't know, Doom seems to set all to zero
    pub misc1: i32,
    /// Don't know, Doom seems to set all to zero
    pub misc2: i32,
}

impl State {
    pub const fn new(
        sprite: SpriteNum,
        frame: i32,
        tics: i32,
        action: ActionF,
        next_state: StateNum,
        misc1: i32,
        misc2: i32,
    ) -> Self {
        Self {
            sprite,
            frame,
            tics,
            action,
            next_state,
            misc1,
            misc2,
        }
    }
}

impl Clone for State {
    fn clone(&self) -> Self {
        State::new(
            self.sprite,
            self.frame,
            self.tics,
            self.action.clone(),
            self.next_state,
            self.misc1,
            self.misc2,
        )
    }
}

impl fmt::Debug for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("State")
            .field("sprite", &self.sprite)
            .finish()
    }
}

pub const STATES: [State; 967] = [
    State::new(
        SpriteNum::SPR_TROO,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_NULL
    State::new(
        SpriteNum::SPR_SHTG,
        4,
        0,
        ActionF::Player(a_light0),
        StateNum::S_NULL,
        0,
        0,
    ), // S_LIGHTDONE
    State::new(
        SpriteNum::SPR_PUNG,
        0,
        1,
        ActionF::Player(a_weaponready),
        StateNum::S_PUNCH,
        0,
        0,
    ), // S_PUNCH
    State::new(
        SpriteNum::SPR_PUNG,
        0,
        1,
        ActionF::Player(a_lower),
        StateNum::S_PUNCHDOWN,
        0,
        0,
    ), // S_PUNCHDOWN
    State::new(
        SpriteNum::SPR_PUNG,
        0,
        1,
        ActionF::Player(a_raise),
        StateNum::S_PUNCHUP,
        0,
        0,
    ), // S_PUNCHUP
    State::new(
        SpriteNum::SPR_PUNG,
        1,
        4,
        ActionF::None,
        StateNum::S_PUNCH2,
        0,
        0,
    ), // S_PUNCH1
    State::new(
        SpriteNum::SPR_PUNG,
        2,
        4,
        ActionF::Player(a_punch),
        StateNum::S_PUNCH3,
        0,
        0,
    ), // S_PUNCH2
    State::new(
        SpriteNum::SPR_PUNG,
        3,
        5,
        ActionF::None,
        StateNum::S_PUNCH4,
        0,
        0,
    ), // S_PUNCH3
    State::new(
        SpriteNum::SPR_PUNG,
        2,
        4,
        ActionF::None,
        StateNum::S_PUNCH5,
        0,
        0,
    ), // S_PUNCH4
    State::new(
        SpriteNum::SPR_PUNG,
        1,
        5,
        ActionF::Player(a_refire),
        StateNum::S_PUNCH,
        0,
        0,
    ), // S_PUNCH5
    State::new(
        SpriteNum::SPR_PISG,
        0,
        1,
        ActionF::Player(a_weaponready),
        StateNum::S_PISTOL,
        0,
        0,
    ), // S_PISTOL
    State::new(
        SpriteNum::SPR_PISG,
        0,
        1,
        ActionF::Player(a_lower),
        StateNum::S_PISTOLDOWN,
        0,
        0,
    ), // S_PISTOLDOWN
    State::new(
        SpriteNum::SPR_PISG,
        0,
        1,
        ActionF::Player(a_raise),
        StateNum::S_PISTOLUP,
        0,
        0,
    ), // S_PISTOLUP
    State::new(
        SpriteNum::SPR_PISG,
        0,
        4,
        ActionF::None,
        StateNum::S_PISTOL2,
        0,
        0,
    ), // S_PISTOL1
    State::new(
        SpriteNum::SPR_PISG,
        1,
        6,
        ActionF::Player(a_firepistol),
        StateNum::S_PISTOL3,
        0,
        0,
    ), // S_PISTOL2
    State::new(
        SpriteNum::SPR_PISG,
        2,
        4,
        ActionF::None,
        StateNum::S_PISTOL4,
        0,
        0,
    ), // S_PISTOL3
    State::new(
        SpriteNum::SPR_PISG,
        1,
        5,
        ActionF::Player(a_refire),
        StateNum::S_PISTOL,
        0,
        0,
    ), // S_PISTOL4
    State::new(
        SpriteNum::SPR_PISF,
        32768,
        7,
        ActionF::Player(a_light1),
        StateNum::S_LIGHTDONE,
        0,
        0,
    ), // S_PISTOLFLASH
    State::new(
        SpriteNum::SPR_SHTG,
        0,
        1,
        ActionF::Player(a_weaponready),
        StateNum::S_SGUN,
        0,
        0,
    ), // S_SGUN
    State::new(
        SpriteNum::SPR_SHTG,
        0,
        1,
        ActionF::Player(a_lower),
        StateNum::S_SGUNDOWN,
        0,
        0,
    ), // S_SGUNDOWN
    State::new(
        SpriteNum::SPR_SHTG,
        0,
        1,
        ActionF::Player(a_raise),
        StateNum::S_SGUNUP,
        0,
        0,
    ), // S_SGUNUP
    State::new(
        SpriteNum::SPR_SHTG,
        0,
        3,
        ActionF::None,
        StateNum::S_SGUN2,
        0,
        0,
    ), // S_SGUN1
    State::new(
        SpriteNum::SPR_SHTG,
        0,
        7,
        ActionF::Player(a_fireshotgun),
        StateNum::S_SGUN3,
        0,
        0,
    ), // S_SGUN2
    State::new(
        SpriteNum::SPR_SHTG,
        1,
        5,
        ActionF::None,
        StateNum::S_SGUN4,
        0,
        0,
    ), // S_SGUN3
    State::new(
        SpriteNum::SPR_SHTG,
        2,
        5,
        ActionF::None,
        StateNum::S_SGUN5,
        0,
        0,
    ), // S_SGUN4
    State::new(
        SpriteNum::SPR_SHTG,
        3,
        4,
        ActionF::None,
        StateNum::S_SGUN6,
        0,
        0,
    ), // S_SGUN5
    State::new(
        SpriteNum::SPR_SHTG,
        2,
        5,
        ActionF::None,
        StateNum::S_SGUN7,
        0,
        0,
    ), // S_SGUN6
    State::new(
        SpriteNum::SPR_SHTG,
        1,
        5,
        ActionF::None,
        StateNum::S_SGUN8,
        0,
        0,
    ), // S_SGUN7
    State::new(
        SpriteNum::SPR_SHTG,
        0,
        3,
        ActionF::None,
        StateNum::S_SGUN9,
        0,
        0,
    ), // S_SGUN8
    State::new(
        SpriteNum::SPR_SHTG,
        0,
        7,
        ActionF::Player(a_refire),
        StateNum::S_SGUN,
        0,
        0,
    ), // S_SGUN9
    State::new(
        SpriteNum::SPR_SHTF,
        32768,
        4,
        ActionF::Player(a_light1),
        StateNum::S_SGUNFLASH2,
        0,
        0,
    ), // S_SGUNFLASH1
    State::new(
        SpriteNum::SPR_SHTF,
        32769,
        3,
        ActionF::Player(a_light2),
        StateNum::S_LIGHTDONE,
        0,
        0,
    ), // S_SGUNFLASH2
    State::new(
        SpriteNum::SPR_SHT2,
        0,
        1,
        ActionF::Player(a_weaponready),
        StateNum::S_DSGUN,
        0,
        0,
    ), // S_DSGUN
    State::new(
        SpriteNum::SPR_SHT2,
        0,
        1,
        ActionF::Player(a_lower),
        StateNum::S_DSGUNDOWN,
        0,
        0,
    ), // S_DSGUNDOWN
    State::new(
        SpriteNum::SPR_SHT2,
        0,
        1,
        ActionF::Player(a_raise),
        StateNum::S_DSGUNUP,
        0,
        0,
    ), // S_DSGUNUP
    State::new(
        SpriteNum::SPR_SHT2,
        0,
        3,
        ActionF::None,
        StateNum::S_DSGUN2,
        0,
        0,
    ), // S_DSGUN1
    State::new(
        SpriteNum::SPR_SHT2,
        0,
        7,
        ActionF::Player(a_fireshotgun2),
        StateNum::S_DSGUN3,
        0,
        0,
    ), // S_DSGUN2
    State::new(
        SpriteNum::SPR_SHT2,
        1,
        7,
        ActionF::None,
        StateNum::S_DSGUN4,
        0,
        0,
    ), // S_DSGUN3
    State::new(
        SpriteNum::SPR_SHT2,
        2,
        7,
        ActionF::Player(a_checkreload),
        StateNum::S_DSGUN5,
        0,
        0,
    ), // S_DSGUN4
    State::new(
        SpriteNum::SPR_SHT2,
        3,
        7,
        ActionF::Player(a_openshotgun2),
        StateNum::S_DSGUN6,
        0,
        0,
    ), // S_DSGUN5
    State::new(
        SpriteNum::SPR_SHT2,
        4,
        7,
        ActionF::None,
        StateNum::S_DSGUN7,
        0,
        0,
    ), // S_DSGUN6
    State::new(
        SpriteNum::SPR_SHT2,
        5,
        7,
        ActionF::Player(a_loadshotgun2),
        StateNum::S_DSGUN8,
        0,
        0,
    ), // S_DSGUN7
    State::new(
        SpriteNum::SPR_SHT2,
        6,
        6,
        ActionF::None,
        StateNum::S_DSGUN9,
        0,
        0,
    ), // S_DSGUN8
    State::new(
        SpriteNum::SPR_SHT2,
        7,
        6,
        ActionF::Player(a_closeshotgun2),
        StateNum::S_DSGUN10,
        0,
        0,
    ), // S_DSGUN9
    State::new(
        SpriteNum::SPR_SHT2,
        0,
        5,
        ActionF::Player(a_refire),
        StateNum::S_DSGUN,
        0,
        0,
    ), // S_DSGUN10
    State::new(
        SpriteNum::SPR_SHT2,
        1,
        7,
        ActionF::None,
        StateNum::S_DSNR2,
        0,
        0,
    ), // S_DSNR1
    State::new(
        SpriteNum::SPR_SHT2,
        0,
        3,
        ActionF::None,
        StateNum::S_DSGUNDOWN,
        0,
        0,
    ), // S_DSNR2
    State::new(
        SpriteNum::SPR_SHT2,
        32776,
        5,
        ActionF::Player(a_light1),
        StateNum::S_DSGUNFLASH2,
        0,
        0,
    ), // S_DSGUNFLASH1
    State::new(
        SpriteNum::SPR_SHT2,
        32777,
        4,
        ActionF::Player(a_light2),
        StateNum::S_LIGHTDONE,
        0,
        0,
    ), // S_DSGUNFLASH2
    State::new(
        SpriteNum::SPR_CHGG,
        0,
        1,
        ActionF::Player(a_weaponready),
        StateNum::S_CHAIN,
        0,
        0,
    ), // S_CHAIN
    State::new(
        SpriteNum::SPR_CHGG,
        0,
        1,
        ActionF::Player(a_lower),
        StateNum::S_CHAINDOWN,
        0,
        0,
    ), // S_CHAINDOWN
    State::new(
        SpriteNum::SPR_CHGG,
        0,
        1,
        ActionF::Player(a_raise),
        StateNum::S_CHAINUP,
        0,
        0,
    ), // S_CHAINUP
    State::new(
        SpriteNum::SPR_CHGG,
        0,
        4,
        ActionF::Player(a_firecgun),
        StateNum::S_CHAIN2,
        0,
        0,
    ), // S_CHAIN1
    State::new(
        SpriteNum::SPR_CHGG,
        1,
        4,
        ActionF::Player(a_firecgun),
        StateNum::S_CHAIN3,
        0,
        0,
    ), // S_CHAIN2
    State::new(
        SpriteNum::SPR_CHGG,
        1,
        0,
        ActionF::Player(a_refire),
        StateNum::S_CHAIN,
        0,
        0,
    ), // S_CHAIN3
    State::new(
        SpriteNum::SPR_CHGF,
        32768,
        5,
        ActionF::Player(a_light1),
        StateNum::S_LIGHTDONE,
        0,
        0,
    ), // S_CHAINFLASH1
    State::new(
        SpriteNum::SPR_CHGF,
        32769,
        5,
        ActionF::Player(a_light2),
        StateNum::S_LIGHTDONE,
        0,
        0,
    ), // S_CHAINFLASH2
    State::new(
        SpriteNum::SPR_MISG,
        0,
        1,
        ActionF::Player(a_weaponready),
        StateNum::S_MISSILE,
        0,
        0,
    ), // S_MISSILE
    State::new(
        SpriteNum::SPR_MISG,
        0,
        1,
        ActionF::Player(a_lower),
        StateNum::S_MISSILEDOWN,
        0,
        0,
    ), // S_MISSILEDOWN
    State::new(
        SpriteNum::SPR_MISG,
        0,
        1,
        ActionF::Player(a_raise),
        StateNum::S_MISSILEUP,
        0,
        0,
    ), // S_MISSILEUP
    State::new(
        SpriteNum::SPR_MISG,
        1,
        8,
        ActionF::Player(a_gunflash),
        StateNum::S_MISSILE2,
        0,
        0,
    ), // S_MISSILE1
    State::new(
        SpriteNum::SPR_MISG,
        1,
        12,
        ActionF::Player(a_firemissile),
        StateNum::S_MISSILE3,
        0,
        0,
    ), // S_MISSILE2
    State::new(
        SpriteNum::SPR_MISG,
        1,
        0,
        ActionF::Player(a_refire),
        StateNum::S_MISSILE,
        0,
        0,
    ), // S_MISSILE3
    State::new(
        SpriteNum::SPR_MISF,
        32768,
        3,
        ActionF::Player(a_light1),
        StateNum::S_MISSILEFLASH2,
        0,
        0,
    ), // S_MISSILEFLASH1
    State::new(
        SpriteNum::SPR_MISF,
        32769,
        4,
        ActionF::None,
        StateNum::S_MISSILEFLASH3,
        0,
        0,
    ), // S_MISSILEFLASH2
    State::new(
        SpriteNum::SPR_MISF,
        32770,
        4,
        ActionF::Player(a_light2),
        StateNum::S_MISSILEFLASH4,
        0,
        0,
    ), // S_MISSILEFLASH3
    State::new(
        SpriteNum::SPR_MISF,
        32771,
        4,
        ActionF::Player(a_light2),
        StateNum::S_LIGHTDONE,
        0,
        0,
    ), // S_MISSILEFLASH4
    State::new(
        SpriteNum::SPR_SAWG,
        2,
        4,
        ActionF::Player(a_weaponready),
        StateNum::S_SAWB,
        0,
        0,
    ), // S_SAW
    State::new(
        SpriteNum::SPR_SAWG,
        3,
        4,
        ActionF::Player(a_weaponready),
        StateNum::S_SAW,
        0,
        0,
    ), // S_SAWB
    State::new(
        SpriteNum::SPR_SAWG,
        2,
        1,
        ActionF::Player(a_lower),
        StateNum::S_SAWDOWN,
        0,
        0,
    ), // S_SAWDOWN
    State::new(
        SpriteNum::SPR_SAWG,
        2,
        1,
        ActionF::Player(a_raise),
        StateNum::S_SAWUP,
        0,
        0,
    ), // S_SAWUP
    State::new(
        SpriteNum::SPR_SAWG,
        0,
        4,
        ActionF::Player(a_saw),
        StateNum::S_SAW2,
        0,
        0,
    ), // S_SAW1
    State::new(
        SpriteNum::SPR_SAWG,
        1,
        4,
        ActionF::Player(a_saw),
        StateNum::S_SAW3,
        0,
        0,
    ), // S_SAW2
    State::new(
        SpriteNum::SPR_SAWG,
        1,
        0,
        ActionF::Player(a_refire),
        StateNum::S_SAW,
        0,
        0,
    ), // S_SAW3
    State::new(
        SpriteNum::SPR_PLSG,
        0,
        1,
        ActionF::Player(a_weaponready),
        StateNum::S_PLASMA,
        0,
        0,
    ), // S_PLASMA
    State::new(
        SpriteNum::SPR_PLSG,
        0,
        1,
        ActionF::Player(a_lower),
        StateNum::S_PLASMADOWN,
        0,
        0,
    ), // S_PLASMADOWN
    State::new(
        SpriteNum::SPR_PLSG,
        0,
        1,
        ActionF::Player(a_raise),
        StateNum::S_PLASMAUP,
        0,
        0,
    ), // S_PLASMAUP
    State::new(
        SpriteNum::SPR_PLSG,
        0,
        3,
        ActionF::Player(a_fireplasma),
        StateNum::S_PLASMA2,
        0,
        0,
    ), // S_PLASMA1
    State::new(
        SpriteNum::SPR_PLSG,
        1,
        20,
        ActionF::Player(a_refire),
        StateNum::S_PLASMA,
        0,
        0,
    ), // S_PLASMA2
    State::new(
        SpriteNum::SPR_PLSF,
        32768,
        4,
        ActionF::Player(a_light1),
        StateNum::S_LIGHTDONE,
        0,
        0,
    ), // S_PLASMAFLASH1
    State::new(
        SpriteNum::SPR_PLSF,
        32769,
        4,
        ActionF::Player(a_light1),
        StateNum::S_LIGHTDONE,
        0,
        0,
    ), // S_PLASMAFLASH2
    State::new(
        SpriteNum::SPR_BFGG,
        0,
        1,
        ActionF::Player(a_weaponready),
        StateNum::S_BFG,
        0,
        0,
    ), // S_BFG
    State::new(
        SpriteNum::SPR_BFGG,
        0,
        1,
        ActionF::Player(a_lower),
        StateNum::S_BFGDOWN,
        0,
        0,
    ), // S_BFGDOWN
    State::new(
        SpriteNum::SPR_BFGG,
        0,
        1,
        ActionF::Player(a_raise),
        StateNum::S_BFGUP,
        0,
        0,
    ), // S_BFGUP
    State::new(
        SpriteNum::SPR_BFGG,
        0,
        20,
        ActionF::Player(a_bfgsound),
        StateNum::S_BFG2,
        0,
        0,
    ), // S_BFG1
    State::new(
        SpriteNum::SPR_BFGG,
        1,
        10,
        ActionF::Player(a_gunflash),
        StateNum::S_BFG3,
        0,
        0,
    ), // S_BFG2
    State::new(
        SpriteNum::SPR_BFGG,
        1,
        10,
        ActionF::Player(a_firebfg),
        StateNum::S_BFG4,
        0,
        0,
    ), // S_BFG3
    State::new(
        SpriteNum::SPR_BFGG,
        1,
        20,
        ActionF::Player(a_refire),
        StateNum::S_BFG,
        0,
        0,
    ), // S_BFG4
    State::new(
        SpriteNum::SPR_BFGF,
        32768,
        11,
        ActionF::Player(a_light1),
        StateNum::S_BFGFLASH2,
        0,
        0,
    ), // S_BFGFLASH1
    State::new(
        SpriteNum::SPR_BFGF,
        32769,
        6,
        ActionF::Player(a_light2),
        StateNum::S_LIGHTDONE,
        0,
        0,
    ), // S_BFGFLASH2
    State::new(
        SpriteNum::SPR_BLUD,
        2,
        8,
        ActionF::None,
        StateNum::S_BLOOD2,
        0,
        0,
    ), // S_BLOOD1
    State::new(
        SpriteNum::SPR_BLUD,
        1,
        8,
        ActionF::None,
        StateNum::S_BLOOD3,
        0,
        0,
    ), // S_BLOOD2
    State::new(
        SpriteNum::SPR_BLUD,
        0,
        8,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_BLOOD3
    State::new(
        SpriteNum::SPR_PUFF,
        32768,
        4,
        ActionF::None,
        StateNum::S_PUFF2,
        0,
        0,
    ), // S_PUFF1
    State::new(
        SpriteNum::SPR_PUFF,
        1,
        4,
        ActionF::None,
        StateNum::S_PUFF3,
        0,
        0,
    ), // S_PUFF2
    State::new(
        SpriteNum::SPR_PUFF,
        2,
        4,
        ActionF::None,
        StateNum::S_PUFF4,
        0,
        0,
    ), // S_PUFF3
    State::new(
        SpriteNum::SPR_PUFF,
        3,
        4,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_PUFF4
    State::new(
        SpriteNum::SPR_BAL1,
        32768,
        4,
        ActionF::None,
        StateNum::S_TBALL2,
        0,
        0,
    ), // S_TBALL1
    State::new(
        SpriteNum::SPR_BAL1,
        32769,
        4,
        ActionF::None,
        StateNum::S_TBALL1,
        0,
        0,
    ), // S_TBALL2
    State::new(
        SpriteNum::SPR_BAL1,
        32770,
        6,
        ActionF::None,
        StateNum::S_TBALLX2,
        0,
        0,
    ), // S_TBALLX1
    State::new(
        SpriteNum::SPR_BAL1,
        32771,
        6,
        ActionF::None,
        StateNum::S_TBALLX3,
        0,
        0,
    ), // S_TBALLX2
    State::new(
        SpriteNum::SPR_BAL1,
        32772,
        6,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_TBALLX3
    State::new(
        SpriteNum::SPR_BAL2,
        32768,
        4,
        ActionF::None,
        StateNum::S_RBALL2,
        0,
        0,
    ), // S_RBALL1
    State::new(
        SpriteNum::SPR_BAL2,
        32769,
        4,
        ActionF::None,
        StateNum::S_RBALL1,
        0,
        0,
    ), // S_RBALL2
    State::new(
        SpriteNum::SPR_BAL2,
        32770,
        6,
        ActionF::None,
        StateNum::S_RBALLX2,
        0,
        0,
    ), // S_RBALLX1
    State::new(
        SpriteNum::SPR_BAL2,
        32771,
        6,
        ActionF::None,
        StateNum::S_RBALLX3,
        0,
        0,
    ), // S_RBALLX2
    State::new(
        SpriteNum::SPR_BAL2,
        32772,
        6,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_RBALLX3
    State::new(
        SpriteNum::SPR_PLSS,
        32768,
        6,
        ActionF::None,
        StateNum::S_PLASBALL2,
        0,
        0,
    ), // S_PLASBALL
    State::new(
        SpriteNum::SPR_PLSS,
        32769,
        6,
        ActionF::None,
        StateNum::S_PLASBALL,
        0,
        0,
    ), // S_PLASBALL2
    State::new(
        SpriteNum::SPR_PLSE,
        32768,
        4,
        ActionF::None,
        StateNum::S_PLASEXP2,
        0,
        0,
    ), // S_PLASEXP
    State::new(
        SpriteNum::SPR_PLSE,
        32769,
        4,
        ActionF::None,
        StateNum::S_PLASEXP3,
        0,
        0,
    ), // S_PLASEXP2
    State::new(
        SpriteNum::SPR_PLSE,
        32770,
        4,
        ActionF::None,
        StateNum::S_PLASEXP4,
        0,
        0,
    ), // S_PLASEXP3
    State::new(
        SpriteNum::SPR_PLSE,
        32771,
        4,
        ActionF::None,
        StateNum::S_PLASEXP5,
        0,
        0,
    ), // S_PLASEXP4
    State::new(
        SpriteNum::SPR_PLSE,
        32772,
        4,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_PLASEXP5
    State::new(
        SpriteNum::SPR_MISL,
        32768,
        1,
        ActionF::None,
        StateNum::S_ROCKET,
        0,
        0,
    ), // S_ROCKET
    State::new(
        SpriteNum::SPR_BFS1,
        32768,
        4,
        ActionF::None,
        StateNum::S_BFGSHOT2,
        0,
        0,
    ), // S_BFGSHOT
    State::new(
        SpriteNum::SPR_BFS1,
        32769,
        4,
        ActionF::None,
        StateNum::S_BFGSHOT,
        0,
        0,
    ), // S_BFGSHOT2
    State::new(
        SpriteNum::SPR_BFE1,
        32768,
        8,
        ActionF::None,
        StateNum::S_BFGLAND2,
        0,
        0,
    ), // S_BFGLAND
    State::new(
        SpriteNum::SPR_BFE1,
        32769,
        8,
        ActionF::None,
        StateNum::S_BFGLAND3,
        0,
        0,
    ), // S_BFGLAND2
    State::new(
        SpriteNum::SPR_BFE1,
        32770,
        8,
        ActionF::Actor(a_bfgspray),
        StateNum::S_BFGLAND4,
        0,
        0,
    ), // S_BFGLAND3
    State::new(
        SpriteNum::SPR_BFE1,
        32771,
        8,
        ActionF::None,
        StateNum::S_BFGLAND5,
        0,
        0,
    ), // S_BFGLAND4
    State::new(
        SpriteNum::SPR_BFE1,
        32772,
        8,
        ActionF::None,
        StateNum::S_BFGLAND6,
        0,
        0,
    ), // S_BFGLAND5
    State::new(
        SpriteNum::SPR_BFE1,
        32773,
        8,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_BFGLAND6
    State::new(
        SpriteNum::SPR_BFE2,
        32768,
        8,
        ActionF::None,
        StateNum::S_BFGEXP2,
        0,
        0,
    ), // S_BFGEXP
    State::new(
        SpriteNum::SPR_BFE2,
        32769,
        8,
        ActionF::None,
        StateNum::S_BFGEXP3,
        0,
        0,
    ), // S_BFGEXP2
    State::new(
        SpriteNum::SPR_BFE2,
        32770,
        8,
        ActionF::None,
        StateNum::S_BFGEXP4,
        0,
        0,
    ), // S_BFGEXP3
    State::new(
        SpriteNum::SPR_BFE2,
        32771,
        8,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_BFGEXP4
    State::new(
        SpriteNum::SPR_MISL,
        32769,
        8,
        ActionF::Actor(a_explode),
        StateNum::S_EXPLODE2,
        0,
        0,
    ), // S_EXPLODE1
    State::new(
        SpriteNum::SPR_MISL,
        32770,
        6,
        ActionF::None,
        StateNum::S_EXPLODE3,
        0,
        0,
    ), // S_EXPLODE2
    State::new(
        SpriteNum::SPR_MISL,
        32771,
        4,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_EXPLODE3
    State::new(
        SpriteNum::SPR_TFOG,
        32768,
        6,
        ActionF::None,
        StateNum::S_TFOG01,
        0,
        0,
    ), // S_TFOG
    State::new(
        SpriteNum::SPR_TFOG,
        32769,
        6,
        ActionF::None,
        StateNum::S_TFOG02,
        0,
        0,
    ), // S_TFOG01
    State::new(
        SpriteNum::SPR_TFOG,
        32768,
        6,
        ActionF::None,
        StateNum::S_TFOG2,
        0,
        0,
    ), // S_TFOG02
    State::new(
        SpriteNum::SPR_TFOG,
        32769,
        6,
        ActionF::None,
        StateNum::S_TFOG3,
        0,
        0,
    ), // S_TFOG2
    State::new(
        SpriteNum::SPR_TFOG,
        32770,
        6,
        ActionF::None,
        StateNum::S_TFOG4,
        0,
        0,
    ), // S_TFOG3
    State::new(
        SpriteNum::SPR_TFOG,
        32771,
        6,
        ActionF::None,
        StateNum::S_TFOG5,
        0,
        0,
    ), // S_TFOG4
    State::new(
        SpriteNum::SPR_TFOG,
        32772,
        6,
        ActionF::None,
        StateNum::S_TFOG6,
        0,
        0,
    ), // S_TFOG5
    State::new(
        SpriteNum::SPR_TFOG,
        32773,
        6,
        ActionF::None,
        StateNum::S_TFOG7,
        0,
        0,
    ), // S_TFOG6
    State::new(
        SpriteNum::SPR_TFOG,
        32774,
        6,
        ActionF::None,
        StateNum::S_TFOG8,
        0,
        0,
    ), // S_TFOG7
    State::new(
        SpriteNum::SPR_TFOG,
        32775,
        6,
        ActionF::None,
        StateNum::S_TFOG9,
        0,
        0,
    ), // S_TFOG8
    State::new(
        SpriteNum::SPR_TFOG,
        32776,
        6,
        ActionF::None,
        StateNum::S_TFOG10,
        0,
        0,
    ), // S_TFOG9
    State::new(
        SpriteNum::SPR_TFOG,
        32777,
        6,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_TFOG10
    State::new(
        SpriteNum::SPR_IFOG,
        32768,
        6,
        ActionF::None,
        StateNum::S_IFOG01,
        0,
        0,
    ), // S_IFOG
    State::new(
        SpriteNum::SPR_IFOG,
        32769,
        6,
        ActionF::None,
        StateNum::S_IFOG02,
        0,
        0,
    ), // S_IFOG01
    State::new(
        SpriteNum::SPR_IFOG,
        32768,
        6,
        ActionF::None,
        StateNum::S_IFOG2,
        0,
        0,
    ), // S_IFOG02
    State::new(
        SpriteNum::SPR_IFOG,
        32769,
        6,
        ActionF::None,
        StateNum::S_IFOG3,
        0,
        0,
    ), // S_IFOG2
    State::new(
        SpriteNum::SPR_IFOG,
        32770,
        6,
        ActionF::None,
        StateNum::S_IFOG4,
        0,
        0,
    ), // S_IFOG3
    State::new(
        SpriteNum::SPR_IFOG,
        32771,
        6,
        ActionF::None,
        StateNum::S_IFOG5,
        0,
        0,
    ), // S_IFOG4
    State::new(
        SpriteNum::SPR_IFOG,
        32772,
        6,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_IFOG5
    State::new(
        SpriteNum::SPR_PLAY,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_PLAY
    State::new(
        SpriteNum::SPR_PLAY,
        0,
        4,
        ActionF::None,
        StateNum::S_PLAY_RUN2,
        0,
        0,
    ), // S_PLAY_RUN1
    State::new(
        SpriteNum::SPR_PLAY,
        1,
        4,
        ActionF::None,
        StateNum::S_PLAY_RUN3,
        0,
        0,
    ), // S_PLAY_RUN2
    State::new(
        SpriteNum::SPR_PLAY,
        2,
        4,
        ActionF::None,
        StateNum::S_PLAY_RUN4,
        0,
        0,
    ), // S_PLAY_RUN3
    State::new(
        SpriteNum::SPR_PLAY,
        3,
        4,
        ActionF::None,
        StateNum::S_PLAY_RUN1,
        0,
        0,
    ), // S_PLAY_RUN4
    State::new(
        SpriteNum::SPR_PLAY,
        4,
        12,
        ActionF::None,
        StateNum::S_PLAY,
        0,
        0,
    ), // S_PLAY_ATK1
    State::new(
        SpriteNum::SPR_PLAY,
        32773,
        6,
        ActionF::None,
        StateNum::S_PLAY_ATK1,
        0,
        0,
    ), // S_PLAY_ATK2
    State::new(
        SpriteNum::SPR_PLAY,
        6,
        4,
        ActionF::None,
        StateNum::S_PLAY_PAIN2,
        0,
        0,
    ), // S_PLAY_PAIN
    State::new(
        SpriteNum::SPR_PLAY,
        6,
        4,
        ActionF::Actor(a_pain),
        StateNum::S_PLAY,
        0,
        0,
    ), // S_PLAY_PAIN2
    State::new(
        SpriteNum::SPR_PLAY,
        7,
        10,
        ActionF::None,
        StateNum::S_PLAY_DIE2,
        0,
        0,
    ), // S_PLAY_DIE1
    State::new(
        SpriteNum::SPR_PLAY,
        8,
        10,
        ActionF::Actor(a_playerscream),
        StateNum::S_PLAY_DIE3,
        0,
        0,
    ), // S_PLAY_DIE2
    State::new(
        SpriteNum::SPR_PLAY,
        9,
        10,
        ActionF::Actor(a_fall),
        StateNum::S_PLAY_DIE4,
        0,
        0,
    ), // S_PLAY_DIE3
    State::new(
        SpriteNum::SPR_PLAY,
        10,
        10,
        ActionF::None,
        StateNum::S_PLAY_DIE5,
        0,
        0,
    ), // S_PLAY_DIE4
    State::new(
        SpriteNum::SPR_PLAY,
        11,
        10,
        ActionF::None,
        StateNum::S_PLAY_DIE6,
        0,
        0,
    ), // S_PLAY_DIE5
    State::new(
        SpriteNum::SPR_PLAY,
        12,
        10,
        ActionF::None,
        StateNum::S_PLAY_DIE7,
        0,
        0,
    ), // S_PLAY_DIE6
    State::new(
        SpriteNum::SPR_PLAY,
        13,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_PLAY_DIE7
    State::new(
        SpriteNum::SPR_PLAY,
        14,
        5,
        ActionF::None,
        StateNum::S_PLAY_XDIE2,
        0,
        0,
    ), // S_PLAY_XDIE1
    State::new(
        SpriteNum::SPR_PLAY,
        15,
        5,
        ActionF::Actor(a_xscream),
        StateNum::S_PLAY_XDIE3,
        0,
        0,
    ), // S_PLAY_XDIE2
    State::new(
        SpriteNum::SPR_PLAY,
        16,
        5,
        ActionF::Actor(a_fall),
        StateNum::S_PLAY_XDIE4,
        0,
        0,
    ), // S_PLAY_XDIE3
    State::new(
        SpriteNum::SPR_PLAY,
        17,
        5,
        ActionF::None,
        StateNum::S_PLAY_XDIE5,
        0,
        0,
    ), // S_PLAY_XDIE4
    State::new(
        SpriteNum::SPR_PLAY,
        18,
        5,
        ActionF::None,
        StateNum::S_PLAY_XDIE6,
        0,
        0,
    ), // S_PLAY_XDIE5
    State::new(
        SpriteNum::SPR_PLAY,
        19,
        5,
        ActionF::None,
        StateNum::S_PLAY_XDIE7,
        0,
        0,
    ), // S_PLAY_XDIE6
    State::new(
        SpriteNum::SPR_PLAY,
        20,
        5,
        ActionF::None,
        StateNum::S_PLAY_XDIE8,
        0,
        0,
    ), // S_PLAY_XDIE7
    State::new(
        SpriteNum::SPR_PLAY,
        21,
        5,
        ActionF::None,
        StateNum::S_PLAY_XDIE9,
        0,
        0,
    ), // S_PLAY_XDIE8
    State::new(
        SpriteNum::SPR_PLAY,
        22,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_PLAY_XDIE9
    State::new(
        SpriteNum::SPR_POSS,
        0,
        10,
        ActionF::Actor(a_look),
        StateNum::S_POSS_STND2,
        0,
        0,
    ), // S_POSS_STND
    State::new(
        SpriteNum::SPR_POSS,
        1,
        10,
        ActionF::Actor(a_look),
        StateNum::S_POSS_STND,
        0,
        0,
    ), // S_POSS_STND2
    State::new(
        SpriteNum::SPR_POSS,
        0,
        4,
        ActionF::Actor(a_chase),
        StateNum::S_POSS_RUN2,
        0,
        0,
    ), // S_POSS_RUN1
    State::new(
        SpriteNum::SPR_POSS,
        0,
        4,
        ActionF::Actor(a_chase),
        StateNum::S_POSS_RUN3,
        0,
        0,
    ), // S_POSS_RUN2
    State::new(
        SpriteNum::SPR_POSS,
        1,
        4,
        ActionF::Actor(a_chase),
        StateNum::S_POSS_RUN4,
        0,
        0,
    ), // S_POSS_RUN3
    State::new(
        SpriteNum::SPR_POSS,
        1,
        4,
        ActionF::Actor(a_chase),
        StateNum::S_POSS_RUN5,
        0,
        0,
    ), // S_POSS_RUN4
    State::new(
        SpriteNum::SPR_POSS,
        2,
        4,
        ActionF::Actor(a_chase),
        StateNum::S_POSS_RUN6,
        0,
        0,
    ), // S_POSS_RUN5
    State::new(
        SpriteNum::SPR_POSS,
        2,
        4,
        ActionF::Actor(a_chase),
        StateNum::S_POSS_RUN7,
        0,
        0,
    ), // S_POSS_RUN6
    State::new(
        SpriteNum::SPR_POSS,
        3,
        4,
        ActionF::Actor(a_chase),
        StateNum::S_POSS_RUN8,
        0,
        0,
    ), // S_POSS_RUN7
    State::new(
        SpriteNum::SPR_POSS,
        3,
        4,
        ActionF::Actor(a_chase),
        StateNum::S_POSS_RUN1,
        0,
        0,
    ), // S_POSS_RUN8
    State::new(
        SpriteNum::SPR_POSS,
        4,
        10,
        ActionF::Actor(a_facetarget),
        StateNum::S_POSS_ATK2,
        0,
        0,
    ), // S_POSS_ATK1
    State::new(
        SpriteNum::SPR_POSS,
        5,
        8,
        ActionF::Actor(a_posattack),
        StateNum::S_POSS_ATK3,
        0,
        0,
    ), // S_POSS_ATK2
    State::new(
        SpriteNum::SPR_POSS,
        4,
        8,
        ActionF::None,
        StateNum::S_POSS_RUN1,
        0,
        0,
    ), // S_POSS_ATK3
    State::new(
        SpriteNum::SPR_POSS,
        6,
        3,
        ActionF::None,
        StateNum::S_POSS_PAIN2,
        0,
        0,
    ), // S_POSS_PAIN
    State::new(
        SpriteNum::SPR_POSS,
        6,
        3,
        ActionF::Actor(a_pain),
        StateNum::S_POSS_RUN1,
        0,
        0,
    ), // S_POSS_PAIN2
    State::new(
        SpriteNum::SPR_POSS,
        7,
        5,
        ActionF::None,
        StateNum::S_POSS_DIE2,
        0,
        0,
    ), // S_POSS_DIE1
    State::new(
        SpriteNum::SPR_POSS,
        8,
        5,
        ActionF::Actor(a_scream),
        StateNum::S_POSS_DIE3,
        0,
        0,
    ), // S_POSS_DIE2
    State::new(
        SpriteNum::SPR_POSS,
        9,
        5,
        ActionF::Actor(a_fall),
        StateNum::S_POSS_DIE4,
        0,
        0,
    ), // S_POSS_DIE3
    State::new(
        SpriteNum::SPR_POSS,
        10,
        5,
        ActionF::None,
        StateNum::S_POSS_DIE5,
        0,
        0,
    ), // S_POSS_DIE4
    State::new(
        SpriteNum::SPR_POSS,
        11,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_POSS_DIE5
    State::new(
        SpriteNum::SPR_POSS,
        12,
        5,
        ActionF::None,
        StateNum::S_POSS_XDIE2,
        0,
        0,
    ), // S_POSS_XDIE1
    State::new(
        SpriteNum::SPR_POSS,
        13,
        5,
        ActionF::Actor(a_xscream),
        StateNum::S_POSS_XDIE3,
        0,
        0,
    ), // S_POSS_XDIE2
    State::new(
        SpriteNum::SPR_POSS,
        14,
        5,
        ActionF::Actor(a_fall),
        StateNum::S_POSS_XDIE4,
        0,
        0,
    ), // S_POSS_XDIE3
    State::new(
        SpriteNum::SPR_POSS,
        15,
        5,
        ActionF::None,
        StateNum::S_POSS_XDIE5,
        0,
        0,
    ), // S_POSS_XDIE4
    State::new(
        SpriteNum::SPR_POSS,
        16,
        5,
        ActionF::None,
        StateNum::S_POSS_XDIE6,
        0,
        0,
    ), // S_POSS_XDIE5
    State::new(
        SpriteNum::SPR_POSS,
        17,
        5,
        ActionF::None,
        StateNum::S_POSS_XDIE7,
        0,
        0,
    ), // S_POSS_XDIE6
    State::new(
        SpriteNum::SPR_POSS,
        18,
        5,
        ActionF::None,
        StateNum::S_POSS_XDIE8,
        0,
        0,
    ), // S_POSS_XDIE7
    State::new(
        SpriteNum::SPR_POSS,
        19,
        5,
        ActionF::None,
        StateNum::S_POSS_XDIE9,
        0,
        0,
    ), // S_POSS_XDIE8
    State::new(
        SpriteNum::SPR_POSS,
        20,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_POSS_XDIE9
    State::new(
        SpriteNum::SPR_POSS,
        10,
        5,
        ActionF::None,
        StateNum::S_POSS_RAISE2,
        0,
        0,
    ), // S_POSS_RAISE1
    State::new(
        SpriteNum::SPR_POSS,
        9,
        5,
        ActionF::None,
        StateNum::S_POSS_RAISE3,
        0,
        0,
    ), // S_POSS_RAISE2
    State::new(
        SpriteNum::SPR_POSS,
        8,
        5,
        ActionF::None,
        StateNum::S_POSS_RAISE4,
        0,
        0,
    ), // S_POSS_RAISE3
    State::new(
        SpriteNum::SPR_POSS,
        7,
        5,
        ActionF::None,
        StateNum::S_POSS_RUN1,
        0,
        0,
    ), // S_POSS_RAISE4
    State::new(
        SpriteNum::SPR_SPOS,
        0,
        10,
        ActionF::Actor(a_look),
        StateNum::S_SPOS_STND2,
        0,
        0,
    ), // S_SPOS_STND
    State::new(
        SpriteNum::SPR_SPOS,
        1,
        10,
        ActionF::Actor(a_look),
        StateNum::S_SPOS_STND,
        0,
        0,
    ), // S_SPOS_STND2
    State::new(
        SpriteNum::SPR_SPOS,
        0,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_SPOS_RUN2,
        0,
        0,
    ), // S_SPOS_RUN1
    State::new(
        SpriteNum::SPR_SPOS,
        0,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_SPOS_RUN3,
        0,
        0,
    ), // S_SPOS_RUN2
    State::new(
        SpriteNum::SPR_SPOS,
        1,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_SPOS_RUN4,
        0,
        0,
    ), // S_SPOS_RUN3
    State::new(
        SpriteNum::SPR_SPOS,
        1,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_SPOS_RUN5,
        0,
        0,
    ), // S_SPOS_RUN4
    State::new(
        SpriteNum::SPR_SPOS,
        2,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_SPOS_RUN6,
        0,
        0,
    ), // S_SPOS_RUN5
    State::new(
        SpriteNum::SPR_SPOS,
        2,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_SPOS_RUN7,
        0,
        0,
    ), // S_SPOS_RUN6
    State::new(
        SpriteNum::SPR_SPOS,
        3,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_SPOS_RUN8,
        0,
        0,
    ), // S_SPOS_RUN7
    State::new(
        SpriteNum::SPR_SPOS,
        3,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_SPOS_RUN1,
        0,
        0,
    ), // S_SPOS_RUN8
    State::new(
        SpriteNum::SPR_SPOS,
        4,
        10,
        ActionF::Actor(a_facetarget),
        StateNum::S_SPOS_ATK2,
        0,
        0,
    ), // S_SPOS_ATK1
    State::new(
        SpriteNum::SPR_SPOS,
        32773,
        10,
        ActionF::Actor(a_sposattack),
        StateNum::S_SPOS_ATK3,
        0,
        0,
    ), // S_SPOS_ATK2
    State::new(
        SpriteNum::SPR_SPOS,
        4,
        10,
        ActionF::None,
        StateNum::S_SPOS_RUN1,
        0,
        0,
    ), // S_SPOS_ATK3
    State::new(
        SpriteNum::SPR_SPOS,
        6,
        3,
        ActionF::None,
        StateNum::S_SPOS_PAIN2,
        0,
        0,
    ), // S_SPOS_PAIN
    State::new(
        SpriteNum::SPR_SPOS,
        6,
        3,
        ActionF::Actor(a_pain),
        StateNum::S_SPOS_RUN1,
        0,
        0,
    ), // S_SPOS_PAIN2
    State::new(
        SpriteNum::SPR_SPOS,
        7,
        5,
        ActionF::None,
        StateNum::S_SPOS_DIE2,
        0,
        0,
    ), // S_SPOS_DIE1
    State::new(
        SpriteNum::SPR_SPOS,
        8,
        5,
        ActionF::Actor(a_scream),
        StateNum::S_SPOS_DIE3,
        0,
        0,
    ), // S_SPOS_DIE2
    State::new(
        SpriteNum::SPR_SPOS,
        9,
        5,
        ActionF::Actor(a_fall),
        StateNum::S_SPOS_DIE4,
        0,
        0,
    ), // S_SPOS_DIE3
    State::new(
        SpriteNum::SPR_SPOS,
        10,
        5,
        ActionF::None,
        StateNum::S_SPOS_DIE5,
        0,
        0,
    ), // S_SPOS_DIE4
    State::new(
        SpriteNum::SPR_SPOS,
        11,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_SPOS_DIE5
    State::new(
        SpriteNum::SPR_SPOS,
        12,
        5,
        ActionF::None,
        StateNum::S_SPOS_XDIE2,
        0,
        0,
    ), // S_SPOS_XDIE1
    State::new(
        SpriteNum::SPR_SPOS,
        13,
        5,
        ActionF::Actor(a_xscream),
        StateNum::S_SPOS_XDIE3,
        0,
        0,
    ), // S_SPOS_XDIE2
    State::new(
        SpriteNum::SPR_SPOS,
        14,
        5,
        ActionF::Actor(a_fall),
        StateNum::S_SPOS_XDIE4,
        0,
        0,
    ), // S_SPOS_XDIE3
    State::new(
        SpriteNum::SPR_SPOS,
        15,
        5,
        ActionF::None,
        StateNum::S_SPOS_XDIE5,
        0,
        0,
    ), // S_SPOS_XDIE4
    State::new(
        SpriteNum::SPR_SPOS,
        16,
        5,
        ActionF::None,
        StateNum::S_SPOS_XDIE6,
        0,
        0,
    ), // S_SPOS_XDIE5
    State::new(
        SpriteNum::SPR_SPOS,
        17,
        5,
        ActionF::None,
        StateNum::S_SPOS_XDIE7,
        0,
        0,
    ), // S_SPOS_XDIE6
    State::new(
        SpriteNum::SPR_SPOS,
        18,
        5,
        ActionF::None,
        StateNum::S_SPOS_XDIE8,
        0,
        0,
    ), // S_SPOS_XDIE7
    State::new(
        SpriteNum::SPR_SPOS,
        19,
        5,
        ActionF::None,
        StateNum::S_SPOS_XDIE9,
        0,
        0,
    ), // S_SPOS_XDIE8
    State::new(
        SpriteNum::SPR_SPOS,
        20,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_SPOS_XDIE9
    State::new(
        SpriteNum::SPR_SPOS,
        11,
        5,
        ActionF::None,
        StateNum::S_SPOS_RAISE2,
        0,
        0,
    ), // S_SPOS_RAISE1
    State::new(
        SpriteNum::SPR_SPOS,
        10,
        5,
        ActionF::None,
        StateNum::S_SPOS_RAISE3,
        0,
        0,
    ), // S_SPOS_RAISE2
    State::new(
        SpriteNum::SPR_SPOS,
        9,
        5,
        ActionF::None,
        StateNum::S_SPOS_RAISE4,
        0,
        0,
    ), // S_SPOS_RAISE3
    State::new(
        SpriteNum::SPR_SPOS,
        8,
        5,
        ActionF::None,
        StateNum::S_SPOS_RAISE5,
        0,
        0,
    ), // S_SPOS_RAISE4
    State::new(
        SpriteNum::SPR_SPOS,
        7,
        5,
        ActionF::None,
        StateNum::S_SPOS_RUN1,
        0,
        0,
    ), // S_SPOS_RAISE5
    State::new(
        SpriteNum::SPR_VILE,
        0,
        10,
        ActionF::Actor(a_look),
        StateNum::S_VILE_STND2,
        0,
        0,
    ), // S_VILE_STND
    State::new(
        SpriteNum::SPR_VILE,
        1,
        10,
        ActionF::Actor(a_look),
        StateNum::S_VILE_STND,
        0,
        0,
    ), // S_VILE_STND2
    State::new(
        SpriteNum::SPR_VILE,
        0,
        2,
        ActionF::Actor(a_vilechase),
        StateNum::S_VILE_RUN2,
        0,
        0,
    ), // S_VILE_RUN1
    State::new(
        SpriteNum::SPR_VILE,
        0,
        2,
        ActionF::Actor(a_vilechase),
        StateNum::S_VILE_RUN3,
        0,
        0,
    ), // S_VILE_RUN2
    State::new(
        SpriteNum::SPR_VILE,
        1,
        2,
        ActionF::Actor(a_vilechase),
        StateNum::S_VILE_RUN4,
        0,
        0,
    ), // S_VILE_RUN3
    State::new(
        SpriteNum::SPR_VILE,
        1,
        2,
        ActionF::Actor(a_vilechase),
        StateNum::S_VILE_RUN5,
        0,
        0,
    ), // S_VILE_RUN4
    State::new(
        SpriteNum::SPR_VILE,
        2,
        2,
        ActionF::Actor(a_vilechase),
        StateNum::S_VILE_RUN6,
        0,
        0,
    ), // S_VILE_RUN5
    State::new(
        SpriteNum::SPR_VILE,
        2,
        2,
        ActionF::Actor(a_vilechase),
        StateNum::S_VILE_RUN7,
        0,
        0,
    ), // S_VILE_RUN6
    State::new(
        SpriteNum::SPR_VILE,
        3,
        2,
        ActionF::Actor(a_vilechase),
        StateNum::S_VILE_RUN8,
        0,
        0,
    ), // S_VILE_RUN7
    State::new(
        SpriteNum::SPR_VILE,
        3,
        2,
        ActionF::Actor(a_vilechase),
        StateNum::S_VILE_RUN9,
        0,
        0,
    ), // S_VILE_RUN8
    State::new(
        SpriteNum::SPR_VILE,
        4,
        2,
        ActionF::Actor(a_vilechase),
        StateNum::S_VILE_RUN10,
        0,
        0,
    ), // S_VILE_RUN9
    State::new(
        SpriteNum::SPR_VILE,
        4,
        2,
        ActionF::Actor(a_vilechase),
        StateNum::S_VILE_RUN11,
        0,
        0,
    ), // S_VILE_RUN10
    State::new(
        SpriteNum::SPR_VILE,
        5,
        2,
        ActionF::Actor(a_vilechase),
        StateNum::S_VILE_RUN12,
        0,
        0,
    ), // S_VILE_RUN11
    State::new(
        SpriteNum::SPR_VILE,
        5,
        2,
        ActionF::Actor(a_vilechase),
        StateNum::S_VILE_RUN1,
        0,
        0,
    ), // S_VILE_RUN12
    State::new(
        SpriteNum::SPR_VILE,
        32774,
        0,
        ActionF::Actor(a_vilestart),
        StateNum::S_VILE_ATK2,
        0,
        0,
    ), // S_VILE_ATK1
    State::new(
        SpriteNum::SPR_VILE,
        32774,
        10,
        ActionF::Actor(a_facetarget),
        StateNum::S_VILE_ATK3,
        0,
        0,
    ), // S_VILE_ATK2
    State::new(
        SpriteNum::SPR_VILE,
        32775,
        8,
        ActionF::Actor(a_viletarget),
        StateNum::S_VILE_ATK4,
        0,
        0,
    ), // S_VILE_ATK3
    State::new(
        SpriteNum::SPR_VILE,
        32776,
        8,
        ActionF::Actor(a_facetarget),
        StateNum::S_VILE_ATK5,
        0,
        0,
    ), // S_VILE_ATK4
    State::new(
        SpriteNum::SPR_VILE,
        32777,
        8,
        ActionF::Actor(a_facetarget),
        StateNum::S_VILE_ATK6,
        0,
        0,
    ), // S_VILE_ATK5
    State::new(
        SpriteNum::SPR_VILE,
        32778,
        8,
        ActionF::Actor(a_facetarget),
        StateNum::S_VILE_ATK7,
        0,
        0,
    ), // S_VILE_ATK6
    State::new(
        SpriteNum::SPR_VILE,
        32779,
        8,
        ActionF::Actor(a_facetarget),
        StateNum::S_VILE_ATK8,
        0,
        0,
    ), // S_VILE_ATK7
    State::new(
        SpriteNum::SPR_VILE,
        32780,
        8,
        ActionF::Actor(a_facetarget),
        StateNum::S_VILE_ATK9,
        0,
        0,
    ), // S_VILE_ATK8
    State::new(
        SpriteNum::SPR_VILE,
        32781,
        8,
        ActionF::Actor(a_facetarget),
        StateNum::S_VILE_ATK10,
        0,
        0,
    ), // S_VILE_ATK9
    State::new(
        SpriteNum::SPR_VILE,
        32782,
        8,
        ActionF::Actor(a_vileattack),
        StateNum::S_VILE_ATK11,
        0,
        0,
    ), // S_VILE_ATK10
    State::new(
        SpriteNum::SPR_VILE,
        32783,
        20,
        ActionF::None,
        StateNum::S_VILE_RUN1,
        0,
        0,
    ), // S_VILE_ATK11
    State::new(
        SpriteNum::SPR_VILE,
        32794,
        10,
        ActionF::None,
        StateNum::S_VILE_HEAL2,
        0,
        0,
    ), // S_VILE_HEAL1
    State::new(
        SpriteNum::SPR_VILE,
        32795,
        10,
        ActionF::None,
        StateNum::S_VILE_HEAL3,
        0,
        0,
    ), // S_VILE_HEAL2
    State::new(
        SpriteNum::SPR_VILE,
        32796,
        10,
        ActionF::None,
        StateNum::S_VILE_RUN1,
        0,
        0,
    ), // S_VILE_HEAL3
    State::new(
        SpriteNum::SPR_VILE,
        16,
        5,
        ActionF::None,
        StateNum::S_VILE_PAIN2,
        0,
        0,
    ), // S_VILE_PAIN
    State::new(
        SpriteNum::SPR_VILE,
        16,
        5,
        ActionF::Actor(a_pain),
        StateNum::S_VILE_RUN1,
        0,
        0,
    ), // S_VILE_PAIN2
    State::new(
        SpriteNum::SPR_VILE,
        16,
        7,
        ActionF::None,
        StateNum::S_VILE_DIE2,
        0,
        0,
    ), // S_VILE_DIE1
    State::new(
        SpriteNum::SPR_VILE,
        17,
        7,
        ActionF::Actor(a_scream),
        StateNum::S_VILE_DIE3,
        0,
        0,
    ), // S_VILE_DIE2
    State::new(
        SpriteNum::SPR_VILE,
        18,
        7,
        ActionF::Actor(a_fall),
        StateNum::S_VILE_DIE4,
        0,
        0,
    ), // S_VILE_DIE3
    State::new(
        SpriteNum::SPR_VILE,
        19,
        7,
        ActionF::None,
        StateNum::S_VILE_DIE5,
        0,
        0,
    ), // S_VILE_DIE4
    State::new(
        SpriteNum::SPR_VILE,
        20,
        7,
        ActionF::None,
        StateNum::S_VILE_DIE6,
        0,
        0,
    ), // S_VILE_DIE5
    State::new(
        SpriteNum::SPR_VILE,
        21,
        7,
        ActionF::None,
        StateNum::S_VILE_DIE7,
        0,
        0,
    ), // S_VILE_DIE6
    State::new(
        SpriteNum::SPR_VILE,
        22,
        7,
        ActionF::None,
        StateNum::S_VILE_DIE8,
        0,
        0,
    ), // S_VILE_DIE7
    State::new(
        SpriteNum::SPR_VILE,
        23,
        5,
        ActionF::None,
        StateNum::S_VILE_DIE9,
        0,
        0,
    ), // S_VILE_DIE8
    State::new(
        SpriteNum::SPR_VILE,
        24,
        5,
        ActionF::None,
        StateNum::S_VILE_DIE10,
        0,
        0,
    ), // S_VILE_DIE9
    State::new(
        SpriteNum::SPR_VILE,
        25,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_VILE_DIE10
    State::new(
        SpriteNum::SPR_FIRE,
        32768,
        2,
        ActionF::Actor(a_startfire),
        StateNum::S_FIRE2,
        0,
        0,
    ), // S_FIRE1
    State::new(
        SpriteNum::SPR_FIRE,
        32769,
        2,
        ActionF::Actor(a_fire),
        StateNum::S_FIRE3,
        0,
        0,
    ), // S_FIRE2
    State::new(
        SpriteNum::SPR_FIRE,
        32768,
        2,
        ActionF::Actor(a_fire),
        StateNum::S_FIRE4,
        0,
        0,
    ), // S_FIRE3
    State::new(
        SpriteNum::SPR_FIRE,
        32769,
        2,
        ActionF::Actor(a_fire),
        StateNum::S_FIRE5,
        0,
        0,
    ), // S_FIRE4
    State::new(
        SpriteNum::SPR_FIRE,
        32770,
        2,
        ActionF::Actor(a_firecrackle),
        StateNum::S_FIRE6,
        0,
        0,
    ), // S_FIRE5
    State::new(
        SpriteNum::SPR_FIRE,
        32769,
        2,
        ActionF::Actor(a_fire),
        StateNum::S_FIRE7,
        0,
        0,
    ), // S_FIRE6
    State::new(
        SpriteNum::SPR_FIRE,
        32770,
        2,
        ActionF::Actor(a_fire),
        StateNum::S_FIRE8,
        0,
        0,
    ), // S_FIRE7
    State::new(
        SpriteNum::SPR_FIRE,
        32769,
        2,
        ActionF::Actor(a_fire),
        StateNum::S_FIRE9,
        0,
        0,
    ), // S_FIRE8
    State::new(
        SpriteNum::SPR_FIRE,
        32770,
        2,
        ActionF::Actor(a_fire),
        StateNum::S_FIRE10,
        0,
        0,
    ), // S_FIRE9
    State::new(
        SpriteNum::SPR_FIRE,
        32771,
        2,
        ActionF::Actor(a_fire),
        StateNum::S_FIRE11,
        0,
        0,
    ), // S_FIRE10
    State::new(
        SpriteNum::SPR_FIRE,
        32770,
        2,
        ActionF::Actor(a_fire),
        StateNum::S_FIRE12,
        0,
        0,
    ), // S_FIRE11
    State::new(
        SpriteNum::SPR_FIRE,
        32771,
        2,
        ActionF::Actor(a_fire),
        StateNum::S_FIRE13,
        0,
        0,
    ), // S_FIRE12
    State::new(
        SpriteNum::SPR_FIRE,
        32770,
        2,
        ActionF::Actor(a_fire),
        StateNum::S_FIRE14,
        0,
        0,
    ), // S_FIRE13
    State::new(
        SpriteNum::SPR_FIRE,
        32771,
        2,
        ActionF::Actor(a_fire),
        StateNum::S_FIRE15,
        0,
        0,
    ), // S_FIRE14
    State::new(
        SpriteNum::SPR_FIRE,
        32772,
        2,
        ActionF::Actor(a_fire),
        StateNum::S_FIRE16,
        0,
        0,
    ), // S_FIRE15
    State::new(
        SpriteNum::SPR_FIRE,
        32771,
        2,
        ActionF::Actor(a_fire),
        StateNum::S_FIRE17,
        0,
        0,
    ), // S_FIRE16
    State::new(
        SpriteNum::SPR_FIRE,
        32772,
        2,
        ActionF::Actor(a_fire),
        StateNum::S_FIRE18,
        0,
        0,
    ), // S_FIRE17
    State::new(
        SpriteNum::SPR_FIRE,
        32771,
        2,
        ActionF::Actor(a_fire),
        StateNum::S_FIRE19,
        0,
        0,
    ), // S_FIRE18
    State::new(
        SpriteNum::SPR_FIRE,
        32772,
        2,
        ActionF::Actor(a_firecrackle),
        StateNum::S_FIRE20,
        0,
        0,
    ), // S_FIRE19
    State::new(
        SpriteNum::SPR_FIRE,
        32773,
        2,
        ActionF::Actor(a_fire),
        StateNum::S_FIRE21,
        0,
        0,
    ), // S_FIRE20
    State::new(
        SpriteNum::SPR_FIRE,
        32772,
        2,
        ActionF::Actor(a_fire),
        StateNum::S_FIRE22,
        0,
        0,
    ), // S_FIRE21
    State::new(
        SpriteNum::SPR_FIRE,
        32773,
        2,
        ActionF::Actor(a_fire),
        StateNum::S_FIRE23,
        0,
        0,
    ), // S_FIRE22
    State::new(
        SpriteNum::SPR_FIRE,
        32772,
        2,
        ActionF::Actor(a_fire),
        StateNum::S_FIRE24,
        0,
        0,
    ), // S_FIRE23
    State::new(
        SpriteNum::SPR_FIRE,
        32773,
        2,
        ActionF::Actor(a_fire),
        StateNum::S_FIRE25,
        0,
        0,
    ), // S_FIRE24
    State::new(
        SpriteNum::SPR_FIRE,
        32774,
        2,
        ActionF::Actor(a_fire),
        StateNum::S_FIRE26,
        0,
        0,
    ), // S_FIRE25
    State::new(
        SpriteNum::SPR_FIRE,
        32775,
        2,
        ActionF::Actor(a_fire),
        StateNum::S_FIRE27,
        0,
        0,
    ), // S_FIRE26
    State::new(
        SpriteNum::SPR_FIRE,
        32774,
        2,
        ActionF::Actor(a_fire),
        StateNum::S_FIRE28,
        0,
        0,
    ), // S_FIRE27
    State::new(
        SpriteNum::SPR_FIRE,
        32775,
        2,
        ActionF::Actor(a_fire),
        StateNum::S_FIRE29,
        0,
        0,
    ), // S_FIRE28
    State::new(
        SpriteNum::SPR_FIRE,
        32774,
        2,
        ActionF::Actor(a_fire),
        StateNum::S_FIRE30,
        0,
        0,
    ), // S_FIRE29
    State::new(
        SpriteNum::SPR_FIRE,
        32775,
        2,
        ActionF::Actor(a_fire),
        StateNum::S_NULL,
        0,
        0,
    ), // S_FIRE30
    State::new(
        SpriteNum::SPR_PUFF,
        1,
        4,
        ActionF::None,
        StateNum::S_SMOKE2,
        0,
        0,
    ), // S_SMOKE1
    State::new(
        SpriteNum::SPR_PUFF,
        2,
        4,
        ActionF::None,
        StateNum::S_SMOKE3,
        0,
        0,
    ), // S_SMOKE2
    State::new(
        SpriteNum::SPR_PUFF,
        1,
        4,
        ActionF::None,
        StateNum::S_SMOKE4,
        0,
        0,
    ), // S_SMOKE3
    State::new(
        SpriteNum::SPR_PUFF,
        2,
        4,
        ActionF::None,
        StateNum::S_SMOKE5,
        0,
        0,
    ), // S_SMOKE4
    State::new(
        SpriteNum::SPR_PUFF,
        3,
        4,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_SMOKE5
    State::new(
        SpriteNum::SPR_FATB,
        32768,
        2,
        ActionF::Actor(a_tracer),
        StateNum::S_TRACER2,
        0,
        0,
    ), // S_TRACER
    State::new(
        SpriteNum::SPR_FATB,
        32769,
        2,
        ActionF::Actor(a_tracer),
        StateNum::S_TRACER,
        0,
        0,
    ), // S_TRACER2
    State::new(
        SpriteNum::SPR_FBXP,
        32768,
        8,
        ActionF::None,
        StateNum::S_TRACEEXP2,
        0,
        0,
    ), // S_TRACEEXP1
    State::new(
        SpriteNum::SPR_FBXP,
        32769,
        6,
        ActionF::None,
        StateNum::S_TRACEEXP3,
        0,
        0,
    ), // S_TRACEEXP2
    State::new(
        SpriteNum::SPR_FBXP,
        32770,
        4,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_TRACEEXP3
    State::new(
        SpriteNum::SPR_SKEL,
        0,
        10,
        ActionF::Actor(a_look),
        StateNum::S_SKEL_STND2,
        0,
        0,
    ), // S_SKEL_STND
    State::new(
        SpriteNum::SPR_SKEL,
        1,
        10,
        ActionF::Actor(a_look),
        StateNum::S_SKEL_STND,
        0,
        0,
    ), // S_SKEL_STND2
    State::new(
        SpriteNum::SPR_SKEL,
        0,
        2,
        ActionF::Actor(a_chase),
        StateNum::S_SKEL_RUN2,
        0,
        0,
    ), // S_SKEL_RUN1
    State::new(
        SpriteNum::SPR_SKEL,
        0,
        2,
        ActionF::Actor(a_chase),
        StateNum::S_SKEL_RUN3,
        0,
        0,
    ), // S_SKEL_RUN2
    State::new(
        SpriteNum::SPR_SKEL,
        1,
        2,
        ActionF::Actor(a_chase),
        StateNum::S_SKEL_RUN4,
        0,
        0,
    ), // S_SKEL_RUN3
    State::new(
        SpriteNum::SPR_SKEL,
        1,
        2,
        ActionF::Actor(a_chase),
        StateNum::S_SKEL_RUN5,
        0,
        0,
    ), // S_SKEL_RUN4
    State::new(
        SpriteNum::SPR_SKEL,
        2,
        2,
        ActionF::Actor(a_chase),
        StateNum::S_SKEL_RUN6,
        0,
        0,
    ), // S_SKEL_RUN5
    State::new(
        SpriteNum::SPR_SKEL,
        2,
        2,
        ActionF::Actor(a_chase),
        StateNum::S_SKEL_RUN7,
        0,
        0,
    ), // S_SKEL_RUN6
    State::new(
        SpriteNum::SPR_SKEL,
        3,
        2,
        ActionF::Actor(a_chase),
        StateNum::S_SKEL_RUN8,
        0,
        0,
    ), // S_SKEL_RUN7
    State::new(
        SpriteNum::SPR_SKEL,
        3,
        2,
        ActionF::Actor(a_chase),
        StateNum::S_SKEL_RUN9,
        0,
        0,
    ), // S_SKEL_RUN8
    State::new(
        SpriteNum::SPR_SKEL,
        4,
        2,
        ActionF::Actor(a_chase),
        StateNum::S_SKEL_RUN10,
        0,
        0,
    ), // S_SKEL_RUN9
    State::new(
        SpriteNum::SPR_SKEL,
        4,
        2,
        ActionF::Actor(a_chase),
        StateNum::S_SKEL_RUN11,
        0,
        0,
    ), // S_SKEL_RUN10
    State::new(
        SpriteNum::SPR_SKEL,
        5,
        2,
        ActionF::Actor(a_chase),
        StateNum::S_SKEL_RUN12,
        0,
        0,
    ), // S_SKEL_RUN11
    State::new(
        SpriteNum::SPR_SKEL,
        5,
        2,
        ActionF::Actor(a_chase),
        StateNum::S_SKEL_RUN1,
        0,
        0,
    ), // S_SKEL_RUN12
    State::new(
        SpriteNum::SPR_SKEL,
        6,
        0,
        ActionF::Actor(a_facetarget),
        StateNum::S_SKEL_FIST2,
        0,
        0,
    ), // S_SKEL_FIST1
    State::new(
        SpriteNum::SPR_SKEL,
        6,
        6,
        ActionF::Actor(a_skelwhoosh),
        StateNum::S_SKEL_FIST3,
        0,
        0,
    ), // S_SKEL_FIST2
    State::new(
        SpriteNum::SPR_SKEL,
        7,
        6,
        ActionF::Actor(a_facetarget),
        StateNum::S_SKEL_FIST4,
        0,
        0,
    ), // S_SKEL_FIST3
    State::new(
        SpriteNum::SPR_SKEL,
        8,
        6,
        ActionF::Actor(a_skelfist),
        StateNum::S_SKEL_RUN1,
        0,
        0,
    ), // S_SKEL_FIST4
    State::new(
        SpriteNum::SPR_SKEL,
        32777,
        0,
        ActionF::Actor(a_facetarget),
        StateNum::S_SKEL_MISS2,
        0,
        0,
    ), // S_SKEL_MISS1
    State::new(
        SpriteNum::SPR_SKEL,
        32777,
        10,
        ActionF::Actor(a_facetarget),
        StateNum::S_SKEL_MISS3,
        0,
        0,
    ), // S_SKEL_MISS2
    State::new(
        SpriteNum::SPR_SKEL,
        10,
        10,
        ActionF::Actor(a_skelmissile),
        StateNum::S_SKEL_MISS4,
        0,
        0,
    ), // S_SKEL_MISS3
    State::new(
        SpriteNum::SPR_SKEL,
        10,
        10,
        ActionF::Actor(a_facetarget),
        StateNum::S_SKEL_RUN1,
        0,
        0,
    ), // S_SKEL_MISS4
    State::new(
        SpriteNum::SPR_SKEL,
        11,
        5,
        ActionF::None,
        StateNum::S_SKEL_PAIN2,
        0,
        0,
    ), // S_SKEL_PAIN
    State::new(
        SpriteNum::SPR_SKEL,
        11,
        5,
        ActionF::Actor(a_pain),
        StateNum::S_SKEL_RUN1,
        0,
        0,
    ), // S_SKEL_PAIN2
    State::new(
        SpriteNum::SPR_SKEL,
        11,
        7,
        ActionF::None,
        StateNum::S_SKEL_DIE2,
        0,
        0,
    ), // S_SKEL_DIE1
    State::new(
        SpriteNum::SPR_SKEL,
        12,
        7,
        ActionF::None,
        StateNum::S_SKEL_DIE3,
        0,
        0,
    ), // S_SKEL_DIE2
    State::new(
        SpriteNum::SPR_SKEL,
        13,
        7,
        ActionF::Actor(a_scream),
        StateNum::S_SKEL_DIE4,
        0,
        0,
    ), // S_SKEL_DIE3
    State::new(
        SpriteNum::SPR_SKEL,
        14,
        7,
        ActionF::Actor(a_fall),
        StateNum::S_SKEL_DIE5,
        0,
        0,
    ), // S_SKEL_DIE4
    State::new(
        SpriteNum::SPR_SKEL,
        15,
        7,
        ActionF::None,
        StateNum::S_SKEL_DIE6,
        0,
        0,
    ), // S_SKEL_DIE5
    State::new(
        SpriteNum::SPR_SKEL,
        16,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_SKEL_DIE6
    State::new(
        SpriteNum::SPR_SKEL,
        16,
        5,
        ActionF::None,
        StateNum::S_SKEL_RAISE2,
        0,
        0,
    ), // S_SKEL_RAISE1
    State::new(
        SpriteNum::SPR_SKEL,
        15,
        5,
        ActionF::None,
        StateNum::S_SKEL_RAISE3,
        0,
        0,
    ), // S_SKEL_RAISE2
    State::new(
        SpriteNum::SPR_SKEL,
        14,
        5,
        ActionF::None,
        StateNum::S_SKEL_RAISE4,
        0,
        0,
    ), // S_SKEL_RAISE3
    State::new(
        SpriteNum::SPR_SKEL,
        13,
        5,
        ActionF::None,
        StateNum::S_SKEL_RAISE5,
        0,
        0,
    ), // S_SKEL_RAISE4
    State::new(
        SpriteNum::SPR_SKEL,
        12,
        5,
        ActionF::None,
        StateNum::S_SKEL_RAISE6,
        0,
        0,
    ), // S_SKEL_RAISE5
    State::new(
        SpriteNum::SPR_SKEL,
        11,
        5,
        ActionF::None,
        StateNum::S_SKEL_RUN1,
        0,
        0,
    ), // S_SKEL_RAISE6
    State::new(
        SpriteNum::SPR_MANF,
        32768,
        4,
        ActionF::None,
        StateNum::S_FATSHOT2,
        0,
        0,
    ), // S_FATSHOT1
    State::new(
        SpriteNum::SPR_MANF,
        32769,
        4,
        ActionF::None,
        StateNum::S_FATSHOT1,
        0,
        0,
    ), // S_FATSHOT2
    State::new(
        SpriteNum::SPR_MISL,
        32769,
        8,
        ActionF::None,
        StateNum::S_FATSHOTX2,
        0,
        0,
    ), // S_FATSHOTX1
    State::new(
        SpriteNum::SPR_MISL,
        32770,
        6,
        ActionF::None,
        StateNum::S_FATSHOTX3,
        0,
        0,
    ), // S_FATSHOTX2
    State::new(
        SpriteNum::SPR_MISL,
        32771,
        4,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_FATSHOTX3
    State::new(
        SpriteNum::SPR_FATT,
        0,
        15,
        ActionF::Actor(a_look),
        StateNum::S_FATT_STND2,
        0,
        0,
    ), // S_FATT_STND
    State::new(
        SpriteNum::SPR_FATT,
        1,
        15,
        ActionF::Actor(a_look),
        StateNum::S_FATT_STND,
        0,
        0,
    ), // S_FATT_STND2
    State::new(
        SpriteNum::SPR_FATT,
        0,
        4,
        ActionF::Actor(a_chase),
        StateNum::S_FATT_RUN2,
        0,
        0,
    ), // S_FATT_RUN1
    State::new(
        SpriteNum::SPR_FATT,
        0,
        4,
        ActionF::Actor(a_chase),
        StateNum::S_FATT_RUN3,
        0,
        0,
    ), // S_FATT_RUN2
    State::new(
        SpriteNum::SPR_FATT,
        1,
        4,
        ActionF::Actor(a_chase),
        StateNum::S_FATT_RUN4,
        0,
        0,
    ), // S_FATT_RUN3
    State::new(
        SpriteNum::SPR_FATT,
        1,
        4,
        ActionF::Actor(a_chase),
        StateNum::S_FATT_RUN5,
        0,
        0,
    ), // S_FATT_RUN4
    State::new(
        SpriteNum::SPR_FATT,
        2,
        4,
        ActionF::Actor(a_chase),
        StateNum::S_FATT_RUN6,
        0,
        0,
    ), // S_FATT_RUN5
    State::new(
        SpriteNum::SPR_FATT,
        2,
        4,
        ActionF::Actor(a_chase),
        StateNum::S_FATT_RUN7,
        0,
        0,
    ), // S_FATT_RUN6
    State::new(
        SpriteNum::SPR_FATT,
        3,
        4,
        ActionF::Actor(a_chase),
        StateNum::S_FATT_RUN8,
        0,
        0,
    ), // S_FATT_RUN7
    State::new(
        SpriteNum::SPR_FATT,
        3,
        4,
        ActionF::Actor(a_chase),
        StateNum::S_FATT_RUN9,
        0,
        0,
    ), // S_FATT_RUN8
    State::new(
        SpriteNum::SPR_FATT,
        4,
        4,
        ActionF::Actor(a_chase),
        StateNum::S_FATT_RUN10,
        0,
        0,
    ), // S_FATT_RUN9
    State::new(
        SpriteNum::SPR_FATT,
        4,
        4,
        ActionF::Actor(a_chase),
        StateNum::S_FATT_RUN11,
        0,
        0,
    ), // S_FATT_RUN10
    State::new(
        SpriteNum::SPR_FATT,
        5,
        4,
        ActionF::Actor(a_chase),
        StateNum::S_FATT_RUN12,
        0,
        0,
    ), // S_FATT_RUN11
    State::new(
        SpriteNum::SPR_FATT,
        5,
        4,
        ActionF::Actor(a_chase),
        StateNum::S_FATT_RUN1,
        0,
        0,
    ), // S_FATT_RUN12
    State::new(
        SpriteNum::SPR_FATT,
        6,
        20,
        ActionF::Actor(a_fatraise),
        StateNum::S_FATT_ATK2,
        0,
        0,
    ), // S_FATT_ATK1
    State::new(
        SpriteNum::SPR_FATT,
        32775,
        10,
        ActionF::Actor(a_fatattack1),
        StateNum::S_FATT_ATK3,
        0,
        0,
    ), // S_FATT_ATK2
    State::new(
        SpriteNum::SPR_FATT,
        8,
        5,
        ActionF::Actor(a_facetarget),
        StateNum::S_FATT_ATK4,
        0,
        0,
    ), // S_FATT_ATK3
    State::new(
        SpriteNum::SPR_FATT,
        6,
        5,
        ActionF::Actor(a_facetarget),
        StateNum::S_FATT_ATK5,
        0,
        0,
    ), // S_FATT_ATK4
    State::new(
        SpriteNum::SPR_FATT,
        32775,
        10,
        ActionF::Actor(a_fatattack2),
        StateNum::S_FATT_ATK6,
        0,
        0,
    ), // S_FATT_ATK5
    State::new(
        SpriteNum::SPR_FATT,
        8,
        5,
        ActionF::Actor(a_facetarget),
        StateNum::S_FATT_ATK7,
        0,
        0,
    ), // S_FATT_ATK6
    State::new(
        SpriteNum::SPR_FATT,
        6,
        5,
        ActionF::Actor(a_facetarget),
        StateNum::S_FATT_ATK8,
        0,
        0,
    ), // S_FATT_ATK7
    State::new(
        SpriteNum::SPR_FATT,
        32775,
        10,
        ActionF::Actor(a_fatattack3),
        StateNum::S_FATT_ATK9,
        0,
        0,
    ), // S_FATT_ATK8
    State::new(
        SpriteNum::SPR_FATT,
        8,
        5,
        ActionF::Actor(a_facetarget),
        StateNum::S_FATT_ATK10,
        0,
        0,
    ), // S_FATT_ATK9
    State::new(
        SpriteNum::SPR_FATT,
        6,
        5,
        ActionF::Actor(a_facetarget),
        StateNum::S_FATT_RUN1,
        0,
        0,
    ), // S_FATT_ATK10
    State::new(
        SpriteNum::SPR_FATT,
        9,
        3,
        ActionF::None,
        StateNum::S_FATT_PAIN2,
        0,
        0,
    ), // S_FATT_PAIN
    State::new(
        SpriteNum::SPR_FATT,
        9,
        3,
        ActionF::Actor(a_pain),
        StateNum::S_FATT_RUN1,
        0,
        0,
    ), // S_FATT_PAIN2
    State::new(
        SpriteNum::SPR_FATT,
        10,
        6,
        ActionF::None,
        StateNum::S_FATT_DIE2,
        0,
        0,
    ), // S_FATT_DIE1
    State::new(
        SpriteNum::SPR_FATT,
        11,
        6,
        ActionF::Actor(a_scream),
        StateNum::S_FATT_DIE3,
        0,
        0,
    ), // S_FATT_DIE2
    State::new(
        SpriteNum::SPR_FATT,
        12,
        6,
        ActionF::Actor(a_fall),
        StateNum::S_FATT_DIE4,
        0,
        0,
    ), // S_FATT_DIE3
    State::new(
        SpriteNum::SPR_FATT,
        13,
        6,
        ActionF::None,
        StateNum::S_FATT_DIE5,
        0,
        0,
    ), // S_FATT_DIE4
    State::new(
        SpriteNum::SPR_FATT,
        14,
        6,
        ActionF::None,
        StateNum::S_FATT_DIE6,
        0,
        0,
    ), // S_FATT_DIE5
    State::new(
        SpriteNum::SPR_FATT,
        15,
        6,
        ActionF::None,
        StateNum::S_FATT_DIE7,
        0,
        0,
    ), // S_FATT_DIE6
    State::new(
        SpriteNum::SPR_FATT,
        16,
        6,
        ActionF::None,
        StateNum::S_FATT_DIE8,
        0,
        0,
    ), // S_FATT_DIE7
    State::new(
        SpriteNum::SPR_FATT,
        17,
        6,
        ActionF::None,
        StateNum::S_FATT_DIE9,
        0,
        0,
    ), // S_FATT_DIE8
    State::new(
        SpriteNum::SPR_FATT,
        18,
        6,
        ActionF::None,
        StateNum::S_FATT_DIE10,
        0,
        0,
    ), // S_FATT_DIE9
    State::new(
        SpriteNum::SPR_FATT,
        19,
        -1,
        ActionF::Actor(a_bossdeath),
        StateNum::S_NULL,
        0,
        0,
    ), // S_FATT_DIE10
    State::new(
        SpriteNum::SPR_FATT,
        17,
        5,
        ActionF::None,
        StateNum::S_FATT_RAISE2,
        0,
        0,
    ), // S_FATT_RAISE1
    State::new(
        SpriteNum::SPR_FATT,
        16,
        5,
        ActionF::None,
        StateNum::S_FATT_RAISE3,
        0,
        0,
    ), // S_FATT_RAISE2
    State::new(
        SpriteNum::SPR_FATT,
        15,
        5,
        ActionF::None,
        StateNum::S_FATT_RAISE4,
        0,
        0,
    ), // S_FATT_RAISE3
    State::new(
        SpriteNum::SPR_FATT,
        14,
        5,
        ActionF::None,
        StateNum::S_FATT_RAISE5,
        0,
        0,
    ), // S_FATT_RAISE4
    State::new(
        SpriteNum::SPR_FATT,
        13,
        5,
        ActionF::None,
        StateNum::S_FATT_RAISE6,
        0,
        0,
    ), // S_FATT_RAISE5
    State::new(
        SpriteNum::SPR_FATT,
        12,
        5,
        ActionF::None,
        StateNum::S_FATT_RAISE7,
        0,
        0,
    ), // S_FATT_RAISE6
    State::new(
        SpriteNum::SPR_FATT,
        11,
        5,
        ActionF::None,
        StateNum::S_FATT_RAISE8,
        0,
        0,
    ), // S_FATT_RAISE7
    State::new(
        SpriteNum::SPR_FATT,
        10,
        5,
        ActionF::None,
        StateNum::S_FATT_RUN1,
        0,
        0,
    ), // S_FATT_RAISE8
    State::new(
        SpriteNum::SPR_CPOS,
        0,
        10,
        ActionF::Actor(a_look),
        StateNum::S_CPOS_STND2,
        0,
        0,
    ), // S_CPOS_STND
    State::new(
        SpriteNum::SPR_CPOS,
        1,
        10,
        ActionF::Actor(a_look),
        StateNum::S_CPOS_STND,
        0,
        0,
    ), // S_CPOS_STND2
    State::new(
        SpriteNum::SPR_CPOS,
        0,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_CPOS_RUN2,
        0,
        0,
    ), // S_CPOS_RUN1
    State::new(
        SpriteNum::SPR_CPOS,
        0,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_CPOS_RUN3,
        0,
        0,
    ), // S_CPOS_RUN2
    State::new(
        SpriteNum::SPR_CPOS,
        1,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_CPOS_RUN4,
        0,
        0,
    ), // S_CPOS_RUN3
    State::new(
        SpriteNum::SPR_CPOS,
        1,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_CPOS_RUN5,
        0,
        0,
    ), // S_CPOS_RUN4
    State::new(
        SpriteNum::SPR_CPOS,
        2,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_CPOS_RUN6,
        0,
        0,
    ), // S_CPOS_RUN5
    State::new(
        SpriteNum::SPR_CPOS,
        2,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_CPOS_RUN7,
        0,
        0,
    ), // S_CPOS_RUN6
    State::new(
        SpriteNum::SPR_CPOS,
        3,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_CPOS_RUN8,
        0,
        0,
    ), // S_CPOS_RUN7
    State::new(
        SpriteNum::SPR_CPOS,
        3,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_CPOS_RUN1,
        0,
        0,
    ), // S_CPOS_RUN8
    State::new(
        SpriteNum::SPR_CPOS,
        4,
        10,
        ActionF::Actor(a_facetarget),
        StateNum::S_CPOS_ATK2,
        0,
        0,
    ), // S_CPOS_ATK1
    State::new(
        SpriteNum::SPR_CPOS,
        32773,
        4,
        ActionF::Actor(a_cposattack),
        StateNum::S_CPOS_ATK3,
        0,
        0,
    ), // S_CPOS_ATK2
    State::new(
        SpriteNum::SPR_CPOS,
        32772,
        4,
        ActionF::Actor(a_cposattack),
        StateNum::S_CPOS_ATK4,
        0,
        0,
    ), // S_CPOS_ATK3
    State::new(
        SpriteNum::SPR_CPOS,
        5,
        1,
        ActionF::Actor(a_cposrefire),
        StateNum::S_CPOS_ATK2,
        0,
        0,
    ), // S_CPOS_ATK4
    State::new(
        SpriteNum::SPR_CPOS,
        6,
        3,
        ActionF::None,
        StateNum::S_CPOS_PAIN2,
        0,
        0,
    ), // S_CPOS_PAIN
    State::new(
        SpriteNum::SPR_CPOS,
        6,
        3,
        ActionF::Actor(a_pain),
        StateNum::S_CPOS_RUN1,
        0,
        0,
    ), // S_CPOS_PAIN2
    State::new(
        SpriteNum::SPR_CPOS,
        7,
        5,
        ActionF::None,
        StateNum::S_CPOS_DIE2,
        0,
        0,
    ), // S_CPOS_DIE1
    State::new(
        SpriteNum::SPR_CPOS,
        8,
        5,
        ActionF::Actor(a_scream),
        StateNum::S_CPOS_DIE3,
        0,
        0,
    ), // S_CPOS_DIE2
    State::new(
        SpriteNum::SPR_CPOS,
        9,
        5,
        ActionF::Actor(a_fall),
        StateNum::S_CPOS_DIE4,
        0,
        0,
    ), // S_CPOS_DIE3
    State::new(
        SpriteNum::SPR_CPOS,
        10,
        5,
        ActionF::None,
        StateNum::S_CPOS_DIE5,
        0,
        0,
    ), // S_CPOS_DIE4
    State::new(
        SpriteNum::SPR_CPOS,
        11,
        5,
        ActionF::None,
        StateNum::S_CPOS_DIE6,
        0,
        0,
    ), // S_CPOS_DIE5
    State::new(
        SpriteNum::SPR_CPOS,
        12,
        5,
        ActionF::None,
        StateNum::S_CPOS_DIE7,
        0,
        0,
    ), // S_CPOS_DIE6
    State::new(
        SpriteNum::SPR_CPOS,
        13,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_CPOS_DIE7
    State::new(
        SpriteNum::SPR_CPOS,
        14,
        5,
        ActionF::None,
        StateNum::S_CPOS_XDIE2,
        0,
        0,
    ), // S_CPOS_XDIE1
    State::new(
        SpriteNum::SPR_CPOS,
        15,
        5,
        ActionF::Actor(a_xscream),
        StateNum::S_CPOS_XDIE3,
        0,
        0,
    ), // S_CPOS_XDIE2
    State::new(
        SpriteNum::SPR_CPOS,
        16,
        5,
        ActionF::Actor(a_fall),
        StateNum::S_CPOS_XDIE4,
        0,
        0,
    ), // S_CPOS_XDIE3
    State::new(
        SpriteNum::SPR_CPOS,
        17,
        5,
        ActionF::None,
        StateNum::S_CPOS_XDIE5,
        0,
        0,
    ), // S_CPOS_XDIE4
    State::new(
        SpriteNum::SPR_CPOS,
        18,
        5,
        ActionF::None,
        StateNum::S_CPOS_XDIE6,
        0,
        0,
    ), // S_CPOS_XDIE5
    State::new(
        SpriteNum::SPR_CPOS,
        19,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_CPOS_XDIE6
    State::new(
        SpriteNum::SPR_CPOS,
        13,
        5,
        ActionF::None,
        StateNum::S_CPOS_RAISE2,
        0,
        0,
    ), // S_CPOS_RAISE1
    State::new(
        SpriteNum::SPR_CPOS,
        12,
        5,
        ActionF::None,
        StateNum::S_CPOS_RAISE3,
        0,
        0,
    ), // S_CPOS_RAISE2
    State::new(
        SpriteNum::SPR_CPOS,
        11,
        5,
        ActionF::None,
        StateNum::S_CPOS_RAISE4,
        0,
        0,
    ), // S_CPOS_RAISE3
    State::new(
        SpriteNum::SPR_CPOS,
        10,
        5,
        ActionF::None,
        StateNum::S_CPOS_RAISE5,
        0,
        0,
    ), // S_CPOS_RAISE4
    State::new(
        SpriteNum::SPR_CPOS,
        9,
        5,
        ActionF::None,
        StateNum::S_CPOS_RAISE6,
        0,
        0,
    ), // S_CPOS_RAISE5
    State::new(
        SpriteNum::SPR_CPOS,
        8,
        5,
        ActionF::None,
        StateNum::S_CPOS_RAISE7,
        0,
        0,
    ), // S_CPOS_RAISE6
    State::new(
        SpriteNum::SPR_CPOS,
        7,
        5,
        ActionF::None,
        StateNum::S_CPOS_RUN1,
        0,
        0,
    ), // S_CPOS_RAISE7
    State::new(
        SpriteNum::SPR_TROO,
        0,
        10,
        ActionF::Actor(a_look),
        StateNum::S_TROO_STND2,
        0,
        0,
    ), // S_TROO_STND
    State::new(
        SpriteNum::SPR_TROO,
        1,
        10,
        ActionF::Actor(a_look),
        StateNum::S_TROO_STND,
        0,
        0,
    ), // S_TROO_STND2
    State::new(
        SpriteNum::SPR_TROO,
        0,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_TROO_RUN2,
        0,
        0,
    ), // S_TROO_RUN1
    State::new(
        SpriteNum::SPR_TROO,
        0,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_TROO_RUN3,
        0,
        0,
    ), // S_TROO_RUN2
    State::new(
        SpriteNum::SPR_TROO,
        1,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_TROO_RUN4,
        0,
        0,
    ), // S_TROO_RUN3
    State::new(
        SpriteNum::SPR_TROO,
        1,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_TROO_RUN5,
        0,
        0,
    ), // S_TROO_RUN4
    State::new(
        SpriteNum::SPR_TROO,
        2,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_TROO_RUN6,
        0,
        0,
    ), // S_TROO_RUN5
    State::new(
        SpriteNum::SPR_TROO,
        2,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_TROO_RUN7,
        0,
        0,
    ), // S_TROO_RUN6
    State::new(
        SpriteNum::SPR_TROO,
        3,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_TROO_RUN8,
        0,
        0,
    ), // S_TROO_RUN7
    State::new(
        SpriteNum::SPR_TROO,
        3,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_TROO_RUN1,
        0,
        0,
    ), // S_TROO_RUN8
    State::new(
        SpriteNum::SPR_TROO,
        4,
        8,
        ActionF::Actor(a_facetarget),
        StateNum::S_TROO_ATK2,
        0,
        0,
    ), // S_TROO_ATK1
    State::new(
        SpriteNum::SPR_TROO,
        5,
        8,
        ActionF::Actor(a_facetarget),
        StateNum::S_TROO_ATK3,
        0,
        0,
    ), // S_TROO_ATK2
    State::new(
        SpriteNum::SPR_TROO,
        6,
        6,
        ActionF::Actor(a_troopattack),
        StateNum::S_TROO_RUN1,
        0,
        0,
    ), // S_TROO_ATK3
    State::new(
        SpriteNum::SPR_TROO,
        7,
        2,
        ActionF::None,
        StateNum::S_TROO_PAIN2,
        0,
        0,
    ), // S_TROO_PAIN
    State::new(
        SpriteNum::SPR_TROO,
        7,
        2,
        ActionF::Actor(a_pain),
        StateNum::S_TROO_RUN1,
        0,
        0,
    ), // S_TROO_PAIN2
    State::new(
        SpriteNum::SPR_TROO,
        8,
        8,
        ActionF::None,
        StateNum::S_TROO_DIE2,
        0,
        0,
    ), // S_TROO_DIE1
    State::new(
        SpriteNum::SPR_TROO,
        9,
        8,
        ActionF::Actor(a_scream),
        StateNum::S_TROO_DIE3,
        0,
        0,
    ), // S_TROO_DIE2
    State::new(
        SpriteNum::SPR_TROO,
        10,
        6,
        ActionF::None,
        StateNum::S_TROO_DIE4,
        0,
        0,
    ), // S_TROO_DIE3
    State::new(
        SpriteNum::SPR_TROO,
        11,
        6,
        ActionF::Actor(a_fall),
        StateNum::S_TROO_DIE5,
        0,
        0,
    ), // S_TROO_DIE4
    State::new(
        SpriteNum::SPR_TROO,
        12,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_TROO_DIE5
    State::new(
        SpriteNum::SPR_TROO,
        13,
        5,
        ActionF::None,
        StateNum::S_TROO_XDIE2,
        0,
        0,
    ), // S_TROO_XDIE1
    State::new(
        SpriteNum::SPR_TROO,
        14,
        5,
        ActionF::Actor(a_xscream),
        StateNum::S_TROO_XDIE3,
        0,
        0,
    ), // S_TROO_XDIE2
    State::new(
        SpriteNum::SPR_TROO,
        15,
        5,
        ActionF::None,
        StateNum::S_TROO_XDIE4,
        0,
        0,
    ), // S_TROO_XDIE3
    State::new(
        SpriteNum::SPR_TROO,
        16,
        5,
        ActionF::Actor(a_fall),
        StateNum::S_TROO_XDIE5,
        0,
        0,
    ), // S_TROO_XDIE4
    State::new(
        SpriteNum::SPR_TROO,
        17,
        5,
        ActionF::None,
        StateNum::S_TROO_XDIE6,
        0,
        0,
    ), // S_TROO_XDIE5
    State::new(
        SpriteNum::SPR_TROO,
        18,
        5,
        ActionF::None,
        StateNum::S_TROO_XDIE7,
        0,
        0,
    ), // S_TROO_XDIE6
    State::new(
        SpriteNum::SPR_TROO,
        19,
        5,
        ActionF::None,
        StateNum::S_TROO_XDIE8,
        0,
        0,
    ), // S_TROO_XDIE7
    State::new(
        SpriteNum::SPR_TROO,
        20,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_TROO_XDIE8
    State::new(
        SpriteNum::SPR_TROO,
        12,
        8,
        ActionF::None,
        StateNum::S_TROO_RAISE2,
        0,
        0,
    ), // S_TROO_RAISE1
    State::new(
        SpriteNum::SPR_TROO,
        11,
        8,
        ActionF::None,
        StateNum::S_TROO_RAISE3,
        0,
        0,
    ), // S_TROO_RAISE2
    State::new(
        SpriteNum::SPR_TROO,
        10,
        6,
        ActionF::None,
        StateNum::S_TROO_RAISE4,
        0,
        0,
    ), // S_TROO_RAISE3
    State::new(
        SpriteNum::SPR_TROO,
        9,
        6,
        ActionF::None,
        StateNum::S_TROO_RAISE5,
        0,
        0,
    ), // S_TROO_RAISE4
    State::new(
        SpriteNum::SPR_TROO,
        8,
        6,
        ActionF::None,
        StateNum::S_TROO_RUN1,
        0,
        0,
    ), // S_TROO_RAISE5
    State::new(
        SpriteNum::SPR_SARG,
        0,
        10,
        ActionF::Actor(a_look),
        StateNum::S_SARG_STND2,
        0,
        0,
    ), // S_SARG_STND
    State::new(
        SpriteNum::SPR_SARG,
        1,
        10,
        ActionF::Actor(a_look),
        StateNum::S_SARG_STND,
        0,
        0,
    ), // S_SARG_STND2
    State::new(
        SpriteNum::SPR_SARG,
        0,
        2,
        ActionF::Actor(a_chase),
        StateNum::S_SARG_RUN2,
        0,
        0,
    ), // S_SARG_RUN1
    State::new(
        SpriteNum::SPR_SARG,
        0,
        2,
        ActionF::Actor(a_chase),
        StateNum::S_SARG_RUN3,
        0,
        0,
    ), // S_SARG_RUN2
    State::new(
        SpriteNum::SPR_SARG,
        1,
        2,
        ActionF::Actor(a_chase),
        StateNum::S_SARG_RUN4,
        0,
        0,
    ), // S_SARG_RUN3
    State::new(
        SpriteNum::SPR_SARG,
        1,
        2,
        ActionF::Actor(a_chase),
        StateNum::S_SARG_RUN5,
        0,
        0,
    ), // S_SARG_RUN4
    State::new(
        SpriteNum::SPR_SARG,
        2,
        2,
        ActionF::Actor(a_chase),
        StateNum::S_SARG_RUN6,
        0,
        0,
    ), // S_SARG_RUN5
    State::new(
        SpriteNum::SPR_SARG,
        2,
        2,
        ActionF::Actor(a_chase),
        StateNum::S_SARG_RUN7,
        0,
        0,
    ), // S_SARG_RUN6
    State::new(
        SpriteNum::SPR_SARG,
        3,
        2,
        ActionF::Actor(a_chase),
        StateNum::S_SARG_RUN8,
        0,
        0,
    ), // S_SARG_RUN7
    State::new(
        SpriteNum::SPR_SARG,
        3,
        2,
        ActionF::Actor(a_chase),
        StateNum::S_SARG_RUN1,
        0,
        0,
    ), // S_SARG_RUN8
    State::new(
        SpriteNum::SPR_SARG,
        4,
        8,
        ActionF::Actor(a_facetarget),
        StateNum::S_SARG_ATK2,
        0,
        0,
    ), // S_SARG_ATK1
    State::new(
        SpriteNum::SPR_SARG,
        5,
        8,
        ActionF::Actor(a_facetarget),
        StateNum::S_SARG_ATK3,
        0,
        0,
    ), // S_SARG_ATK2
    State::new(
        SpriteNum::SPR_SARG,
        6,
        8,
        ActionF::Actor(a_sargattack),
        StateNum::S_SARG_RUN1,
        0,
        0,
    ), // S_SARG_ATK3
    State::new(
        SpriteNum::SPR_SARG,
        7,
        2,
        ActionF::None,
        StateNum::S_SARG_PAIN2,
        0,
        0,
    ), // S_SARG_PAIN
    State::new(
        SpriteNum::SPR_SARG,
        7,
        2,
        ActionF::Actor(a_pain),
        StateNum::S_SARG_RUN1,
        0,
        0,
    ), // S_SARG_PAIN2
    State::new(
        SpriteNum::SPR_SARG,
        8,
        8,
        ActionF::None,
        StateNum::S_SARG_DIE2,
        0,
        0,
    ), // S_SARG_DIE1
    State::new(
        SpriteNum::SPR_SARG,
        9,
        8,
        ActionF::Actor(a_scream),
        StateNum::S_SARG_DIE3,
        0,
        0,
    ), // S_SARG_DIE2
    State::new(
        SpriteNum::SPR_SARG,
        10,
        4,
        ActionF::None,
        StateNum::S_SARG_DIE4,
        0,
        0,
    ), // S_SARG_DIE3
    State::new(
        SpriteNum::SPR_SARG,
        11,
        4,
        ActionF::Actor(a_fall),
        StateNum::S_SARG_DIE5,
        0,
        0,
    ), // S_SARG_DIE4
    State::new(
        SpriteNum::SPR_SARG,
        12,
        4,
        ActionF::None,
        StateNum::S_SARG_DIE6,
        0,
        0,
    ), // S_SARG_DIE5
    State::new(
        SpriteNum::SPR_SARG,
        13,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_SARG_DIE6
    State::new(
        SpriteNum::SPR_SARG,
        13,
        5,
        ActionF::None,
        StateNum::S_SARG_RAISE2,
        0,
        0,
    ), // S_SARG_RAISE1
    State::new(
        SpriteNum::SPR_SARG,
        12,
        5,
        ActionF::None,
        StateNum::S_SARG_RAISE3,
        0,
        0,
    ), // S_SARG_RAISE2
    State::new(
        SpriteNum::SPR_SARG,
        11,
        5,
        ActionF::None,
        StateNum::S_SARG_RAISE4,
        0,
        0,
    ), // S_SARG_RAISE3
    State::new(
        SpriteNum::SPR_SARG,
        10,
        5,
        ActionF::None,
        StateNum::S_SARG_RAISE5,
        0,
        0,
    ), // S_SARG_RAISE4
    State::new(
        SpriteNum::SPR_SARG,
        9,
        5,
        ActionF::None,
        StateNum::S_SARG_RAISE6,
        0,
        0,
    ), // S_SARG_RAISE5
    State::new(
        SpriteNum::SPR_SARG,
        8,
        5,
        ActionF::None,
        StateNum::S_SARG_RUN1,
        0,
        0,
    ), // S_SARG_RAISE6
    State::new(
        SpriteNum::SPR_HEAD,
        0,
        10,
        ActionF::Actor(a_look),
        StateNum::S_HEAD_STND,
        0,
        0,
    ), // S_HEAD_STND
    State::new(
        SpriteNum::SPR_HEAD,
        0,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_HEAD_RUN1,
        0,
        0,
    ), // S_HEAD_RUN1
    State::new(
        SpriteNum::SPR_HEAD,
        1,
        5,
        ActionF::Actor(a_facetarget),
        StateNum::S_HEAD_ATK2,
        0,
        0,
    ), // S_HEAD_ATK1
    State::new(
        SpriteNum::SPR_HEAD,
        2,
        5,
        ActionF::Actor(a_facetarget),
        StateNum::S_HEAD_ATK3,
        0,
        0,
    ), // S_HEAD_ATK2
    State::new(
        SpriteNum::SPR_HEAD,
        32771,
        5,
        ActionF::Actor(a_headattack),
        StateNum::S_HEAD_RUN1,
        0,
        0,
    ), // S_HEAD_ATK3
    State::new(
        SpriteNum::SPR_HEAD,
        4,
        3,
        ActionF::None,
        StateNum::S_HEAD_PAIN2,
        0,
        0,
    ), // S_HEAD_PAIN
    State::new(
        SpriteNum::SPR_HEAD,
        4,
        3,
        ActionF::Actor(a_pain),
        StateNum::S_HEAD_PAIN3,
        0,
        0,
    ), // S_HEAD_PAIN2
    State::new(
        SpriteNum::SPR_HEAD,
        5,
        6,
        ActionF::None,
        StateNum::S_HEAD_RUN1,
        0,
        0,
    ), // S_HEAD_PAIN3
    State::new(
        SpriteNum::SPR_HEAD,
        6,
        8,
        ActionF::None,
        StateNum::S_HEAD_DIE2,
        0,
        0,
    ), // S_HEAD_DIE1
    State::new(
        SpriteNum::SPR_HEAD,
        7,
        8,
        ActionF::Actor(a_scream),
        StateNum::S_HEAD_DIE3,
        0,
        0,
    ), // S_HEAD_DIE2
    State::new(
        SpriteNum::SPR_HEAD,
        8,
        8,
        ActionF::None,
        StateNum::S_HEAD_DIE4,
        0,
        0,
    ), // S_HEAD_DIE3
    State::new(
        SpriteNum::SPR_HEAD,
        9,
        8,
        ActionF::None,
        StateNum::S_HEAD_DIE5,
        0,
        0,
    ), // S_HEAD_DIE4
    State::new(
        SpriteNum::SPR_HEAD,
        10,
        8,
        ActionF::Actor(a_fall),
        StateNum::S_HEAD_DIE6,
        0,
        0,
    ), // S_HEAD_DIE5
    State::new(
        SpriteNum::SPR_HEAD,
        11,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_HEAD_DIE6
    State::new(
        SpriteNum::SPR_HEAD,
        11,
        8,
        ActionF::None,
        StateNum::S_HEAD_RAISE2,
        0,
        0,
    ), // S_HEAD_RAISE1
    State::new(
        SpriteNum::SPR_HEAD,
        10,
        8,
        ActionF::None,
        StateNum::S_HEAD_RAISE3,
        0,
        0,
    ), // S_HEAD_RAISE2
    State::new(
        SpriteNum::SPR_HEAD,
        9,
        8,
        ActionF::None,
        StateNum::S_HEAD_RAISE4,
        0,
        0,
    ), // S_HEAD_RAISE3
    State::new(
        SpriteNum::SPR_HEAD,
        8,
        8,
        ActionF::None,
        StateNum::S_HEAD_RAISE5,
        0,
        0,
    ), // S_HEAD_RAISE4
    State::new(
        SpriteNum::SPR_HEAD,
        7,
        8,
        ActionF::None,
        StateNum::S_HEAD_RAISE6,
        0,
        0,
    ), // S_HEAD_RAISE5
    State::new(
        SpriteNum::SPR_HEAD,
        6,
        8,
        ActionF::None,
        StateNum::S_HEAD_RUN1,
        0,
        0,
    ), // S_HEAD_RAISE6
    State::new(
        SpriteNum::SPR_BAL7,
        32768,
        4,
        ActionF::None,
        StateNum::S_BRBALL2,
        0,
        0,
    ), // S_BRBALL1
    State::new(
        SpriteNum::SPR_BAL7,
        32769,
        4,
        ActionF::None,
        StateNum::S_BRBALL1,
        0,
        0,
    ), // S_BRBALL2
    State::new(
        SpriteNum::SPR_BAL7,
        32770,
        6,
        ActionF::None,
        StateNum::S_BRBALLX2,
        0,
        0,
    ), // S_BRBALLX1
    State::new(
        SpriteNum::SPR_BAL7,
        32771,
        6,
        ActionF::None,
        StateNum::S_BRBALLX3,
        0,
        0,
    ), // S_BRBALLX2
    State::new(
        SpriteNum::SPR_BAL7,
        32772,
        6,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_BRBALLX3
    State::new(
        SpriteNum::SPR_BOSS,
        0,
        10,
        ActionF::Actor(a_look),
        StateNum::S_BOSS_STND2,
        0,
        0,
    ), // S_BOSS_STND
    State::new(
        SpriteNum::SPR_BOSS,
        1,
        10,
        ActionF::Actor(a_look),
        StateNum::S_BOSS_STND,
        0,
        0,
    ), // S_BOSS_STND2
    State::new(
        SpriteNum::SPR_BOSS,
        0,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_BOSS_RUN2,
        0,
        0,
    ), // S_BOSS_RUN1
    State::new(
        SpriteNum::SPR_BOSS,
        0,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_BOSS_RUN3,
        0,
        0,
    ), // S_BOSS_RUN2
    State::new(
        SpriteNum::SPR_BOSS,
        1,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_BOSS_RUN4,
        0,
        0,
    ), // S_BOSS_RUN3
    State::new(
        SpriteNum::SPR_BOSS,
        1,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_BOSS_RUN5,
        0,
        0,
    ), // S_BOSS_RUN4
    State::new(
        SpriteNum::SPR_BOSS,
        2,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_BOSS_RUN6,
        0,
        0,
    ), // S_BOSS_RUN5
    State::new(
        SpriteNum::SPR_BOSS,
        2,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_BOSS_RUN7,
        0,
        0,
    ), // S_BOSS_RUN6
    State::new(
        SpriteNum::SPR_BOSS,
        3,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_BOSS_RUN8,
        0,
        0,
    ), // S_BOSS_RUN7
    State::new(
        SpriteNum::SPR_BOSS,
        3,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_BOSS_RUN1,
        0,
        0,
    ), // S_BOSS_RUN8
    State::new(
        SpriteNum::SPR_BOSS,
        4,
        8,
        ActionF::Actor(a_facetarget),
        StateNum::S_BOSS_ATK2,
        0,
        0,
    ), // S_BOSS_ATK1
    State::new(
        SpriteNum::SPR_BOSS,
        5,
        8,
        ActionF::Actor(a_facetarget),
        StateNum::S_BOSS_ATK3,
        0,
        0,
    ), // S_BOSS_ATK2
    State::new(
        SpriteNum::SPR_BOSS,
        6,
        8,
        ActionF::Actor(a_bruisattack),
        StateNum::S_BOSS_RUN1,
        0,
        0,
    ), // S_BOSS_ATK3
    State::new(
        SpriteNum::SPR_BOSS,
        7,
        2,
        ActionF::None,
        StateNum::S_BOSS_PAIN2,
        0,
        0,
    ), // S_BOSS_PAIN
    State::new(
        SpriteNum::SPR_BOSS,
        7,
        2,
        ActionF::Actor(a_pain),
        StateNum::S_BOSS_RUN1,
        0,
        0,
    ), // S_BOSS_PAIN2
    State::new(
        SpriteNum::SPR_BOSS,
        8,
        8,
        ActionF::None,
        StateNum::S_BOSS_DIE2,
        0,
        0,
    ), // S_BOSS_DIE1
    State::new(
        SpriteNum::SPR_BOSS,
        9,
        8,
        ActionF::Actor(a_scream),
        StateNum::S_BOSS_DIE3,
        0,
        0,
    ), // S_BOSS_DIE2
    State::new(
        SpriteNum::SPR_BOSS,
        10,
        8,
        ActionF::None,
        StateNum::S_BOSS_DIE4,
        0,
        0,
    ), // S_BOSS_DIE3
    State::new(
        SpriteNum::SPR_BOSS,
        11,
        8,
        ActionF::Actor(a_fall),
        StateNum::S_BOSS_DIE5,
        0,
        0,
    ), // S_BOSS_DIE4
    State::new(
        SpriteNum::SPR_BOSS,
        12,
        8,
        ActionF::None,
        StateNum::S_BOSS_DIE6,
        0,
        0,
    ), // S_BOSS_DIE5
    State::new(
        SpriteNum::SPR_BOSS,
        13,
        8,
        ActionF::None,
        StateNum::S_BOSS_DIE7,
        0,
        0,
    ), // S_BOSS_DIE6
    State::new(
        SpriteNum::SPR_BOSS,
        14,
        -1,
        ActionF::Actor(a_bossdeath),
        StateNum::S_NULL,
        0,
        0,
    ), // S_BOSS_DIE7
    State::new(
        SpriteNum::SPR_BOSS,
        14,
        8,
        ActionF::None,
        StateNum::S_BOSS_RAISE2,
        0,
        0,
    ), // S_BOSS_RAISE1
    State::new(
        SpriteNum::SPR_BOSS,
        13,
        8,
        ActionF::None,
        StateNum::S_BOSS_RAISE3,
        0,
        0,
    ), // S_BOSS_RAISE2
    State::new(
        SpriteNum::SPR_BOSS,
        12,
        8,
        ActionF::None,
        StateNum::S_BOSS_RAISE4,
        0,
        0,
    ), // S_BOSS_RAISE3
    State::new(
        SpriteNum::SPR_BOSS,
        11,
        8,
        ActionF::None,
        StateNum::S_BOSS_RAISE5,
        0,
        0,
    ), // S_BOSS_RAISE4
    State::new(
        SpriteNum::SPR_BOSS,
        10,
        8,
        ActionF::None,
        StateNum::S_BOSS_RAISE6,
        0,
        0,
    ), // S_BOSS_RAISE5
    State::new(
        SpriteNum::SPR_BOSS,
        9,
        8,
        ActionF::None,
        StateNum::S_BOSS_RAISE7,
        0,
        0,
    ), // S_BOSS_RAISE6
    State::new(
        SpriteNum::SPR_BOSS,
        8,
        8,
        ActionF::None,
        StateNum::S_BOSS_RUN1,
        0,
        0,
    ), // S_BOSS_RAISE7
    State::new(
        SpriteNum::SPR_BOS2,
        0,
        10,
        ActionF::Actor(a_look),
        StateNum::S_BOS2_STND2,
        0,
        0,
    ), // S_BOS2_STND
    State::new(
        SpriteNum::SPR_BOS2,
        1,
        10,
        ActionF::Actor(a_look),
        StateNum::S_BOS2_STND,
        0,
        0,
    ), // S_BOS2_STND2
    State::new(
        SpriteNum::SPR_BOS2,
        0,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_BOS2_RUN2,
        0,
        0,
    ), // S_BOS2_RUN1
    State::new(
        SpriteNum::SPR_BOS2,
        0,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_BOS2_RUN3,
        0,
        0,
    ), // S_BOS2_RUN2
    State::new(
        SpriteNum::SPR_BOS2,
        1,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_BOS2_RUN4,
        0,
        0,
    ), // S_BOS2_RUN3
    State::new(
        SpriteNum::SPR_BOS2,
        1,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_BOS2_RUN5,
        0,
        0,
    ), // S_BOS2_RUN4
    State::new(
        SpriteNum::SPR_BOS2,
        2,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_BOS2_RUN6,
        0,
        0,
    ), // S_BOS2_RUN5
    State::new(
        SpriteNum::SPR_BOS2,
        2,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_BOS2_RUN7,
        0,
        0,
    ), // S_BOS2_RUN6
    State::new(
        SpriteNum::SPR_BOS2,
        3,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_BOS2_RUN8,
        0,
        0,
    ), // S_BOS2_RUN7
    State::new(
        SpriteNum::SPR_BOS2,
        3,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_BOS2_RUN1,
        0,
        0,
    ), // S_BOS2_RUN8
    State::new(
        SpriteNum::SPR_BOS2,
        4,
        8,
        ActionF::Actor(a_facetarget),
        StateNum::S_BOS2_ATK2,
        0,
        0,
    ), // S_BOS2_ATK1
    State::new(
        SpriteNum::SPR_BOS2,
        5,
        8,
        ActionF::Actor(a_facetarget),
        StateNum::S_BOS2_ATK3,
        0,
        0,
    ), // S_BOS2_ATK2
    State::new(
        SpriteNum::SPR_BOS2,
        6,
        8,
        ActionF::Actor(a_bruisattack),
        StateNum::S_BOS2_RUN1,
        0,
        0,
    ), // S_BOS2_ATK3
    State::new(
        SpriteNum::SPR_BOS2,
        7,
        2,
        ActionF::None,
        StateNum::S_BOS2_PAIN2,
        0,
        0,
    ), // S_BOS2_PAIN
    State::new(
        SpriteNum::SPR_BOS2,
        7,
        2,
        ActionF::Actor(a_pain),
        StateNum::S_BOS2_RUN1,
        0,
        0,
    ), // S_BOS2_PAIN2
    State::new(
        SpriteNum::SPR_BOS2,
        8,
        8,
        ActionF::None,
        StateNum::S_BOS2_DIE2,
        0,
        0,
    ), // S_BOS2_DIE1
    State::new(
        SpriteNum::SPR_BOS2,
        9,
        8,
        ActionF::Actor(a_scream),
        StateNum::S_BOS2_DIE3,
        0,
        0,
    ), // S_BOS2_DIE2
    State::new(
        SpriteNum::SPR_BOS2,
        10,
        8,
        ActionF::None,
        StateNum::S_BOS2_DIE4,
        0,
        0,
    ), // S_BOS2_DIE3
    State::new(
        SpriteNum::SPR_BOS2,
        11,
        8,
        ActionF::Actor(a_fall),
        StateNum::S_BOS2_DIE5,
        0,
        0,
    ), // S_BOS2_DIE4
    State::new(
        SpriteNum::SPR_BOS2,
        12,
        8,
        ActionF::None,
        StateNum::S_BOS2_DIE6,
        0,
        0,
    ), // S_BOS2_DIE5
    State::new(
        SpriteNum::SPR_BOS2,
        13,
        8,
        ActionF::None,
        StateNum::S_BOS2_DIE7,
        0,
        0,
    ), // S_BOS2_DIE6
    State::new(
        SpriteNum::SPR_BOS2,
        14,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_BOS2_DIE7
    State::new(
        SpriteNum::SPR_BOS2,
        14,
        8,
        ActionF::None,
        StateNum::S_BOS2_RAISE2,
        0,
        0,
    ), // S_BOS2_RAISE1
    State::new(
        SpriteNum::SPR_BOS2,
        13,
        8,
        ActionF::None,
        StateNum::S_BOS2_RAISE3,
        0,
        0,
    ), // S_BOS2_RAISE2
    State::new(
        SpriteNum::SPR_BOS2,
        12,
        8,
        ActionF::None,
        StateNum::S_BOS2_RAISE4,
        0,
        0,
    ), // S_BOS2_RAISE3
    State::new(
        SpriteNum::SPR_BOS2,
        11,
        8,
        ActionF::None,
        StateNum::S_BOS2_RAISE5,
        0,
        0,
    ), // S_BOS2_RAISE4
    State::new(
        SpriteNum::SPR_BOS2,
        10,
        8,
        ActionF::None,
        StateNum::S_BOS2_RAISE6,
        0,
        0,
    ), // S_BOS2_RAISE5
    State::new(
        SpriteNum::SPR_BOS2,
        9,
        8,
        ActionF::None,
        StateNum::S_BOS2_RAISE7,
        0,
        0,
    ), // S_BOS2_RAISE6
    State::new(
        SpriteNum::SPR_BOS2,
        8,
        8,
        ActionF::None,
        StateNum::S_BOS2_RUN1,
        0,
        0,
    ), // S_BOS2_RAISE7
    State::new(
        SpriteNum::SPR_SKUL,
        32768,
        10,
        ActionF::Actor(a_look),
        StateNum::S_SKULL_STND2,
        0,
        0,
    ), // S_SKULL_STND
    State::new(
        SpriteNum::SPR_SKUL,
        32769,
        10,
        ActionF::Actor(a_look),
        StateNum::S_SKULL_STND,
        0,
        0,
    ), // S_SKULL_STND2
    State::new(
        SpriteNum::SPR_SKUL,
        32768,
        6,
        ActionF::Actor(a_chase),
        StateNum::S_SKULL_RUN2,
        0,
        0,
    ), // S_SKULL_RUN1
    State::new(
        SpriteNum::SPR_SKUL,
        32769,
        6,
        ActionF::Actor(a_chase),
        StateNum::S_SKULL_RUN1,
        0,
        0,
    ), // S_SKULL_RUN2
    State::new(
        SpriteNum::SPR_SKUL,
        32770,
        10,
        ActionF::Actor(a_facetarget),
        StateNum::S_SKULL_ATK2,
        0,
        0,
    ), // S_SKULL_ATK1
    State::new(
        SpriteNum::SPR_SKUL,
        32771,
        4,
        ActionF::Actor(a_skullattack),
        StateNum::S_SKULL_ATK3,
        0,
        0,
    ), // S_SKULL_ATK2
    State::new(
        SpriteNum::SPR_SKUL,
        32770,
        4,
        ActionF::None,
        StateNum::S_SKULL_ATK4,
        0,
        0,
    ), // S_SKULL_ATK3
    State::new(
        SpriteNum::SPR_SKUL,
        32771,
        4,
        ActionF::None,
        StateNum::S_SKULL_ATK3,
        0,
        0,
    ), // S_SKULL_ATK4
    State::new(
        SpriteNum::SPR_SKUL,
        32772,
        3,
        ActionF::None,
        StateNum::S_SKULL_PAIN2,
        0,
        0,
    ), // S_SKULL_PAIN
    State::new(
        SpriteNum::SPR_SKUL,
        32772,
        3,
        ActionF::Actor(a_pain),
        StateNum::S_SKULL_RUN1,
        0,
        0,
    ), // S_SKULL_PAIN2
    State::new(
        SpriteNum::SPR_SKUL,
        32773,
        6,
        ActionF::None,
        StateNum::S_SKULL_DIE2,
        0,
        0,
    ), // S_SKULL_DIE1
    State::new(
        SpriteNum::SPR_SKUL,
        32774,
        6,
        ActionF::Actor(a_scream),
        StateNum::S_SKULL_DIE3,
        0,
        0,
    ), // S_SKULL_DIE2
    State::new(
        SpriteNum::SPR_SKUL,
        32775,
        6,
        ActionF::None,
        StateNum::S_SKULL_DIE4,
        0,
        0,
    ), // S_SKULL_DIE3
    State::new(
        SpriteNum::SPR_SKUL,
        32776,
        6,
        ActionF::Actor(a_fall),
        StateNum::S_SKULL_DIE5,
        0,
        0,
    ), // S_SKULL_DIE4
    State::new(
        SpriteNum::SPR_SKUL,
        9,
        6,
        ActionF::None,
        StateNum::S_SKULL_DIE6,
        0,
        0,
    ), // S_SKULL_DIE5
    State::new(
        SpriteNum::SPR_SKUL,
        10,
        6,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_SKULL_DIE6
    State::new(
        SpriteNum::SPR_SPID,
        0,
        10,
        ActionF::Actor(a_look),
        StateNum::S_SPID_STND2,
        0,
        0,
    ), // S_SPID_STND
    State::new(
        SpriteNum::SPR_SPID,
        1,
        10,
        ActionF::Actor(a_look),
        StateNum::S_SPID_STND,
        0,
        0,
    ), // S_SPID_STND2
    State::new(
        SpriteNum::SPR_SPID,
        0,
        3,
        ActionF::Actor(a_metal),
        StateNum::S_SPID_RUN2,
        0,
        0,
    ), // S_SPID_RUN1
    State::new(
        SpriteNum::SPR_SPID,
        0,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_SPID_RUN3,
        0,
        0,
    ), // S_SPID_RUN2
    State::new(
        SpriteNum::SPR_SPID,
        1,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_SPID_RUN4,
        0,
        0,
    ), // S_SPID_RUN3
    State::new(
        SpriteNum::SPR_SPID,
        1,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_SPID_RUN5,
        0,
        0,
    ), // S_SPID_RUN4
    State::new(
        SpriteNum::SPR_SPID,
        2,
        3,
        ActionF::Actor(a_metal),
        StateNum::S_SPID_RUN6,
        0,
        0,
    ), // S_SPID_RUN5
    State::new(
        SpriteNum::SPR_SPID,
        2,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_SPID_RUN7,
        0,
        0,
    ), // S_SPID_RUN6
    State::new(
        SpriteNum::SPR_SPID,
        3,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_SPID_RUN8,
        0,
        0,
    ), // S_SPID_RUN7
    State::new(
        SpriteNum::SPR_SPID,
        3,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_SPID_RUN9,
        0,
        0,
    ), // S_SPID_RUN8
    State::new(
        SpriteNum::SPR_SPID,
        4,
        3,
        ActionF::Actor(a_metal),
        StateNum::S_SPID_RUN10,
        0,
        0,
    ), // S_SPID_RUN9
    State::new(
        SpriteNum::SPR_SPID,
        4,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_SPID_RUN11,
        0,
        0,
    ), // S_SPID_RUN10
    State::new(
        SpriteNum::SPR_SPID,
        5,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_SPID_RUN12,
        0,
        0,
    ), // S_SPID_RUN11
    State::new(
        SpriteNum::SPR_SPID,
        5,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_SPID_RUN1,
        0,
        0,
    ), // S_SPID_RUN12
    State::new(
        SpriteNum::SPR_SPID,
        32768,
        20,
        ActionF::Actor(a_facetarget),
        StateNum::S_SPID_ATK2,
        0,
        0,
    ), // S_SPID_ATK1
    State::new(
        SpriteNum::SPR_SPID,
        32774,
        4,
        ActionF::Actor(a_sposattack),
        StateNum::S_SPID_ATK3,
        0,
        0,
    ), // S_SPID_ATK2
    State::new(
        SpriteNum::SPR_SPID,
        32775,
        4,
        ActionF::Actor(a_sposattack),
        StateNum::S_SPID_ATK4,
        0,
        0,
    ), // S_SPID_ATK3
    State::new(
        SpriteNum::SPR_SPID,
        32775,
        1,
        ActionF::Actor(a_spidrefire),
        StateNum::S_SPID_ATK2,
        0,
        0,
    ), // S_SPID_ATK4
    State::new(
        SpriteNum::SPR_SPID,
        8,
        3,
        ActionF::None,
        StateNum::S_SPID_PAIN2,
        0,
        0,
    ), // S_SPID_PAIN
    State::new(
        SpriteNum::SPR_SPID,
        8,
        3,
        ActionF::Actor(a_pain),
        StateNum::S_SPID_RUN1,
        0,
        0,
    ), // S_SPID_PAIN2
    State::new(
        SpriteNum::SPR_SPID,
        9,
        20,
        ActionF::Actor(a_scream),
        StateNum::S_SPID_DIE2,
        0,
        0,
    ), // S_SPID_DIE1
    State::new(
        SpriteNum::SPR_SPID,
        10,
        10,
        ActionF::Actor(a_fall),
        StateNum::S_SPID_DIE3,
        0,
        0,
    ), // S_SPID_DIE2
    State::new(
        SpriteNum::SPR_SPID,
        11,
        10,
        ActionF::None,
        StateNum::S_SPID_DIE4,
        0,
        0,
    ), // S_SPID_DIE3
    State::new(
        SpriteNum::SPR_SPID,
        12,
        10,
        ActionF::None,
        StateNum::S_SPID_DIE5,
        0,
        0,
    ), // S_SPID_DIE4
    State::new(
        SpriteNum::SPR_SPID,
        13,
        10,
        ActionF::None,
        StateNum::S_SPID_DIE6,
        0,
        0,
    ), // S_SPID_DIE5
    State::new(
        SpriteNum::SPR_SPID,
        14,
        10,
        ActionF::None,
        StateNum::S_SPID_DIE7,
        0,
        0,
    ), // S_SPID_DIE6
    State::new(
        SpriteNum::SPR_SPID,
        15,
        10,
        ActionF::None,
        StateNum::S_SPID_DIE8,
        0,
        0,
    ), // S_SPID_DIE7
    State::new(
        SpriteNum::SPR_SPID,
        16,
        10,
        ActionF::None,
        StateNum::S_SPID_DIE9,
        0,
        0,
    ), // S_SPID_DIE8
    State::new(
        SpriteNum::SPR_SPID,
        17,
        10,
        ActionF::None,
        StateNum::S_SPID_DIE10,
        0,
        0,
    ), // S_SPID_DIE9
    State::new(
        SpriteNum::SPR_SPID,
        18,
        30,
        ActionF::None,
        StateNum::S_SPID_DIE11,
        0,
        0,
    ), // S_SPID_DIE10
    State::new(
        SpriteNum::SPR_SPID,
        18,
        -1,
        ActionF::Actor(a_bossdeath),
        StateNum::S_NULL,
        0,
        0,
    ), // S_SPID_DIE11
    State::new(
        SpriteNum::SPR_BSPI,
        0,
        10,
        ActionF::Actor(a_look),
        StateNum::S_BSPI_STND2,
        0,
        0,
    ), // S_BSPI_STND
    State::new(
        SpriteNum::SPR_BSPI,
        1,
        10,
        ActionF::Actor(a_look),
        StateNum::S_BSPI_STND,
        0,
        0,
    ), // S_BSPI_STND2
    State::new(
        SpriteNum::SPR_BSPI,
        0,
        20,
        ActionF::None,
        StateNum::S_BSPI_RUN1,
        0,
        0,
    ), // S_BSPI_SIGHT
    State::new(
        SpriteNum::SPR_BSPI,
        0,
        3,
        ActionF::Actor(a_babymetal),
        StateNum::S_BSPI_RUN2,
        0,
        0,
    ), // S_BSPI_RUN1
    State::new(
        SpriteNum::SPR_BSPI,
        0,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_BSPI_RUN3,
        0,
        0,
    ), // S_BSPI_RUN2
    State::new(
        SpriteNum::SPR_BSPI,
        1,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_BSPI_RUN4,
        0,
        0,
    ), // S_BSPI_RUN3
    State::new(
        SpriteNum::SPR_BSPI,
        1,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_BSPI_RUN5,
        0,
        0,
    ), // S_BSPI_RUN4
    State::new(
        SpriteNum::SPR_BSPI,
        2,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_BSPI_RUN6,
        0,
        0,
    ), // S_BSPI_RUN5
    State::new(
        SpriteNum::SPR_BSPI,
        2,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_BSPI_RUN7,
        0,
        0,
    ), // S_BSPI_RUN6
    State::new(
        SpriteNum::SPR_BSPI,
        3,
        3,
        ActionF::Actor(a_babymetal),
        StateNum::S_BSPI_RUN8,
        0,
        0,
    ), // S_BSPI_RUN7
    State::new(
        SpriteNum::SPR_BSPI,
        3,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_BSPI_RUN9,
        0,
        0,
    ), // S_BSPI_RUN8
    State::new(
        SpriteNum::SPR_BSPI,
        4,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_BSPI_RUN10,
        0,
        0,
    ), // S_BSPI_RUN9
    State::new(
        SpriteNum::SPR_BSPI,
        4,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_BSPI_RUN11,
        0,
        0,
    ), // S_BSPI_RUN10
    State::new(
        SpriteNum::SPR_BSPI,
        5,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_BSPI_RUN12,
        0,
        0,
    ), // S_BSPI_RUN11
    State::new(
        SpriteNum::SPR_BSPI,
        5,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_BSPI_RUN1,
        0,
        0,
    ), // S_BSPI_RUN12
    State::new(
        SpriteNum::SPR_BSPI,
        32768,
        20,
        ActionF::Actor(a_facetarget),
        StateNum::S_BSPI_ATK2,
        0,
        0,
    ), // S_BSPI_ATK1
    State::new(
        SpriteNum::SPR_BSPI,
        32774,
        4,
        ActionF::Actor(a_bspiattack),
        StateNum::S_BSPI_ATK3,
        0,
        0,
    ), // S_BSPI_ATK2
    State::new(
        SpriteNum::SPR_BSPI,
        32775,
        4,
        ActionF::None,
        StateNum::S_BSPI_ATK4,
        0,
        0,
    ), // S_BSPI_ATK3
    State::new(
        SpriteNum::SPR_BSPI,
        32775,
        1,
        ActionF::Actor(a_spidrefire),
        StateNum::S_BSPI_ATK2,
        0,
        0,
    ), // S_BSPI_ATK4
    State::new(
        SpriteNum::SPR_BSPI,
        8,
        3,
        ActionF::None,
        StateNum::S_BSPI_PAIN2,
        0,
        0,
    ), // S_BSPI_PAIN
    State::new(
        SpriteNum::SPR_BSPI,
        8,
        3,
        ActionF::Actor(a_pain),
        StateNum::S_BSPI_RUN1,
        0,
        0,
    ), // S_BSPI_PAIN2
    State::new(
        SpriteNum::SPR_BSPI,
        9,
        20,
        ActionF::Actor(a_scream),
        StateNum::S_BSPI_DIE2,
        0,
        0,
    ), // S_BSPI_DIE1
    State::new(
        SpriteNum::SPR_BSPI,
        10,
        7,
        ActionF::Actor(a_fall),
        StateNum::S_BSPI_DIE3,
        0,
        0,
    ), // S_BSPI_DIE2
    State::new(
        SpriteNum::SPR_BSPI,
        11,
        7,
        ActionF::None,
        StateNum::S_BSPI_DIE4,
        0,
        0,
    ), // S_BSPI_DIE3
    State::new(
        SpriteNum::SPR_BSPI,
        12,
        7,
        ActionF::None,
        StateNum::S_BSPI_DIE5,
        0,
        0,
    ), // S_BSPI_DIE4
    State::new(
        SpriteNum::SPR_BSPI,
        13,
        7,
        ActionF::None,
        StateNum::S_BSPI_DIE6,
        0,
        0,
    ), // S_BSPI_DIE5
    State::new(
        SpriteNum::SPR_BSPI,
        14,
        7,
        ActionF::None,
        StateNum::S_BSPI_DIE7,
        0,
        0,
    ), // S_BSPI_DIE6
    State::new(
        SpriteNum::SPR_BSPI,
        15,
        -1,
        ActionF::Actor(a_bossdeath),
        StateNum::S_NULL,
        0,
        0,
    ), // S_BSPI_DIE7
    State::new(
        SpriteNum::SPR_BSPI,
        15,
        5,
        ActionF::None,
        StateNum::S_BSPI_RAISE2,
        0,
        0,
    ), // S_BSPI_RAISE1
    State::new(
        SpriteNum::SPR_BSPI,
        14,
        5,
        ActionF::None,
        StateNum::S_BSPI_RAISE3,
        0,
        0,
    ), // S_BSPI_RAISE2
    State::new(
        SpriteNum::SPR_BSPI,
        13,
        5,
        ActionF::None,
        StateNum::S_BSPI_RAISE4,
        0,
        0,
    ), // S_BSPI_RAISE3
    State::new(
        SpriteNum::SPR_BSPI,
        12,
        5,
        ActionF::None,
        StateNum::S_BSPI_RAISE5,
        0,
        0,
    ), // S_BSPI_RAISE4
    State::new(
        SpriteNum::SPR_BSPI,
        11,
        5,
        ActionF::None,
        StateNum::S_BSPI_RAISE6,
        0,
        0,
    ), // S_BSPI_RAISE5
    State::new(
        SpriteNum::SPR_BSPI,
        10,
        5,
        ActionF::None,
        StateNum::S_BSPI_RAISE7,
        0,
        0,
    ), // S_BSPI_RAISE6
    State::new(
        SpriteNum::SPR_BSPI,
        9,
        5,
        ActionF::None,
        StateNum::S_BSPI_RUN1,
        0,
        0,
    ), // S_BSPI_RAISE7
    State::new(
        SpriteNum::SPR_APLS,
        32768,
        5,
        ActionF::None,
        StateNum::S_ARACH_PLAZ2,
        0,
        0,
    ), // S_ARACH_PLAZ
    State::new(
        SpriteNum::SPR_APLS,
        32769,
        5,
        ActionF::None,
        StateNum::S_ARACH_PLAZ,
        0,
        0,
    ), // S_ARACH_PLAZ2
    State::new(
        SpriteNum::SPR_APBX,
        32768,
        5,
        ActionF::None,
        StateNum::S_ARACH_PLEX2,
        0,
        0,
    ), // S_ARACH_PLEX
    State::new(
        SpriteNum::SPR_APBX,
        32769,
        5,
        ActionF::None,
        StateNum::S_ARACH_PLEX3,
        0,
        0,
    ), // S_ARACH_PLEX2
    State::new(
        SpriteNum::SPR_APBX,
        32770,
        5,
        ActionF::None,
        StateNum::S_ARACH_PLEX4,
        0,
        0,
    ), // S_ARACH_PLEX3
    State::new(
        SpriteNum::SPR_APBX,
        32771,
        5,
        ActionF::None,
        StateNum::S_ARACH_PLEX5,
        0,
        0,
    ), // S_ARACH_PLEX4
    State::new(
        SpriteNum::SPR_APBX,
        32772,
        5,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_ARACH_PLEX5
    State::new(
        SpriteNum::SPR_CYBR,
        0,
        10,
        ActionF::Actor(a_look),
        StateNum::S_CYBER_STND2,
        0,
        0,
    ), // S_CYBER_STND
    State::new(
        SpriteNum::SPR_CYBR,
        1,
        10,
        ActionF::Actor(a_look),
        StateNum::S_CYBER_STND,
        0,
        0,
    ), // S_CYBER_STND2
    State::new(
        SpriteNum::SPR_CYBR,
        0,
        3,
        ActionF::Actor(a_hoof),
        StateNum::S_CYBER_RUN2,
        0,
        0,
    ), // S_CYBER_RUN1
    State::new(
        SpriteNum::SPR_CYBR,
        0,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_CYBER_RUN3,
        0,
        0,
    ), // S_CYBER_RUN2
    State::new(
        SpriteNum::SPR_CYBR,
        1,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_CYBER_RUN4,
        0,
        0,
    ), // S_CYBER_RUN3
    State::new(
        SpriteNum::SPR_CYBR,
        1,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_CYBER_RUN5,
        0,
        0,
    ), // S_CYBER_RUN4
    State::new(
        SpriteNum::SPR_CYBR,
        2,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_CYBER_RUN6,
        0,
        0,
    ), // S_CYBER_RUN5
    State::new(
        SpriteNum::SPR_CYBR,
        2,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_CYBER_RUN7,
        0,
        0,
    ), // S_CYBER_RUN6
    State::new(
        SpriteNum::SPR_CYBR,
        3,
        3,
        ActionF::Actor(a_metal),
        StateNum::S_CYBER_RUN8,
        0,
        0,
    ), // S_CYBER_RUN7
    State::new(
        SpriteNum::SPR_CYBR,
        3,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_CYBER_RUN1,
        0,
        0,
    ), // S_CYBER_RUN8
    State::new(
        SpriteNum::SPR_CYBR,
        4,
        6,
        ActionF::Actor(a_facetarget),
        StateNum::S_CYBER_ATK2,
        0,
        0,
    ), // S_CYBER_ATK1
    State::new(
        SpriteNum::SPR_CYBR,
        5,
        12,
        ActionF::Actor(a_cyberattack),
        StateNum::S_CYBER_ATK3,
        0,
        0,
    ), // S_CYBER_ATK2
    State::new(
        SpriteNum::SPR_CYBR,
        4,
        12,
        ActionF::Actor(a_facetarget),
        StateNum::S_CYBER_ATK4,
        0,
        0,
    ), // S_CYBER_ATK3
    State::new(
        SpriteNum::SPR_CYBR,
        5,
        12,
        ActionF::Actor(a_cyberattack),
        StateNum::S_CYBER_ATK5,
        0,
        0,
    ), // S_CYBER_ATK4
    State::new(
        SpriteNum::SPR_CYBR,
        4,
        12,
        ActionF::Actor(a_facetarget),
        StateNum::S_CYBER_ATK6,
        0,
        0,
    ), // S_CYBER_ATK5
    State::new(
        SpriteNum::SPR_CYBR,
        5,
        12,
        ActionF::Actor(a_cyberattack),
        StateNum::S_CYBER_RUN1,
        0,
        0,
    ), // S_CYBER_ATK6
    State::new(
        SpriteNum::SPR_CYBR,
        6,
        10,
        ActionF::Actor(a_pain),
        StateNum::S_CYBER_RUN1,
        0,
        0,
    ), // S_CYBER_PAIN
    State::new(
        SpriteNum::SPR_CYBR,
        7,
        10,
        ActionF::None,
        StateNum::S_CYBER_DIE2,
        0,
        0,
    ), // S_CYBER_DIE1
    State::new(
        SpriteNum::SPR_CYBR,
        8,
        10,
        ActionF::Actor(a_scream),
        StateNum::S_CYBER_DIE3,
        0,
        0,
    ), // S_CYBER_DIE2
    State::new(
        SpriteNum::SPR_CYBR,
        9,
        10,
        ActionF::None,
        StateNum::S_CYBER_DIE4,
        0,
        0,
    ), // S_CYBER_DIE3
    State::new(
        SpriteNum::SPR_CYBR,
        10,
        10,
        ActionF::None,
        StateNum::S_CYBER_DIE5,
        0,
        0,
    ), // S_CYBER_DIE4
    State::new(
        SpriteNum::SPR_CYBR,
        11,
        10,
        ActionF::None,
        StateNum::S_CYBER_DIE6,
        0,
        0,
    ), // S_CYBER_DIE5
    State::new(
        SpriteNum::SPR_CYBR,
        12,
        10,
        ActionF::Actor(a_fall),
        StateNum::S_CYBER_DIE7,
        0,
        0,
    ), // S_CYBER_DIE6
    State::new(
        SpriteNum::SPR_CYBR,
        13,
        10,
        ActionF::None,
        StateNum::S_CYBER_DIE8,
        0,
        0,
    ), // S_CYBER_DIE7
    State::new(
        SpriteNum::SPR_CYBR,
        14,
        10,
        ActionF::None,
        StateNum::S_CYBER_DIE9,
        0,
        0,
    ), // S_CYBER_DIE8
    State::new(
        SpriteNum::SPR_CYBR,
        15,
        30,
        ActionF::None,
        StateNum::S_CYBER_DIE10,
        0,
        0,
    ), // S_CYBER_DIE9
    State::new(
        SpriteNum::SPR_CYBR,
        15,
        -1,
        ActionF::Actor(a_bossdeath),
        StateNum::S_NULL,
        0,
        0,
    ), // S_CYBER_DIE10
    State::new(
        SpriteNum::SPR_PAIN,
        0,
        10,
        ActionF::Actor(a_look),
        StateNum::S_PAIN_STND,
        0,
        0,
    ), // S_PAIN_STND
    State::new(
        SpriteNum::SPR_PAIN,
        0,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_PAIN_RUN2,
        0,
        0,
    ), // S_PAIN_RUN1
    State::new(
        SpriteNum::SPR_PAIN,
        0,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_PAIN_RUN3,
        0,
        0,
    ), // S_PAIN_RUN2
    State::new(
        SpriteNum::SPR_PAIN,
        1,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_PAIN_RUN4,
        0,
        0,
    ), // S_PAIN_RUN3
    State::new(
        SpriteNum::SPR_PAIN,
        1,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_PAIN_RUN5,
        0,
        0,
    ), // S_PAIN_RUN4
    State::new(
        SpriteNum::SPR_PAIN,
        2,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_PAIN_RUN6,
        0,
        0,
    ), // S_PAIN_RUN5
    State::new(
        SpriteNum::SPR_PAIN,
        2,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_PAIN_RUN1,
        0,
        0,
    ), // S_PAIN_RUN6
    State::new(
        SpriteNum::SPR_PAIN,
        3,
        5,
        ActionF::Actor(a_facetarget),
        StateNum::S_PAIN_ATK2,
        0,
        0,
    ), // S_PAIN_ATK1
    State::new(
        SpriteNum::SPR_PAIN,
        4,
        5,
        ActionF::Actor(a_facetarget),
        StateNum::S_PAIN_ATK3,
        0,
        0,
    ), // S_PAIN_ATK2
    State::new(
        SpriteNum::SPR_PAIN,
        32773,
        5,
        ActionF::Actor(a_facetarget),
        StateNum::S_PAIN_ATK4,
        0,
        0,
    ), // S_PAIN_ATK3
    State::new(
        SpriteNum::SPR_PAIN,
        32773,
        0,
        ActionF::Actor(a_painattack),
        StateNum::S_PAIN_RUN1,
        0,
        0,
    ), // S_PAIN_ATK4
    State::new(
        SpriteNum::SPR_PAIN,
        6,
        6,
        ActionF::None,
        StateNum::S_PAIN_PAIN2,
        0,
        0,
    ), // S_PAIN_PAIN
    State::new(
        SpriteNum::SPR_PAIN,
        6,
        6,
        ActionF::Actor(a_pain),
        StateNum::S_PAIN_RUN1,
        0,
        0,
    ), // S_PAIN_PAIN2
    State::new(
        SpriteNum::SPR_PAIN,
        32775,
        8,
        ActionF::None,
        StateNum::S_PAIN_DIE2,
        0,
        0,
    ), // S_PAIN_DIE1
    State::new(
        SpriteNum::SPR_PAIN,
        32776,
        8,
        ActionF::Actor(a_scream),
        StateNum::S_PAIN_DIE3,
        0,
        0,
    ), // S_PAIN_DIE2
    State::new(
        SpriteNum::SPR_PAIN,
        32777,
        8,
        ActionF::None,
        StateNum::S_PAIN_DIE4,
        0,
        0,
    ), // S_PAIN_DIE3
    State::new(
        SpriteNum::SPR_PAIN,
        32778,
        8,
        ActionF::None,
        StateNum::S_PAIN_DIE5,
        0,
        0,
    ), // S_PAIN_DIE4
    State::new(
        SpriteNum::SPR_PAIN,
        32779,
        8,
        ActionF::Actor(a_paindie),
        StateNum::S_PAIN_DIE6,
        0,
        0,
    ), // S_PAIN_DIE5
    State::new(
        SpriteNum::SPR_PAIN,
        32780,
        8,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_PAIN_DIE6
    State::new(
        SpriteNum::SPR_PAIN,
        12,
        8,
        ActionF::None,
        StateNum::S_PAIN_RAISE2,
        0,
        0,
    ), // S_PAIN_RAISE1
    State::new(
        SpriteNum::SPR_PAIN,
        11,
        8,
        ActionF::None,
        StateNum::S_PAIN_RAISE3,
        0,
        0,
    ), // S_PAIN_RAISE2
    State::new(
        SpriteNum::SPR_PAIN,
        10,
        8,
        ActionF::None,
        StateNum::S_PAIN_RAISE4,
        0,
        0,
    ), // S_PAIN_RAISE3
    State::new(
        SpriteNum::SPR_PAIN,
        9,
        8,
        ActionF::None,
        StateNum::S_PAIN_RAISE5,
        0,
        0,
    ), // S_PAIN_RAISE4
    State::new(
        SpriteNum::SPR_PAIN,
        8,
        8,
        ActionF::None,
        StateNum::S_PAIN_RAISE6,
        0,
        0,
    ), // S_PAIN_RAISE5
    State::new(
        SpriteNum::SPR_PAIN,
        7,
        8,
        ActionF::None,
        StateNum::S_PAIN_RUN1,
        0,
        0,
    ), // S_PAIN_RAISE6
    State::new(
        SpriteNum::SPR_SSWV,
        0,
        10,
        ActionF::Actor(a_look),
        StateNum::S_SSWV_STND2,
        0,
        0,
    ), // S_SSWV_STND
    State::new(
        SpriteNum::SPR_SSWV,
        1,
        10,
        ActionF::Actor(a_look),
        StateNum::S_SSWV_STND,
        0,
        0,
    ), // S_SSWV_STND2
    State::new(
        SpriteNum::SPR_SSWV,
        0,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_SSWV_RUN2,
        0,
        0,
    ), // S_SSWV_RUN1
    State::new(
        SpriteNum::SPR_SSWV,
        0,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_SSWV_RUN3,
        0,
        0,
    ), // S_SSWV_RUN2
    State::new(
        SpriteNum::SPR_SSWV,
        1,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_SSWV_RUN4,
        0,
        0,
    ), // S_SSWV_RUN3
    State::new(
        SpriteNum::SPR_SSWV,
        1,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_SSWV_RUN5,
        0,
        0,
    ), // S_SSWV_RUN4
    State::new(
        SpriteNum::SPR_SSWV,
        2,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_SSWV_RUN6,
        0,
        0,
    ), // S_SSWV_RUN5
    State::new(
        SpriteNum::SPR_SSWV,
        2,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_SSWV_RUN7,
        0,
        0,
    ), // S_SSWV_RUN6
    State::new(
        SpriteNum::SPR_SSWV,
        3,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_SSWV_RUN8,
        0,
        0,
    ), // S_SSWV_RUN7
    State::new(
        SpriteNum::SPR_SSWV,
        3,
        3,
        ActionF::Actor(a_chase),
        StateNum::S_SSWV_RUN1,
        0,
        0,
    ), // S_SSWV_RUN8
    State::new(
        SpriteNum::SPR_SSWV,
        4,
        10,
        ActionF::Actor(a_facetarget),
        StateNum::S_SSWV_ATK2,
        0,
        0,
    ), // S_SSWV_ATK1
    State::new(
        SpriteNum::SPR_SSWV,
        5,
        10,
        ActionF::Actor(a_facetarget),
        StateNum::S_SSWV_ATK3,
        0,
        0,
    ), // S_SSWV_ATK2
    State::new(
        SpriteNum::SPR_SSWV,
        32774,
        4,
        ActionF::Actor(a_cposattack),
        StateNum::S_SSWV_ATK4,
        0,
        0,
    ), // S_SSWV_ATK3
    State::new(
        SpriteNum::SPR_SSWV,
        5,
        6,
        ActionF::Actor(a_facetarget),
        StateNum::S_SSWV_ATK5,
        0,
        0,
    ), // S_SSWV_ATK4
    State::new(
        SpriteNum::SPR_SSWV,
        32774,
        4,
        ActionF::Actor(a_cposattack),
        StateNum::S_SSWV_ATK6,
        0,
        0,
    ), // S_SSWV_ATK5
    State::new(
        SpriteNum::SPR_SSWV,
        5,
        1,
        ActionF::Actor(a_cposrefire),
        StateNum::S_SSWV_ATK2,
        0,
        0,
    ), // S_SSWV_ATK6
    State::new(
        SpriteNum::SPR_SSWV,
        7,
        3,
        ActionF::None,
        StateNum::S_SSWV_PAIN2,
        0,
        0,
    ), // S_SSWV_PAIN
    State::new(
        SpriteNum::SPR_SSWV,
        7,
        3,
        ActionF::Actor(a_pain),
        StateNum::S_SSWV_RUN1,
        0,
        0,
    ), // S_SSWV_PAIN2
    State::new(
        SpriteNum::SPR_SSWV,
        8,
        5,
        ActionF::None,
        StateNum::S_SSWV_DIE2,
        0,
        0,
    ), // S_SSWV_DIE1
    State::new(
        SpriteNum::SPR_SSWV,
        9,
        5,
        ActionF::Actor(a_scream),
        StateNum::S_SSWV_DIE3,
        0,
        0,
    ), // S_SSWV_DIE2
    State::new(
        SpriteNum::SPR_SSWV,
        10,
        5,
        ActionF::Actor(a_fall),
        StateNum::S_SSWV_DIE4,
        0,
        0,
    ), // S_SSWV_DIE3
    State::new(
        SpriteNum::SPR_SSWV,
        11,
        5,
        ActionF::None,
        StateNum::S_SSWV_DIE5,
        0,
        0,
    ), // S_SSWV_DIE4
    State::new(
        SpriteNum::SPR_SSWV,
        12,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_SSWV_DIE5
    State::new(
        SpriteNum::SPR_SSWV,
        13,
        5,
        ActionF::None,
        StateNum::S_SSWV_XDIE2,
        0,
        0,
    ), // S_SSWV_XDIE1
    State::new(
        SpriteNum::SPR_SSWV,
        14,
        5,
        ActionF::Actor(a_xscream),
        StateNum::S_SSWV_XDIE3,
        0,
        0,
    ), // S_SSWV_XDIE2
    State::new(
        SpriteNum::SPR_SSWV,
        15,
        5,
        ActionF::Actor(a_fall),
        StateNum::S_SSWV_XDIE4,
        0,
        0,
    ), // S_SSWV_XDIE3
    State::new(
        SpriteNum::SPR_SSWV,
        16,
        5,
        ActionF::None,
        StateNum::S_SSWV_XDIE5,
        0,
        0,
    ), // S_SSWV_XDIE4
    State::new(
        SpriteNum::SPR_SSWV,
        17,
        5,
        ActionF::None,
        StateNum::S_SSWV_XDIE6,
        0,
        0,
    ), // S_SSWV_XDIE5
    State::new(
        SpriteNum::SPR_SSWV,
        18,
        5,
        ActionF::None,
        StateNum::S_SSWV_XDIE7,
        0,
        0,
    ), // S_SSWV_XDIE6
    State::new(
        SpriteNum::SPR_SSWV,
        19,
        5,
        ActionF::None,
        StateNum::S_SSWV_XDIE8,
        0,
        0,
    ), // S_SSWV_XDIE7
    State::new(
        SpriteNum::SPR_SSWV,
        20,
        5,
        ActionF::None,
        StateNum::S_SSWV_XDIE9,
        0,
        0,
    ), // S_SSWV_XDIE8
    State::new(
        SpriteNum::SPR_SSWV,
        21,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_SSWV_XDIE9
    State::new(
        SpriteNum::SPR_SSWV,
        12,
        5,
        ActionF::None,
        StateNum::S_SSWV_RAISE2,
        0,
        0,
    ), // S_SSWV_RAISE1
    State::new(
        SpriteNum::SPR_SSWV,
        11,
        5,
        ActionF::None,
        StateNum::S_SSWV_RAISE3,
        0,
        0,
    ), // S_SSWV_RAISE2
    State::new(
        SpriteNum::SPR_SSWV,
        10,
        5,
        ActionF::None,
        StateNum::S_SSWV_RAISE4,
        0,
        0,
    ), // S_SSWV_RAISE3
    State::new(
        SpriteNum::SPR_SSWV,
        9,
        5,
        ActionF::None,
        StateNum::S_SSWV_RAISE5,
        0,
        0,
    ), // S_SSWV_RAISE4
    State::new(
        SpriteNum::SPR_SSWV,
        8,
        5,
        ActionF::None,
        StateNum::S_SSWV_RUN1,
        0,
        0,
    ), // S_SSWV_RAISE5
    State::new(
        SpriteNum::SPR_KEEN,
        0,
        -1,
        ActionF::None,
        StateNum::S_KEENSTND,
        0,
        0,
    ), // S_KEENSTND
    State::new(
        SpriteNum::SPR_KEEN,
        0,
        6,
        ActionF::None,
        StateNum::S_COMMKEEN2,
        0,
        0,
    ), // S_COMMKEEN
    State::new(
        SpriteNum::SPR_KEEN,
        1,
        6,
        ActionF::None,
        StateNum::S_COMMKEEN3,
        0,
        0,
    ), // S_COMMKEEN2
    State::new(
        SpriteNum::SPR_KEEN,
        2,
        6,
        ActionF::Actor(a_scream),
        StateNum::S_COMMKEEN4,
        0,
        0,
    ), // S_COMMKEEN3
    State::new(
        SpriteNum::SPR_KEEN,
        3,
        6,
        ActionF::None,
        StateNum::S_COMMKEEN5,
        0,
        0,
    ), // S_COMMKEEN4
    State::new(
        SpriteNum::SPR_KEEN,
        4,
        6,
        ActionF::None,
        StateNum::S_COMMKEEN6,
        0,
        0,
    ), // S_COMMKEEN5
    State::new(
        SpriteNum::SPR_KEEN,
        5,
        6,
        ActionF::None,
        StateNum::S_COMMKEEN7,
        0,
        0,
    ), // S_COMMKEEN6
    State::new(
        SpriteNum::SPR_KEEN,
        6,
        6,
        ActionF::None,
        StateNum::S_COMMKEEN8,
        0,
        0,
    ), // S_COMMKEEN7
    State::new(
        SpriteNum::SPR_KEEN,
        7,
        6,
        ActionF::None,
        StateNum::S_COMMKEEN9,
        0,
        0,
    ), // S_COMMKEEN8
    State::new(
        SpriteNum::SPR_KEEN,
        8,
        6,
        ActionF::None,
        StateNum::S_COMMKEEN10,
        0,
        0,
    ), // S_COMMKEEN9
    State::new(
        SpriteNum::SPR_KEEN,
        9,
        6,
        ActionF::None,
        StateNum::S_COMMKEEN11,
        0,
        0,
    ), // S_COMMKEEN10
    State::new(
        SpriteNum::SPR_KEEN,
        10,
        6,
        ActionF::Actor(a_keendie),
        StateNum::S_COMMKEEN12,
        0,
        0,
    ), // S_COMMKEEN11
    State::new(
        SpriteNum::SPR_KEEN,
        11,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_COMMKEEN12
    State::new(
        SpriteNum::SPR_KEEN,
        12,
        4,
        ActionF::None,
        StateNum::S_KEENPAIN2,
        0,
        0,
    ), // S_KEENPAIN
    State::new(
        SpriteNum::SPR_KEEN,
        12,
        8,
        ActionF::Actor(a_pain),
        StateNum::S_KEENSTND,
        0,
        0,
    ), // S_KEENPAIN2
    State::new(
        SpriteNum::SPR_BBRN,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_BRAIN
    State::new(
        SpriteNum::SPR_BBRN,
        1,
        36,
        ActionF::Actor(a_brainpain),
        StateNum::S_BRAIN,
        0,
        0,
    ), // S_BRAIN_PAIN
    State::new(
        SpriteNum::SPR_BBRN,
        0,
        100,
        ActionF::Actor(a_brainscream),
        StateNum::S_BRAIN_DIE2,
        0,
        0,
    ), // S_BRAIN_DIE1
    State::new(
        SpriteNum::SPR_BBRN,
        0,
        10,
        ActionF::None,
        StateNum::S_BRAIN_DIE3,
        0,
        0,
    ), // S_BRAIN_DIE2
    State::new(
        SpriteNum::SPR_BBRN,
        0,
        10,
        ActionF::None,
        StateNum::S_BRAIN_DIE4,
        0,
        0,
    ), // S_BRAIN_DIE3
    State::new(
        SpriteNum::SPR_BBRN,
        0,
        -1,
        ActionF::Actor(a_braindie),
        StateNum::S_NULL,
        0,
        0,
    ), // S_BRAIN_DIE4
    State::new(
        SpriteNum::SPR_SSWV,
        0,
        10,
        ActionF::Actor(a_look),
        StateNum::S_BRAINEYE,
        0,
        0,
    ), // S_BRAINEYE
    State::new(
        SpriteNum::SPR_SSWV,
        0,
        181,
        ActionF::Actor(a_brainawake),
        StateNum::S_BRAINEYE1,
        0,
        0,
    ), // S_BRAINEYESEE
    State::new(
        SpriteNum::SPR_SSWV,
        0,
        150,
        ActionF::Actor(a_brainspit),
        StateNum::S_BRAINEYE1,
        0,
        0,
    ), // S_BRAINEYE1
    State::new(
        SpriteNum::SPR_BOSF,
        32768,
        3,
        ActionF::Actor(a_spawnsound),
        StateNum::S_SPAWN2,
        0,
        0,
    ), // S_SPAWN1
    State::new(
        SpriteNum::SPR_BOSF,
        32769,
        3,
        ActionF::Actor(a_spawnfly),
        StateNum::S_SPAWN3,
        0,
        0,
    ), // S_SPAWN2
    State::new(
        SpriteNum::SPR_BOSF,
        32770,
        3,
        ActionF::Actor(a_spawnfly),
        StateNum::S_SPAWN4,
        0,
        0,
    ), // S_SPAWN3
    State::new(
        SpriteNum::SPR_BOSF,
        32771,
        3,
        ActionF::Actor(a_spawnfly),
        StateNum::S_SPAWN1,
        0,
        0,
    ), // S_SPAWN4
    State::new(
        SpriteNum::SPR_FIRE,
        32768,
        4,
        ActionF::Actor(a_fire),
        StateNum::S_SPAWNFIRE2,
        0,
        0,
    ), // S_SPAWNFIRE1
    State::new(
        SpriteNum::SPR_FIRE,
        32769,
        4,
        ActionF::Actor(a_fire),
        StateNum::S_SPAWNFIRE3,
        0,
        0,
    ), // S_SPAWNFIRE2
    State::new(
        SpriteNum::SPR_FIRE,
        32770,
        4,
        ActionF::Actor(a_fire),
        StateNum::S_SPAWNFIRE4,
        0,
        0,
    ), // S_SPAWNFIRE3
    State::new(
        SpriteNum::SPR_FIRE,
        32771,
        4,
        ActionF::Actor(a_fire),
        StateNum::S_SPAWNFIRE5,
        0,
        0,
    ), // S_SPAWNFIRE4
    State::new(
        SpriteNum::SPR_FIRE,
        32772,
        4,
        ActionF::Actor(a_fire),
        StateNum::S_SPAWNFIRE6,
        0,
        0,
    ), // S_SPAWNFIRE5
    State::new(
        SpriteNum::SPR_FIRE,
        32773,
        4,
        ActionF::Actor(a_fire),
        StateNum::S_SPAWNFIRE7,
        0,
        0,
    ), // S_SPAWNFIRE6
    State::new(
        SpriteNum::SPR_FIRE,
        32774,
        4,
        ActionF::Actor(a_fire),
        StateNum::S_SPAWNFIRE8,
        0,
        0,
    ), // S_SPAWNFIRE7
    State::new(
        SpriteNum::SPR_FIRE,
        32775,
        4,
        ActionF::Actor(a_fire),
        StateNum::S_NULL,
        0,
        0,
    ), // S_SPAWNFIRE8
    State::new(
        SpriteNum::SPR_MISL,
        32769,
        10,
        ActionF::None,
        StateNum::S_BRAINEXPLODE2,
        0,
        0,
    ), // S_BRAINEXPLODE1
    State::new(
        SpriteNum::SPR_MISL,
        32770,
        10,
        ActionF::None,
        StateNum::S_BRAINEXPLODE3,
        0,
        0,
    ), // S_BRAINEXPLODE2
    State::new(
        SpriteNum::SPR_MISL,
        32771,
        10,
        ActionF::Actor(a_brainexplode),
        StateNum::S_NULL,
        0,
        0,
    ), // S_BRAINEXPLODE3
    State::new(
        SpriteNum::SPR_ARM1,
        0,
        6,
        ActionF::None,
        StateNum::S_ARM1A,
        0,
        0,
    ), // S_ARM1
    State::new(
        SpriteNum::SPR_ARM1,
        32769,
        7,
        ActionF::None,
        StateNum::S_ARM1,
        0,
        0,
    ), // S_ARM1A
    State::new(
        SpriteNum::SPR_ARM2,
        0,
        6,
        ActionF::None,
        StateNum::S_ARM2A,
        0,
        0,
    ), // S_ARM2
    State::new(
        SpriteNum::SPR_ARM2,
        32769,
        6,
        ActionF::None,
        StateNum::S_ARM2,
        0,
        0,
    ), // S_ARM2A
    State::new(
        SpriteNum::SPR_BAR1,
        0,
        6,
        ActionF::None,
        StateNum::S_BAR2,
        0,
        0,
    ), // S_BAR1
    State::new(
        SpriteNum::SPR_BAR1,
        1,
        6,
        ActionF::None,
        StateNum::S_BAR1,
        0,
        0,
    ), // S_BAR2
    State::new(
        SpriteNum::SPR_BEXP,
        32768,
        5,
        ActionF::None,
        StateNum::S_BEXP2,
        0,
        0,
    ), // S_BEXP
    State::new(
        SpriteNum::SPR_BEXP,
        32769,
        5,
        ActionF::Actor(a_scream),
        StateNum::S_BEXP3,
        0,
        0,
    ), // S_BEXP2
    State::new(
        SpriteNum::SPR_BEXP,
        32770,
        5,
        ActionF::None,
        StateNum::S_BEXP4,
        0,
        0,
    ), // S_BEXP3
    State::new(
        SpriteNum::SPR_BEXP,
        32771,
        10,
        ActionF::Actor(a_explode),
        StateNum::S_BEXP5,
        0,
        0,
    ), // S_BEXP4
    State::new(
        SpriteNum::SPR_BEXP,
        32772,
        10,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_BEXP5
    State::new(
        SpriteNum::SPR_FCAN,
        32768,
        4,
        ActionF::None,
        StateNum::S_BBAR2,
        0,
        0,
    ), // S_BBAR1
    State::new(
        SpriteNum::SPR_FCAN,
        32769,
        4,
        ActionF::None,
        StateNum::S_BBAR3,
        0,
        0,
    ), // S_BBAR2
    State::new(
        SpriteNum::SPR_FCAN,
        32770,
        4,
        ActionF::None,
        StateNum::S_BBAR1,
        0,
        0,
    ), // S_BBAR3
    State::new(
        SpriteNum::SPR_BON1,
        0,
        6,
        ActionF::None,
        StateNum::S_BON1A,
        0,
        0,
    ), // S_BON1
    State::new(
        SpriteNum::SPR_BON1,
        1,
        6,
        ActionF::None,
        StateNum::S_BON1B,
        0,
        0,
    ), // S_BON1A
    State::new(
        SpriteNum::SPR_BON1,
        2,
        6,
        ActionF::None,
        StateNum::S_BON1C,
        0,
        0,
    ), // S_BON1B
    State::new(
        SpriteNum::SPR_BON1,
        3,
        6,
        ActionF::None,
        StateNum::S_BON1D,
        0,
        0,
    ), // S_BON1C
    State::new(
        SpriteNum::SPR_BON1,
        2,
        6,
        ActionF::None,
        StateNum::S_BON1E,
        0,
        0,
    ), // S_BON1D
    State::new(
        SpriteNum::SPR_BON1,
        1,
        6,
        ActionF::None,
        StateNum::S_BON1,
        0,
        0,
    ), // S_BON1E
    State::new(
        SpriteNum::SPR_BON2,
        0,
        6,
        ActionF::None,
        StateNum::S_BON2A,
        0,
        0,
    ), // S_BON2
    State::new(
        SpriteNum::SPR_BON2,
        1,
        6,
        ActionF::None,
        StateNum::S_BON2B,
        0,
        0,
    ), // S_BON2A
    State::new(
        SpriteNum::SPR_BON2,
        2,
        6,
        ActionF::None,
        StateNum::S_BON2C,
        0,
        0,
    ), // S_BON2B
    State::new(
        SpriteNum::SPR_BON2,
        3,
        6,
        ActionF::None,
        StateNum::S_BON2D,
        0,
        0,
    ), // S_BON2C
    State::new(
        SpriteNum::SPR_BON2,
        2,
        6,
        ActionF::None,
        StateNum::S_BON2E,
        0,
        0,
    ), // S_BON2D
    State::new(
        SpriteNum::SPR_BON2,
        1,
        6,
        ActionF::None,
        StateNum::S_BON2,
        0,
        0,
    ), // S_BON2E
    State::new(
        SpriteNum::SPR_BKEY,
        0,
        10,
        ActionF::None,
        StateNum::S_BKEY2,
        0,
        0,
    ), // S_BKEY
    State::new(
        SpriteNum::SPR_BKEY,
        32769,
        10,
        ActionF::None,
        StateNum::S_BKEY,
        0,
        0,
    ), // S_BKEY2
    State::new(
        SpriteNum::SPR_RKEY,
        0,
        10,
        ActionF::None,
        StateNum::S_RKEY2,
        0,
        0,
    ), // S_RKEY
    State::new(
        SpriteNum::SPR_RKEY,
        32769,
        10,
        ActionF::None,
        StateNum::S_RKEY,
        0,
        0,
    ), // S_RKEY2
    State::new(
        SpriteNum::SPR_YKEY,
        0,
        10,
        ActionF::None,
        StateNum::S_YKEY2,
        0,
        0,
    ), // S_YKEY
    State::new(
        SpriteNum::SPR_YKEY,
        32769,
        10,
        ActionF::None,
        StateNum::S_YKEY,
        0,
        0,
    ), // S_YKEY2
    State::new(
        SpriteNum::SPR_BSKU,
        0,
        10,
        ActionF::None,
        StateNum::S_BSKULL2,
        0,
        0,
    ), // S_BSKULL
    State::new(
        SpriteNum::SPR_BSKU,
        32769,
        10,
        ActionF::None,
        StateNum::S_BSKULL,
        0,
        0,
    ), // S_BSKULL2
    State::new(
        SpriteNum::SPR_RSKU,
        0,
        10,
        ActionF::None,
        StateNum::S_RSKULL2,
        0,
        0,
    ), // S_RSKULL
    State::new(
        SpriteNum::SPR_RSKU,
        32769,
        10,
        ActionF::None,
        StateNum::S_RSKULL,
        0,
        0,
    ), // S_RSKULL2
    State::new(
        SpriteNum::SPR_YSKU,
        0,
        10,
        ActionF::None,
        StateNum::S_YSKULL2,
        0,
        0,
    ), // S_YSKULL
    State::new(
        SpriteNum::SPR_YSKU,
        32769,
        10,
        ActionF::None,
        StateNum::S_YSKULL,
        0,
        0,
    ), // S_YSKULL2
    State::new(
        SpriteNum::SPR_STIM,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_STIM
    State::new(
        SpriteNum::SPR_MEDI,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_MEDI
    State::new(
        SpriteNum::SPR_SOUL,
        32768,
        6,
        ActionF::None,
        StateNum::S_SOUL2,
        0,
        0,
    ), // S_SOUL
    State::new(
        SpriteNum::SPR_SOUL,
        32769,
        6,
        ActionF::None,
        StateNum::S_SOUL3,
        0,
        0,
    ), // S_SOUL2
    State::new(
        SpriteNum::SPR_SOUL,
        32770,
        6,
        ActionF::None,
        StateNum::S_SOUL4,
        0,
        0,
    ), // S_SOUL3
    State::new(
        SpriteNum::SPR_SOUL,
        32771,
        6,
        ActionF::None,
        StateNum::S_SOUL5,
        0,
        0,
    ), // S_SOUL4
    State::new(
        SpriteNum::SPR_SOUL,
        32770,
        6,
        ActionF::None,
        StateNum::S_SOUL6,
        0,
        0,
    ), // S_SOUL5
    State::new(
        SpriteNum::SPR_SOUL,
        32769,
        6,
        ActionF::None,
        StateNum::S_SOUL,
        0,
        0,
    ), // S_SOUL6
    State::new(
        SpriteNum::SPR_PINV,
        32768,
        6,
        ActionF::None,
        StateNum::S_PINV2,
        0,
        0,
    ), // S_PINV
    State::new(
        SpriteNum::SPR_PINV,
        32769,
        6,
        ActionF::None,
        StateNum::S_PINV3,
        0,
        0,
    ), // S_PINV2
    State::new(
        SpriteNum::SPR_PINV,
        32770,
        6,
        ActionF::None,
        StateNum::S_PINV4,
        0,
        0,
    ), // S_PINV3
    State::new(
        SpriteNum::SPR_PINV,
        32771,
        6,
        ActionF::None,
        StateNum::S_PINV,
        0,
        0,
    ), // S_PINV4
    State::new(
        SpriteNum::SPR_PSTR,
        32768,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_PSTR
    State::new(
        SpriteNum::SPR_PINS,
        32768,
        6,
        ActionF::None,
        StateNum::S_PINS2,
        0,
        0,
    ), // S_PINS
    State::new(
        SpriteNum::SPR_PINS,
        32769,
        6,
        ActionF::None,
        StateNum::S_PINS3,
        0,
        0,
    ), // S_PINS2
    State::new(
        SpriteNum::SPR_PINS,
        32770,
        6,
        ActionF::None,
        StateNum::S_PINS4,
        0,
        0,
    ), // S_PINS3
    State::new(
        SpriteNum::SPR_PINS,
        32771,
        6,
        ActionF::None,
        StateNum::S_PINS,
        0,
        0,
    ), // S_PINS4
    State::new(
        SpriteNum::SPR_MEGA,
        32768,
        6,
        ActionF::None,
        StateNum::S_MEGA2,
        0,
        0,
    ), // S_MEGA
    State::new(
        SpriteNum::SPR_MEGA,
        32769,
        6,
        ActionF::None,
        StateNum::S_MEGA3,
        0,
        0,
    ), // S_MEGA2
    State::new(
        SpriteNum::SPR_MEGA,
        32770,
        6,
        ActionF::None,
        StateNum::S_MEGA4,
        0,
        0,
    ), // S_MEGA3
    State::new(
        SpriteNum::SPR_MEGA,
        32771,
        6,
        ActionF::None,
        StateNum::S_MEGA,
        0,
        0,
    ), // S_MEGA4
    State::new(
        SpriteNum::SPR_SUIT,
        32768,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_SUIT
    State::new(
        SpriteNum::SPR_PMAP,
        32768,
        6,
        ActionF::None,
        StateNum::S_PMAP2,
        0,
        0,
    ), // S_PMAP
    State::new(
        SpriteNum::SPR_PMAP,
        32769,
        6,
        ActionF::None,
        StateNum::S_PMAP3,
        0,
        0,
    ), // S_PMAP2
    State::new(
        SpriteNum::SPR_PMAP,
        32770,
        6,
        ActionF::None,
        StateNum::S_PMAP4,
        0,
        0,
    ), // S_PMAP3
    State::new(
        SpriteNum::SPR_PMAP,
        32771,
        6,
        ActionF::None,
        StateNum::S_PMAP5,
        0,
        0,
    ), // S_PMAP4
    State::new(
        SpriteNum::SPR_PMAP,
        32770,
        6,
        ActionF::None,
        StateNum::S_PMAP6,
        0,
        0,
    ), // S_PMAP5
    State::new(
        SpriteNum::SPR_PMAP,
        32769,
        6,
        ActionF::None,
        StateNum::S_PMAP,
        0,
        0,
    ), // S_PMAP6
    State::new(
        SpriteNum::SPR_PVIS,
        32768,
        6,
        ActionF::None,
        StateNum::S_PVIS2,
        0,
        0,
    ), // S_PVIS
    State::new(
        SpriteNum::SPR_PVIS,
        1,
        6,
        ActionF::None,
        StateNum::S_PVIS,
        0,
        0,
    ), // S_PVIS2
    State::new(
        SpriteNum::SPR_CLIP,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_CLIP
    State::new(
        SpriteNum::SPR_AMMO,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_AMMO
    State::new(
        SpriteNum::SPR_ROCK,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_ROCK
    State::new(
        SpriteNum::SPR_BROK,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_BROK
    State::new(
        SpriteNum::SPR_CELL,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_CELL
    State::new(
        SpriteNum::SPR_CELP,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_CELP
    State::new(
        SpriteNum::SPR_SHEL,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_SHEL
    State::new(
        SpriteNum::SPR_SBOX,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_SBOX
    State::new(
        SpriteNum::SPR_BPAK,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_BPAK
    State::new(
        SpriteNum::SPR_BFUG,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_BFUG
    State::new(
        SpriteNum::SPR_MGUN,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_MGUN
    State::new(
        SpriteNum::SPR_CSAW,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_CSAW
    State::new(
        SpriteNum::SPR_LAUN,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_LAUN
    State::new(
        SpriteNum::SPR_PLAS,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_PLAS
    State::new(
        SpriteNum::SPR_SHOT,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_SHOT
    State::new(
        SpriteNum::SPR_SGN2,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_SHOT2
    State::new(
        SpriteNum::SPR_COLU,
        32768,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_COLU
    State::new(
        SpriteNum::SPR_SMT2,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_STALAG
    State::new(
        SpriteNum::SPR_GOR1,
        0,
        10,
        ActionF::None,
        StateNum::S_BLOODYTWITCH2,
        0,
        0,
    ), // S_BLOODYTWITCH
    State::new(
        SpriteNum::SPR_GOR1,
        1,
        15,
        ActionF::None,
        StateNum::S_BLOODYTWITCH3,
        0,
        0,
    ), // S_BLOODYTWITCH2
    State::new(
        SpriteNum::SPR_GOR1,
        2,
        8,
        ActionF::None,
        StateNum::S_BLOODYTWITCH4,
        0,
        0,
    ), // S_BLOODYTWITCH3
    State::new(
        SpriteNum::SPR_GOR1,
        1,
        6,
        ActionF::None,
        StateNum::S_BLOODYTWITCH,
        0,
        0,
    ), // S_BLOODYTWITCH4
    State::new(
        SpriteNum::SPR_PLAY,
        13,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_DEADTORSO
    State::new(
        SpriteNum::SPR_PLAY,
        18,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_DEADBOTTOM
    State::new(
        SpriteNum::SPR_POL2,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_HEADSONSTICK
    State::new(
        SpriteNum::SPR_POL5,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_GIBS
    State::new(
        SpriteNum::SPR_POL4,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_HEADONASTICK
    State::new(
        SpriteNum::SPR_POL3,
        32768,
        6,
        ActionF::None,
        StateNum::S_HEADCANDLES2,
        0,
        0,
    ), // S_HEADCANDLES
    State::new(
        SpriteNum::SPR_POL3,
        32769,
        6,
        ActionF::None,
        StateNum::S_HEADCANDLES,
        0,
        0,
    ), // S_HEADCANDLES2
    State::new(
        SpriteNum::SPR_POL1,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_DEADSTICK
    State::new(
        SpriteNum::SPR_POL6,
        0,
        6,
        ActionF::None,
        StateNum::S_LIVESTICK2,
        0,
        0,
    ), // S_LIVESTICK
    State::new(
        SpriteNum::SPR_POL6,
        1,
        8,
        ActionF::None,
        StateNum::S_LIVESTICK,
        0,
        0,
    ), // S_LIVESTICK2
    State::new(
        SpriteNum::SPR_GOR2,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_MEAT2
    State::new(
        SpriteNum::SPR_GOR3,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_MEAT3
    State::new(
        SpriteNum::SPR_GOR4,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_MEAT4
    State::new(
        SpriteNum::SPR_GOR5,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_MEAT5
    State::new(
        SpriteNum::SPR_SMIT,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_STALAGTITE
    State::new(
        SpriteNum::SPR_COL1,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_TALLGRNCOL
    State::new(
        SpriteNum::SPR_COL2,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_SHRTGRNCOL
    State::new(
        SpriteNum::SPR_COL3,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_TALLREDCOL
    State::new(
        SpriteNum::SPR_COL4,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_SHRTREDCOL
    State::new(
        SpriteNum::SPR_CAND,
        32768,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_CANDLESTIK
    State::new(
        SpriteNum::SPR_CBRA,
        32768,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_CANDELABRA
    State::new(
        SpriteNum::SPR_COL6,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_SKULLCOL
    State::new(
        SpriteNum::SPR_TRE1,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_TORCHTREE
    State::new(
        SpriteNum::SPR_TRE2,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_BIGTREE
    State::new(
        SpriteNum::SPR_ELEC,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_TECHPILLAR
    State::new(
        SpriteNum::SPR_CEYE,
        32768,
        6,
        ActionF::None,
        StateNum::S_EVILEYE2,
        0,
        0,
    ), // S_EVILEYE
    State::new(
        SpriteNum::SPR_CEYE,
        32769,
        6,
        ActionF::None,
        StateNum::S_EVILEYE3,
        0,
        0,
    ), // S_EVILEYE2
    State::new(
        SpriteNum::SPR_CEYE,
        32770,
        6,
        ActionF::None,
        StateNum::S_EVILEYE4,
        0,
        0,
    ), // S_EVILEYE3
    State::new(
        SpriteNum::SPR_CEYE,
        32769,
        6,
        ActionF::None,
        StateNum::S_EVILEYE,
        0,
        0,
    ), // S_EVILEYE4
    State::new(
        SpriteNum::SPR_FSKU,
        32768,
        6,
        ActionF::None,
        StateNum::S_FLOATSKULL2,
        0,
        0,
    ), // S_FLOATSKULL
    State::new(
        SpriteNum::SPR_FSKU,
        32769,
        6,
        ActionF::None,
        StateNum::S_FLOATSKULL3,
        0,
        0,
    ), // S_FLOATSKULL2
    State::new(
        SpriteNum::SPR_FSKU,
        32770,
        6,
        ActionF::None,
        StateNum::S_FLOATSKULL,
        0,
        0,
    ), // S_FLOATSKULL3
    State::new(
        SpriteNum::SPR_COL5,
        0,
        14,
        ActionF::None,
        StateNum::S_HEARTCOL2,
        0,
        0,
    ), // S_HEARTCOL
    State::new(
        SpriteNum::SPR_COL5,
        1,
        14,
        ActionF::None,
        StateNum::S_HEARTCOL,
        0,
        0,
    ), // S_HEARTCOL2
    State::new(
        SpriteNum::SPR_TBLU,
        32768,
        4,
        ActionF::None,
        StateNum::S_BLUETORCH2,
        0,
        0,
    ), // S_BLUETORCH
    State::new(
        SpriteNum::SPR_TBLU,
        32769,
        4,
        ActionF::None,
        StateNum::S_BLUETORCH3,
        0,
        0,
    ), // S_BLUETORCH2
    State::new(
        SpriteNum::SPR_TBLU,
        32770,
        4,
        ActionF::None,
        StateNum::S_BLUETORCH4,
        0,
        0,
    ), // S_BLUETORCH3
    State::new(
        SpriteNum::SPR_TBLU,
        32771,
        4,
        ActionF::None,
        StateNum::S_BLUETORCH,
        0,
        0,
    ), // S_BLUETORCH4
    State::new(
        SpriteNum::SPR_TGRN,
        32768,
        4,
        ActionF::None,
        StateNum::S_GREENTORCH2,
        0,
        0,
    ), // S_GREENTORCH
    State::new(
        SpriteNum::SPR_TGRN,
        32769,
        4,
        ActionF::None,
        StateNum::S_GREENTORCH3,
        0,
        0,
    ), // S_GREENTORCH2
    State::new(
        SpriteNum::SPR_TGRN,
        32770,
        4,
        ActionF::None,
        StateNum::S_GREENTORCH4,
        0,
        0,
    ), // S_GREENTORCH3
    State::new(
        SpriteNum::SPR_TGRN,
        32771,
        4,
        ActionF::None,
        StateNum::S_GREENTORCH,
        0,
        0,
    ), // S_GREENTORCH4
    State::new(
        SpriteNum::SPR_TRED,
        32768,
        4,
        ActionF::None,
        StateNum::S_REDTORCH2,
        0,
        0,
    ), // S_REDTORCH
    State::new(
        SpriteNum::SPR_TRED,
        32769,
        4,
        ActionF::None,
        StateNum::S_REDTORCH3,
        0,
        0,
    ), // S_REDTORCH2
    State::new(
        SpriteNum::SPR_TRED,
        32770,
        4,
        ActionF::None,
        StateNum::S_REDTORCH4,
        0,
        0,
    ), // S_REDTORCH3
    State::new(
        SpriteNum::SPR_TRED,
        32771,
        4,
        ActionF::None,
        StateNum::S_REDTORCH,
        0,
        0,
    ), // S_REDTORCH4
    State::new(
        SpriteNum::SPR_SMBT,
        32768,
        4,
        ActionF::None,
        StateNum::S_BTORCHSHRT2,
        0,
        0,
    ), // S_BTORCHSHRT
    State::new(
        SpriteNum::SPR_SMBT,
        32769,
        4,
        ActionF::None,
        StateNum::S_BTORCHSHRT3,
        0,
        0,
    ), // S_BTORCHSHRT2
    State::new(
        SpriteNum::SPR_SMBT,
        32770,
        4,
        ActionF::None,
        StateNum::S_BTORCHSHRT4,
        0,
        0,
    ), // S_BTORCHSHRT3
    State::new(
        SpriteNum::SPR_SMBT,
        32771,
        4,
        ActionF::None,
        StateNum::S_BTORCHSHRT,
        0,
        0,
    ), // S_BTORCHSHRT4
    State::new(
        SpriteNum::SPR_SMGT,
        32768,
        4,
        ActionF::None,
        StateNum::S_GTORCHSHRT2,
        0,
        0,
    ), // S_GTORCHSHRT
    State::new(
        SpriteNum::SPR_SMGT,
        32769,
        4,
        ActionF::None,
        StateNum::S_GTORCHSHRT3,
        0,
        0,
    ), // S_GTORCHSHRT2
    State::new(
        SpriteNum::SPR_SMGT,
        32770,
        4,
        ActionF::None,
        StateNum::S_GTORCHSHRT4,
        0,
        0,
    ), // S_GTORCHSHRT3
    State::new(
        SpriteNum::SPR_SMGT,
        32771,
        4,
        ActionF::None,
        StateNum::S_GTORCHSHRT,
        0,
        0,
    ), // S_GTORCHSHRT4
    State::new(
        SpriteNum::SPR_SMRT,
        32768,
        4,
        ActionF::None,
        StateNum::S_RTORCHSHRT2,
        0,
        0,
    ), // S_RTORCHSHRT
    State::new(
        SpriteNum::SPR_SMRT,
        32769,
        4,
        ActionF::None,
        StateNum::S_RTORCHSHRT3,
        0,
        0,
    ), // S_RTORCHSHRT2
    State::new(
        SpriteNum::SPR_SMRT,
        32770,
        4,
        ActionF::None,
        StateNum::S_RTORCHSHRT4,
        0,
        0,
    ), // S_RTORCHSHRT3
    State::new(
        SpriteNum::SPR_SMRT,
        32771,
        4,
        ActionF::None,
        StateNum::S_RTORCHSHRT,
        0,
        0,
    ), // S_RTORCHSHRT4
    State::new(
        SpriteNum::SPR_HDB1,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_HANGNOGUTS
    State::new(
        SpriteNum::SPR_HDB2,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_HANGBNOBRAIN
    State::new(
        SpriteNum::SPR_HDB3,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_HANGTLOOKDN
    State::new(
        SpriteNum::SPR_HDB4,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_HANGTSKULL
    State::new(
        SpriteNum::SPR_HDB5,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_HANGTLOOKUP
    State::new(
        SpriteNum::SPR_HDB6,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_HANGTNOBRAIN
    State::new(
        SpriteNum::SPR_POB1,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_COLONGIBS
    State::new(
        SpriteNum::SPR_POB2,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_SMALLPOOL
    State::new(
        SpriteNum::SPR_BRS1,
        0,
        -1,
        ActionF::None,
        StateNum::S_NULL,
        0,
        0,
    ), // S_BRAINSTEM
    State::new(
        SpriteNum::SPR_TLMP,
        32768,
        4,
        ActionF::None,
        StateNum::S_TECHLAMP2,
        0,
        0,
    ), // S_TECHLAMP
    State::new(
        SpriteNum::SPR_TLMP,
        32769,
        4,
        ActionF::None,
        StateNum::S_TECHLAMP3,
        0,
        0,
    ), // S_TECHLAMP2
    State::new(
        SpriteNum::SPR_TLMP,
        32770,
        4,
        ActionF::None,
        StateNum::S_TECHLAMP4,
        0,
        0,
    ), // S_TECHLAMP3
    State::new(
        SpriteNum::SPR_TLMP,
        32771,
        4,
        ActionF::None,
        StateNum::S_TECHLAMP,
        0,
        0,
    ), // S_TECHLAMP4
    State::new(
        SpriteNum::SPR_TLP2,
        32768,
        4,
        ActionF::None,
        StateNum::S_TECH2LAMP2,
        0,
        0,
    ), // S_TECH2LAMP
    State::new(
        SpriteNum::SPR_TLP2,
        32769,
        4,
        ActionF::None,
        StateNum::S_TECH2LAMP3,
        0,
        0,
    ), // S_TECH2LAMP2
    State::new(
        SpriteNum::SPR_TLP2,
        32770,
        4,
        ActionF::None,
        StateNum::S_TECH2LAMP4,
        0,
        0,
    ), // S_TECH2LAMP3
    State::new(
        SpriteNum::SPR_TLP2,
        32771,
        4,
        ActionF::None,
        StateNum::S_TECH2LAMP,
        0,
        0,
    ), // S_TECH2LAMP4
];
