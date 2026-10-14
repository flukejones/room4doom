use log::{info, warn};

use super::PicData;
pub use wad::animations::AnimationDef;
use wad::{WadData, animations::VANILLA_ANIMATIONS};
#[derive(Debug, Default)]
pub struct PicAnimation {
    is_texture: bool,
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

    pub fn num_pics(&self) -> usize {
        self.numpics
    }

    /// Initialise animated texture/flat definitions from WAD data. OG:
    /// P_InitPicAnims.
    pub fn init(pic_data: &PicData, wad: &WadData) -> Vec<Self> {
        let mut anims = Vec::with_capacity(VANILLA_ANIMATIONS.len());

        for def in &VANILLA_ANIMATIONS {
            if let Some(anim) = Self::resolve(
                pic_data,
                def.is_texture,
                def.start_name,
                def.end_name,
                def.speed,
            ) {
                anims.push(anim);
            }
        }

        if let Some(lump) = wad.get_lump("ANIMATED") {
            let boom_entries = wad::boom::parse_animated(&lump.data);
            let mut added = 0;
            for entry in &boom_entries {
                let already = anims.iter().any(|a| {
                    a.is_texture == entry.is_texture
                        && a.basepic == {
                            if entry.is_texture {
                                pic_data
                                    .wallpic_num_for_name(&entry.start_name)
                                    .unwrap_or(usize::MAX)
                            } else {
                                pic_data
                                    .flat_num_for_name(&entry.start_name)
                                    .unwrap_or(usize::MAX)
                            }
                        }
                });
                if already {
                    continue;
                }
                if let Some(anim) = Self::resolve(
                    pic_data,
                    entry.is_texture,
                    &entry.start_name,
                    &entry.end_name,
                    entry.speed as usize,
                ) {
                    anims.push(anim);
                    added += 1;
                }
            }
            if added > 0 {
                info!("Extended animation list with {added} entries from ANIMATED lump");
            }
        }

        info!("Initialised {} animated textures", anims.len());
        anims
    }

    fn resolve(
        pic_data: &PicData,
        is_texture: bool,
        start_name: &str,
        end_name: &str,
        speed: usize,
    ) -> Option<Self> {
        let (basepic, picnum) = if is_texture {
            let start = pic_data.wallpic_num_for_name(start_name)?;
            let end = pic_data.wallpic_num_for_name(end_name)?;
            (start, end)
        } else {
            let start = pic_data.flat_num_for_name(start_name)?;
            let end = pic_data.flat_num_for_name(end_name)?;
            (start, end)
        };

        let numpics = picnum - basepic + 1;
        if numpics < 2 {
            warn!("init_animations: bad cycle from {start_name} to {end_name} ({numpics} frames)");
            return None;
        }

        Some(Self {
            is_texture,
            basepic,
            numpics,
            speed,
        })
    }
}
