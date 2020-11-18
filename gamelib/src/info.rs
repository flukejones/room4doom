use crate::local::ActionF;
use crate::map_object::MapObjectFlag;
use crate::sounds::SfxEnum;

#[derive(Debug)]
#[allow(non_camel_case_types)]
pub enum SpriteNum {
    SPR_TROO,
    SPR_SHTG,
    SPR_PUNG,
    SPR_PISG,
    SPR_PISF,
    SPR_SHTF,
    SPR_SHT2,
    SPR_CHGG,
    SPR_CHGF,
    SPR_MISG,
    SPR_MISF,
    SPR_SAWG,
    SPR_PLSG,
    SPR_PLSF,
    SPR_BFGG,
    SPR_BFGF,
    SPR_BLUD,
    SPR_PUFF,
    SPR_BAL1,
    SPR_BAL2,
    SPR_PLSS,
    SPR_PLSE,
    SPR_MISL,
    SPR_BFS1,
    SPR_BFE1,
    SPR_BFE2,
    SPR_TFOG,
    SPR_IFOG,
    SPR_PLAY,
    SPR_POSS,
    SPR_SPOS,
    SPR_VILE,
    SPR_FIRE,
    SPR_FATB,
    SPR_FBXP,
    SPR_SKEL,
    SPR_MANF,
    SPR_FATT,
    SPR_CPOS,
    SPR_SARG,
    SPR_HEAD,
    SPR_BAL7,
    SPR_BOSS,
    SPR_BOS2,
    SPR_SKUL,
    SPR_SPID,
    SPR_BSPI,
    SPR_APLS,
    SPR_APBX,
    SPR_CYBR,
    SPR_PAIN,
    SPR_SSWV,
    SPR_KEEN,
    SPR_BBRN,
    SPR_BOSF,
    SPR_ARM1,
    SPR_ARM2,
    SPR_BAR1,
    SPR_BEXP,
    SPR_FCAN,
    SPR_BON1,
    SPR_BON2,
    SPR_BKEY,
    SPR_RKEY,
    SPR_YKEY,
    SPR_BSKU,
    SPR_RSKU,
    SPR_YSKU,
    SPR_STIM,
    SPR_MEDI,
    SPR_SOUL,
    SPR_PINV,
    SPR_PSTR,
    SPR_PINS,
    SPR_MEGA,
    SPR_SUIT,
    SPR_PMAP,
    SPR_PVIS,
    SPR_CLIP,
    SPR_AMMO,
    SPR_ROCK,
    SPR_BROK,
    SPR_CELL,
    SPR_CELP,
    SPR_SHEL,
    SPR_SBOX,
    SPR_BPAK,
    SPR_BFUG,
    SPR_MGUN,
    SPR_CSAW,
    SPR_LAUN,
    SPR_PLAS,
    SPR_SHOT,
    SPR_SGN2,
    SPR_COLU,
    SPR_SMT2,
    SPR_GOR1,
    SPR_POL2,
    SPR_POL5,
    SPR_POL4,
    SPR_POL3,
    SPR_POL1,
    SPR_POL6,
    SPR_GOR2,
    SPR_GOR3,
    SPR_GOR4,
    SPR_GOR5,
    SPR_SMIT,
    SPR_COL1,
    SPR_COL2,
    SPR_COL3,
    SPR_COL4,
    SPR_CAND,
    SPR_CBRA,
    SPR_COL6,
    SPR_TRE1,
    SPR_TRE2,
    SPR_ELEC,
    SPR_CEYE,
    SPR_FSKU,
    SPR_COL5,
    SPR_TBLU,
    SPR_TGRN,
    SPR_TRED,
    SPR_SMBT,
    SPR_SMGT,
    SPR_SMRT,
    SPR_HDB1,
    SPR_HDB2,
    SPR_HDB3,
    SPR_HDB4,
    SPR_HDB5,
    SPR_HDB6,
    SPR_POB1,
    SPR_POB2,
    SPR_BRS1,
    SPR_TLMP,
    SPR_TLP2,
    NUMSPRITES,
}

impl Default for SpriteNum {
    fn default() -> Self {
        SpriteNum::SPR_TROO
    }
}

#[derive(Debug, Copy, Clone)]
#[allow(non_camel_case_types)]
pub enum StateNum {
    S_NULL,
    S_LIGHTDONE,
    S_PUNCH,
    S_PUNCHDOWN,
    S_PUNCHUP,
    S_PUNCH1,
    S_PUNCH2,
    S_PUNCH3,
    S_PUNCH4,
    S_PUNCH5,
    S_PISTOL,
    S_PISTOLDOWN,
    S_PISTOLUP,
    S_PISTOL1,
    S_PISTOL2,
    S_PISTOL3,
    S_PISTOL4,
    S_PISTOLFLASH,
    S_SGUN,
    S_SGUNDOWN,
    S_SGUNUP,
    S_SGUN1,
    S_SGUN2,
    S_SGUN3,
    S_SGUN4,
    S_SGUN5,
    S_SGUN6,
    S_SGUN7,
    S_SGUN8,
    S_SGUN9,
    S_SGUNFLASH1,
    S_SGUNFLASH2,
    S_DSGUN,
    S_DSGUNDOWN,
    S_DSGUNUP,
    S_DSGUN1,
    S_DSGUN2,
    S_DSGUN3,
    S_DSGUN4,
    S_DSGUN5,
    S_DSGUN6,
    S_DSGUN7,
    S_DSGUN8,
    S_DSGUN9,
    S_DSGUN10,
    S_DSNR1,
    S_DSNR2,
    S_DSGUNFLASH1,
    S_DSGUNFLASH2,
    S_CHAIN,
    S_CHAINDOWN,
    S_CHAINUP,
    S_CHAIN1,
    S_CHAIN2,
    S_CHAIN3,
    S_CHAINFLASH1,
    S_CHAINFLASH2,
    S_MISSILE,
    S_MISSILEDOWN,
    S_MISSILEUP,
    S_MISSILE1,
    S_MISSILE2,
    S_MISSILE3,
    S_MISSILEFLASH1,
    S_MISSILEFLASH2,
    S_MISSILEFLASH3,
    S_MISSILEFLASH4,
    S_SAW,
    S_SAWB,
    S_SAWDOWN,
    S_SAWUP,
    S_SAW1,
    S_SAW2,
    S_SAW3,
    S_PLASMA,
    S_PLASMADOWN,
    S_PLASMAUP,
    S_PLASMA1,
    S_PLASMA2,
    S_PLASMAFLASH1,
    S_PLASMAFLASH2,
    S_BFG,
    S_BFGDOWN,
    S_BFGUP,
    S_BFG1,
    S_BFG2,
    S_BFG3,
    S_BFG4,
    S_BFGFLASH1,
    S_BFGFLASH2,
    S_BLOOD1,
    S_BLOOD2,
    S_BLOOD3,
    S_PUFF1,
    S_PUFF2,
    S_PUFF3,
    S_PUFF4,
    S_TBALL1,
    S_TBALL2,
    S_TBALLX1,
    S_TBALLX2,
    S_TBALLX3,
    S_RBALL1,
    S_RBALL2,
    S_RBALLX1,
    S_RBALLX2,
    S_RBALLX3,
    S_PLASBALL,
    S_PLASBALL2,
    S_PLASEXP,
    S_PLASEXP2,
    S_PLASEXP3,
    S_PLASEXP4,
    S_PLASEXP5,
    S_ROCKET,
    S_BFGSHOT,
    S_BFGSHOT2,
    S_BFGLAND,
    S_BFGLAND2,
    S_BFGLAND3,
    S_BFGLAND4,
    S_BFGLAND5,
    S_BFGLAND6,
    S_BFGEXP,
    S_BFGEXP2,
    S_BFGEXP3,
    S_BFGEXP4,
    S_EXPLODE1,
    S_EXPLODE2,
    S_EXPLODE3,
    S_TFOG,
    S_TFOG01,
    S_TFOG02,
    S_TFOG2,
    S_TFOG3,
    S_TFOG4,
    S_TFOG5,
    S_TFOG6,
    S_TFOG7,
    S_TFOG8,
    S_TFOG9,
    S_TFOG10,
    S_IFOG,
    S_IFOG01,
    S_IFOG02,
    S_IFOG2,
    S_IFOG3,
    S_IFOG4,
    S_IFOG5,
    S_PLAY,
    S_PLAY_RUN1,
    S_PLAY_RUN2,
    S_PLAY_RUN3,
    S_PLAY_RUN4,
    S_PLAY_ATK1,
    S_PLAY_ATK2,
    S_PLAY_PAIN,
    S_PLAY_PAIN2,
    S_PLAY_DIE1,
    S_PLAY_DIE2,
    S_PLAY_DIE3,
    S_PLAY_DIE4,
    S_PLAY_DIE5,
    S_PLAY_DIE6,
    S_PLAY_DIE7,
    S_PLAY_XDIE1,
    S_PLAY_XDIE2,
    S_PLAY_XDIE3,
    S_PLAY_XDIE4,
    S_PLAY_XDIE5,
    S_PLAY_XDIE6,
    S_PLAY_XDIE7,
    S_PLAY_XDIE8,
    S_PLAY_XDIE9,
    S_POSS_STND,
    S_POSS_STND2,
    S_POSS_RUN1,
    S_POSS_RUN2,
    S_POSS_RUN3,
    S_POSS_RUN4,
    S_POSS_RUN5,
    S_POSS_RUN6,
    S_POSS_RUN7,
    S_POSS_RUN8,
    S_POSS_ATK1,
    S_POSS_ATK2,
    S_POSS_ATK3,
    S_POSS_PAIN,
    S_POSS_PAIN2,
    S_POSS_DIE1,
    S_POSS_DIE2,
    S_POSS_DIE3,
    S_POSS_DIE4,
    S_POSS_DIE5,
    S_POSS_XDIE1,
    S_POSS_XDIE2,
    S_POSS_XDIE3,
    S_POSS_XDIE4,
    S_POSS_XDIE5,
    S_POSS_XDIE6,
    S_POSS_XDIE7,
    S_POSS_XDIE8,
    S_POSS_XDIE9,
    S_POSS_RAISE1,
    S_POSS_RAISE2,
    S_POSS_RAISE3,
    S_POSS_RAISE4,
    S_SPOS_STND,
    S_SPOS_STND2,
    S_SPOS_RUN1,
    S_SPOS_RUN2,
    S_SPOS_RUN3,
    S_SPOS_RUN4,
    S_SPOS_RUN5,
    S_SPOS_RUN6,
    S_SPOS_RUN7,
    S_SPOS_RUN8,
    S_SPOS_ATK1,
    S_SPOS_ATK2,
    S_SPOS_ATK3,
    S_SPOS_PAIN,
    S_SPOS_PAIN2,
    S_SPOS_DIE1,
    S_SPOS_DIE2,
    S_SPOS_DIE3,
    S_SPOS_DIE4,
    S_SPOS_DIE5,
    S_SPOS_XDIE1,
    S_SPOS_XDIE2,
    S_SPOS_XDIE3,
    S_SPOS_XDIE4,
    S_SPOS_XDIE5,
    S_SPOS_XDIE6,
    S_SPOS_XDIE7,
    S_SPOS_XDIE8,
    S_SPOS_XDIE9,
    S_SPOS_RAISE1,
    S_SPOS_RAISE2,
    S_SPOS_RAISE3,
    S_SPOS_RAISE4,
    S_SPOS_RAISE5,
    S_VILE_STND,
    S_VILE_STND2,
    S_VILE_RUN1,
    S_VILE_RUN2,
    S_VILE_RUN3,
    S_VILE_RUN4,
    S_VILE_RUN5,
    S_VILE_RUN6,
    S_VILE_RUN7,
    S_VILE_RUN8,
    S_VILE_RUN9,
    S_VILE_RUN10,
    S_VILE_RUN11,
    S_VILE_RUN12,
    S_VILE_ATK1,
    S_VILE_ATK2,
    S_VILE_ATK3,
    S_VILE_ATK4,
    S_VILE_ATK5,
    S_VILE_ATK6,
    S_VILE_ATK7,
    S_VILE_ATK8,
    S_VILE_ATK9,
    S_VILE_ATK10,
    S_VILE_ATK11,
    S_VILE_HEAL1,
    S_VILE_HEAL2,
    S_VILE_HEAL3,
    S_VILE_PAIN,
    S_VILE_PAIN2,
    S_VILE_DIE1,
    S_VILE_DIE2,
    S_VILE_DIE3,
    S_VILE_DIE4,
    S_VILE_DIE5,
    S_VILE_DIE6,
    S_VILE_DIE7,
    S_VILE_DIE8,
    S_VILE_DIE9,
    S_VILE_DIE10,
    S_FIRE1,
    S_FIRE2,
    S_FIRE3,
    S_FIRE4,
    S_FIRE5,
    S_FIRE6,
    S_FIRE7,
    S_FIRE8,
    S_FIRE9,
    S_FIRE10,
    S_FIRE11,
    S_FIRE12,
    S_FIRE13,
    S_FIRE14,
    S_FIRE15,
    S_FIRE16,
    S_FIRE17,
    S_FIRE18,
    S_FIRE19,
    S_FIRE20,
    S_FIRE21,
    S_FIRE22,
    S_FIRE23,
    S_FIRE24,
    S_FIRE25,
    S_FIRE26,
    S_FIRE27,
    S_FIRE28,
    S_FIRE29,
    S_FIRE30,
    S_SMOKE1,
    S_SMOKE2,
    S_SMOKE3,
    S_SMOKE4,
    S_SMOKE5,
    S_TRACER,
    S_TRACER2,
    S_TRACEEXP1,
    S_TRACEEXP2,
    S_TRACEEXP3,
    S_SKEL_STND,
    S_SKEL_STND2,
    S_SKEL_RUN1,
    S_SKEL_RUN2,
    S_SKEL_RUN3,
    S_SKEL_RUN4,
    S_SKEL_RUN5,
    S_SKEL_RUN6,
    S_SKEL_RUN7,
    S_SKEL_RUN8,
    S_SKEL_RUN9,
    S_SKEL_RUN10,
    S_SKEL_RUN11,
    S_SKEL_RUN12,
    S_SKEL_FIST1,
    S_SKEL_FIST2,
    S_SKEL_FIST3,
    S_SKEL_FIST4,
    S_SKEL_MISS1,
    S_SKEL_MISS2,
    S_SKEL_MISS3,
    S_SKEL_MISS4,
    S_SKEL_PAIN,
    S_SKEL_PAIN2,
    S_SKEL_DIE1,
    S_SKEL_DIE2,
    S_SKEL_DIE3,
    S_SKEL_DIE4,
    S_SKEL_DIE5,
    S_SKEL_DIE6,
    S_SKEL_RAISE1,
    S_SKEL_RAISE2,
    S_SKEL_RAISE3,
    S_SKEL_RAISE4,
    S_SKEL_RAISE5,
    S_SKEL_RAISE6,
    S_FATSHOT1,
    S_FATSHOT2,
    S_FATSHOTX1,
    S_FATSHOTX2,
    S_FATSHOTX3,
    S_FATT_STND,
    S_FATT_STND2,
    S_FATT_RUN1,
    S_FATT_RUN2,
    S_FATT_RUN3,
    S_FATT_RUN4,
    S_FATT_RUN5,
    S_FATT_RUN6,
    S_FATT_RUN7,
    S_FATT_RUN8,
    S_FATT_RUN9,
    S_FATT_RUN10,
    S_FATT_RUN11,
    S_FATT_RUN12,
    S_FATT_ATK1,
    S_FATT_ATK2,
    S_FATT_ATK3,
    S_FATT_ATK4,
    S_FATT_ATK5,
    S_FATT_ATK6,
    S_FATT_ATK7,
    S_FATT_ATK8,
    S_FATT_ATK9,
    S_FATT_ATK10,
    S_FATT_PAIN,
    S_FATT_PAIN2,
    S_FATT_DIE1,
    S_FATT_DIE2,
    S_FATT_DIE3,
    S_FATT_DIE4,
    S_FATT_DIE5,
    S_FATT_DIE6,
    S_FATT_DIE7,
    S_FATT_DIE8,
    S_FATT_DIE9,
    S_FATT_DIE10,
    S_FATT_RAISE1,
    S_FATT_RAISE2,
    S_FATT_RAISE3,
    S_FATT_RAISE4,
    S_FATT_RAISE5,
    S_FATT_RAISE6,
    S_FATT_RAISE7,
    S_FATT_RAISE8,
    S_CPOS_STND,
    S_CPOS_STND2,
    S_CPOS_RUN1,
    S_CPOS_RUN2,
    S_CPOS_RUN3,
    S_CPOS_RUN4,
    S_CPOS_RUN5,
    S_CPOS_RUN6,
    S_CPOS_RUN7,
    S_CPOS_RUN8,
    S_CPOS_ATK1,
    S_CPOS_ATK2,
    S_CPOS_ATK3,
    S_CPOS_ATK4,
    S_CPOS_PAIN,
    S_CPOS_PAIN2,
    S_CPOS_DIE1,
    S_CPOS_DIE2,
    S_CPOS_DIE3,
    S_CPOS_DIE4,
    S_CPOS_DIE5,
    S_CPOS_DIE6,
    S_CPOS_DIE7,
    S_CPOS_XDIE1,
    S_CPOS_XDIE2,
    S_CPOS_XDIE3,
    S_CPOS_XDIE4,
    S_CPOS_XDIE5,
    S_CPOS_XDIE6,
    S_CPOS_RAISE1,
    S_CPOS_RAISE2,
    S_CPOS_RAISE3,
    S_CPOS_RAISE4,
    S_CPOS_RAISE5,
    S_CPOS_RAISE6,
    S_CPOS_RAISE7,
    S_TROO_STND,
    S_TROO_STND2,
    S_TROO_RUN1,
    S_TROO_RUN2,
    S_TROO_RUN3,
    S_TROO_RUN4,
    S_TROO_RUN5,
    S_TROO_RUN6,
    S_TROO_RUN7,
    S_TROO_RUN8,
    S_TROO_ATK1,
    S_TROO_ATK2,
    S_TROO_ATK3,
    S_TROO_PAIN,
    S_TROO_PAIN2,
    S_TROO_DIE1,
    S_TROO_DIE2,
    S_TROO_DIE3,
    S_TROO_DIE4,
    S_TROO_DIE5,
    S_TROO_XDIE1,
    S_TROO_XDIE2,
    S_TROO_XDIE3,
    S_TROO_XDIE4,
    S_TROO_XDIE5,
    S_TROO_XDIE6,
    S_TROO_XDIE7,
    S_TROO_XDIE8,
    S_TROO_RAISE1,
    S_TROO_RAISE2,
    S_TROO_RAISE3,
    S_TROO_RAISE4,
    S_TROO_RAISE5,
    S_SARG_STND,
    S_SARG_STND2,
    S_SARG_RUN1,
    S_SARG_RUN2,
    S_SARG_RUN3,
    S_SARG_RUN4,
    S_SARG_RUN5,
    S_SARG_RUN6,
    S_SARG_RUN7,
    S_SARG_RUN8,
    S_SARG_ATK1,
    S_SARG_ATK2,
    S_SARG_ATK3,
    S_SARG_PAIN,
    S_SARG_PAIN2,
    S_SARG_DIE1,
    S_SARG_DIE2,
    S_SARG_DIE3,
    S_SARG_DIE4,
    S_SARG_DIE5,
    S_SARG_DIE6,
    S_SARG_RAISE1,
    S_SARG_RAISE2,
    S_SARG_RAISE3,
    S_SARG_RAISE4,
    S_SARG_RAISE5,
    S_SARG_RAISE6,
    S_HEAD_STND,
    S_HEAD_RUN1,
    S_HEAD_ATK1,
    S_HEAD_ATK2,
    S_HEAD_ATK3,
    S_HEAD_PAIN,
    S_HEAD_PAIN2,
    S_HEAD_PAIN3,
    S_HEAD_DIE1,
    S_HEAD_DIE2,
    S_HEAD_DIE3,
    S_HEAD_DIE4,
    S_HEAD_DIE5,
    S_HEAD_DIE6,
    S_HEAD_RAISE1,
    S_HEAD_RAISE2,
    S_HEAD_RAISE3,
    S_HEAD_RAISE4,
    S_HEAD_RAISE5,
    S_HEAD_RAISE6,
    S_BRBALL1,
    S_BRBALL2,
    S_BRBALLX1,
    S_BRBALLX2,
    S_BRBALLX3,
    S_BOSS_STND,
    S_BOSS_STND2,
    S_BOSS_RUN1,
    S_BOSS_RUN2,
    S_BOSS_RUN3,
    S_BOSS_RUN4,
    S_BOSS_RUN5,
    S_BOSS_RUN6,
    S_BOSS_RUN7,
    S_BOSS_RUN8,
    S_BOSS_ATK1,
    S_BOSS_ATK2,
    S_BOSS_ATK3,
    S_BOSS_PAIN,
    S_BOSS_PAIN2,
    S_BOSS_DIE1,
    S_BOSS_DIE2,
    S_BOSS_DIE3,
    S_BOSS_DIE4,
    S_BOSS_DIE5,
    S_BOSS_DIE6,
    S_BOSS_DIE7,
    S_BOSS_RAISE1,
    S_BOSS_RAISE2,
    S_BOSS_RAISE3,
    S_BOSS_RAISE4,
    S_BOSS_RAISE5,
    S_BOSS_RAISE6,
    S_BOSS_RAISE7,
    S_BOS2_STND,
    S_BOS2_STND2,
    S_BOS2_RUN1,
    S_BOS2_RUN2,
    S_BOS2_RUN3,
    S_BOS2_RUN4,
    S_BOS2_RUN5,
    S_BOS2_RUN6,
    S_BOS2_RUN7,
    S_BOS2_RUN8,
    S_BOS2_ATK1,
    S_BOS2_ATK2,
    S_BOS2_ATK3,
    S_BOS2_PAIN,
    S_BOS2_PAIN2,
    S_BOS2_DIE1,
    S_BOS2_DIE2,
    S_BOS2_DIE3,
    S_BOS2_DIE4,
    S_BOS2_DIE5,
    S_BOS2_DIE6,
    S_BOS2_DIE7,
    S_BOS2_RAISE1,
    S_BOS2_RAISE2,
    S_BOS2_RAISE3,
    S_BOS2_RAISE4,
    S_BOS2_RAISE5,
    S_BOS2_RAISE6,
    S_BOS2_RAISE7,
    S_SKULL_STND,
    S_SKULL_STND2,
    S_SKULL_RUN1,
    S_SKULL_RUN2,
    S_SKULL_ATK1,
    S_SKULL_ATK2,
    S_SKULL_ATK3,
    S_SKULL_ATK4,
    S_SKULL_PAIN,
    S_SKULL_PAIN2,
    S_SKULL_DIE1,
    S_SKULL_DIE2,
    S_SKULL_DIE3,
    S_SKULL_DIE4,
    S_SKULL_DIE5,
    S_SKULL_DIE6,
    S_SPID_STND,
    S_SPID_STND2,
    S_SPID_RUN1,
    S_SPID_RUN2,
    S_SPID_RUN3,
    S_SPID_RUN4,
    S_SPID_RUN5,
    S_SPID_RUN6,
    S_SPID_RUN7,
    S_SPID_RUN8,
    S_SPID_RUN9,
    S_SPID_RUN10,
    S_SPID_RUN11,
    S_SPID_RUN12,
    S_SPID_ATK1,
    S_SPID_ATK2,
    S_SPID_ATK3,
    S_SPID_ATK4,
    S_SPID_PAIN,
    S_SPID_PAIN2,
    S_SPID_DIE1,
    S_SPID_DIE2,
    S_SPID_DIE3,
    S_SPID_DIE4,
    S_SPID_DIE5,
    S_SPID_DIE6,
    S_SPID_DIE7,
    S_SPID_DIE8,
    S_SPID_DIE9,
    S_SPID_DIE10,
    S_SPID_DIE11,
    S_BSPI_STND,
    S_BSPI_STND2,
    S_BSPI_SIGHT,
    S_BSPI_RUN1,
    S_BSPI_RUN2,
    S_BSPI_RUN3,
    S_BSPI_RUN4,
    S_BSPI_RUN5,
    S_BSPI_RUN6,
    S_BSPI_RUN7,
    S_BSPI_RUN8,
    S_BSPI_RUN9,
    S_BSPI_RUN10,
    S_BSPI_RUN11,
    S_BSPI_RUN12,
    S_BSPI_ATK1,
    S_BSPI_ATK2,
    S_BSPI_ATK3,
    S_BSPI_ATK4,
    S_BSPI_PAIN,
    S_BSPI_PAIN2,
    S_BSPI_DIE1,
    S_BSPI_DIE2,
    S_BSPI_DIE3,
    S_BSPI_DIE4,
    S_BSPI_DIE5,
    S_BSPI_DIE6,
    S_BSPI_DIE7,
    S_BSPI_RAISE1,
    S_BSPI_RAISE2,
    S_BSPI_RAISE3,
    S_BSPI_RAISE4,
    S_BSPI_RAISE5,
    S_BSPI_RAISE6,
    S_BSPI_RAISE7,
    S_ARACH_PLAZ,
    S_ARACH_PLAZ2,
    S_ARACH_PLEX,
    S_ARACH_PLEX2,
    S_ARACH_PLEX3,
    S_ARACH_PLEX4,
    S_ARACH_PLEX5,
    S_CYBER_STND,
    S_CYBER_STND2,
    S_CYBER_RUN1,
    S_CYBER_RUN2,
    S_CYBER_RUN3,
    S_CYBER_RUN4,
    S_CYBER_RUN5,
    S_CYBER_RUN6,
    S_CYBER_RUN7,
    S_CYBER_RUN8,
    S_CYBER_ATK1,
    S_CYBER_ATK2,
    S_CYBER_ATK3,
    S_CYBER_ATK4,
    S_CYBER_ATK5,
    S_CYBER_ATK6,
    S_CYBER_PAIN,
    S_CYBER_DIE1,
    S_CYBER_DIE2,
    S_CYBER_DIE3,
    S_CYBER_DIE4,
    S_CYBER_DIE5,
    S_CYBER_DIE6,
    S_CYBER_DIE7,
    S_CYBER_DIE8,
    S_CYBER_DIE9,
    S_CYBER_DIE10,
    S_PAIN_STND,
    S_PAIN_RUN1,
    S_PAIN_RUN2,
    S_PAIN_RUN3,
    S_PAIN_RUN4,
    S_PAIN_RUN5,
    S_PAIN_RUN6,
    S_PAIN_ATK1,
    S_PAIN_ATK2,
    S_PAIN_ATK3,
    S_PAIN_ATK4,
    S_PAIN_PAIN,
    S_PAIN_PAIN2,
    S_PAIN_DIE1,
    S_PAIN_DIE2,
    S_PAIN_DIE3,
    S_PAIN_DIE4,
    S_PAIN_DIE5,
    S_PAIN_DIE6,
    S_PAIN_RAISE1,
    S_PAIN_RAISE2,
    S_PAIN_RAISE3,
    S_PAIN_RAISE4,
    S_PAIN_RAISE5,
    S_PAIN_RAISE6,
    S_SSWV_STND,
    S_SSWV_STND2,
    S_SSWV_RUN1,
    S_SSWV_RUN2,
    S_SSWV_RUN3,
    S_SSWV_RUN4,
    S_SSWV_RUN5,
    S_SSWV_RUN6,
    S_SSWV_RUN7,
    S_SSWV_RUN8,
    S_SSWV_ATK1,
    S_SSWV_ATK2,
    S_SSWV_ATK3,
    S_SSWV_ATK4,
    S_SSWV_ATK5,
    S_SSWV_ATK6,
    S_SSWV_PAIN,
    S_SSWV_PAIN2,
    S_SSWV_DIE1,
    S_SSWV_DIE2,
    S_SSWV_DIE3,
    S_SSWV_DIE4,
    S_SSWV_DIE5,
    S_SSWV_XDIE1,
    S_SSWV_XDIE2,
    S_SSWV_XDIE3,
    S_SSWV_XDIE4,
    S_SSWV_XDIE5,
    S_SSWV_XDIE6,
    S_SSWV_XDIE7,
    S_SSWV_XDIE8,
    S_SSWV_XDIE9,
    S_SSWV_RAISE1,
    S_SSWV_RAISE2,
    S_SSWV_RAISE3,
    S_SSWV_RAISE4,
    S_SSWV_RAISE5,
    S_KEENSTND,
    S_COMMKEEN,
    S_COMMKEEN2,
    S_COMMKEEN3,
    S_COMMKEEN4,
    S_COMMKEEN5,
    S_COMMKEEN6,
    S_COMMKEEN7,
    S_COMMKEEN8,
    S_COMMKEEN9,
    S_COMMKEEN10,
    S_COMMKEEN11,
    S_COMMKEEN12,
    S_KEENPAIN,
    S_KEENPAIN2,
    S_BRAIN,
    S_BRAIN_PAIN,
    S_BRAIN_DIE1,
    S_BRAIN_DIE2,
    S_BRAIN_DIE3,
    S_BRAIN_DIE4,
    S_BRAINEYE,
    S_BRAINEYESEE,
    S_BRAINEYE1,
    S_SPAWN1,
    S_SPAWN2,
    S_SPAWN3,
    S_SPAWN4,
    S_SPAWNFIRE1,
    S_SPAWNFIRE2,
    S_SPAWNFIRE3,
    S_SPAWNFIRE4,
    S_SPAWNFIRE5,
    S_SPAWNFIRE6,
    S_SPAWNFIRE7,
    S_SPAWNFIRE8,
    S_BRAINEXPLODE1,
    S_BRAINEXPLODE2,
    S_BRAINEXPLODE3,
    S_ARM1,
    S_ARM1A,
    S_ARM2,
    S_ARM2A,
    S_BAR1,
    S_BAR2,
    S_BEXP,
    S_BEXP2,
    S_BEXP3,
    S_BEXP4,
    S_BEXP5,
    S_BBAR1,
    S_BBAR2,
    S_BBAR3,
    S_BON1,
    S_BON1A,
    S_BON1B,
    S_BON1C,
    S_BON1D,
    S_BON1E,
    S_BON2,
    S_BON2A,
    S_BON2B,
    S_BON2C,
    S_BON2D,
    S_BON2E,
    S_BKEY,
    S_BKEY2,
    S_RKEY,
    S_RKEY2,
    S_YKEY,
    S_YKEY2,
    S_BSKULL,
    S_BSKULL2,
    S_RSKULL,
    S_RSKULL2,
    S_YSKULL,
    S_YSKULL2,
    S_STIM,
    S_MEDI,
    S_SOUL,
    S_SOUL2,
    S_SOUL3,
    S_SOUL4,
    S_SOUL5,
    S_SOUL6,
    S_PINV,
    S_PINV2,
    S_PINV3,
    S_PINV4,
    S_PSTR,
    S_PINS,
    S_PINS2,
    S_PINS3,
    S_PINS4,
    S_MEGA,
    S_MEGA2,
    S_MEGA3,
    S_MEGA4,
    S_SUIT,
    S_PMAP,
    S_PMAP2,
    S_PMAP3,
    S_PMAP4,
    S_PMAP5,
    S_PMAP6,
    S_PVIS,
    S_PVIS2,
    S_CLIP,
    S_AMMO,
    S_ROCK,
    S_BROK,
    S_CELL,
    S_CELP,
    S_SHEL,
    S_SBOX,
    S_BPAK,
    S_BFUG,
    S_MGUN,
    S_CSAW,
    S_LAUN,
    S_PLAS,
    S_SHOT,
    S_SHOT2,
    S_COLU,
    S_STALAG,
    S_BLOODYTWITCH,
    S_BLOODYTWITCH2,
    S_BLOODYTWITCH3,
    S_BLOODYTWITCH4,
    S_DEADTORSO,
    S_DEADBOTTOM,
    S_HEADSONSTICK,
    S_GIBS,
    S_HEADONASTICK,
    S_HEADCANDLES,
    S_HEADCANDLES2,
    S_DEADSTICK,
    S_LIVESTICK,
    S_LIVESTICK2,
    S_MEAT2,
    S_MEAT3,
    S_MEAT4,
    S_MEAT5,
    S_STALAGTITE,
    S_TALLGRNCOL,
    S_SHRTGRNCOL,
    S_TALLREDCOL,
    S_SHRTREDCOL,
    S_CANDLESTIK,
    S_CANDELABRA,
    S_SKULLCOL,
    S_TORCHTREE,
    S_BIGTREE,
    S_TECHPILLAR,
    S_EVILEYE,
    S_EVILEYE2,
    S_EVILEYE3,
    S_EVILEYE4,
    S_FLOATSKULL,
    S_FLOATSKULL2,
    S_FLOATSKULL3,
    S_HEARTCOL,
    S_HEARTCOL2,
    S_BLUETORCH,
    S_BLUETORCH2,
    S_BLUETORCH3,
    S_BLUETORCH4,
    S_GREENTORCH,
    S_GREENTORCH2,
    S_GREENTORCH3,
    S_GREENTORCH4,
    S_REDTORCH,
    S_REDTORCH2,
    S_REDTORCH3,
    S_REDTORCH4,
    S_BTORCHSHRT,
    S_BTORCHSHRT2,
    S_BTORCHSHRT3,
    S_BTORCHSHRT4,
    S_GTORCHSHRT,
    S_GTORCHSHRT2,
    S_GTORCHSHRT3,
    S_GTORCHSHRT4,
    S_RTORCHSHRT,
    S_RTORCHSHRT2,
    S_RTORCHSHRT3,
    S_RTORCHSHRT4,
    S_HANGNOGUTS,
    S_HANGBNOBRAIN,
    S_HANGTLOOKDN,
    S_HANGTSKULL,
    S_HANGTLOOKUP,
    S_HANGTNOBRAIN,
    S_COLONGIBS,
    S_SMALLPOOL,
    S_BRAINSTEM,
    S_TECHLAMP,
    S_TECHLAMP2,
    S_TECHLAMP3,
    S_TECHLAMP4,
    S_TECH2LAMP,
    S_TECH2LAMP2,
    S_TECH2LAMP3,
    S_TECH2LAMP4,
    NUMSTATES,
}

#[derive(Debug, Copy, Clone)]
#[allow(non_camel_case_types)]
pub enum MapObjectType {
    MT_PLAYER,
    MT_POSSESSED,
    MT_SHOTGUY,
    MT_VILE,
    MT_FIRE,
    MT_UNDEAD,
    MT_TRACER,
    MT_SMOKE,
    MT_FATSO,
    MT_FATSHOT,
    MT_CHAINGUY,
    MT_TROOP,
    MT_SERGEANT,
    MT_SHADOWS,
    MT_HEAD,
    MT_BRUISER,
    MT_BRUISERSHOT,
    MT_KNIGHT,
    MT_SKULL,
    MT_SPIDER,
    MT_BABY,
    MT_CYBORG,
    MT_PAIN,
    MT_WOLFSS,
    MT_KEEN,
    MT_BOSSBRAIN,
    MT_BOSSSPIT,
    MT_BOSSTARGET,
    MT_SPAWNSHOT,
    MT_SPAWNFIRE,
    MT_BARREL,
    MT_TROOPSHOT,
    MT_HEADSHOT,
    MT_ROCKET,
    MT_PLASMA,
    MT_BFG,
    MT_ARACHPLAZ,
    MT_PUFF,
    MT_BLOOD,
    MT_TFOG,
    MT_IFOG,
    MT_TELEPORTMAN,
    MT_EXTRABFG,
    MT_MISC0,
    MT_MISC1,
    MT_MISC2,
    MT_MISC3,
    MT_MISC4,
    MT_MISC5,
    MT_MISC6,
    MT_MISC7,
    MT_MISC8,
    MT_MISC9,
    MT_MISC10,
    MT_MISC11,
    MT_MISC12,
    MT_INV,
    MT_MISC13,
    MT_INS,
    MT_MISC14,
    MT_MISC15,
    MT_MISC16,
    MT_MEGA,
    MT_CLIP,
    MT_MISC17,
    MT_MISC18,
    MT_MISC19,
    MT_MISC20,
    MT_MISC21,
    MT_MISC22,
    MT_MISC23,
    MT_MISC24,
    MT_MISC25,
    MT_CHAINGUN,
    MT_MISC26,
    MT_MISC27,
    MT_MISC28,
    MT_SHOTGUN,
    MT_SUPERSHOTGUN,
    MT_MISC29,
    MT_MISC30,
    MT_MISC31,
    MT_MISC32,
    MT_MISC33,
    MT_MISC34,
    MT_MISC35,
    MT_MISC36,
    MT_MISC37,
    MT_MISC38,
    MT_MISC39,
    MT_MISC40,
    MT_MISC41,
    MT_MISC42,
    MT_MISC43,
    MT_MISC44,
    MT_MISC45,
    MT_MISC46,
    MT_MISC47,
    MT_MISC48,
    MT_MISC49,
    MT_MISC50,
    MT_MISC51,
    MT_MISC52,
    MT_MISC53,
    MT_MISC54,
    MT_MISC55,
    MT_MISC56,
    MT_MISC57,
    MT_MISC58,
    MT_MISC59,
    MT_MISC60,
    MT_MISC61,
    MT_MISC62,
    MT_MISC63,
    MT_MISC64,
    MT_MISC65,
    MT_MISC66,
    MT_MISC67,
    MT_MISC68,
    MT_MISC69,
    MT_MISC70,
    MT_MISC71,
    MT_MISC72,
    MT_MISC73,
    MT_MISC74,
    MT_MISC75,
    MT_MISC76,
    MT_MISC77,
    MT_MISC78,
    MT_MISC79,
    MT_MISC80,
    MT_MISC81,
    MT_MISC82,
    MT_MISC83,
    MT_MISC84,
    MT_MISC85,
    MT_MISC86,
    NUMMOBJTYPES,
}

#[derive(Debug, Copy, Clone)]
pub struct MapObjectInfo {
    pub doomednum:    i32,
    pub spawnstate:   StateNum,
    pub spawnhealth:  i32,
    pub seestate:     StateNum,
    pub seesound:     SfxEnum,
    pub reactiontime: i32,
    pub attacksound:  SfxEnum,
    pub painstate:    StateNum,
    pub painchance:   i32,
    pub painsound:    SfxEnum,
    pub meleestate:   StateNum,
    pub missilestate: StateNum,
    pub deathstate:   StateNum,
    pub xdeathstate:  StateNum,
    pub deathsound:   SfxEnum,
    pub speed:        f32,
    pub radius:       f32,
    pub height:       f32,
    pub mass:         i32,
    pub damage:       i32,
    pub activesound:  SfxEnum,
    pub flags:        u32,
    pub raisestate:   StateNum,
}

impl MapObjectInfo {
    fn new(
        doomednum: i32,
        spawnstate: StateNum,
        spawnhealth: i32,
        seestate: StateNum,
        seesound: SfxEnum,
        reactiontime: i32,
        attacksound: SfxEnum,
        painstate: StateNum,
        painchance: i32,
        painsound: SfxEnum,
        meleestate: StateNum,
        missilestate: StateNum,
        deathstate: StateNum,
        xdeathstate: StateNum,
        deathsound: SfxEnum,
        speed: f32,
        radius: f32,
        height: f32,
        mass: i32,
        damage: i32,
        activesound: SfxEnum,
        flags: u32,
        raisestate: StateNum,
    ) -> Self {
        Self {
            doomednum,
            spawnstate,
            spawnhealth,
            seestate,
            seesound,
            reactiontime,
            attacksound,
            painstate,
            painchance,
            painsound,
            meleestate,
            missilestate,
            deathstate,
            xdeathstate,
            deathsound,
            speed,
            radius,
            height,
            mass,
            damage,
            activesound,
            flags,
            raisestate,
        }
    }
}

pub struct State {
    /// Sprite to use
    pub sprite:     SpriteNum,
    /// The frame within this sprite to show for the state
    pub frame:      i32,
    /// How many tics this state takes. On nightmare it is shifted >> 1
    pub tics:       i32,
    // void (*action) (): i32,
    /// An action callback to run on this state
    action:         ActionF,
    /// The state that should come after this. Can be looped.
    pub next_state: StateNum,
    /// Don't know, Doom seems to set all to zero
    pub misc1:      i32,
    /// Don't know, Doom seems to set all to zero
    pub misc2:      i32,
}

impl State {
    pub fn new(
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

//pub const STATESJ: [State; NUM_CATEGORIES] = [
/// The States are an immutable set of predefined parameters, which
pub const STATESJ: [State; 1] = [
    State::new(
        SpriteNum::SPR_TROO,
        0,
        -1,
        ActionF::actionf_v,
        StateNum::S_NULL,
        0,
        0,
    ), // S_NULL
];

const FRACBITS: i32 = 16;
const FRACUNIT: f32 = (1 << FRACBITS) as f32;

/// This variable exists only to help create the mobs array
const NUM_CATEGORIES: usize = MapObjectType::NUMMOBJTYPES as usize;

pub const MOBJINFO: [MapObjectInfo; NUM_CATEGORIES] = [
    // MT_PLAYER
    MapObjectInfo::new(
        -1,                     // doomednum
        StateNum::S_PLAY,       // spawnstate
        100,                    // spawnhealth
        StateNum::S_PLAY_RUN1,  // seestate
        SfxEnum::sfx_None,      // seesound
        0,                      // reactiontime
        SfxEnum::sfx_None,      // attacksound
        StateNum::S_PLAY_PAIN,  // painstate
        255,                    // painchance
        SfxEnum::sfx_plpain,    // painsound
        StateNum::S_NULL,       // meleestate
        StateNum::S_PLAY_ATK1,  // missilestate
        StateNum::S_PLAY_DIE1,  // deathstate
        StateNum::S_PLAY_XDIE1, // xdeathstate
        SfxEnum::sfx_pldeth,    // deathsound
        0.0,                    // speed
        16.0 * FRACUNIT,        // radius
        56.0 * FRACUNIT,        // height
        100,                    // mass
        0,                      // damage
        SfxEnum::sfx_None,      // activesound
        MapObjectFlag::MF_SOLID as u32
            | MapObjectFlag::MF_SHOOTABLE as u32
            | MapObjectFlag::MF_DROPOFF as u32
            | MapObjectFlag::MF_PICKUP as u32
            | MapObjectFlag::MF_NOTDMATCH as u32, // flags
        StateNum::S_NULL,       // raisestate
    ),
    MapObjectInfo::new(
        // MT_POSSESSED
        3004,                   // doomednum
        StateNum::S_POSS_STND,  // spawnstate
        20,                     // spawnhealth
        StateNum::S_POSS_RUN1,  // seestate
        SfxEnum::sfx_posit1,    // seesound
        8,                      // reactiontime
        SfxEnum::sfx_pistol,    // attacksound
        StateNum::S_POSS_PAIN,  // painstate
        200,                    // painchance
        SfxEnum::sfx_popain,    // painsound
        StateNum::S_NULL,       // meleestate
        StateNum::S_POSS_ATK1,  // missilestate
        StateNum::S_POSS_DIE1,  // deathstate
        StateNum::S_POSS_XDIE1, // xdeathstate
        SfxEnum::sfx_podth1,    // deathsound
        8.0,                    // speed
        20.0 * FRACUNIT,        // radius
        56.0 * FRACUNIT,        // height
        100,                    // mass
        0,                      // damage
        SfxEnum::sfx_posact,    // activesound
        MapObjectFlag::MF_SOLID as u32
            | MapObjectFlag::MF_SHOOTABLE as u32
            | MapObjectFlag::MF_COUNTKILL as u32, // flags
        StateNum::S_POSS_RAISE1, // raisestate
    ),
    MapObjectInfo::new(
        // MT_SHOTGUY
        9,                      // doomednum
        StateNum::S_POSS_STND,  // spawnstate
        30,                     // spawnhealth
        StateNum::S_POSS_RUN1,  // seestate
        SfxEnum::sfx_posit2,    // seesound
        8,                      // reactiontime
        SfxEnum::sfx_None,      // attacksound
        StateNum::S_POSS_PAIN,  // painstate
        170,                    // painchance
        SfxEnum::sfx_popain,    // painsound
        StateNum::S_NULL,       // meleestate
        StateNum::S_POSS_ATK1,  // missilestate
        StateNum::S_POSS_DIE1,  // deathstate
        StateNum::S_POSS_XDIE1, // xdeathstate
        SfxEnum::sfx_podth2,    // deathsound
        8.0,                    // speed
        20.0 * FRACUNIT,        // radius
        56.0 * FRACUNIT,        // height
        100,                    // mass
        0,                      // damage
        SfxEnum::sfx_posact,    // activesound
        MapObjectFlag::MF_SOLID as u32
            | MapObjectFlag::MF_SHOOTABLE as u32
            | MapObjectFlag::MF_COUNTKILL as u32, // flags
        StateNum::S_POSS_RAISE1, // raisestate
    ),
    MapObjectInfo::new(
        // MT_VILE
        64,                    // doomednum
        StateNum::S_VILE_STND, // spawnstate
        700,                   // spawnhealth
        StateNum::S_VILE_RUN1, // seestate
        SfxEnum::sfx_vilsit,   // seesound
        8,                     // reactiontime
        SfxEnum::sfx_None,     // attacksound
        StateNum::S_VILE_PAIN, // painstate
        10,                    // painchance
        SfxEnum::sfx_vipain,   // painsound
        StateNum::S_NULL,      // meleestate
        StateNum::S_VILE_ATK1, // missilestate
        StateNum::S_VILE_DIE1, // deathstate
        StateNum::S_NULL,      // xdeathstate
        SfxEnum::sfx_vildth,   // deathsound
        15.0,                  // speed
        20.0 * FRACUNIT,       // radius
        56.0 * FRACUNIT,       // height
        500,                   // mass
        0,                     // damage
        SfxEnum::sfx_vilact,   // activesound
        MapObjectFlag::MF_SOLID as u32
            | MapObjectFlag::MF_SHOOTABLE as u32
            | MapObjectFlag::MF_COUNTKILL as u32, // flags
        StateNum::S_NULL,      // raisestate
    ),
    MapObjectInfo::new(
        // MT_FIRE
        -1,                // doomednum
        StateNum::S_FIRE1, // spawnstate
        1000,              // spawnhealth
        StateNum::S_NULL,  // seestate
        SfxEnum::sfx_None, // seesound
        8,                 // reactiontime
        SfxEnum::sfx_None, // attacksound
        StateNum::S_NULL,  // painstate
        0,                 // painchance
        SfxEnum::sfx_None, // painsound
        StateNum::S_NULL,  // meleestate
        StateNum::S_NULL,  // missilestate
        StateNum::S_NULL,  // deathstate
        StateNum::S_NULL,  // xdeathstate
        SfxEnum::sfx_None, // deathsound
        0.0,               // speed
        20.0 * FRACUNIT,   // radius
        16.0 * FRACUNIT,   // height
        100,               // mass
        0,                 // damage
        SfxEnum::sfx_None, // activesound
        MapObjectFlag::MF_NOBLOCKMAP as u32
            | MapObjectFlag::MF_NOGRAVITY as u32, // flags
        StateNum::S_NULL,  // raisestate
    ),
    MapObjectInfo::new(
        // MT_UNDEAD
        66,                     // doomednum
        StateNum::S_SKEL_STND,  // spawnstate
        300,                    // spawnhealth
        StateNum::S_SKEL_RUN1,  // seestate
        SfxEnum::sfx_skesit,    // seesound
        8,                      // reactiontime
        SfxEnum::sfx_None,      // attacksound
        StateNum::S_SKEL_PAIN,  // painstate
        100,                    // painchance
        SfxEnum::sfx_popain,    // painsound
        StateNum::S_SKEL_FIST1, // meleestate
        StateNum::S_SKEL_MISS1, // missilestate
        StateNum::S_SKEL_DIE1,  // deathstate
        StateNum::S_NULL,       // xdeathstate
        SfxEnum::sfx_skedth,    // deathsound
        10.0,                   // speed
        20.0 * FRACUNIT,        // radius
        56.0 * FRACUNIT,        // height
        500,                    // mass
        0,                      // damage
        SfxEnum::sfx_skeact,    // activesound
        MapObjectFlag::MF_SOLID as u32
            | MapObjectFlag::MF_SHOOTABLE as u32
            | MapObjectFlag::MF_COUNTKILL as u32, // flags
        StateNum::S_SKEL_RAISE1, // raisestate
    ),
    MapObjectInfo::new(
        // MT_TRACER
        -1,                    // doomednum
        StateNum::S_TRACER,    // spawnstate
        1000,                  // spawnhealth
        StateNum::S_NULL,      // seestate
        SfxEnum::sfx_skeatk,   // seesound
        8,                     // reactiontime
        SfxEnum::sfx_None,     // attacksound
        StateNum::S_NULL,      // painstate
        0,                     // painchance
        SfxEnum::sfx_None,     // painsound
        StateNum::S_NULL,      // meleestate
        StateNum::S_NULL,      // missilestate
        StateNum::S_TRACEEXP1, // deathstate
        StateNum::S_NULL,      // xdeathstate
        SfxEnum::sfx_barexp,   // deathsound
        10.0 * FRACUNIT,       // speed
        11.0 * FRACUNIT,       // radius
        8.0 * FRACUNIT,        // height
        100,                   // mass
        10,                    // damage
        SfxEnum::sfx_None,     // activesound
        MapObjectFlag::MF_NOBLOCKMAP as u32
            | MapObjectFlag::MF_MISSILE as u32
            | MapObjectFlag::MF_DROPOFF as u32
            | MapObjectFlag::MF_NOGRAVITY as u32, // flags
        StateNum::S_NULL,      // raisestate
    ),
    MapObjectInfo::new(
        // MT_SMOKE
        -1,                 // doomednum
        StateNum::S_SMOKE1, // spawnstate
        1000,               // spawnhealth
        StateNum::S_NULL,   // seestate
        SfxEnum::sfx_None,  // seesound
        8,                  // reactiontime
        SfxEnum::sfx_None,  // attacksound
        StateNum::S_NULL,   // painstate
        0,                  // painchance
        SfxEnum::sfx_None,  // painsound
        StateNum::S_NULL,   // meleestate
        StateNum::S_NULL,   // missilestate
        StateNum::S_NULL,   // deathstate
        StateNum::S_NULL,   // xdeathstate
        SfxEnum::sfx_None,  // deathsound
        0.0,                // speed
        20.0 * FRACUNIT,    // radius
        16.0 * FRACUNIT,    // height
        100,                // mass
        0,                  // damage
        SfxEnum::sfx_None,  // activesound
        MapObjectFlag::MF_NOBLOCKMAP as u32
            | MapObjectFlag::MF_NOGRAVITY as u32, // flags
        StateNum::S_NULL,   // raisestate
    ),
    MapObjectInfo::new(
        // MT_FATSO
        67,                    // doomednum
        StateNum::S_FATT_STND, // spawnstate
        600,                   // spawnhealth
        StateNum::S_FATT_RUN1, // seestate
        SfxEnum::sfx_mansit,   // seesound
        8,                     // reactiontime
        SfxEnum::sfx_None,     // attacksound
        StateNum::S_FATT_PAIN, // painstate
        80,                    // painchance
        SfxEnum::sfx_mnpain,   // painsound
        StateNum::S_NULL,      // meleestate
        StateNum::S_FATT_ATK1, // missilestate
        StateNum::S_FATT_DIE1, // deathstate
        StateNum::S_NULL,      // xdeathstate
        SfxEnum::sfx_mandth,   // deathsound
        8.0,                   // speed
        48.0 * FRACUNIT,       // radius
        64.0 * FRACUNIT,       // height
        1000,                  // mass
        0,                     // damage
        SfxEnum::sfx_posact,   // activesound
        MapObjectFlag::MF_SOLID as u32
            | MapObjectFlag::MF_SHOOTABLE as u32
            | MapObjectFlag::MF_COUNTKILL as u32, // flags
        StateNum::S_FATT_RAISE1, // raisestate
    ),
    MapObjectInfo::new(
        // MT_FATSHOT
        -1,                    // doomednum
        StateNum::S_FATSHOT1,  // spawnstate
        1000,                  // spawnhealth
        StateNum::S_NULL,      // seestate
        SfxEnum::sfx_firsht,   // seesound
        8,                     // reactiontime
        SfxEnum::sfx_None,     // attacksound
        StateNum::S_NULL,      // painstate
        0,                     // painchance
        SfxEnum::sfx_None,     // painsound
        StateNum::S_NULL,      // meleestate
        StateNum::S_NULL,      // missilestate
        StateNum::S_FATSHOTX1, // deathstate
        StateNum::S_NULL,      // xdeathstate
        SfxEnum::sfx_firxpl,   // deathsound
        20.0 * FRACUNIT,       // speed
        6.0 * FRACUNIT,        // radius
        8.0 * FRACUNIT,        // height
        100,                   // mass
        8,                     // damage
        SfxEnum::sfx_None,     // activesound
        MapObjectFlag::MF_NOBLOCKMAP as u32
            | MapObjectFlag::MF_MISSILE as u32
            | MapObjectFlag::MF_DROPOFF as u32
            | MapObjectFlag::MF_NOGRAVITY as u32, // flags
        StateNum::S_NULL,      // raisestate
    ),
    MapObjectInfo::new(
        // MT_CHAINGUY
        65,                     // doomednum
        StateNum::S_POSS_STND,  // spawnstate
        70,                     // spawnhealth
        StateNum::S_POSS_RUN1,  // seestate
        SfxEnum::sfx_posit2,    // seesound
        8,                      // reactiontime
        SfxEnum::sfx_None,      // attacksound
        StateNum::S_POSS_PAIN,  // painstate
        170,                    // painchance
        SfxEnum::sfx_popain,    // painsound
        StateNum::S_NULL,       // meleestate
        StateNum::S_POSS_ATK1,  // missilestate
        StateNum::S_POSS_DIE1,  // deathstate
        StateNum::S_POSS_XDIE1, // xdeathstate
        SfxEnum::sfx_podth2,    // deathsound
        8.0,                    // speed
        20.0 * FRACUNIT,        // radius
        56.0 * FRACUNIT,        // height
        100,                    // mass
        0,                      // damage
        SfxEnum::sfx_posact,    // activesound
        MapObjectFlag::MF_SOLID as u32
            | MapObjectFlag::MF_SHOOTABLE as u32
            | MapObjectFlag::MF_COUNTKILL as u32, // flags
        StateNum::S_POSS_RAISE1, // raisestate
    ),
    MapObjectInfo::new(
        // MT_TROOP
        3001,                   // doomednum
        StateNum::S_TROO_STND,  // spawnstate
        60,                     // spawnhealth
        StateNum::S_TROO_RUN1,  // seestate
        SfxEnum::sfx_bgsit1,    // seesound
        8,                      // reactiontime
        SfxEnum::sfx_None,      // attacksound
        StateNum::S_TROO_PAIN,  // painstate
        200,                    // painchance
        SfxEnum::sfx_popain,    // painsound
        StateNum::S_TROO_ATK1,  // meleestate
        StateNum::S_TROO_ATK1,  // missilestate
        StateNum::S_TROO_DIE1,  // deathstate
        StateNum::S_TROO_XDIE1, // xdeathstate
        SfxEnum::sfx_bgdth1,    // deathsound
        8.0,                    // speed
        20.0 * FRACUNIT,        // radius
        56.0 * FRACUNIT,        // height
        100,                    // mass
        0,                      // damage
        SfxEnum::sfx_bgact,     // activesound
        MapObjectFlag::MF_SOLID as u32
            | MapObjectFlag::MF_SHOOTABLE as u32
            | MapObjectFlag::MF_COUNTKILL as u32, // flags
        StateNum::S_TROO_RAISE1, // raisestate
    ),
    MapObjectInfo::new(
        // MT_SERGEANT
        3002,                  // doomednum
        StateNum::S_SARG_STND, // spawnstate
        150,                   // spawnhealth
        StateNum::S_SARG_RUN1, // seestate
        SfxEnum::sfx_sgtsit,   // seesound
        8,                     // reactiontime
        SfxEnum::sfx_sgtatk,   // attacksound
        StateNum::S_SARG_PAIN, // painstate
        180,                   // painchance
        SfxEnum::sfx_dmpain,   // painsound
        StateNum::S_SARG_ATK1, // meleestate
        StateNum::S_NULL,      // missilestate
        StateNum::S_SARG_DIE1, // deathstate
        StateNum::S_NULL,      // xdeathstate
        SfxEnum::sfx_sgtdth,   // deathsound
        10.0,                  // speed
        30.0 * FRACUNIT,       // radius
        56.0 * FRACUNIT,       // height
        400,                   // mass
        0,                     // damage
        SfxEnum::sfx_dmact,    // activesound
        MapObjectFlag::MF_SOLID as u32
            | MapObjectFlag::MF_SHOOTABLE as u32
            | MapObjectFlag::MF_COUNTKILL as u32, // flags
        StateNum::S_SARG_RAISE1, // raisestate
    ),
    MapObjectInfo::new(
        // MT_SHADOWS
        58,                    // doomednum
        StateNum::S_SARG_STND, // spawnstate
        150,                   // spawnhealth
        StateNum::S_SARG_RUN1, // seestate
        SfxEnum::sfx_sgtsit,   // seesound
        8,                     // reactiontime
        SfxEnum::sfx_sgtatk,   // attacksound
        StateNum::S_SARG_PAIN, // painstate
        180,                   // painchance
        SfxEnum::sfx_dmpain,   // painsound
        StateNum::S_SARG_ATK1, // meleestate
        StateNum::S_NULL,      // missilestate
        StateNum::S_SARG_DIE1, // deathstate
        StateNum::S_NULL,      // xdeathstate
        SfxEnum::sfx_sgtdth,   // deathsound
        10.0,                  // speed
        30.0 * FRACUNIT,       // radius
        56.0 * FRACUNIT,       // height
        400,                   // mass
        0,                     // damage
        SfxEnum::sfx_dmact,    // activesound
        MapObjectFlag::MF_SOLID as u32
            | MapObjectFlag::MF_SHOOTABLE as u32
            | MapObjectFlag::MF_SHADOW as u32
            | MapObjectFlag::MF_COUNTKILL as u32, // flags
        StateNum::S_SARG_RAISE1, // raisestate
    ),
    MapObjectInfo::new(
        // MT_HEAD
        3005,                  // doomednum
        StateNum::S_HEAD_STND, // spawnstate
        400,                   // spawnhealth
        StateNum::S_HEAD_RUN1, // seestate
        SfxEnum::sfx_cacsit,   // seesound
        8,                     // reactiontime
        SfxEnum::sfx_None,     // attacksound
        StateNum::S_HEAD_PAIN, // painstate
        128,                   // painchance
        SfxEnum::sfx_dmpain,   // painsound
        StateNum::S_NULL,      // meleestate
        StateNum::S_HEAD_ATK1, // missilestate
        StateNum::S_HEAD_DIE1, // deathstate
        StateNum::S_NULL,      // xdeathstate
        SfxEnum::sfx_cacdth,   // deathsound
        8.0,                   // speed
        31.0 * FRACUNIT,       // radius
        56.0 * FRACUNIT,       // height
        400,                   // mass
        0,                     // damage
        SfxEnum::sfx_dmact,    // activesound
        MapObjectFlag::MF_SOLID as u32
            | MapObjectFlag::MF_SHOOTABLE as u32
            | MapObjectFlag::MF_FLOAT as u32
            | MapObjectFlag::MF_NOGRAVITY as u32
            | MapObjectFlag::MF_COUNTKILL as u32, // flags
        StateNum::S_HEAD_RAISE1, // raisestate
    ),
    MapObjectInfo::new(
        // MT_BRUISER
        3003,                  // doomednum
        StateNum::S_BOSS_STND, // spawnstate
        1000,                  // spawnhealth
        StateNum::S_BOSS_RUN1, // seestate
        SfxEnum::sfx_brssit,   // seesound
        8,                     // reactiontime
        SfxEnum::sfx_None,     // attacksound
        StateNum::S_BOSS_PAIN, // painstate
        50,                    // painchance
        SfxEnum::sfx_dmpain,   // painsound
        StateNum::S_BOSS_ATK1, // meleestate
        StateNum::S_BOSS_ATK1, // missilestate
        StateNum::S_BOSS_DIE1, // deathstate
        StateNum::S_NULL,      // xdeathstate
        SfxEnum::sfx_brsdth,   // deathsound
        8.0,                   // speed
        24.0 * FRACUNIT,       // radius
        64.0 * FRACUNIT,       // height
        1000,                  // mass
        0,                     // damage
        SfxEnum::sfx_dmact,    // activesound
        MapObjectFlag::MF_SOLID as u32
            | MapObjectFlag::MF_SHOOTABLE as u32
            | MapObjectFlag::MF_COUNTKILL as u32, // flags
        StateNum::S_BOSS_RAISE1, // raisestate
    ),
    MapObjectInfo::new(
        // MT_BRUISERSHOT
        -1,                   // doomednum
        StateNum::S_BRBALL1,  // spawnstate
        1000,                 // spawnhealth
        StateNum::S_NULL,     // seestate
        SfxEnum::sfx_firsht,  // seesound
        8,                    // reactiontime
        SfxEnum::sfx_None,    // attacksound
        StateNum::S_NULL,     // painstate
        0,                    // painchance
        SfxEnum::sfx_None,    // painsound
        StateNum::S_NULL,     // meleestate
        StateNum::S_NULL,     // missilestate
        StateNum::S_BRBALLX1, // deathstate
        StateNum::S_NULL,     // xdeathstate
        SfxEnum::sfx_firxpl,  // deathsound
        15.0 * FRACUNIT,      // speed
        6.0 * FRACUNIT,       // radius
        8.0 * FRACUNIT,       // height
        100,                  // mass
        8,                    // damage
        SfxEnum::sfx_None,    // activesound
        MapObjectFlag::MF_NOBLOCKMAP as u32
            | MapObjectFlag::MF_MISSILE as u32
            | MapObjectFlag::MF_DROPOFF as u32
            | MapObjectFlag::MF_NOGRAVITY as u32, // flags
        StateNum::S_NULL,     // raisestate
    ),
    MapObjectInfo::new(
        // MT_KNIGHT
        69,                    // doomednum
        StateNum::S_BOS2_STND, // spawnstate
        500,                   // spawnhealth
        StateNum::S_BOS2_RUN1, // seestate
        SfxEnum::sfx_kntsit,   // seesound
        8,                     // reactiontime
        SfxEnum::sfx_None,     // attacksound
        StateNum::S_BOS2_PAIN, // painstate
        50,                    // painchance
        SfxEnum::sfx_dmpain,   // painsound
        StateNum::S_BOS2_ATK1, // meleestate
        StateNum::S_BOS2_ATK1, // missilestate
        StateNum::S_BOS2_DIE1, // deathstate
        StateNum::S_NULL,      // xdeathstate
        SfxEnum::sfx_kntdth,   // deathsound
        8.0,                   // speed
        24.0 * FRACUNIT,       // radius
        64.0 * FRACUNIT,       // height
        1000,                  // mass
        0,                     // damage
        SfxEnum::sfx_dmact,    // activesound
        MapObjectFlag::MF_SOLID as u32
            | MapObjectFlag::MF_SHOOTABLE as u32
            | MapObjectFlag::MF_COUNTKILL as u32, // flags
        StateNum::S_BOS2_RAISE1, // raisestate
    ),
    MapObjectInfo::new(
        // MT_SKULL
        3006,                   // doomednum
        StateNum::S_SKULL_STND, // spawnstate
        100,                    // spawnhealth
        StateNum::S_SKULL_RUN1, // seestate
        SfxEnum::sfx_None,      // seesound
        8,                      // reactiontime
        SfxEnum::sfx_sklatk,    // attacksound
        StateNum::S_SKULL_PAIN, // painstate
        256,                    // painchance
        SfxEnum::sfx_dmpain,    // painsound
        StateNum::S_NULL,       // meleestate
        StateNum::S_SKULL_ATK1, // missilestate
        StateNum::S_SKULL_DIE1, // deathstate
        StateNum::S_NULL,       // xdeathstate
        SfxEnum::sfx_firxpl,    // deathsound
        8.0,                    // speed
        16.0 * FRACUNIT,        // radius
        56.0 * FRACUNIT,        // height
        50,                     // mass
        3,                      // damage
        SfxEnum::sfx_dmact,     // activesound
        MapObjectFlag::MF_SOLID as u32
            | MapObjectFlag::MF_SHOOTABLE as u32
            | MapObjectFlag::MF_FLOAT as u32
            | MapObjectFlag::MF_NOGRAVITY as u32, // flags
        StateNum::S_NULL,       // raisestate
    ),
    MapObjectInfo::new(
        // MT_SPIDER
        7,                     // doomednum
        StateNum::S_SPID_STND, // spawnstate
        3000,                  // spawnhealth
        StateNum::S_SPID_RUN1, // seestate
        SfxEnum::sfx_spisit,   // seesound
        8,                     // reactiontime
        SfxEnum::sfx_shotgn,   // attacksound
        StateNum::S_SPID_PAIN, // painstate
        40,                    // painchance
        SfxEnum::sfx_dmpain,   // painsound
        StateNum::S_NULL,      // meleestate
        StateNum::S_SPID_ATK1, // missilestate
        StateNum::S_SPID_DIE1, // deathstate
        StateNum::S_NULL,      // xdeathstate
        SfxEnum::sfx_spidth,   // deathsound
        12.0,                  // speed
        128.0 * FRACUNIT,      // radius
        100.0 * FRACUNIT,      // height
        1000,                  // mass
        0,                     // damage
        SfxEnum::sfx_dmact,    // activesound
        MapObjectFlag::MF_SOLID as u32
            | MapObjectFlag::MF_SHOOTABLE as u32
            | MapObjectFlag::MF_COUNTKILL as u32, // flags
        StateNum::S_NULL,      // raisestate
    ),
    MapObjectInfo::new(
        // MT_BABY
        68,                     // doomednum
        StateNum::S_BSPI_STND,  // spawnstate
        500,                    // spawnhealth
        StateNum::S_BSPI_SIGHT, // seestate
        SfxEnum::sfx_bspsit,    // seesound
        8,                      // reactiontime
        SfxEnum::sfx_None,      // attacksound
        StateNum::S_BSPI_PAIN,  // painstate
        128,                    // painchance
        SfxEnum::sfx_dmpain,    // painsound
        StateNum::S_NULL,       // meleestate
        StateNum::S_BSPI_ATK1,  // missilestate
        StateNum::S_BSPI_DIE1,  // deathstate
        StateNum::S_NULL,       // xdeathstate
        SfxEnum::sfx_bspdth,    // deathsound
        12.0,                   // speed
        64.0 * FRACUNIT,        // radius
        64.0 * FRACUNIT,        // height
        600,                    // mass
        0,                      // damage
        SfxEnum::sfx_bspact,    // activesound
        MapObjectFlag::MF_SOLID as u32
            | MapObjectFlag::MF_SHOOTABLE as u32
            | MapObjectFlag::MF_COUNTKILL as u32, // flags
        StateNum::S_BSPI_RAISE1, // raisestate
    ),
    MapObjectInfo::new(
        // MT_CYBORG
        16,                     // doomednum
        StateNum::S_CYBER_STND, // spawnstate
        4000,                   // spawnhealth
        StateNum::S_CYBER_RUN1, // seestate
        SfxEnum::sfx_cybsit,    // seesound
        8,                      // reactiontime
        SfxEnum::sfx_None,      // attacksound
        StateNum::S_CYBER_PAIN, // painstate
        20,                     // painchance
        SfxEnum::sfx_dmpain,    // painsound
        StateNum::S_NULL,       // meleestate
        StateNum::S_CYBER_ATK1, // missilestate
        StateNum::S_CYBER_DIE1, // deathstate
        StateNum::S_NULL,       // xdeathstate
        SfxEnum::sfx_cybdth,    // deathsound
        16.0,                   // speed
        40.0 * FRACUNIT,        // radius
        110.0 * FRACUNIT,       // height
        1000,                   // mass
        0,                      // damage
        SfxEnum::sfx_dmact,     // activesound
        MapObjectFlag::MF_SOLID as u32
            | MapObjectFlag::MF_SHOOTABLE as u32
            | MapObjectFlag::MF_COUNTKILL as u32, // flags
        StateNum::S_NULL,       // raisestate
    ),
    MapObjectInfo::new(
        // MT_PAIN
        71,                    // doomednum
        StateNum::S_PAIN_STND, // spawnstate
        400,                   // spawnhealth
        StateNum::S_PAIN_RUN1, // seestate
        SfxEnum::sfx_pesit,    // seesound
        8,                     // reactiontime
        SfxEnum::sfx_None,     // attacksound
        StateNum::S_PAIN_PAIN, // painstate
        128,                   // painchance
        SfxEnum::sfx_pepain,   // painsound
        StateNum::S_NULL,      // meleestate
        StateNum::S_PAIN_ATK1, // missilestate
        StateNum::S_PAIN_DIE1, // deathstate
        StateNum::S_NULL,      // xdeathstate
        SfxEnum::sfx_pedth,    // deathsound
        8.0,                   // speed
        31.0 * FRACUNIT,       // radius
        56.0 * FRACUNIT,       // height
        400,                   // mass
        0,                     // damage
        SfxEnum::sfx_dmact,    // activesound
        MapObjectFlag::MF_SOLID as u32
            | MapObjectFlag::MF_SHOOTABLE as u32
            | MapObjectFlag::MF_FLOAT as u32
            | MapObjectFlag::MF_NOGRAVITY as u32
            | MapObjectFlag::MF_COUNTKILL as u32, // flags
        StateNum::S_PAIN_RAISE1, // raisestate
    ),
    MapObjectInfo::new(
        // MT_WOLFSS
        84,                     // doomednum
        StateNum::S_SSWV_STND,  // spawnstate
        50,                     // spawnhealth
        StateNum::S_SSWV_RUN1,  // seestate
        SfxEnum::sfx_sssit,     // seesound
        8,                      // reactiontime
        SfxEnum::sfx_None,      // attacksound
        StateNum::S_SSWV_PAIN,  // painstate
        170,                    // painchance
        SfxEnum::sfx_popain,    // painsound
        StateNum::S_NULL,       // meleestate
        StateNum::S_SSWV_ATK1,  // missilestate
        StateNum::S_SSWV_DIE1,  // deathstate
        StateNum::S_SSWV_XDIE1, // xdeathstate
        SfxEnum::sfx_ssdth,     // deathsound
        8.0,                    // speed
        20.0 * FRACUNIT,        // radius
        56.0 * FRACUNIT,        // height
        100,                    // mass
        0,                      // damage
        SfxEnum::sfx_posact,    // activesound
        MapObjectFlag::MF_SOLID as u32
            | MapObjectFlag::MF_SHOOTABLE as u32
            | MapObjectFlag::MF_COUNTKILL as u32, // flags
        StateNum::S_SSWV_RAISE1, // raisestate
    ),
    MapObjectInfo::new(
        // MT_KEEN
        72,                   // doomednum
        StateNum::S_KEENSTND, // spawnstate
        100,                  // spawnhealth
        StateNum::S_NULL,     // seestate
        SfxEnum::sfx_None,    // seesound
        8,                    // reactiontime
        SfxEnum::sfx_None,    // attacksound
        StateNum::S_KEENPAIN, // painstate
        256,                  // painchance
        SfxEnum::sfx_keenpn,  // painsound
        StateNum::S_NULL,     // meleestate
        StateNum::S_NULL,     // missilestate
        StateNum::S_COMMKEEN, // deathstate
        StateNum::S_NULL,     // xdeathstate
        SfxEnum::sfx_keendt,  // deathsound
        0.0,                  // speed
        16.0 * FRACUNIT,      // radius
        72.0 * FRACUNIT,      // height
        10000000,             // mass
        0,                    // damage
        SfxEnum::sfx_None,    // activesound
        MapObjectFlag::MF_SOLID as u32
            | MapObjectFlag::MF_SPAWNCEILING as u32
            | MapObjectFlag::MF_NOGRAVITY as u32
            | MapObjectFlag::MF_SHOOTABLE as u32
            | MapObjectFlag::MF_COUNTKILL as u32, // flags
        StateNum::S_NULL,     // raisestate
    ),
    MapObjectInfo::new(
        // MT_BOSSBRAIN
        88,                     // doomednum
        StateNum::S_BRAIN,      // spawnstate
        250,                    // spawnhealth
        StateNum::S_NULL,       // seestate
        SfxEnum::sfx_None,      // seesound
        8,                      // reactiontime
        SfxEnum::sfx_None,      // attacksound
        StateNum::S_BRAIN_PAIN, // painstate
        255,                    // painchance
        SfxEnum::sfx_bospn,     // painsound
        StateNum::S_NULL,       // meleestate
        StateNum::S_NULL,       // missilestate
        StateNum::S_BRAIN_DIE1, // deathstate
        StateNum::S_NULL,       // xdeathstate
        SfxEnum::sfx_bosdth,    // deathsound
        0.0,                    // speed
        16.0 * FRACUNIT,        // radius
        16.0 * FRACUNIT,        // height
        10000000,               // mass
        0,                      // damage
        SfxEnum::sfx_None,      // activesound
        MapObjectFlag::MF_SOLID as u32 | MapObjectFlag::MF_SHOOTABLE as u32, // flags
        StateNum::S_NULL, // raisestate
    ),
    MapObjectInfo::new(
        // MT_BOSSSPIT
        89,                      // doomednum
        StateNum::S_BRAINEYE,    // spawnstate
        1000,                    // spawnhealth
        StateNum::S_BRAINEYESEE, // seestate
        SfxEnum::sfx_None,       // seesound
        8,                       // reactiontime
        SfxEnum::sfx_None,       // attacksound
        StateNum::S_NULL,        // painstate
        0,                       // painchance
        SfxEnum::sfx_None,       // painsound
        StateNum::S_NULL,        // meleestate
        StateNum::S_NULL,        // missilestate
        StateNum::S_NULL,        // deathstate
        StateNum::S_NULL,        // xdeathstate
        SfxEnum::sfx_None,       // deathsound
        0.0,                     // speed
        20.0 * FRACUNIT,         // radius
        32.0 * FRACUNIT,         // height
        100,                     // mass
        0,                       // damage
        SfxEnum::sfx_None,       // activesound
        MapObjectFlag::MF_NOBLOCKMAP as u32 | MapObjectFlag::MF_NOSECTOR as u32, // flags
        StateNum::S_NULL, // raisestate
    ),
    MapObjectInfo::new(
        // MT_BOSSTARGET
        87,                // doomednum
        StateNum::S_NULL,  // spawnstate
        1000,              // spawnhealth
        StateNum::S_NULL,  // seestate
        SfxEnum::sfx_None, // seesound
        8,                 // reactiontime
        SfxEnum::sfx_None, // attacksound
        StateNum::S_NULL,  // painstate
        0,                 // painchance
        SfxEnum::sfx_None, // painsound
        StateNum::S_NULL,  // meleestate
        StateNum::S_NULL,  // missilestate
        StateNum::S_NULL,  // deathstate
        StateNum::S_NULL,  // xdeathstate
        SfxEnum::sfx_None, // deathsound
        0.0,               // speed
        20.0 * FRACUNIT,   // radius
        32.0 * FRACUNIT,   // height
        100,               // mass
        0,                 // damage
        SfxEnum::sfx_None, // activesound
        MapObjectFlag::MF_NOBLOCKMAP as u32 | MapObjectFlag::MF_NOSECTOR as u32, // flags
        StateNum::S_NULL, // raisestate
    ),
    MapObjectInfo::new(
        // MT_SPAWNSHOT
        -1,                  // doomednum
        StateNum::S_SPAWN1,  // spawnstate
        1000,                // spawnhealth
        StateNum::S_NULL,    // seestate
        SfxEnum::sfx_bospit, // seesound
        8,                   // reactiontime
        SfxEnum::sfx_None,   // attacksound
        StateNum::S_NULL,    // painstate
        0,                   // painchance
        SfxEnum::sfx_None,   // painsound
        StateNum::S_NULL,    // meleestate
        StateNum::S_NULL,    // missilestate
        StateNum::S_NULL,    // deathstate
        StateNum::S_NULL,    // xdeathstate
        SfxEnum::sfx_firxpl, // deathsound
        10.0 * FRACUNIT,     // speed
        6.0 * FRACUNIT,      // radius
        32.0 * FRACUNIT,     // height
        100,                 // mass
        3,                   // damage
        SfxEnum::sfx_None,   // activesound
        MapObjectFlag::MF_NOBLOCKMAP as u32
            | MapObjectFlag::MF_MISSILE as u32
            | MapObjectFlag::MF_DROPOFF as u32
            | MapObjectFlag::MF_NOGRAVITY as u32
            | MapObjectFlag::MF_NOCLIP as u32, // flags
        StateNum::S_NULL,    // raisestate
    ),
    MapObjectInfo::new(
        // MT_SPAWNFIRE
        -1,                     // doomednum
        StateNum::S_SPAWNFIRE1, // spawnstate
        1000,                   // spawnhealth
        StateNum::S_NULL,       // seestate
        SfxEnum::sfx_None,      // seesound
        8,                      // reactiontime
        SfxEnum::sfx_None,      // attacksound
        StateNum::S_NULL,       // painstate
        0,                      // painchance
        SfxEnum::sfx_None,      // painsound
        StateNum::S_NULL,       // meleestate
        StateNum::S_NULL,       // missilestate
        StateNum::S_NULL,       // deathstate
        StateNum::S_NULL,       // xdeathstate
        SfxEnum::sfx_None,      // deathsound
        0.0,                    // speed
        20.0 * FRACUNIT,        // radius
        16.0 * FRACUNIT,        // height
        100,                    // mass
        0,                      // damage
        SfxEnum::sfx_None,      // activesound
        MapObjectFlag::MF_NOBLOCKMAP as u32
            | MapObjectFlag::MF_NOGRAVITY as u32, // flags
        StateNum::S_NULL,       // raisestate
    ),
    MapObjectInfo::new(
        // MT_BARREL
        2035,                // doomednum
        StateNum::S_BAR1,    // spawnstate
        20,                  // spawnhealth
        StateNum::S_NULL,    // seestate
        SfxEnum::sfx_None,   // seesound
        8,                   // reactiontime
        SfxEnum::sfx_None,   // attacksound
        StateNum::S_NULL,    // painstate
        0,                   // painchance
        SfxEnum::sfx_None,   // painsound
        StateNum::S_NULL,    // meleestate
        StateNum::S_NULL,    // missilestate
        StateNum::S_BEXP,    // deathstate
        StateNum::S_NULL,    // xdeathstate
        SfxEnum::sfx_barexp, // deathsound
        0.0,                 // speed
        10.0 * FRACUNIT,     // radius
        42.0 * FRACUNIT,     // height
        100,                 // mass
        0,                   // damage
        SfxEnum::sfx_None,   // activesound
        MapObjectFlag::MF_SOLID as u32
            | MapObjectFlag::MF_SHOOTABLE as u32
            | MapObjectFlag::MF_NOBLOOD as u32, // flags
        StateNum::S_NULL,    // raisestate
    ),
    MapObjectInfo::new(
        // MT_TROOPSHOT
        -1,                  // doomednum
        StateNum::S_TBALL1,  // spawnstate
        1000,                // spawnhealth
        StateNum::S_NULL,    // seestate
        SfxEnum::sfx_firsht, // seesound
        8,                   // reactiontime
        SfxEnum::sfx_None,   // attacksound
        StateNum::S_NULL,    // painstate
        0,                   // painchance
        SfxEnum::sfx_None,   // painsound
        StateNum::S_NULL,    // meleestate
        StateNum::S_NULL,    // missilestate
        StateNum::S_TBALLX1, // deathstate
        StateNum::S_NULL,    // xdeathstate
        SfxEnum::sfx_firxpl, // deathsound
        10.0 * FRACUNIT,     // speed
        6.0 * FRACUNIT,      // radius
        8.0 * FRACUNIT,      // height
        100,                 // mass
        3,                   // damage
        SfxEnum::sfx_None,   // activesound
        MapObjectFlag::MF_NOBLOCKMAP as u32
            | MapObjectFlag::MF_MISSILE as u32
            | MapObjectFlag::MF_DROPOFF as u32
            | MapObjectFlag::MF_NOGRAVITY as u32, // flags
        StateNum::S_NULL,    // raisestate
    ),
    MapObjectInfo::new(
        // MT_HEADSHOT
        -1,                  // doomednum
        StateNum::S_RBALL1,  // spawnstate
        1000,                // spawnhealth
        StateNum::S_NULL,    // seestate
        SfxEnum::sfx_firsht, // seesound
        8,                   // reactiontime
        SfxEnum::sfx_None,   // attacksound
        StateNum::S_NULL,    // painstate
        0,                   // painchance
        SfxEnum::sfx_None,   // painsound
        StateNum::S_NULL,    // meleestate
        StateNum::S_NULL,    // missilestate
        StateNum::S_RBALLX1, // deathstate
        StateNum::S_NULL,    // xdeathstate
        SfxEnum::sfx_firxpl, // deathsound
        10.0 * FRACUNIT,     // speed
        6.0 * FRACUNIT,      // radius
        8.0 * FRACUNIT,      // height
        100,                 // mass
        5,                   // damage
        SfxEnum::sfx_None,   // activesound
        MapObjectFlag::MF_NOBLOCKMAP as u32
            | MapObjectFlag::MF_MISSILE as u32
            | MapObjectFlag::MF_DROPOFF as u32
            | MapObjectFlag::MF_NOGRAVITY as u32, // flags
        StateNum::S_NULL,    // raisestate
    ),
    MapObjectInfo::new(
        // MT_ROCKET
        -1,                   // doomednum
        StateNum::S_ROCKET,   // spawnstate
        1000,                 // spawnhealth
        StateNum::S_NULL,     // seestate
        SfxEnum::sfx_rlaunc,  // seesound
        8,                    // reactiontime
        SfxEnum::sfx_None,    // attacksound
        StateNum::S_NULL,     // painstate
        0,                    // painchance
        SfxEnum::sfx_None,    // painsound
        StateNum::S_NULL,     // meleestate
        StateNum::S_NULL,     // missilestate
        StateNum::S_EXPLODE1, // deathstate
        StateNum::S_NULL,     // xdeathstate
        SfxEnum::sfx_barexp,  // deathsound
        20.0 * FRACUNIT,      // speed
        11.0 * FRACUNIT,      // radius
        8.0 * FRACUNIT,       // height
        100,                  // mass
        20,                   // damage
        SfxEnum::sfx_None,    // activesound
        MapObjectFlag::MF_NOBLOCKMAP as u32
            | MapObjectFlag::MF_MISSILE as u32
            | MapObjectFlag::MF_DROPOFF as u32
            | MapObjectFlag::MF_NOGRAVITY as u32, // flags
        StateNum::S_NULL,     // raisestate
    ),
    MapObjectInfo::new(
        // MT_PLASMA
        -1,                   // doomednum
        StateNum::S_PLASBALL, // spawnstate
        1000,                 // spawnhealth
        StateNum::S_NULL,     // seestate
        SfxEnum::sfx_plasma,  // seesound
        8,                    // reactiontime
        SfxEnum::sfx_None,    // attacksound
        StateNum::S_NULL,     // painstate
        0,                    // painchance
        SfxEnum::sfx_None,    // painsound
        StateNum::S_NULL,     // meleestate
        StateNum::S_NULL,     // missilestate
        StateNum::S_PLASEXP,  // deathstate
        StateNum::S_NULL,     // xdeathstate
        SfxEnum::sfx_firxpl,  // deathsound
        25.0 * FRACUNIT,      // speed
        13.0 * FRACUNIT,      // radius
        8.0 * FRACUNIT,       // height
        100,                  // mass
        5,                    // damage
        SfxEnum::sfx_None,    // activesound
        MapObjectFlag::MF_NOBLOCKMAP as u32
            | MapObjectFlag::MF_MISSILE as u32
            | MapObjectFlag::MF_DROPOFF as u32
            | MapObjectFlag::MF_NOGRAVITY as u32, // flags
        StateNum::S_NULL,     // raisestate
    ),
    MapObjectInfo::new(
        // MT_BFG
        -1,                  // doomednum
        StateNum::S_BFGSHOT, // spawnstate
        1000,                // spawnhealth
        StateNum::S_NULL,    // seestate
        SfxEnum::sfx_None,   // seesound
        8,                   // reactiontime
        SfxEnum::sfx_None,   // attacksound
        StateNum::S_NULL,    // painstate
        0,                   // painchance
        SfxEnum::sfx_None,   // painsound
        StateNum::S_NULL,    // meleestate
        StateNum::S_NULL,    // missilestate
        StateNum::S_BFGLAND, // deathstate
        StateNum::S_NULL,    // xdeathstate
        SfxEnum::sfx_rxplod, // deathsound
        25.0 * FRACUNIT,     // speed
        13.0 * FRACUNIT,     // radius
        8.0 * FRACUNIT,      // height
        100,                 // mass
        100,                 // damage
        SfxEnum::sfx_None,   // activesound
        MapObjectFlag::MF_NOBLOCKMAP as u32
            | MapObjectFlag::MF_MISSILE as u32
            | MapObjectFlag::MF_DROPOFF as u32
            | MapObjectFlag::MF_NOGRAVITY as u32, // flags
        StateNum::S_NULL,    // raisestate
    ),
    MapObjectInfo::new(
        // MT_ARACHPLAZ
        -1,                     // doomednum
        StateNum::S_ARACH_PLAZ, // spawnstate
        1000,                   // spawnhealth
        StateNum::S_NULL,       // seestate
        SfxEnum::sfx_plasma,    // seesound
        8,                      // reactiontime
        SfxEnum::sfx_None,      // attacksound
        StateNum::S_NULL,       // painstate
        0,                      // painchance
        SfxEnum::sfx_None,      // painsound
        StateNum::S_NULL,       // meleestate
        StateNum::S_NULL,       // missilestate
        StateNum::S_ARACH_PLEX, // deathstate
        StateNum::S_NULL,       // xdeathstate
        SfxEnum::sfx_firxpl,    // deathsound
        25.0 * FRACUNIT,        // speed
        13.0 * FRACUNIT,        // radius
        8.0 * FRACUNIT,         // height
        100,                    // mass
        5,                      // damage
        SfxEnum::sfx_None,      // activesound
        MapObjectFlag::MF_NOBLOCKMAP as u32
            | MapObjectFlag::MF_MISSILE as u32
            | MapObjectFlag::MF_DROPOFF as u32
            | MapObjectFlag::MF_NOGRAVITY as u32, // flags
        StateNum::S_NULL,       // raisestate
    ),
    MapObjectInfo::new(
        // MT_PUFF
        -1,                // doomednum
        StateNum::S_PUFF1, // spawnstate
        1000,              // spawnhealth
        StateNum::S_NULL,  // seestate
        SfxEnum::sfx_None, // seesound
        8,                 // reactiontime
        SfxEnum::sfx_None, // attacksound
        StateNum::S_NULL,  // painstate
        0,                 // painchance
        SfxEnum::sfx_None, // painsound
        StateNum::S_NULL,  // meleestate
        StateNum::S_NULL,  // missilestate
        StateNum::S_NULL,  // deathstate
        StateNum::S_NULL,  // xdeathstate
        SfxEnum::sfx_None, // deathsound
        0.0,               // speed
        20.0 * FRACUNIT,   // radius
        16.0 * FRACUNIT,   // height
        100,               // mass
        0,                 // damage
        SfxEnum::sfx_None, // activesound
        MapObjectFlag::MF_NOBLOCKMAP as u32
            | MapObjectFlag::MF_NOGRAVITY as u32, // flags
        StateNum::S_NULL,  // raisestate
    ),
    MapObjectInfo::new(
        // MT_BLOOD
        -1,                                  // doomednum
        StateNum::S_BLOOD1,                  // spawnstate
        1000,                                // spawnhealth
        StateNum::S_NULL,                    // seestate
        SfxEnum::sfx_None,                   // seesound
        8,                                   // reactiontime
        SfxEnum::sfx_None,                   // attacksound
        StateNum::S_NULL,                    // painstate
        0,                                   // painchance
        SfxEnum::sfx_None,                   // painsound
        StateNum::S_NULL,                    // meleestate
        StateNum::S_NULL,                    // missilestate
        StateNum::S_NULL,                    // deathstate
        StateNum::S_NULL,                    // xdeathstate
        SfxEnum::sfx_None,                   // deathsound
        0.0,                                 // speed
        20.0 * FRACUNIT,                     // radius
        16.0 * FRACUNIT,                     // height
        100,                                 // mass
        0,                                   // damage
        SfxEnum::sfx_None,                   // activesound
        MapObjectFlag::MF_NOBLOCKMAP as u32, // flags
        StateNum::S_NULL,                    // raisestate
    ),
    MapObjectInfo::new(
        // MT_TFOG
        -1,                // doomednum
        StateNum::S_TFOG,  // spawnstate
        1000,              // spawnhealth
        StateNum::S_NULL,  // seestate
        SfxEnum::sfx_None, // seesound
        8,                 // reactiontime
        SfxEnum::sfx_None, // attacksound
        StateNum::S_NULL,  // painstate
        0,                 // painchance
        SfxEnum::sfx_None, // painsound
        StateNum::S_NULL,  // meleestate
        StateNum::S_NULL,  // missilestate
        StateNum::S_NULL,  // deathstate
        StateNum::S_NULL,  // xdeathstate
        SfxEnum::sfx_None, // deathsound
        0.0,               // speed
        20.0 * FRACUNIT,   // radius
        16.0 * FRACUNIT,   // height
        100,               // mass
        0,                 // damage
        SfxEnum::sfx_None, // activesound
        MapObjectFlag::MF_NOBLOCKMAP as u32
            | MapObjectFlag::MF_NOGRAVITY as u32, // flags
        StateNum::S_NULL,  // raisestate
    ),
    MapObjectInfo::new(
        // MT_IFOG
        -1,                // doomednum
        StateNum::S_IFOG,  // spawnstate
        1000,              // spawnhealth
        StateNum::S_NULL,  // seestate
        SfxEnum::sfx_None, // seesound
        8,                 // reactiontime
        SfxEnum::sfx_None, // attacksound
        StateNum::S_NULL,  // painstate
        0,                 // painchance
        SfxEnum::sfx_None, // painsound
        StateNum::S_NULL,  // meleestate
        StateNum::S_NULL,  // missilestate
        StateNum::S_NULL,  // deathstate
        StateNum::S_NULL,  // xdeathstate
        SfxEnum::sfx_None, // deathsound
        0.0,               // speed
        20.0 * FRACUNIT,   // radius
        16.0 * FRACUNIT,   // height
        100,               // mass
        0,                 // damage
        SfxEnum::sfx_None, // activesound
        MapObjectFlag::MF_NOBLOCKMAP as u32
            | MapObjectFlag::MF_NOGRAVITY as u32, // flags
        StateNum::S_NULL,  // raisestate
    ),
    MapObjectInfo::new(
        // MT_TELEPORTMAN
        14,                // doomednum
        StateNum::S_NULL,  // spawnstate
        1000,              // spawnhealth
        StateNum::S_NULL,  // seestate
        SfxEnum::sfx_None, // seesound
        8,                 // reactiontime
        SfxEnum::sfx_None, // attacksound
        StateNum::S_NULL,  // painstate
        0,                 // painchance
        SfxEnum::sfx_None, // painsound
        StateNum::S_NULL,  // meleestate
        StateNum::S_NULL,  // missilestate
        StateNum::S_NULL,  // deathstate
        StateNum::S_NULL,  // xdeathstate
        SfxEnum::sfx_None, // deathsound
        0.0,               // speed
        20.0 * FRACUNIT,   // radius
        16.0 * FRACUNIT,   // height
        100,               // mass
        0,                 // damage
        SfxEnum::sfx_None, // activesound
        MapObjectFlag::MF_NOBLOCKMAP as u32 | MapObjectFlag::MF_NOSECTOR as u32, // flags
        StateNum::S_NULL, // raisestate
    ),
    MapObjectInfo::new(
        // MT_EXTRABFG
        -1,                 // doomednum
        StateNum::S_BFGEXP, // spawnstate
        1000,               // spawnhealth
        StateNum::S_NULL,   // seestate
        SfxEnum::sfx_None,  // seesound
        8,                  // reactiontime
        SfxEnum::sfx_None,  // attacksound
        StateNum::S_NULL,   // painstate
        0,                  // painchance
        SfxEnum::sfx_None,  // painsound
        StateNum::S_NULL,   // meleestate
        StateNum::S_NULL,   // missilestate
        StateNum::S_NULL,   // deathstate
        StateNum::S_NULL,   // xdeathstate
        SfxEnum::sfx_None,  // deathsound
        0.0,                // speed
        20.0 * FRACUNIT,    // radius
        16.0 * FRACUNIT,    // height
        100,                // mass
        0,                  // damage
        SfxEnum::sfx_None,  // activesound
        MapObjectFlag::MF_NOBLOCKMAP as u32
            | MapObjectFlag::MF_NOGRAVITY as u32, // flags
        StateNum::S_NULL,   // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC0
        2018,                             // doomednum
        StateNum::S_ARM1,                 // spawnstate
        1000,                             // spawnhealth
        StateNum::S_NULL,                 // seestate
        SfxEnum::sfx_None,                // seesound
        8,                                // reactiontime
        SfxEnum::sfx_None,                // attacksound
        StateNum::S_NULL,                 // painstate
        0,                                // painchance
        SfxEnum::sfx_None,                // painsound
        StateNum::S_NULL,                 // meleestate
        StateNum::S_NULL,                 // missilestate
        StateNum::S_NULL,                 // deathstate
        StateNum::S_NULL,                 // xdeathstate
        SfxEnum::sfx_None,                // deathsound
        0.0,                              // speed
        20.0 * FRACUNIT,                  // radius
        16.0 * FRACUNIT,                  // height
        100,                              // mass
        0,                                // damage
        SfxEnum::sfx_None,                // activesound
        MapObjectFlag::MF_SPECIAL as u32, // flags
        StateNum::S_NULL,                 // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC1
        2019,                             // doomednum
        StateNum::S_ARM2,                 // spawnstate
        1000,                             // spawnhealth
        StateNum::S_NULL,                 // seestate
        SfxEnum::sfx_None,                // seesound
        8,                                // reactiontime
        SfxEnum::sfx_None,                // attacksound
        StateNum::S_NULL,                 // painstate
        0,                                // painchance
        SfxEnum::sfx_None,                // painsound
        StateNum::S_NULL,                 // meleestate
        StateNum::S_NULL,                 // missilestate
        StateNum::S_NULL,                 // deathstate
        StateNum::S_NULL,                 // xdeathstate
        SfxEnum::sfx_None,                // deathsound
        0.0,                              // speed
        20.0 * FRACUNIT,                  // radius
        16.0 * FRACUNIT,                  // height
        100,                              // mass
        0,                                // damage
        SfxEnum::sfx_None,                // activesound
        MapObjectFlag::MF_SPECIAL as u32, // flags
        StateNum::S_NULL,                 // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC2
        2014,              // doomednum
        StateNum::S_BON1,  // spawnstate
        1000,              // spawnhealth
        StateNum::S_NULL,  // seestate
        SfxEnum::sfx_None, // seesound
        8,                 // reactiontime
        SfxEnum::sfx_None, // attacksound
        StateNum::S_NULL,  // painstate
        0,                 // painchance
        SfxEnum::sfx_None, // painsound
        StateNum::S_NULL,  // meleestate
        StateNum::S_NULL,  // missilestate
        StateNum::S_NULL,  // deathstate
        StateNum::S_NULL,  // xdeathstate
        SfxEnum::sfx_None, // deathsound
        0.0,               // speed
        20.0 * FRACUNIT,   // radius
        16.0 * FRACUNIT,   // height
        100,               // mass
        0,                 // damage
        SfxEnum::sfx_None, // activesound
        MapObjectFlag::MF_SPECIAL as u32 | MapObjectFlag::MF_COUNTITEM as u32, // flags
        StateNum::S_NULL, // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC3
        2015,              // doomednum
        StateNum::S_BON2,  // spawnstate
        1000,              // spawnhealth
        StateNum::S_NULL,  // seestate
        SfxEnum::sfx_None, // seesound
        8,                 // reactiontime
        SfxEnum::sfx_None, // attacksound
        StateNum::S_NULL,  // painstate
        0,                 // painchance
        SfxEnum::sfx_None, // painsound
        StateNum::S_NULL,  // meleestate
        StateNum::S_NULL,  // missilestate
        StateNum::S_NULL,  // deathstate
        StateNum::S_NULL,  // xdeathstate
        SfxEnum::sfx_None, // deathsound
        0.0,               // speed
        20.0 * FRACUNIT,   // radius
        16.0 * FRACUNIT,   // height
        100,               // mass
        0,                 // damage
        SfxEnum::sfx_None, // activesound
        MapObjectFlag::MF_SPECIAL as u32 | MapObjectFlag::MF_COUNTITEM as u32, // flags
        StateNum::S_NULL, // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC4
        5,                 // doomednum
        StateNum::S_BKEY,  // spawnstate
        1000,              // spawnhealth
        StateNum::S_NULL,  // seestate
        SfxEnum::sfx_None, // seesound
        8,                 // reactiontime
        SfxEnum::sfx_None, // attacksound
        StateNum::S_NULL,  // painstate
        0,                 // painchance
        SfxEnum::sfx_None, // painsound
        StateNum::S_NULL,  // meleestate
        StateNum::S_NULL,  // missilestate
        StateNum::S_NULL,  // deathstate
        StateNum::S_NULL,  // xdeathstate
        SfxEnum::sfx_None, // deathsound
        0.0,               // speed
        20.0 * FRACUNIT,   // radius
        16.0 * FRACUNIT,   // height
        100,               // mass
        0,                 // damage
        SfxEnum::sfx_None, // activesound
        MapObjectFlag::MF_SPECIAL as u32 | MapObjectFlag::MF_NOTDMATCH as u32, // flags
        StateNum::S_NULL, // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC5
        13,                // doomednum
        StateNum::S_RKEY,  // spawnstate
        1000,              // spawnhealth
        StateNum::S_NULL,  // seestate
        SfxEnum::sfx_None, // seesound
        8,                 // reactiontime
        SfxEnum::sfx_None, // attacksound
        StateNum::S_NULL,  // painstate
        0,                 // painchance
        SfxEnum::sfx_None, // painsound
        StateNum::S_NULL,  // meleestate
        StateNum::S_NULL,  // missilestate
        StateNum::S_NULL,  // deathstate
        StateNum::S_NULL,  // xdeathstate
        SfxEnum::sfx_None, // deathsound
        0.0,               // speed
        20.0 * FRACUNIT,   // radius
        16.0 * FRACUNIT,   // height
        100,               // mass
        0,                 // damage
        SfxEnum::sfx_None, // activesound
        MapObjectFlag::MF_SPECIAL as u32 | MapObjectFlag::MF_NOTDMATCH as u32, // flags
        StateNum::S_NULL, // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC6
        6,                 // doomednum
        StateNum::S_YKEY,  // spawnstate
        1000,              // spawnhealth
        StateNum::S_NULL,  // seestate
        SfxEnum::sfx_None, // seesound
        8,                 // reactiontime
        SfxEnum::sfx_None, // attacksound
        StateNum::S_NULL,  // painstate
        0,                 // painchance
        SfxEnum::sfx_None, // painsound
        StateNum::S_NULL,  // meleestate
        StateNum::S_NULL,  // missilestate
        StateNum::S_NULL,  // deathstate
        StateNum::S_NULL,  // xdeathstate
        SfxEnum::sfx_None, // deathsound
        0.0,               // speed
        20.0 * FRACUNIT,   // radius
        16.0 * FRACUNIT,   // height
        100,               // mass
        0,                 // damage
        SfxEnum::sfx_None, // activesound
        MapObjectFlag::MF_SPECIAL as u32 | MapObjectFlag::MF_NOTDMATCH as u32, // flags
        StateNum::S_NULL, // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC7
        39,                 // doomednum
        StateNum::S_YSKULL, // spawnstate
        1000,               // spawnhealth
        StateNum::S_NULL,   // seestate
        SfxEnum::sfx_None,  // seesound
        8,                  // reactiontime
        SfxEnum::sfx_None,  // attacksound
        StateNum::S_NULL,   // painstate
        0,                  // painchance
        SfxEnum::sfx_None,  // painsound
        StateNum::S_NULL,   // meleestate
        StateNum::S_NULL,   // missilestate
        StateNum::S_NULL,   // deathstate
        StateNum::S_NULL,   // xdeathstate
        SfxEnum::sfx_None,  // deathsound
        0.0,                // speed
        20.0 * FRACUNIT,    // radius
        16.0 * FRACUNIT,    // height
        100,                // mass
        0,                  // damage
        SfxEnum::sfx_None,  // activesound
        MapObjectFlag::MF_SPECIAL as u32 | MapObjectFlag::MF_NOTDMATCH as u32, // flags
        StateNum::S_NULL, // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC8
        38,                 // doomednum
        StateNum::S_RSKULL, // spawnstate
        1000,               // spawnhealth
        StateNum::S_NULL,   // seestate
        SfxEnum::sfx_None,  // seesound
        8,                  // reactiontime
        SfxEnum::sfx_None,  // attacksound
        StateNum::S_NULL,   // painstate
        0,                  // painchance
        SfxEnum::sfx_None,  // painsound
        StateNum::S_NULL,   // meleestate
        StateNum::S_NULL,   // missilestate
        StateNum::S_NULL,   // deathstate
        StateNum::S_NULL,   // xdeathstate
        SfxEnum::sfx_None,  // deathsound
        0.0,                // speed
        20.0 * FRACUNIT,    // radius
        16.0 * FRACUNIT,    // height
        100,                // mass
        0,                  // damage
        SfxEnum::sfx_None,  // activesound
        MapObjectFlag::MF_SPECIAL as u32 | MapObjectFlag::MF_NOTDMATCH as u32, // flags
        StateNum::S_NULL, // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC9
        40,                 // doomednum
        StateNum::S_BSKULL, // spawnstate
        1000,               // spawnhealth
        StateNum::S_NULL,   // seestate
        SfxEnum::sfx_None,  // seesound
        8,                  // reactiontime
        SfxEnum::sfx_None,  // attacksound
        StateNum::S_NULL,   // painstate
        0,                  // painchance
        SfxEnum::sfx_None,  // painsound
        StateNum::S_NULL,   // meleestate
        StateNum::S_NULL,   // missilestate
        StateNum::S_NULL,   // deathstate
        StateNum::S_NULL,   // xdeathstate
        SfxEnum::sfx_None,  // deathsound
        0.0,                // speed
        20.0 * FRACUNIT,    // radius
        16.0 * FRACUNIT,    // height
        100,                // mass
        0,                  // damage
        SfxEnum::sfx_None,  // activesound
        MapObjectFlag::MF_SPECIAL as u32 | MapObjectFlag::MF_NOTDMATCH as u32, // flags
        StateNum::S_NULL, // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC10
        2011,                             // doomednum
        StateNum::S_STIM,                 // spawnstate
        1000,                             // spawnhealth
        StateNum::S_NULL,                 // seestate
        SfxEnum::sfx_None,                // seesound
        8,                                // reactiontime
        SfxEnum::sfx_None,                // attacksound
        StateNum::S_NULL,                 // painstate
        0,                                // painchance
        SfxEnum::sfx_None,                // painsound
        StateNum::S_NULL,                 // meleestate
        StateNum::S_NULL,                 // missilestate
        StateNum::S_NULL,                 // deathstate
        StateNum::S_NULL,                 // xdeathstate
        SfxEnum::sfx_None,                // deathsound
        0.0,                              // speed
        20.0 * FRACUNIT,                  // radius
        16.0 * FRACUNIT,                  // height
        100,                              // mass
        0,                                // damage
        SfxEnum::sfx_None,                // activesound
        MapObjectFlag::MF_SPECIAL as u32, // flags
        StateNum::S_NULL,                 // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC11
        2012,                             // doomednum
        StateNum::S_MEDI,                 // spawnstate
        1000,                             // spawnhealth
        StateNum::S_NULL,                 // seestate
        SfxEnum::sfx_None,                // seesound
        8,                                // reactiontime
        SfxEnum::sfx_None,                // attacksound
        StateNum::S_NULL,                 // painstate
        0,                                // painchance
        SfxEnum::sfx_None,                // painsound
        StateNum::S_NULL,                 // meleestate
        StateNum::S_NULL,                 // missilestate
        StateNum::S_NULL,                 // deathstate
        StateNum::S_NULL,                 // xdeathstate
        SfxEnum::sfx_None,                // deathsound
        0.0,                              // speed
        20.0 * FRACUNIT,                  // radius
        16.0 * FRACUNIT,                  // height
        100,                              // mass
        0,                                // damage
        SfxEnum::sfx_None,                // activesound
        MapObjectFlag::MF_SPECIAL as u32, // flags
        StateNum::S_NULL,                 // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC12
        2013,              // doomednum
        StateNum::S_SOUL,  // spawnstate
        1000,              // spawnhealth
        StateNum::S_NULL,  // seestate
        SfxEnum::sfx_None, // seesound
        8,                 // reactiontime
        SfxEnum::sfx_None, // attacksound
        StateNum::S_NULL,  // painstate
        0,                 // painchance
        SfxEnum::sfx_None, // painsound
        StateNum::S_NULL,  // meleestate
        StateNum::S_NULL,  // missilestate
        StateNum::S_NULL,  // deathstate
        StateNum::S_NULL,  // xdeathstate
        SfxEnum::sfx_None, // deathsound
        0.0,               // speed
        20.0 * FRACUNIT,   // radius
        16.0 * FRACUNIT,   // height
        100,               // mass
        0,                 // damage
        SfxEnum::sfx_None, // activesound
        MapObjectFlag::MF_SPECIAL as u32 | MapObjectFlag::MF_COUNTITEM as u32, // flags
        StateNum::S_NULL, // raisestate
    ),
    MapObjectInfo::new(
        // MT_INV
        2022,              // doomednum
        StateNum::S_PINV,  // spawnstate
        1000,              // spawnhealth
        StateNum::S_NULL,  // seestate
        SfxEnum::sfx_None, // seesound
        8,                 // reactiontime
        SfxEnum::sfx_None, // attacksound
        StateNum::S_NULL,  // painstate
        0,                 // painchance
        SfxEnum::sfx_None, // painsound
        StateNum::S_NULL,  // meleestate
        StateNum::S_NULL,  // missilestate
        StateNum::S_NULL,  // deathstate
        StateNum::S_NULL,  // xdeathstate
        SfxEnum::sfx_None, // deathsound
        0.0,               // speed
        20.0 * FRACUNIT,   // radius
        16.0 * FRACUNIT,   // height
        100,               // mass
        0,                 // damage
        SfxEnum::sfx_None, // activesound
        MapObjectFlag::MF_SPECIAL as u32 | MapObjectFlag::MF_COUNTITEM as u32, // flags
        StateNum::S_NULL, // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC13
        2023,              // doomednum
        StateNum::S_PSTR,  // spawnstate
        1000,              // spawnhealth
        StateNum::S_NULL,  // seestate
        SfxEnum::sfx_None, // seesound
        8,                 // reactiontime
        SfxEnum::sfx_None, // attacksound
        StateNum::S_NULL,  // painstate
        0,                 // painchance
        SfxEnum::sfx_None, // painsound
        StateNum::S_NULL,  // meleestate
        StateNum::S_NULL,  // missilestate
        StateNum::S_NULL,  // deathstate
        StateNum::S_NULL,  // xdeathstate
        SfxEnum::sfx_None, // deathsound
        0.0,               // speed
        20.0 * FRACUNIT,   // radius
        16.0 * FRACUNIT,   // height
        100,               // mass
        0,                 // damage
        SfxEnum::sfx_None, // activesound
        MapObjectFlag::MF_SPECIAL as u32 | MapObjectFlag::MF_COUNTITEM as u32, // flags
        StateNum::S_NULL, // raisestate
    ),
    MapObjectInfo::new(
        // MT_INS
        2024,              // doomednum
        StateNum::S_PINS,  // spawnstate
        1000,              // spawnhealth
        StateNum::S_NULL,  // seestate
        SfxEnum::sfx_None, // seesound
        8,                 // reactiontime
        SfxEnum::sfx_None, // attacksound
        StateNum::S_NULL,  // painstate
        0,                 // painchance
        SfxEnum::sfx_None, // painsound
        StateNum::S_NULL,  // meleestate
        StateNum::S_NULL,  // missilestate
        StateNum::S_NULL,  // deathstate
        StateNum::S_NULL,  // xdeathstate
        SfxEnum::sfx_None, // deathsound
        0.0,               // speed
        20.0 * FRACUNIT,   // radius
        16.0 * FRACUNIT,   // height
        100,               // mass
        0,                 // damage
        SfxEnum::sfx_None, // activesound
        MapObjectFlag::MF_SPECIAL as u32 | MapObjectFlag::MF_COUNTITEM as u32, // flags
        StateNum::S_NULL, // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC14
        2025,                             // doomednum
        StateNum::S_SUIT,                 // spawnstate
        1000,                             // spawnhealth
        StateNum::S_NULL,                 // seestate
        SfxEnum::sfx_None,                // seesound
        8,                                // reactiontime
        SfxEnum::sfx_None,                // attacksound
        StateNum::S_NULL,                 // painstate
        0,                                // painchance
        SfxEnum::sfx_None,                // painsound
        StateNum::S_NULL,                 // meleestate
        StateNum::S_NULL,                 // missilestate
        StateNum::S_NULL,                 // deathstate
        StateNum::S_NULL,                 // xdeathstate
        SfxEnum::sfx_None,                // deathsound
        0.0,                              // speed
        20.0 * FRACUNIT,                  // radius
        16.0 * FRACUNIT,                  // height
        100,                              // mass
        0,                                // damage
        SfxEnum::sfx_None,                // activesound
        MapObjectFlag::MF_SPECIAL as u32, // flags
        StateNum::S_NULL,                 // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC15
        2026,              // doomednum
        StateNum::S_PMAP,  // spawnstate
        1000,              // spawnhealth
        StateNum::S_NULL,  // seestate
        SfxEnum::sfx_None, // seesound
        8,                 // reactiontime
        SfxEnum::sfx_None, // attacksound
        StateNum::S_NULL,  // painstate
        0,                 // painchance
        SfxEnum::sfx_None, // painsound
        StateNum::S_NULL,  // meleestate
        StateNum::S_NULL,  // missilestate
        StateNum::S_NULL,  // deathstate
        StateNum::S_NULL,  // xdeathstate
        SfxEnum::sfx_None, // deathsound
        0.0,               // speed
        20.0 * FRACUNIT,   // radius
        16.0 * FRACUNIT,   // height
        100,               // mass
        0,                 // damage
        SfxEnum::sfx_None, // activesound
        MapObjectFlag::MF_SPECIAL as u32 | MapObjectFlag::MF_COUNTITEM as u32, // flags
        StateNum::S_NULL, // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC16
        2045,              // doomednum
        StateNum::S_PVIS,  // spawnstate
        1000,              // spawnhealth
        StateNum::S_NULL,  // seestate
        SfxEnum::sfx_None, // seesound
        8,                 // reactiontime
        SfxEnum::sfx_None, // attacksound
        StateNum::S_NULL,  // painstate
        0,                 // painchance
        SfxEnum::sfx_None, // painsound
        StateNum::S_NULL,  // meleestate
        StateNum::S_NULL,  // missilestate
        StateNum::S_NULL,  // deathstate
        StateNum::S_NULL,  // xdeathstate
        SfxEnum::sfx_None, // deathsound
        0.0,               // speed
        20.0 * FRACUNIT,   // radius
        16.0 * FRACUNIT,   // height
        100,               // mass
        0,                 // damage
        SfxEnum::sfx_None, // activesound
        MapObjectFlag::MF_SPECIAL as u32 | MapObjectFlag::MF_COUNTITEM as u32, // flags
        StateNum::S_NULL, // raisestate
    ),
    MapObjectInfo::new(
        // MT_MEGA
        83,                // doomednum
        StateNum::S_MEGA,  // spawnstate
        1000,              // spawnhealth
        StateNum::S_NULL,  // seestate
        SfxEnum::sfx_None, // seesound
        8,                 // reactiontime
        SfxEnum::sfx_None, // attacksound
        StateNum::S_NULL,  // painstate
        0,                 // painchance
        SfxEnum::sfx_None, // painsound
        StateNum::S_NULL,  // meleestate
        StateNum::S_NULL,  // missilestate
        StateNum::S_NULL,  // deathstate
        StateNum::S_NULL,  // xdeathstate
        SfxEnum::sfx_None, // deathsound
        0.0,               // speed
        20.0 * FRACUNIT,   // radius
        16.0 * FRACUNIT,   // height
        100,               // mass
        0,                 // damage
        SfxEnum::sfx_None, // activesound
        MapObjectFlag::MF_SPECIAL as u32 | MapObjectFlag::MF_COUNTITEM as u32, // flags
        StateNum::S_NULL, // raisestate
    ),
    MapObjectInfo::new(
        // MT_CLIP
        2007,                             // doomednum
        StateNum::S_CLIP,                 // spawnstate
        1000,                             // spawnhealth
        StateNum::S_NULL,                 // seestate
        SfxEnum::sfx_None,                // seesound
        8,                                // reactiontime
        SfxEnum::sfx_None,                // attacksound
        StateNum::S_NULL,                 // painstate
        0,                                // painchance
        SfxEnum::sfx_None,                // painsound
        StateNum::S_NULL,                 // meleestate
        StateNum::S_NULL,                 // missilestate
        StateNum::S_NULL,                 // deathstate
        StateNum::S_NULL,                 // xdeathstate
        SfxEnum::sfx_None,                // deathsound
        0.0,                              // speed
        20.0 * FRACUNIT,                  // radius
        16.0 * FRACUNIT,                  // height
        100,                              // mass
        0,                                // damage
        SfxEnum::sfx_None,                // activesound
        MapObjectFlag::MF_SPECIAL as u32, // flags
        StateNum::S_NULL,                 // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC17
        2048,                             // doomednum
        StateNum::S_AMMO,                 // spawnstate
        1000,                             // spawnhealth
        StateNum::S_NULL,                 // seestate
        SfxEnum::sfx_None,                // seesound
        8,                                // reactiontime
        SfxEnum::sfx_None,                // attacksound
        StateNum::S_NULL,                 // painstate
        0,                                // painchance
        SfxEnum::sfx_None,                // painsound
        StateNum::S_NULL,                 // meleestate
        StateNum::S_NULL,                 // missilestate
        StateNum::S_NULL,                 // deathstate
        StateNum::S_NULL,                 // xdeathstate
        SfxEnum::sfx_None,                // deathsound
        0.0,                              // speed
        20.0 * FRACUNIT,                  // radius
        16.0 * FRACUNIT,                  // height
        100,                              // mass
        0,                                // damage
        SfxEnum::sfx_None,                // activesound
        MapObjectFlag::MF_SPECIAL as u32, // flags
        StateNum::S_NULL,                 // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC18
        2010,                             // doomednum
        StateNum::S_ROCK,                 // spawnstate
        1000,                             // spawnhealth
        StateNum::S_NULL,                 // seestate
        SfxEnum::sfx_None,                // seesound
        8,                                // reactiontime
        SfxEnum::sfx_None,                // attacksound
        StateNum::S_NULL,                 // painstate
        0,                                // painchance
        SfxEnum::sfx_None,                // painsound
        StateNum::S_NULL,                 // meleestate
        StateNum::S_NULL,                 // missilestate
        StateNum::S_NULL,                 // deathstate
        StateNum::S_NULL,                 // xdeathstate
        SfxEnum::sfx_None,                // deathsound
        0.0,                              // speed
        20.0 * FRACUNIT,                  // radius
        16.0 * FRACUNIT,                  // height
        100,                              // mass
        0,                                // damage
        SfxEnum::sfx_None,                // activesound
        MapObjectFlag::MF_SPECIAL as u32, // flags
        StateNum::S_NULL,                 // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC19
        2046,                             // doomednum
        StateNum::S_BROK,                 // spawnstate
        1000,                             // spawnhealth
        StateNum::S_NULL,                 // seestate
        SfxEnum::sfx_None,                // seesound
        8,                                // reactiontime
        SfxEnum::sfx_None,                // attacksound
        StateNum::S_NULL,                 // painstate
        0,                                // painchance
        SfxEnum::sfx_None,                // painsound
        StateNum::S_NULL,                 // meleestate
        StateNum::S_NULL,                 // missilestate
        StateNum::S_NULL,                 // deathstate
        StateNum::S_NULL,                 // xdeathstate
        SfxEnum::sfx_None,                // deathsound
        0.0,                              // speed
        20.0 * FRACUNIT,                  // radius
        16.0 * FRACUNIT,                  // height
        100,                              // mass
        0,                                // damage
        SfxEnum::sfx_None,                // activesound
        MapObjectFlag::MF_SPECIAL as u32, // flags
        StateNum::S_NULL,                 // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC20
        2047,                             // doomednum
        StateNum::S_CELL,                 // spawnstate
        1000,                             // spawnhealth
        StateNum::S_NULL,                 // seestate
        SfxEnum::sfx_None,                // seesound
        8,                                // reactiontime
        SfxEnum::sfx_None,                // attacksound
        StateNum::S_NULL,                 // painstate
        0,                                // painchance
        SfxEnum::sfx_None,                // painsound
        StateNum::S_NULL,                 // meleestate
        StateNum::S_NULL,                 // missilestate
        StateNum::S_NULL,                 // deathstate
        StateNum::S_NULL,                 // xdeathstate
        SfxEnum::sfx_None,                // deathsound
        0.0,                              // speed
        20.0 * FRACUNIT,                  // radius
        16.0 * FRACUNIT,                  // height
        100,                              // mass
        0,                                // damage
        SfxEnum::sfx_None,                // activesound
        MapObjectFlag::MF_SPECIAL as u32, // flags
        StateNum::S_NULL,                 // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC21
        17,                               // doomednum
        StateNum::S_CELP,                 // spawnstate
        1000,                             // spawnhealth
        StateNum::S_NULL,                 // seestate
        SfxEnum::sfx_None,                // seesound
        8,                                // reactiontime
        SfxEnum::sfx_None,                // attacksound
        StateNum::S_NULL,                 // painstate
        0,                                // painchance
        SfxEnum::sfx_None,                // painsound
        StateNum::S_NULL,                 // meleestate
        StateNum::S_NULL,                 // missilestate
        StateNum::S_NULL,                 // deathstate
        StateNum::S_NULL,                 // xdeathstate
        SfxEnum::sfx_None,                // deathsound
        0.0,                              // speed
        20.0 * FRACUNIT,                  // radius
        16.0 * FRACUNIT,                  // height
        100,                              // mass
        0,                                // damage
        SfxEnum::sfx_None,                // activesound
        MapObjectFlag::MF_SPECIAL as u32, // flags
        StateNum::S_NULL,                 // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC22
        2008,                             // doomednum
        StateNum::S_SHEL,                 // spawnstate
        1000,                             // spawnhealth
        StateNum::S_NULL,                 // seestate
        SfxEnum::sfx_None,                // seesound
        8,                                // reactiontime
        SfxEnum::sfx_None,                // attacksound
        StateNum::S_NULL,                 // painstate
        0,                                // painchance
        SfxEnum::sfx_None,                // painsound
        StateNum::S_NULL,                 // meleestate
        StateNum::S_NULL,                 // missilestate
        StateNum::S_NULL,                 // deathstate
        StateNum::S_NULL,                 // xdeathstate
        SfxEnum::sfx_None,                // deathsound
        0.0,                              // speed
        20.0 * FRACUNIT,                  // radius
        16.0 * FRACUNIT,                  // height
        100,                              // mass
        0,                                // damage
        SfxEnum::sfx_None,                // activesound
        MapObjectFlag::MF_SPECIAL as u32, // flags
        StateNum::S_NULL,                 // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC23
        2049,                             // doomednum
        StateNum::S_SBOX,                 // spawnstate
        1000,                             // spawnhealth
        StateNum::S_NULL,                 // seestate
        SfxEnum::sfx_None,                // seesound
        8,                                // reactiontime
        SfxEnum::sfx_None,                // attacksound
        StateNum::S_NULL,                 // painstate
        0,                                // painchance
        SfxEnum::sfx_None,                // painsound
        StateNum::S_NULL,                 // meleestate
        StateNum::S_NULL,                 // missilestate
        StateNum::S_NULL,                 // deathstate
        StateNum::S_NULL,                 // xdeathstate
        SfxEnum::sfx_None,                // deathsound
        0.0,                              // speed
        20.0 * FRACUNIT,                  // radius
        16.0 * FRACUNIT,                  // height
        100,                              // mass
        0,                                // damage
        SfxEnum::sfx_None,                // activesound
        MapObjectFlag::MF_SPECIAL as u32, // flags
        StateNum::S_NULL,                 // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC24
        8,                                // doomednum
        StateNum::S_BPAK,                 // spawnstate
        1000,                             // spawnhealth
        StateNum::S_NULL,                 // seestate
        SfxEnum::sfx_None,                // seesound
        8,                                // reactiontime
        SfxEnum::sfx_None,                // attacksound
        StateNum::S_NULL,                 // painstate
        0,                                // painchance
        SfxEnum::sfx_None,                // painsound
        StateNum::S_NULL,                 // meleestate
        StateNum::S_NULL,                 // missilestate
        StateNum::S_NULL,                 // deathstate
        StateNum::S_NULL,                 // xdeathstate
        SfxEnum::sfx_None,                // deathsound
        0.0,                              // speed
        20.0 * FRACUNIT,                  // radius
        16.0 * FRACUNIT,                  // height
        100,                              // mass
        0,                                // damage
        SfxEnum::sfx_None,                // activesound
        MapObjectFlag::MF_SPECIAL as u32, // flags
        StateNum::S_NULL,                 // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC25
        2006,                             // doomednum
        StateNum::S_BFUG,                 // spawnstate
        1000,                             // spawnhealth
        StateNum::S_NULL,                 // seestate
        SfxEnum::sfx_None,                // seesound
        8,                                // reactiontime
        SfxEnum::sfx_None,                // attacksound
        StateNum::S_NULL,                 // painstate
        0,                                // painchance
        SfxEnum::sfx_None,                // painsound
        StateNum::S_NULL,                 // meleestate
        StateNum::S_NULL,                 // missilestate
        StateNum::S_NULL,                 // deathstate
        StateNum::S_NULL,                 // xdeathstate
        SfxEnum::sfx_None,                // deathsound
        0.0,                              // speed
        20.0 * FRACUNIT,                  // radius
        16.0 * FRACUNIT,                  // height
        100,                              // mass
        0,                                // damage
        SfxEnum::sfx_None,                // activesound
        MapObjectFlag::MF_SPECIAL as u32, // flags
        StateNum::S_NULL,                 // raisestate
    ),
    MapObjectInfo::new(
        // MT_CHAINGUN
        2002,                             // doomednum
        StateNum::S_MGUN,                 // spawnstate
        1000,                             // spawnhealth
        StateNum::S_NULL,                 // seestate
        SfxEnum::sfx_None,                // seesound
        8,                                // reactiontime
        SfxEnum::sfx_None,                // attacksound
        StateNum::S_NULL,                 // painstate
        0,                                // painchance
        SfxEnum::sfx_None,                // painsound
        StateNum::S_NULL,                 // meleestate
        StateNum::S_NULL,                 // missilestate
        StateNum::S_NULL,                 // deathstate
        StateNum::S_NULL,                 // xdeathstate
        SfxEnum::sfx_None,                // deathsound
        0.0,                              // speed
        20.0 * FRACUNIT,                  // radius
        16.0 * FRACUNIT,                  // height
        100,                              // mass
        0,                                // damage
        SfxEnum::sfx_None,                // activesound
        MapObjectFlag::MF_SPECIAL as u32, // flags
        StateNum::S_NULL,                 // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC26
        2005,                             // doomednum
        StateNum::S_CSAW,                 // spawnstate
        1000,                             // spawnhealth
        StateNum::S_NULL,                 // seestate
        SfxEnum::sfx_None,                // seesound
        8,                                // reactiontime
        SfxEnum::sfx_None,                // attacksound
        StateNum::S_NULL,                 // painstate
        0,                                // painchance
        SfxEnum::sfx_None,                // painsound
        StateNum::S_NULL,                 // meleestate
        StateNum::S_NULL,                 // missilestate
        StateNum::S_NULL,                 // deathstate
        StateNum::S_NULL,                 // xdeathstate
        SfxEnum::sfx_None,                // deathsound
        0.0,                              // speed
        20.0 * FRACUNIT,                  // radius
        16.0 * FRACUNIT,                  // height
        100,                              // mass
        0,                                // damage
        SfxEnum::sfx_None,                // activesound
        MapObjectFlag::MF_SPECIAL as u32, // flags
        StateNum::S_NULL,                 // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC27
        2003,                             // doomednum
        StateNum::S_LAUN,                 // spawnstate
        1000,                             // spawnhealth
        StateNum::S_NULL,                 // seestate
        SfxEnum::sfx_None,                // seesound
        8,                                // reactiontime
        SfxEnum::sfx_None,                // attacksound
        StateNum::S_NULL,                 // painstate
        0,                                // painchance
        SfxEnum::sfx_None,                // painsound
        StateNum::S_NULL,                 // meleestate
        StateNum::S_NULL,                 // missilestate
        StateNum::S_NULL,                 // deathstate
        StateNum::S_NULL,                 // xdeathstate
        SfxEnum::sfx_None,                // deathsound
        0.0,                              // speed
        20.0 * FRACUNIT,                  // radius
        16.0 * FRACUNIT,                  // height
        100,                              // mass
        0,                                // damage
        SfxEnum::sfx_None,                // activesound
        MapObjectFlag::MF_SPECIAL as u32, // flags
        StateNum::S_NULL,                 // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC28
        2004,                             // doomednum
        StateNum::S_PLAS,                 // spawnstate
        1000,                             // spawnhealth
        StateNum::S_NULL,                 // seestate
        SfxEnum::sfx_None,                // seesound
        8,                                // reactiontime
        SfxEnum::sfx_None,                // attacksound
        StateNum::S_NULL,                 // painstate
        0,                                // painchance
        SfxEnum::sfx_None,                // painsound
        StateNum::S_NULL,                 // meleestate
        StateNum::S_NULL,                 // missilestate
        StateNum::S_NULL,                 // deathstate
        StateNum::S_NULL,                 // xdeathstate
        SfxEnum::sfx_None,                // deathsound
        0.0,                              // speed
        20.0 * FRACUNIT,                  // radius
        16.0 * FRACUNIT,                  // height
        100,                              // mass
        0,                                // damage
        SfxEnum::sfx_None,                // activesound
        MapObjectFlag::MF_SPECIAL as u32, // flags
        StateNum::S_NULL,                 // raisestate
    ),
    MapObjectInfo::new(
        // MT_SHOTGUN
        2001,                             // doomednum
        StateNum::S_SHOT,                 // spawnstate
        1000,                             // spawnhealth
        StateNum::S_NULL,                 // seestate
        SfxEnum::sfx_None,                // seesound
        8,                                // reactiontime
        SfxEnum::sfx_None,                // attacksound
        StateNum::S_NULL,                 // painstate
        0,                                // painchance
        SfxEnum::sfx_None,                // painsound
        StateNum::S_NULL,                 // meleestate
        StateNum::S_NULL,                 // missilestate
        StateNum::S_NULL,                 // deathstate
        StateNum::S_NULL,                 // xdeathstate
        SfxEnum::sfx_None,                // deathsound
        0.0,                              // speed
        20.0 * FRACUNIT,                  // radius
        16.0 * FRACUNIT,                  // height
        100,                              // mass
        0,                                // damage
        SfxEnum::sfx_None,                // activesound
        MapObjectFlag::MF_SPECIAL as u32, // flags
        StateNum::S_NULL,                 // raisestate
    ),
    MapObjectInfo::new(
        // MT_SUPERSHOTGUN
        82,                               // doomednum
        StateNum::S_SHOT2,                // spawnstate
        1000,                             // spawnhealth
        StateNum::S_NULL,                 // seestate
        SfxEnum::sfx_None,                // seesound
        8,                                // reactiontime
        SfxEnum::sfx_None,                // attacksound
        StateNum::S_NULL,                 // painstate
        0,                                // painchance
        SfxEnum::sfx_None,                // painsound
        StateNum::S_NULL,                 // meleestate
        StateNum::S_NULL,                 // missilestate
        StateNum::S_NULL,                 // deathstate
        StateNum::S_NULL,                 // xdeathstate
        SfxEnum::sfx_None,                // deathsound
        0.0,                              // speed
        20.0 * FRACUNIT,                  // radius
        16.0 * FRACUNIT,                  // height
        100,                              // mass
        0,                                // damage
        SfxEnum::sfx_None,                // activesound
        MapObjectFlag::MF_SPECIAL as u32, // flags
        StateNum::S_NULL,                 // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC29
        85,                             // doomednum
        StateNum::S_TECHLAMP,           // spawnstate
        1000,                           // spawnhealth
        StateNum::S_NULL,               // seestate
        SfxEnum::sfx_None,              // seesound
        8,                              // reactiontime
        SfxEnum::sfx_None,              // attacksound
        StateNum::S_NULL,               // painstate
        0,                              // painchance
        SfxEnum::sfx_None,              // painsound
        StateNum::S_NULL,               // meleestate
        StateNum::S_NULL,               // missilestate
        StateNum::S_NULL,               // deathstate
        StateNum::S_NULL,               // xdeathstate
        SfxEnum::sfx_None,              // deathsound
        0.0,                            // speed
        16.0 * FRACUNIT,                // radius
        16.0 * FRACUNIT,                // height
        100,                            // mass
        0,                              // damage
        SfxEnum::sfx_None,              // activesound
        MapObjectFlag::MF_SOLID as u32, // flags
        StateNum::S_NULL,               // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC30
        86,                             // doomednum
        StateNum::S_TECH2LAMP,          // spawnstate
        1000,                           // spawnhealth
        StateNum::S_NULL,               // seestate
        SfxEnum::sfx_None,              // seesound
        8,                              // reactiontime
        SfxEnum::sfx_None,              // attacksound
        StateNum::S_NULL,               // painstate
        0,                              // painchance
        SfxEnum::sfx_None,              // painsound
        StateNum::S_NULL,               // meleestate
        StateNum::S_NULL,               // missilestate
        StateNum::S_NULL,               // deathstate
        StateNum::S_NULL,               // xdeathstate
        SfxEnum::sfx_None,              // deathsound
        0.0,                            // speed
        16.0 * FRACUNIT,                // radius
        16.0 * FRACUNIT,                // height
        100,                            // mass
        0,                              // damage
        SfxEnum::sfx_None,              // activesound
        MapObjectFlag::MF_SOLID as u32, // flags
        StateNum::S_NULL,               // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC31
        2028,                           // doomednum
        StateNum::S_COLU,               // spawnstate
        1000,                           // spawnhealth
        StateNum::S_NULL,               // seestate
        SfxEnum::sfx_None,              // seesound
        8,                              // reactiontime
        SfxEnum::sfx_None,              // attacksound
        StateNum::S_NULL,               // painstate
        0,                              // painchance
        SfxEnum::sfx_None,              // painsound
        StateNum::S_NULL,               // meleestate
        StateNum::S_NULL,               // missilestate
        StateNum::S_NULL,               // deathstate
        StateNum::S_NULL,               // xdeathstate
        SfxEnum::sfx_None,              // deathsound
        0.0,                            // speed
        16.0 * FRACUNIT,                // radius
        16.0 * FRACUNIT,                // height
        100,                            // mass
        0,                              // damage
        SfxEnum::sfx_None,              // activesound
        MapObjectFlag::MF_SOLID as u32, // flags
        StateNum::S_NULL,               // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC32
        30,                             // doomednum
        StateNum::S_TALLGRNCOL,         // spawnstate
        1000,                           // spawnhealth
        StateNum::S_NULL,               // seestate
        SfxEnum::sfx_None,              // seesound
        8,                              // reactiontime
        SfxEnum::sfx_None,              // attacksound
        StateNum::S_NULL,               // painstate
        0,                              // painchance
        SfxEnum::sfx_None,              // painsound
        StateNum::S_NULL,               // meleestate
        StateNum::S_NULL,               // missilestate
        StateNum::S_NULL,               // deathstate
        StateNum::S_NULL,               // xdeathstate
        SfxEnum::sfx_None,              // deathsound
        0.0,                            // speed
        16.0 * FRACUNIT,                // radius
        16.0 * FRACUNIT,                // height
        100,                            // mass
        0,                              // damage
        SfxEnum::sfx_None,              // activesound
        MapObjectFlag::MF_SOLID as u32, // flags
        StateNum::S_NULL,               // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC33
        31,                             // doomednum
        StateNum::S_SHRTGRNCOL,         // spawnstate
        1000,                           // spawnhealth
        StateNum::S_NULL,               // seestate
        SfxEnum::sfx_None,              // seesound
        8,                              // reactiontime
        SfxEnum::sfx_None,              // attacksound
        StateNum::S_NULL,               // painstate
        0,                              // painchance
        SfxEnum::sfx_None,              // painsound
        StateNum::S_NULL,               // meleestate
        StateNum::S_NULL,               // missilestate
        StateNum::S_NULL,               // deathstate
        StateNum::S_NULL,               // xdeathstate
        SfxEnum::sfx_None,              // deathsound
        0.0,                            // speed
        16.0 * FRACUNIT,                // radius
        16.0 * FRACUNIT,                // height
        100,                            // mass
        0,                              // damage
        SfxEnum::sfx_None,              // activesound
        MapObjectFlag::MF_SOLID as u32, // flags
        StateNum::S_NULL,               // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC34
        32,                             // doomednum
        StateNum::S_TALLREDCOL,         // spawnstate
        1000,                           // spawnhealth
        StateNum::S_NULL,               // seestate
        SfxEnum::sfx_None,              // seesound
        8,                              // reactiontime
        SfxEnum::sfx_None,              // attacksound
        StateNum::S_NULL,               // painstate
        0,                              // painchance
        SfxEnum::sfx_None,              // painsound
        StateNum::S_NULL,               // meleestate
        StateNum::S_NULL,               // missilestate
        StateNum::S_NULL,               // deathstate
        StateNum::S_NULL,               // xdeathstate
        SfxEnum::sfx_None,              // deathsound
        0.0,                            // speed
        16.0 * FRACUNIT,                // radius
        16.0 * FRACUNIT,                // height
        100,                            // mass
        0,                              // damage
        SfxEnum::sfx_None,              // activesound
        MapObjectFlag::MF_SOLID as u32, // flags
        StateNum::S_NULL,               // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC35
        33,                             // doomednum
        StateNum::S_SHRTREDCOL,         // spawnstate
        1000,                           // spawnhealth
        StateNum::S_NULL,               // seestate
        SfxEnum::sfx_None,              // seesound
        8,                              // reactiontime
        SfxEnum::sfx_None,              // attacksound
        StateNum::S_NULL,               // painstate
        0,                              // painchance
        SfxEnum::sfx_None,              // painsound
        StateNum::S_NULL,               // meleestate
        StateNum::S_NULL,               // missilestate
        StateNum::S_NULL,               // deathstate
        StateNum::S_NULL,               // xdeathstate
        SfxEnum::sfx_None,              // deathsound
        0.0,                            // speed
        16.0 * FRACUNIT,                // radius
        16.0 * FRACUNIT,                // height
        100,                            // mass
        0,                              // damage
        SfxEnum::sfx_None,              // activesound
        MapObjectFlag::MF_SOLID as u32, // flags
        StateNum::S_NULL,               // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC36
        37,                             // doomednum
        StateNum::S_SKULLCOL,           // spawnstate
        1000,                           // spawnhealth
        StateNum::S_NULL,               // seestate
        SfxEnum::sfx_None,              // seesound
        8,                              // reactiontime
        SfxEnum::sfx_None,              // attacksound
        StateNum::S_NULL,               // painstate
        0,                              // painchance
        SfxEnum::sfx_None,              // painsound
        StateNum::S_NULL,               // meleestate
        StateNum::S_NULL,               // missilestate
        StateNum::S_NULL,               // deathstate
        StateNum::S_NULL,               // xdeathstate
        SfxEnum::sfx_None,              // deathsound
        0.0,                            // speed
        16.0 * FRACUNIT,                // radius
        16.0 * FRACUNIT,                // height
        100,                            // mass
        0,                              // damage
        SfxEnum::sfx_None,              // activesound
        MapObjectFlag::MF_SOLID as u32, // flags
        StateNum::S_NULL,               // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC37
        36,                             // doomednum
        StateNum::S_HEARTCOL,           // spawnstate
        1000,                           // spawnhealth
        StateNum::S_NULL,               // seestate
        SfxEnum::sfx_None,              // seesound
        8,                              // reactiontime
        SfxEnum::sfx_None,              // attacksound
        StateNum::S_NULL,               // painstate
        0,                              // painchance
        SfxEnum::sfx_None,              // painsound
        StateNum::S_NULL,               // meleestate
        StateNum::S_NULL,               // missilestate
        StateNum::S_NULL,               // deathstate
        StateNum::S_NULL,               // xdeathstate
        SfxEnum::sfx_None,              // deathsound
        0.0,                            // speed
        16.0 * FRACUNIT,                // radius
        16.0 * FRACUNIT,                // height
        100,                            // mass
        0,                              // damage
        SfxEnum::sfx_None,              // activesound
        MapObjectFlag::MF_SOLID as u32, // flags
        StateNum::S_NULL,               // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC38
        41,                             // doomednum
        StateNum::S_EVILEYE,            // spawnstate
        1000,                           // spawnhealth
        StateNum::S_NULL,               // seestate
        SfxEnum::sfx_None,              // seesound
        8,                              // reactiontime
        SfxEnum::sfx_None,              // attacksound
        StateNum::S_NULL,               // painstate
        0,                              // painchance
        SfxEnum::sfx_None,              // painsound
        StateNum::S_NULL,               // meleestate
        StateNum::S_NULL,               // missilestate
        StateNum::S_NULL,               // deathstate
        StateNum::S_NULL,               // xdeathstate
        SfxEnum::sfx_None,              // deathsound
        0.0,                            // speed
        16.0 * FRACUNIT,                // radius
        16.0 * FRACUNIT,                // height
        100,                            // mass
        0,                              // damage
        SfxEnum::sfx_None,              // activesound
        MapObjectFlag::MF_SOLID as u32, // flags
        StateNum::S_NULL,               // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC39
        42,                             // doomednum
        StateNum::S_FLOATSKULL,         // spawnstate
        1000,                           // spawnhealth
        StateNum::S_NULL,               // seestate
        SfxEnum::sfx_None,              // seesound
        8,                              // reactiontime
        SfxEnum::sfx_None,              // attacksound
        StateNum::S_NULL,               // painstate
        0,                              // painchance
        SfxEnum::sfx_None,              // painsound
        StateNum::S_NULL,               // meleestate
        StateNum::S_NULL,               // missilestate
        StateNum::S_NULL,               // deathstate
        StateNum::S_NULL,               // xdeathstate
        SfxEnum::sfx_None,              // deathsound
        0.0,                            // speed
        16.0 * FRACUNIT,                // radius
        16.0 * FRACUNIT,                // height
        100,                            // mass
        0,                              // damage
        SfxEnum::sfx_None,              // activesound
        MapObjectFlag::MF_SOLID as u32, // flags
        StateNum::S_NULL,               // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC40
        43,                             // doomednum
        StateNum::S_TORCHTREE,          // spawnstate
        1000,                           // spawnhealth
        StateNum::S_NULL,               // seestate
        SfxEnum::sfx_None,              // seesound
        8,                              // reactiontime
        SfxEnum::sfx_None,              // attacksound
        StateNum::S_NULL,               // painstate
        0,                              // painchance
        SfxEnum::sfx_None,              // painsound
        StateNum::S_NULL,               // meleestate
        StateNum::S_NULL,               // missilestate
        StateNum::S_NULL,               // deathstate
        StateNum::S_NULL,               // xdeathstate
        SfxEnum::sfx_None,              // deathsound
        0.0,                            // speed
        16.0 * FRACUNIT,                // radius
        16.0 * FRACUNIT,                // height
        100,                            // mass
        0,                              // damage
        SfxEnum::sfx_None,              // activesound
        MapObjectFlag::MF_SOLID as u32, // flags
        StateNum::S_NULL,               // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC41
        44,                             // doomednum
        StateNum::S_BLUETORCH,          // spawnstate
        1000,                           // spawnhealth
        StateNum::S_NULL,               // seestate
        SfxEnum::sfx_None,              // seesound
        8,                              // reactiontime
        SfxEnum::sfx_None,              // attacksound
        StateNum::S_NULL,               // painstate
        0,                              // painchance
        SfxEnum::sfx_None,              // painsound
        StateNum::S_NULL,               // meleestate
        StateNum::S_NULL,               // missilestate
        StateNum::S_NULL,               // deathstate
        StateNum::S_NULL,               // xdeathstate
        SfxEnum::sfx_None,              // deathsound
        0.0,                            // speed
        16.0 * FRACUNIT,                // radius
        16.0 * FRACUNIT,                // height
        100,                            // mass
        0,                              // damage
        SfxEnum::sfx_None,              // activesound
        MapObjectFlag::MF_SOLID as u32, // flags
        StateNum::S_NULL,               // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC42
        45,                             // doomednum
        StateNum::S_GREENTORCH,         // spawnstate
        1000,                           // spawnhealth
        StateNum::S_NULL,               // seestate
        SfxEnum::sfx_None,              // seesound
        8,                              // reactiontime
        SfxEnum::sfx_None,              // attacksound
        StateNum::S_NULL,               // painstate
        0,                              // painchance
        SfxEnum::sfx_None,              // painsound
        StateNum::S_NULL,               // meleestate
        StateNum::S_NULL,               // missilestate
        StateNum::S_NULL,               // deathstate
        StateNum::S_NULL,               // xdeathstate
        SfxEnum::sfx_None,              // deathsound
        0.0,                            // speed
        16.0 * FRACUNIT,                // radius
        16.0 * FRACUNIT,                // height
        100,                            // mass
        0,                              // damage
        SfxEnum::sfx_None,              // activesound
        MapObjectFlag::MF_SOLID as u32, // flags
        StateNum::S_NULL,               // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC43
        46,                             // doomednum
        StateNum::S_REDTORCH,           // spawnstate
        1000,                           // spawnhealth
        StateNum::S_NULL,               // seestate
        SfxEnum::sfx_None,              // seesound
        8,                              // reactiontime
        SfxEnum::sfx_None,              // attacksound
        StateNum::S_NULL,               // painstate
        0,                              // painchance
        SfxEnum::sfx_None,              // painsound
        StateNum::S_NULL,               // meleestate
        StateNum::S_NULL,               // missilestate
        StateNum::S_NULL,               // deathstate
        StateNum::S_NULL,               // xdeathstate
        SfxEnum::sfx_None,              // deathsound
        0.0,                            // speed
        16.0 * FRACUNIT,                // radius
        16.0 * FRACUNIT,                // height
        100,                            // mass
        0,                              // damage
        SfxEnum::sfx_None,              // activesound
        MapObjectFlag::MF_SOLID as u32, // flags
        StateNum::S_NULL,               // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC44
        55,                             // doomednum
        StateNum::S_BTORCHSHRT,         // spawnstate
        1000,                           // spawnhealth
        StateNum::S_NULL,               // seestate
        SfxEnum::sfx_None,              // seesound
        8,                              // reactiontime
        SfxEnum::sfx_None,              // attacksound
        StateNum::S_NULL,               // painstate
        0,                              // painchance
        SfxEnum::sfx_None,              // painsound
        StateNum::S_NULL,               // meleestate
        StateNum::S_NULL,               // missilestate
        StateNum::S_NULL,               // deathstate
        StateNum::S_NULL,               // xdeathstate
        SfxEnum::sfx_None,              // deathsound
        0.0,                            // speed
        16.0 * FRACUNIT,                // radius
        16.0 * FRACUNIT,                // height
        100,                            // mass
        0,                              // damage
        SfxEnum::sfx_None,              // activesound
        MapObjectFlag::MF_SOLID as u32, // flags
        StateNum::S_NULL,               // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC45
        56,                             // doomednum
        StateNum::S_GTORCHSHRT,         // spawnstate
        1000,                           // spawnhealth
        StateNum::S_NULL,               // seestate
        SfxEnum::sfx_None,              // seesound
        8,                              // reactiontime
        SfxEnum::sfx_None,              // attacksound
        StateNum::S_NULL,               // painstate
        0,                              // painchance
        SfxEnum::sfx_None,              // painsound
        StateNum::S_NULL,               // meleestate
        StateNum::S_NULL,               // missilestate
        StateNum::S_NULL,               // deathstate
        StateNum::S_NULL,               // xdeathstate
        SfxEnum::sfx_None,              // deathsound
        0.0,                            // speed
        16.0 * FRACUNIT,                // radius
        16.0 * FRACUNIT,                // height
        100,                            // mass
        0,                              // damage
        SfxEnum::sfx_None,              // activesound
        MapObjectFlag::MF_SOLID as u32, // flags
        StateNum::S_NULL,               // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC46
        57,                             // doomednum
        StateNum::S_RTORCHSHRT,         // spawnstate
        1000,                           // spawnhealth
        StateNum::S_NULL,               // seestate
        SfxEnum::sfx_None,              // seesound
        8,                              // reactiontime
        SfxEnum::sfx_None,              // attacksound
        StateNum::S_NULL,               // painstate
        0,                              // painchance
        SfxEnum::sfx_None,              // painsound
        StateNum::S_NULL,               // meleestate
        StateNum::S_NULL,               // missilestate
        StateNum::S_NULL,               // deathstate
        StateNum::S_NULL,               // xdeathstate
        SfxEnum::sfx_None,              // deathsound
        0.0,                            // speed
        16.0 * FRACUNIT,                // radius
        16.0 * FRACUNIT,                // height
        100,                            // mass
        0,                              // damage
        SfxEnum::sfx_None,              // activesound
        MapObjectFlag::MF_SOLID as u32, // flags
        StateNum::S_NULL,               // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC47
        47,                             // doomednum
        StateNum::S_STALAGTITE,         // spawnstate
        1000,                           // spawnhealth
        StateNum::S_NULL,               // seestate
        SfxEnum::sfx_None,              // seesound
        8,                              // reactiontime
        SfxEnum::sfx_None,              // attacksound
        StateNum::S_NULL,               // painstate
        0,                              // painchance
        SfxEnum::sfx_None,              // painsound
        StateNum::S_NULL,               // meleestate
        StateNum::S_NULL,               // missilestate
        StateNum::S_NULL,               // deathstate
        StateNum::S_NULL,               // xdeathstate
        SfxEnum::sfx_None,              // deathsound
        0.0,                            // speed
        16.0 * FRACUNIT,                // radius
        16.0 * FRACUNIT,                // height
        100,                            // mass
        0,                              // damage
        SfxEnum::sfx_None,              // activesound
        MapObjectFlag::MF_SOLID as u32, // flags
        StateNum::S_NULL,               // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC48
        48,                             // doomednum
        StateNum::S_TECHPILLAR,         // spawnstate
        1000,                           // spawnhealth
        StateNum::S_NULL,               // seestate
        SfxEnum::sfx_None,              // seesound
        8,                              // reactiontime
        SfxEnum::sfx_None,              // attacksound
        StateNum::S_NULL,               // painstate
        0,                              // painchance
        SfxEnum::sfx_None,              // painsound
        StateNum::S_NULL,               // meleestate
        StateNum::S_NULL,               // missilestate
        StateNum::S_NULL,               // deathstate
        StateNum::S_NULL,               // xdeathstate
        SfxEnum::sfx_None,              // deathsound
        0.0,                            // speed
        16.0 * FRACUNIT,                // radius
        16.0 * FRACUNIT,                // height
        100,                            // mass
        0,                              // damage
        SfxEnum::sfx_None,              // activesound
        MapObjectFlag::MF_SOLID as u32, // flags
        StateNum::S_NULL,               // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC49
        34,                     // doomednum
        StateNum::S_CANDLESTIK, // spawnstate
        1000,                   // spawnhealth
        StateNum::S_NULL,       // seestate
        SfxEnum::sfx_None,      // seesound
        8,                      // reactiontime
        SfxEnum::sfx_None,      // attacksound
        StateNum::S_NULL,       // painstate
        0,                      // painchance
        SfxEnum::sfx_None,      // painsound
        StateNum::S_NULL,       // meleestate
        StateNum::S_NULL,       // missilestate
        StateNum::S_NULL,       // deathstate
        StateNum::S_NULL,       // xdeathstate
        SfxEnum::sfx_None,      // deathsound
        0.0,                    // speed
        20.0 * FRACUNIT,        // radius
        16.0 * FRACUNIT,        // height
        100,                    // mass
        0,                      // damage
        SfxEnum::sfx_None,      // activesound
        0,                      // flags
        StateNum::S_NULL,       // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC50
        35,                             // doomednum
        StateNum::S_CANDELABRA,         // spawnstate
        1000,                           // spawnhealth
        StateNum::S_NULL,               // seestate
        SfxEnum::sfx_None,              // seesound
        8,                              // reactiontime
        SfxEnum::sfx_None,              // attacksound
        StateNum::S_NULL,               // painstate
        0,                              // painchance
        SfxEnum::sfx_None,              // painsound
        StateNum::S_NULL,               // meleestate
        StateNum::S_NULL,               // missilestate
        StateNum::S_NULL,               // deathstate
        StateNum::S_NULL,               // xdeathstate
        SfxEnum::sfx_None,              // deathsound
        0.0,                            // speed
        16.0 * FRACUNIT,                // radius
        16.0 * FRACUNIT,                // height
        100,                            // mass
        0,                              // damage
        SfxEnum::sfx_None,              // activesound
        MapObjectFlag::MF_SOLID as u32, // flags
        StateNum::S_NULL,               // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC51
        49,                       // doomednum
        StateNum::S_BLOODYTWITCH, // spawnstate
        1000,                     // spawnhealth
        StateNum::S_NULL,         // seestate
        SfxEnum::sfx_None,        // seesound
        8,                        // reactiontime
        SfxEnum::sfx_None,        // attacksound
        StateNum::S_NULL,         // painstate
        0,                        // painchance
        SfxEnum::sfx_None,        // painsound
        StateNum::S_NULL,         // meleestate
        StateNum::S_NULL,         // missilestate
        StateNum::S_NULL,         // deathstate
        StateNum::S_NULL,         // xdeathstate
        SfxEnum::sfx_None,        // deathsound
        0.0,                      // speed
        16.0 * FRACUNIT,          // radius
        68.0 * FRACUNIT,          // height
        100,                      // mass
        0,                        // damage
        SfxEnum::sfx_None,        // activesound
        MapObjectFlag::MF_SOLID as u32
            | MapObjectFlag::MF_SPAWNCEILING as u32
            | MapObjectFlag::MF_NOGRAVITY as u32, // flags
        StateNum::S_NULL,         // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC52
        50,                // doomednum
        StateNum::S_MEAT2, // spawnstate
        1000,              // spawnhealth
        StateNum::S_NULL,  // seestate
        SfxEnum::sfx_None, // seesound
        8,                 // reactiontime
        SfxEnum::sfx_None, // attacksound
        StateNum::S_NULL,  // painstate
        0,                 // painchance
        SfxEnum::sfx_None, // painsound
        StateNum::S_NULL,  // meleestate
        StateNum::S_NULL,  // missilestate
        StateNum::S_NULL,  // deathstate
        StateNum::S_NULL,  // xdeathstate
        SfxEnum::sfx_None, // deathsound
        0.0,               // speed
        16.0 * FRACUNIT,   // radius
        84.0 * FRACUNIT,   // height
        100,               // mass
        0,                 // damage
        SfxEnum::sfx_None, // activesound
        MapObjectFlag::MF_SOLID as u32
            | MapObjectFlag::MF_SPAWNCEILING as u32
            | MapObjectFlag::MF_NOGRAVITY as u32, // flags
        StateNum::S_NULL,  // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC53
        51,                // doomednum
        StateNum::S_MEAT3, // spawnstate
        1000,              // spawnhealth
        StateNum::S_NULL,  // seestate
        SfxEnum::sfx_None, // seesound
        8,                 // reactiontime
        SfxEnum::sfx_None, // attacksound
        StateNum::S_NULL,  // painstate
        0,                 // painchance
        SfxEnum::sfx_None, // painsound
        StateNum::S_NULL,  // meleestate
        StateNum::S_NULL,  // missilestate
        StateNum::S_NULL,  // deathstate
        StateNum::S_NULL,  // xdeathstate
        SfxEnum::sfx_None, // deathsound
        0.0,               // speed
        16.0 * FRACUNIT,   // radius
        84.0 * FRACUNIT,   // height
        100,               // mass
        0,                 // damage
        SfxEnum::sfx_None, // activesound
        MapObjectFlag::MF_SOLID as u32
            | MapObjectFlag::MF_SPAWNCEILING as u32
            | MapObjectFlag::MF_NOGRAVITY as u32, // flags
        StateNum::S_NULL,  // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC54
        52,                // doomednum
        StateNum::S_MEAT4, // spawnstate
        1000,              // spawnhealth
        StateNum::S_NULL,  // seestate
        SfxEnum::sfx_None, // seesound
        8,                 // reactiontime
        SfxEnum::sfx_None, // attacksound
        StateNum::S_NULL,  // painstate
        0,                 // painchance
        SfxEnum::sfx_None, // painsound
        StateNum::S_NULL,  // meleestate
        StateNum::S_NULL,  // missilestate
        StateNum::S_NULL,  // deathstate
        StateNum::S_NULL,  // xdeathstate
        SfxEnum::sfx_None, // deathsound
        0.0,               // speed
        16.0 * FRACUNIT,   // radius
        68.0 * FRACUNIT,   // height
        100,               // mass
        0,                 // damage
        SfxEnum::sfx_None, // activesound
        MapObjectFlag::MF_SOLID as u32
            | MapObjectFlag::MF_SPAWNCEILING as u32
            | MapObjectFlag::MF_NOGRAVITY as u32, // flags
        StateNum::S_NULL,  // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC55
        53,                // doomednum
        StateNum::S_MEAT5, // spawnstate
        1000,              // spawnhealth
        StateNum::S_NULL,  // seestate
        SfxEnum::sfx_None, // seesound
        8,                 // reactiontime
        SfxEnum::sfx_None, // attacksound
        StateNum::S_NULL,  // painstate
        0,                 // painchance
        SfxEnum::sfx_None, // painsound
        StateNum::S_NULL,  // meleestate
        StateNum::S_NULL,  // missilestate
        StateNum::S_NULL,  // deathstate
        StateNum::S_NULL,  // xdeathstate
        SfxEnum::sfx_None, // deathsound
        0.0,               // speed
        16.0 * FRACUNIT,   // radius
        52.0 * FRACUNIT,   // height
        100,               // mass
        0,                 // damage
        SfxEnum::sfx_None, // activesound
        MapObjectFlag::MF_SOLID as u32
            | MapObjectFlag::MF_SPAWNCEILING as u32
            | MapObjectFlag::MF_NOGRAVITY as u32, // flags
        StateNum::S_NULL,  // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC56
        59,                // doomednum
        StateNum::S_MEAT2, // spawnstate
        1000,              // spawnhealth
        StateNum::S_NULL,  // seestate
        SfxEnum::sfx_None, // seesound
        8,                 // reactiontime
        SfxEnum::sfx_None, // attacksound
        StateNum::S_NULL,  // painstate
        0,                 // painchance
        SfxEnum::sfx_None, // painsound
        StateNum::S_NULL,  // meleestate
        StateNum::S_NULL,  // missilestate
        StateNum::S_NULL,  // deathstate
        StateNum::S_NULL,  // xdeathstate
        SfxEnum::sfx_None, // deathsound
        0.0,               // speed
        20.0 * FRACUNIT,   // radius
        84.0 * FRACUNIT,   // height
        100,               // mass
        0,                 // damage
        SfxEnum::sfx_None, // activesound
        MapObjectFlag::MF_SPAWNCEILING as u32
            | MapObjectFlag::MF_NOGRAVITY as u32, // flags
        StateNum::S_NULL,  // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC57
        60,                // doomednum
        StateNum::S_MEAT4, // spawnstate
        1000,              // spawnhealth
        StateNum::S_NULL,  // seestate
        SfxEnum::sfx_None, // seesound
        8,                 // reactiontime
        SfxEnum::sfx_None, // attacksound
        StateNum::S_NULL,  // painstate
        0,                 // painchance
        SfxEnum::sfx_None, // painsound
        StateNum::S_NULL,  // meleestate
        StateNum::S_NULL,  // missilestate
        StateNum::S_NULL,  // deathstate
        StateNum::S_NULL,  // xdeathstate
        SfxEnum::sfx_None, // deathsound
        0.0,               // speed
        20.0 * FRACUNIT,   // radius
        68.0 * FRACUNIT,   // height
        100,               // mass
        0,                 // damage
        SfxEnum::sfx_None, // activesound
        MapObjectFlag::MF_SPAWNCEILING as u32
            | MapObjectFlag::MF_NOGRAVITY as u32, // flags
        StateNum::S_NULL,  // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC58
        61,                // doomednum
        StateNum::S_MEAT3, // spawnstate
        1000,              // spawnhealth
        StateNum::S_NULL,  // seestate
        SfxEnum::sfx_None, // seesound
        8,                 // reactiontime
        SfxEnum::sfx_None, // attacksound
        StateNum::S_NULL,  // painstate
        0,                 // painchance
        SfxEnum::sfx_None, // painsound
        StateNum::S_NULL,  // meleestate
        StateNum::S_NULL,  // missilestate
        StateNum::S_NULL,  // deathstate
        StateNum::S_NULL,  // xdeathstate
        SfxEnum::sfx_None, // deathsound
        0.0,               // speed
        20.0 * FRACUNIT,   // radius
        52.0 * FRACUNIT,   // height
        100,               // mass
        0,                 // damage
        SfxEnum::sfx_None, // activesound
        MapObjectFlag::MF_SPAWNCEILING as u32
            | MapObjectFlag::MF_NOGRAVITY as u32, // flags
        StateNum::S_NULL,  // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC59
        62,                // doomednum
        StateNum::S_MEAT5, // spawnstate
        1000,              // spawnhealth
        StateNum::S_NULL,  // seestate
        SfxEnum::sfx_None, // seesound
        8,                 // reactiontime
        SfxEnum::sfx_None, // attacksound
        StateNum::S_NULL,  // painstate
        0,                 // painchance
        SfxEnum::sfx_None, // painsound
        StateNum::S_NULL,  // meleestate
        StateNum::S_NULL,  // missilestate
        StateNum::S_NULL,  // deathstate
        StateNum::S_NULL,  // xdeathstate
        SfxEnum::sfx_None, // deathsound
        0.0,               // speed
        20.0 * FRACUNIT,   // radius
        52.0 * FRACUNIT,   // height
        100,               // mass
        0,                 // damage
        SfxEnum::sfx_None, // activesound
        MapObjectFlag::MF_SPAWNCEILING as u32
            | MapObjectFlag::MF_NOGRAVITY as u32, // flags
        StateNum::S_NULL,  // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC60
        63,                       // doomednum
        StateNum::S_BLOODYTWITCH, // spawnstate
        1000,                     // spawnhealth
        StateNum::S_NULL,         // seestate
        SfxEnum::sfx_None,        // seesound
        8,                        // reactiontime
        SfxEnum::sfx_None,        // attacksound
        StateNum::S_NULL,         // painstate
        0,                        // painchance
        SfxEnum::sfx_None,        // painsound
        StateNum::S_NULL,         // meleestate
        StateNum::S_NULL,         // missilestate
        StateNum::S_NULL,         // deathstate
        StateNum::S_NULL,         // xdeathstate
        SfxEnum::sfx_None,        // deathsound
        0.0,                      // speed
        20.0 * FRACUNIT,          // radius
        68.0 * FRACUNIT,          // height
        100,                      // mass
        0,                        // damage
        SfxEnum::sfx_None,        // activesound
        MapObjectFlag::MF_SPAWNCEILING as u32
            | MapObjectFlag::MF_NOGRAVITY as u32, // flags
        StateNum::S_NULL,         // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC61
        22,                    // doomednum
        StateNum::S_HEAD_DIE6, // spawnstate
        1000,                  // spawnhealth
        StateNum::S_NULL,      // seestate
        SfxEnum::sfx_None,     // seesound
        8,                     // reactiontime
        SfxEnum::sfx_None,     // attacksound
        StateNum::S_NULL,      // painstate
        0,                     // painchance
        SfxEnum::sfx_None,     // painsound
        StateNum::S_NULL,      // meleestate
        StateNum::S_NULL,      // missilestate
        StateNum::S_NULL,      // deathstate
        StateNum::S_NULL,      // xdeathstate
        SfxEnum::sfx_None,     // deathsound
        0.0,                   // speed
        20.0 * FRACUNIT,       // radius
        16.0 * FRACUNIT,       // height
        100,                   // mass
        0,                     // damage
        SfxEnum::sfx_None,     // activesound
        0,                     // flags
        StateNum::S_NULL,      // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC62
        15,                    // doomednum
        StateNum::S_PLAY_DIE7, // spawnstate
        1000,                  // spawnhealth
        StateNum::S_NULL,      // seestate
        SfxEnum::sfx_None,     // seesound
        8,                     // reactiontime
        SfxEnum::sfx_None,     // attacksound
        StateNum::S_NULL,      // painstate
        0,                     // painchance
        SfxEnum::sfx_None,     // painsound
        StateNum::S_NULL,      // meleestate
        StateNum::S_NULL,      // missilestate
        StateNum::S_NULL,      // deathstate
        StateNum::S_NULL,      // xdeathstate
        SfxEnum::sfx_None,     // deathsound
        0.0,                   // speed
        20.0 * FRACUNIT,       // radius
        16.0 * FRACUNIT,       // height
        100,                   // mass
        0,                     // damage
        SfxEnum::sfx_None,     // activesound
        0,                     // flags
        StateNum::S_NULL,      // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC63
        18,                    // doomednum
        StateNum::S_POSS_DIE5, // spawnstate
        1000,                  // spawnhealth
        StateNum::S_NULL,      // seestate
        SfxEnum::sfx_None,     // seesound
        8,                     // reactiontime
        SfxEnum::sfx_None,     // attacksound
        StateNum::S_NULL,      // painstate
        0,                     // painchance
        SfxEnum::sfx_None,     // painsound
        StateNum::S_NULL,      // meleestate
        StateNum::S_NULL,      // missilestate
        StateNum::S_NULL,      // deathstate
        StateNum::S_NULL,      // xdeathstate
        SfxEnum::sfx_None,     // deathsound
        0.0,                   // speed
        20.0 * FRACUNIT,       // radius
        16.0 * FRACUNIT,       // height
        100,                   // mass
        0,                     // damage
        SfxEnum::sfx_None,     // activesound
        0,                     // flags
        StateNum::S_NULL,      // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC64
        21,                    // doomednum
        StateNum::S_SARG_DIE6, // spawnstate
        1000,                  // spawnhealth
        StateNum::S_NULL,      // seestate
        SfxEnum::sfx_None,     // seesound
        8,                     // reactiontime
        SfxEnum::sfx_None,     // attacksound
        StateNum::S_NULL,      // painstate
        0,                     // painchance
        SfxEnum::sfx_None,     // painsound
        StateNum::S_NULL,      // meleestate
        StateNum::S_NULL,      // missilestate
        StateNum::S_NULL,      // deathstate
        StateNum::S_NULL,      // xdeathstate
        SfxEnum::sfx_None,     // deathsound
        0.0,                   // speed
        20.0 * FRACUNIT,       // radius
        16.0 * FRACUNIT,       // height
        100,                   // mass
        0,                     // damage
        SfxEnum::sfx_None,     // activesound
        0,                     // flags
        StateNum::S_NULL,      // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC65
        23,                     // doomednum
        StateNum::S_SKULL_DIE6, // spawnstate
        1000,                   // spawnhealth
        StateNum::S_NULL,       // seestate
        SfxEnum::sfx_None,      // seesound
        8,                      // reactiontime
        SfxEnum::sfx_None,      // attacksound
        StateNum::S_NULL,       // painstate
        0,                      // painchance
        SfxEnum::sfx_None,      // painsound
        StateNum::S_NULL,       // meleestate
        StateNum::S_NULL,       // missilestate
        StateNum::S_NULL,       // deathstate
        StateNum::S_NULL,       // xdeathstate
        SfxEnum::sfx_None,      // deathsound
        0.0,                    // speed
        20.0 * FRACUNIT,        // radius
        16.0 * FRACUNIT,        // height
        100,                    // mass
        0,                      // damage
        SfxEnum::sfx_None,      // activesound
        0,                      // flags
        StateNum::S_NULL,       // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC66
        20,                    // doomednum
        StateNum::S_TROO_DIE5, // spawnstate
        1000,                  // spawnhealth
        StateNum::S_NULL,      // seestate
        SfxEnum::sfx_None,     // seesound
        8,                     // reactiontime
        SfxEnum::sfx_None,     // attacksound
        StateNum::S_NULL,      // painstate
        0,                     // painchance
        SfxEnum::sfx_None,     // painsound
        StateNum::S_NULL,      // meleestate
        StateNum::S_NULL,      // missilestate
        StateNum::S_NULL,      // deathstate
        StateNum::S_NULL,      // xdeathstate
        SfxEnum::sfx_None,     // deathsound
        0.0,                   // speed
        20.0 * FRACUNIT,       // radius
        16.0 * FRACUNIT,       // height
        100,                   // mass
        0,                     // damage
        SfxEnum::sfx_None,     // activesound
        0,                     // flags
        StateNum::S_NULL,      // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC67
        19,                    // doomednum
        StateNum::S_POSS_DIE5, // spawnstate
        1000,                  // spawnhealth
        StateNum::S_NULL,      // seestate
        SfxEnum::sfx_None,     // seesound
        8,                     // reactiontime
        SfxEnum::sfx_None,     // attacksound
        StateNum::S_NULL,      // painstate
        0,                     // painchance
        SfxEnum::sfx_None,     // painsound
        StateNum::S_NULL,      // meleestate
        StateNum::S_NULL,      // missilestate
        StateNum::S_NULL,      // deathstate
        StateNum::S_NULL,      // xdeathstate
        SfxEnum::sfx_None,     // deathsound
        0.0,                   // speed
        20.0 * FRACUNIT,       // radius
        16.0 * FRACUNIT,       // height
        100,                   // mass
        0,                     // damage
        SfxEnum::sfx_None,     // activesound
        0,                     // flags
        StateNum::S_NULL,      // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC68
        10,                     // doomednum
        StateNum::S_PLAY_XDIE9, // spawnstate
        1000,                   // spawnhealth
        StateNum::S_NULL,       // seestate
        SfxEnum::sfx_None,      // seesound
        8,                      // reactiontime
        SfxEnum::sfx_None,      // attacksound
        StateNum::S_NULL,       // painstate
        0,                      // painchance
        SfxEnum::sfx_None,      // painsound
        StateNum::S_NULL,       // meleestate
        StateNum::S_NULL,       // missilestate
        StateNum::S_NULL,       // deathstate
        StateNum::S_NULL,       // xdeathstate
        SfxEnum::sfx_None,      // deathsound
        0.0,                    // speed
        20.0 * FRACUNIT,        // radius
        16.0 * FRACUNIT,        // height
        100,                    // mass
        0,                      // damage
        SfxEnum::sfx_None,      // activesound
        0,                      // flags
        StateNum::S_NULL,       // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC69
        12,                     // doomednum
        StateNum::S_PLAY_XDIE9, // spawnstate
        1000,                   // spawnhealth
        StateNum::S_NULL,       // seestate
        SfxEnum::sfx_None,      // seesound
        8,                      // reactiontime
        SfxEnum::sfx_None,      // attacksound
        StateNum::S_NULL,       // painstate
        0,                      // painchance
        SfxEnum::sfx_None,      // painsound
        StateNum::S_NULL,       // meleestate
        StateNum::S_NULL,       // missilestate
        StateNum::S_NULL,       // deathstate
        StateNum::S_NULL,       // xdeathstate
        SfxEnum::sfx_None,      // deathsound
        0.0,                    // speed
        20.0 * FRACUNIT,        // radius
        16.0 * FRACUNIT,        // height
        100,                    // mass
        0,                      // damage
        SfxEnum::sfx_None,      // activesound
        0,                      // flags
        StateNum::S_NULL,       // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC70
        28,                             // doomednum
        StateNum::S_HEADSONSTICK,       // spawnstate
        1000,                           // spawnhealth
        StateNum::S_NULL,               // seestate
        SfxEnum::sfx_None,              // seesound
        8,                              // reactiontime
        SfxEnum::sfx_None,              // attacksound
        StateNum::S_NULL,               // painstate
        0,                              // painchance
        SfxEnum::sfx_None,              // painsound
        StateNum::S_NULL,               // meleestate
        StateNum::S_NULL,               // missilestate
        StateNum::S_NULL,               // deathstate
        StateNum::S_NULL,               // xdeathstate
        SfxEnum::sfx_None,              // deathsound
        0.0,                            // speed
        16.0 * FRACUNIT,                // radius
        16.0 * FRACUNIT,                // height
        100,                            // mass
        0,                              // damage
        SfxEnum::sfx_None,              // activesound
        MapObjectFlag::MF_SOLID as u32, // flags
        StateNum::S_NULL,               // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC71
        24,                // doomednum
        StateNum::S_GIBS,  // spawnstate
        1000,              // spawnhealth
        StateNum::S_NULL,  // seestate
        SfxEnum::sfx_None, // seesound
        8,                 // reactiontime
        SfxEnum::sfx_None, // attacksound
        StateNum::S_NULL,  // painstate
        0,                 // painchance
        SfxEnum::sfx_None, // painsound
        StateNum::S_NULL,  // meleestate
        StateNum::S_NULL,  // missilestate
        StateNum::S_NULL,  // deathstate
        StateNum::S_NULL,  // xdeathstate
        SfxEnum::sfx_None, // deathsound
        0.0,               // speed
        20.0 * FRACUNIT,   // radius
        16.0 * FRACUNIT,   // height
        100,               // mass
        0,                 // damage
        SfxEnum::sfx_None, // activesound
        0,                 // flags
        StateNum::S_NULL,  // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC72
        27,                             // doomednum
        StateNum::S_HEADONASTICK,       // spawnstate
        1000,                           // spawnhealth
        StateNum::S_NULL,               // seestate
        SfxEnum::sfx_None,              // seesound
        8,                              // reactiontime
        SfxEnum::sfx_None,              // attacksound
        StateNum::S_NULL,               // painstate
        0,                              // painchance
        SfxEnum::sfx_None,              // painsound
        StateNum::S_NULL,               // meleestate
        StateNum::S_NULL,               // missilestate
        StateNum::S_NULL,               // deathstate
        StateNum::S_NULL,               // xdeathstate
        SfxEnum::sfx_None,              // deathsound
        0.0,                            // speed
        16.0 * FRACUNIT,                // radius
        16.0 * FRACUNIT,                // height
        100,                            // mass
        0,                              // damage
        SfxEnum::sfx_None,              // activesound
        MapObjectFlag::MF_SOLID as u32, // flags
        StateNum::S_NULL,               // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC73
        29,                             // doomednum
        StateNum::S_HEADCANDLES,        // spawnstate
        1000,                           // spawnhealth
        StateNum::S_NULL,               // seestate
        SfxEnum::sfx_None,              // seesound
        8,                              // reactiontime
        SfxEnum::sfx_None,              // attacksound
        StateNum::S_NULL,               // painstate
        0,                              // painchance
        SfxEnum::sfx_None,              // painsound
        StateNum::S_NULL,               // meleestate
        StateNum::S_NULL,               // missilestate
        StateNum::S_NULL,               // deathstate
        StateNum::S_NULL,               // xdeathstate
        SfxEnum::sfx_None,              // deathsound
        0.0,                            // speed
        16.0 * FRACUNIT,                // radius
        16.0 * FRACUNIT,                // height
        100,                            // mass
        0,                              // damage
        SfxEnum::sfx_None,              // activesound
        MapObjectFlag::MF_SOLID as u32, // flags
        StateNum::S_NULL,               // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC74
        25,                             // doomednum
        StateNum::S_DEADSTICK,          // spawnstate
        1000,                           // spawnhealth
        StateNum::S_NULL,               // seestate
        SfxEnum::sfx_None,              // seesound
        8,                              // reactiontime
        SfxEnum::sfx_None,              // attacksound
        StateNum::S_NULL,               // painstate
        0,                              // painchance
        SfxEnum::sfx_None,              // painsound
        StateNum::S_NULL,               // meleestate
        StateNum::S_NULL,               // missilestate
        StateNum::S_NULL,               // deathstate
        StateNum::S_NULL,               // xdeathstate
        SfxEnum::sfx_None,              // deathsound
        0.0,                            // speed
        16.0 * FRACUNIT,                // radius
        16.0 * FRACUNIT,                // height
        100,                            // mass
        0,                              // damage
        SfxEnum::sfx_None,              // activesound
        MapObjectFlag::MF_SOLID as u32, // flags
        StateNum::S_NULL,               // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC75
        26,                             // doomednum
        StateNum::S_LIVESTICK,          // spawnstate
        1000,                           // spawnhealth
        StateNum::S_NULL,               // seestate
        SfxEnum::sfx_None,              // seesound
        8,                              // reactiontime
        SfxEnum::sfx_None,              // attacksound
        StateNum::S_NULL,               // painstate
        0,                              // painchance
        SfxEnum::sfx_None,              // painsound
        StateNum::S_NULL,               // meleestate
        StateNum::S_NULL,               // missilestate
        StateNum::S_NULL,               // deathstate
        StateNum::S_NULL,               // xdeathstate
        SfxEnum::sfx_None,              // deathsound
        0.0,                            // speed
        16.0 * FRACUNIT,                // radius
        16.0 * FRACUNIT,                // height
        100,                            // mass
        0,                              // damage
        SfxEnum::sfx_None,              // activesound
        MapObjectFlag::MF_SOLID as u32, // flags
        StateNum::S_NULL,               // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC76
        54,                             // doomednum
        StateNum::S_BIGTREE,            // spawnstate
        1000,                           // spawnhealth
        StateNum::S_NULL,               // seestate
        SfxEnum::sfx_None,              // seesound
        8,                              // reactiontime
        SfxEnum::sfx_None,              // attacksound
        StateNum::S_NULL,               // painstate
        0,                              // painchance
        SfxEnum::sfx_None,              // painsound
        StateNum::S_NULL,               // meleestate
        StateNum::S_NULL,               // missilestate
        StateNum::S_NULL,               // deathstate
        StateNum::S_NULL,               // xdeathstate
        SfxEnum::sfx_None,              // deathsound
        0.0,                            // speed
        32.0 * FRACUNIT,                // radius
        16.0 * FRACUNIT,                // height
        100,                            // mass
        0,                              // damage
        SfxEnum::sfx_None,              // activesound
        MapObjectFlag::MF_SOLID as u32, // flags
        StateNum::S_NULL,               // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC77
        70,                             // doomednum
        StateNum::S_BBAR1,              // spawnstate
        1000,                           // spawnhealth
        StateNum::S_NULL,               // seestate
        SfxEnum::sfx_None,              // seesound
        8,                              // reactiontime
        SfxEnum::sfx_None,              // attacksound
        StateNum::S_NULL,               // painstate
        0,                              // painchance
        SfxEnum::sfx_None,              // painsound
        StateNum::S_NULL,               // meleestate
        StateNum::S_NULL,               // missilestate
        StateNum::S_NULL,               // deathstate
        StateNum::S_NULL,               // xdeathstate
        SfxEnum::sfx_None,              // deathsound
        0.0,                            // speed
        16.0 * FRACUNIT,                // radius
        16.0 * FRACUNIT,                // height
        100,                            // mass
        0,                              // damage
        SfxEnum::sfx_None,              // activesound
        MapObjectFlag::MF_SOLID as u32, // flags
        StateNum::S_NULL,               // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC78
        73,                     // doomednum
        StateNum::S_HANGNOGUTS, // spawnstate
        1000,                   // spawnhealth
        StateNum::S_NULL,       // seestate
        SfxEnum::sfx_None,      // seesound
        8,                      // reactiontime
        SfxEnum::sfx_None,      // attacksound
        StateNum::S_NULL,       // painstate
        0,                      // painchance
        SfxEnum::sfx_None,      // painsound
        StateNum::S_NULL,       // meleestate
        StateNum::S_NULL,       // missilestate
        StateNum::S_NULL,       // deathstate
        StateNum::S_NULL,       // xdeathstate
        SfxEnum::sfx_None,      // deathsound
        0.0,                    // speed
        16.0 * FRACUNIT,        // radius
        88.0 * FRACUNIT,        // height
        100,                    // mass
        0,                      // damage
        SfxEnum::sfx_None,      // activesound
        MapObjectFlag::MF_SOLID as u32
            | MapObjectFlag::MF_SPAWNCEILING as u32
            | MapObjectFlag::MF_NOGRAVITY as u32, // flags
        StateNum::S_NULL,       // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC79
        74,                       // doomednum
        StateNum::S_HANGBNOBRAIN, // spawnstate
        1000,                     // spawnhealth
        StateNum::S_NULL,         // seestate
        SfxEnum::sfx_None,        // seesound
        8,                        // reactiontime
        SfxEnum::sfx_None,        // attacksound
        StateNum::S_NULL,         // painstate
        0,                        // painchance
        SfxEnum::sfx_None,        // painsound
        StateNum::S_NULL,         // meleestate
        StateNum::S_NULL,         // missilestate
        StateNum::S_NULL,         // deathstate
        StateNum::S_NULL,         // xdeathstate
        SfxEnum::sfx_None,        // deathsound
        0.0,                      // speed
        16.0 * FRACUNIT,          // radius
        88.0 * FRACUNIT,          // height
        100,                      // mass
        0,                        // damage
        SfxEnum::sfx_None,        // activesound
        MapObjectFlag::MF_SOLID as u32
            | MapObjectFlag::MF_SPAWNCEILING as u32
            | MapObjectFlag::MF_NOGRAVITY as u32, // flags
        StateNum::S_NULL,         // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC80
        75,                      // doomednum
        StateNum::S_HANGTLOOKDN, // spawnstate
        1000,                    // spawnhealth
        StateNum::S_NULL,        // seestate
        SfxEnum::sfx_None,       // seesound
        8,                       // reactiontime
        SfxEnum::sfx_None,       // attacksound
        StateNum::S_NULL,        // painstate
        0,                       // painchance
        SfxEnum::sfx_None,       // painsound
        StateNum::S_NULL,        // meleestate
        StateNum::S_NULL,        // missilestate
        StateNum::S_NULL,        // deathstate
        StateNum::S_NULL,        // xdeathstate
        SfxEnum::sfx_None,       // deathsound
        0.0,                     // speed
        16.0 * FRACUNIT,         // radius
        64.0 * FRACUNIT,         // height
        100,                     // mass
        0,                       // damage
        SfxEnum::sfx_None,       // activesound
        MapObjectFlag::MF_SOLID as u32
            | MapObjectFlag::MF_SPAWNCEILING as u32
            | MapObjectFlag::MF_NOGRAVITY as u32, // flags
        StateNum::S_NULL,        // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC81
        76,                     // doomednum
        StateNum::S_HANGTSKULL, // spawnstate
        1000,                   // spawnhealth
        StateNum::S_NULL,       // seestate
        SfxEnum::sfx_None,      // seesound
        8,                      // reactiontime
        SfxEnum::sfx_None,      // attacksound
        StateNum::S_NULL,       // painstate
        0,                      // painchance
        SfxEnum::sfx_None,      // painsound
        StateNum::S_NULL,       // meleestate
        StateNum::S_NULL,       // missilestate
        StateNum::S_NULL,       // deathstate
        StateNum::S_NULL,       // xdeathstate
        SfxEnum::sfx_None,      // deathsound
        0.0,                    // speed
        16.0 * FRACUNIT,        // radius
        64.0 * FRACUNIT,        // height
        100,                    // mass
        0,                      // damage
        SfxEnum::sfx_None,      // activesound
        MapObjectFlag::MF_SOLID as u32
            | MapObjectFlag::MF_SPAWNCEILING as u32
            | MapObjectFlag::MF_NOGRAVITY as u32, // flags
        StateNum::S_NULL,       // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC82
        77,                      // doomednum
        StateNum::S_HANGTLOOKUP, // spawnstate
        1000,                    // spawnhealth
        StateNum::S_NULL,        // seestate
        SfxEnum::sfx_None,       // seesound
        8,                       // reactiontime
        SfxEnum::sfx_None,       // attacksound
        StateNum::S_NULL,        // painstate
        0,                       // painchance
        SfxEnum::sfx_None,       // painsound
        StateNum::S_NULL,        // meleestate
        StateNum::S_NULL,        // missilestate
        StateNum::S_NULL,        // deathstate
        StateNum::S_NULL,        // xdeathstate
        SfxEnum::sfx_None,       // deathsound
        0.0,                     // speed
        16.0 * FRACUNIT,         // radius
        64.0 * FRACUNIT,         // height
        100,                     // mass
        0,                       // damage
        SfxEnum::sfx_None,       // activesound
        MapObjectFlag::MF_SOLID as u32
            | MapObjectFlag::MF_SPAWNCEILING as u32
            | MapObjectFlag::MF_NOGRAVITY as u32, // flags
        StateNum::S_NULL,        // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC83
        78,                       // doomednum
        StateNum::S_HANGTNOBRAIN, // spawnstate
        1000,                     // spawnhealth
        StateNum::S_NULL,         // seestate
        SfxEnum::sfx_None,        // seesound
        8,                        // reactiontime
        SfxEnum::sfx_None,        // attacksound
        StateNum::S_NULL,         // painstate
        0,                        // painchance
        SfxEnum::sfx_None,        // painsound
        StateNum::S_NULL,         // meleestate
        StateNum::S_NULL,         // missilestate
        StateNum::S_NULL,         // deathstate
        StateNum::S_NULL,         // xdeathstate
        SfxEnum::sfx_None,        // deathsound
        0.0,                      // speed
        16.0 * FRACUNIT,          // radius
        64.0 * FRACUNIT,          // height
        100,                      // mass
        0,                        // damage
        SfxEnum::sfx_None,        // activesound
        MapObjectFlag::MF_SOLID as u32
            | MapObjectFlag::MF_SPAWNCEILING as u32
            | MapObjectFlag::MF_NOGRAVITY as u32, // flags
        StateNum::S_NULL,         // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC84
        79,                                  // doomednum
        StateNum::S_COLONGIBS,               // spawnstate
        1000,                                // spawnhealth
        StateNum::S_NULL,                    // seestate
        SfxEnum::sfx_None,                   // seesound
        8,                                   // reactiontime
        SfxEnum::sfx_None,                   // attacksound
        StateNum::S_NULL,                    // painstate
        0,                                   // painchance
        SfxEnum::sfx_None,                   // painsound
        StateNum::S_NULL,                    // meleestate
        StateNum::S_NULL,                    // missilestate
        StateNum::S_NULL,                    // deathstate
        StateNum::S_NULL,                    // xdeathstate
        SfxEnum::sfx_None,                   // deathsound
        0.0,                                 // speed
        20.0 * FRACUNIT,                     // radius
        16.0 * FRACUNIT,                     // height
        100,                                 // mass
        0,                                   // damage
        SfxEnum::sfx_None,                   // activesound
        MapObjectFlag::MF_NOBLOCKMAP as u32, // flags
        StateNum::S_NULL,                    // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC85
        80,                                  // doomednum
        StateNum::S_SMALLPOOL,               // spawnstate
        1000,                                // spawnhealth
        StateNum::S_NULL,                    // seestate
        SfxEnum::sfx_None,                   // seesound
        8,                                   // reactiontime
        SfxEnum::sfx_None,                   // attacksound
        StateNum::S_NULL,                    // painstate
        0,                                   // painchance
        SfxEnum::sfx_None,                   // painsound
        StateNum::S_NULL,                    // meleestate
        StateNum::S_NULL,                    // missilestate
        StateNum::S_NULL,                    // deathstate
        StateNum::S_NULL,                    // xdeathstate
        SfxEnum::sfx_None,                   // deathsound
        0.0,                                 // speed
        20.0 * FRACUNIT,                     // radius
        16.0 * FRACUNIT,                     // height
        100,                                 // mass
        0,                                   // damage
        SfxEnum::sfx_None,                   // activesound
        MapObjectFlag::MF_NOBLOCKMAP as u32, // flags
        StateNum::S_NULL,                    // raisestate
    ),
    MapObjectInfo::new(
        // MT_MISC86
        81,                                  // doomednum
        StateNum::S_BRAINSTEM,               // spawnstate
        1000,                                // spawnhealth
        StateNum::S_NULL,                    // seestate
        SfxEnum::sfx_None,                   // seesound
        8,                                   // reactiontime
        SfxEnum::sfx_None,                   // attacksound
        StateNum::S_NULL,                    // painstate
        0,                                   // painchance
        SfxEnum::sfx_None,                   // painsound
        StateNum::S_NULL,                    // meleestate
        StateNum::S_NULL,                    // missilestate
        StateNum::S_NULL,                    // deathstate
        StateNum::S_NULL,                    // xdeathstate
        SfxEnum::sfx_None,                   // deathsound
        0.0,                                 // speed
        20.0 * FRACUNIT,                     // radius
        16.0 * FRACUNIT,                     // height
        100,                                 // mass
        0,                                   // damage
        SfxEnum::sfx_None,                   // activesound
        MapObjectFlag::MF_NOBLOCKMAP as u32, // flags
        StateNum::S_NULL,                    // raisestate
    ),
];
