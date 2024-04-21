use log::info;

use crate::PicData;
#[derive(Debug, Default)]
pub struct PicAnimation {
    is_texture: bool,
    picnum: usize,
    basepic: usize,
    numpics: usize,
    speed: usize,
}

impl PicAnimation {
    pub fn update(&mut self, textures: &mut PicData, level_time: usize) {
        for i in self.basepic..self.basepic + self.numpics {
            let pic = self.basepic + ((level_time / self.speed + i) % self.numpics);
            if self.is_texture {
                textures.wall_translation[i] = pic;
            } else {
                textures.flat_translation[i] = pic;
            }
        }
    }

    /// Doom function name `P_InitPicAnims`
    pub fn init(pic_data: &PicData) -> Vec<PicAnimation> {
        let mut anims = Vec::with_capacity(ANIM_DEFS.len());

        for def in ANIM_DEFS {
            let mut animation = PicAnimation::default();
            if def.is_texture {
                if let Some(start_num) = pic_data.wallpic_num_for_name(def.start_name) {
                    if let Some(end_num) = pic_data.wallpic_num_for_name(def.end_name) {
                        animation.picnum = end_num;
                        animation.basepic = start_num;
                    }
                } else {
                    continue;
                }
            } else if let Some(start_num) = pic_data.flat_num_for_name(def.start_name) {
                if let Some(end_num) = pic_data.flat_num_for_name(def.end_name) {
                    animation.picnum = end_num;
                    animation.basepic = start_num;
                }
            } else {
                continue;
            }

            //TODO: temporary texture only
            animation.is_texture = def.is_texture;
            animation.numpics = animation.picnum - animation.basepic + 1;
            if animation.numpics < 2 {
                panic!(
                    "init_animations: bad cycle from {} to {}",
                    def.start_name, def.end_name
                );
            }
            animation.speed = def.speed;

            anims.push(animation);
        }
        info!("Initialised animated textures");

        anims
    }
}

pub struct AnimationDef {
    is_texture: bool,
    end_name: &'static str,
    start_name: &'static str,
    speed: usize,
}

impl AnimationDef {
    const fn new(
        is_texture: bool,
        end_name: &'static str,
        start_name: &'static str,
        speed: usize,
    ) -> Self {
        Self {
            is_texture,
            end_name,
            start_name,
            speed,
        }
    }
}

const ANIM_DEFS: [AnimationDef; 22] = [
    AnimationDef::new(false, "NUKAGE3", "NUKAGE1", 8),
    AnimationDef::new(false, "FWATER4", "FWATER1", 8),
    AnimationDef::new(false, "SWATER4", "SWATER1", 8),
    AnimationDef::new(false, "LAVA4", "LAVA1", 8),
    AnimationDef::new(false, "BLOOD3", "BLOOD1", 8),
    // DOOM II flat animations.
    AnimationDef::new(false, "RROCK08", "RROCK05", 8),
    AnimationDef::new(false, "SLIME04", "SLIME01", 8),
    AnimationDef::new(false, "SLIME08", "SLIME05", 8),
    AnimationDef::new(false, "SLIME12", "SLIME09", 8),
    AnimationDef::new(true, "BLODGR4", "BLODGR1", 8),
    AnimationDef::new(true, "SLADRIP3", "SLADRIP1", 8),
    AnimationDef::new(true, "BLODRIP4", "BLODRIP1", 8),
    AnimationDef::new(true, "FIREWALL", "FIREWALA", 8),
    AnimationDef::new(true, "GSTFONT3", "GSTFONT1", 8),
    AnimationDef::new(true, "FIRELAVA", "FIRELAV3", 8),
    AnimationDef::new(true, "FIREMAG3", "FIREMAG1", 8),
    AnimationDef::new(true, "FIREBLU2", "FIREBLU1", 8),
    AnimationDef::new(true, "ROCKRED3", "ROCKRED1", 8),
    AnimationDef::new(true, "BFALL4", "BFALL1", 8),
    AnimationDef::new(true, "SFALL4", "SFALL1", 8),
    AnimationDef::new(true, "WFALL4", "WFALL1", 8),
    AnimationDef::new(true, "DBRAIN4", "DBRAIN1", 8),
];
