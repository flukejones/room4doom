use log::{debug, error};
use wad::lumps::WadPatch;

#[derive(Debug, Clone, Copy)]
pub struct SpriteFrame {
    pub rotate: i8,
    /// Index of the patch to use per view-angle
    pub lump: [i32; 8],
    /// Is the view-angle flipped?
    pub flip: [u8; 8],
}

impl SpriteFrame {
    fn new() -> Self {
        Self {
            rotate: -1,
            lump: [-1; 8],
            flip: [0; 8],
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct SpriteDef {
    num_frames: i32,
    pub frames: Vec<SpriteFrame>,
}

/// Initialise the sprite definitions based on the names and appended bits
pub fn init_spritedefs(names: &[&str], patches: &[WadPatch]) -> Vec<SpriteDef> {
    // Sure, we can function without sprites
    if names.is_empty() {
        panic!("No sprites used, sprite name list is empty");
    }

    // positioning matches names[];
    let mut sprites: Vec<SpriteDef> = vec![SpriteDef::default(); names.len()];

    for (index, name) in names.iter().enumerate() {
        let mut max_frame = -1;
        let mut sprite_tmp = [SpriteFrame::new(); 29];

        // scan the patches. Each patch has the lump name stored.
        for (pindex, patch) in patches.iter().enumerate() {
            if patch.name.starts_with(name) {
                let frame = patch.name.as_bytes()[4] - 'A' as u8;
                let rotation = patch.name.as_bytes()[5] - '0' as u8;

                debug!(
                    "1. Matched {name}, {}, frame {}, rotate {}",
                    patch.name, frame, rotation
                );
                // TODO: check for modified game and fetch new lump from name

                install_sprite(
                    pindex,
                    frame,
                    rotation,
                    false,
                    &mut max_frame,
                    &mut sprite_tmp,
                    name,
                );

                if patch.name.len() >= 7 {
                    let frame = patch.name.as_bytes()[6] - 'A' as u8;
                    let rotation = patch.name.as_bytes()[7] - '0' as u8;
                    debug!(
                        "2. Matched {name}, {}, frame {}, rotate {}",
                        patch.name, frame, rotation
                    );
                    install_sprite(
                        pindex,
                        frame,
                        rotation,
                        true,
                        &mut max_frame,
                        &mut sprite_tmp,
                        name,
                    );
                }
            }
        }

        if max_frame == -1 {
            sprites[index].num_frames = 0;
            continue;
        }

        max_frame += 1;
        for frame in 0..max_frame {
            let rot = sprite_tmp[frame as usize].rotate;
            if rot == -1 {
                // no rotations were found for that frame at all
                error!(
                    "init_sprites: No patches found for {} frame {}",
                    names[index],
                    (frame as u8 + 'A' as u8) as char,
                );
                break;
            }
            if rot == 0 {
                break;
            }
            if rot == 1 {
                for rotation in 0..8 {
                    if sprite_tmp[frame as usize].lump[rotation] == -1 {
                        error!(
                            "init_sprites: Sprite {} frame {} is missing rotations",
                            names[index],
                            (frame as u8 + 'A' as u8) as char,
                        );
                        dbg!(sprite_tmp[frame as usize].lump);
                    }
                }
                break;
            }
        }

        sprites[index].num_frames = max_frame;
        sprites[index].frames = sprite_tmp.to_vec();
    }

    sprites
}

fn install_sprite(
    patch: usize,
    frame: u8,
    mut rotation: u8,
    flipped: bool,
    max_frame: &mut i32,
    tmp: &mut [SpriteFrame; 29],
    name: &str,
) {
    if frame >= 29 || rotation > 8 {
        error!("install_sprite: Bad frame characters in patch {}", name);
    }

    if frame as i32 > *max_frame {
        *max_frame = frame as i32;
    }

    if rotation == 0 {
        // Check existing if any
        let mut sprite = &mut tmp[frame as usize];

        if sprite.rotate == 0 {
            error!(
                "install_sprite: Sprite {} frame {} has multiple rot=0 lump",
                name,
                ('A' as u8 + frame) as char,
            );
        }
        if sprite.rotate == 1 {
            error!(
                "install_sprite: Sprite {} frame {} has has rotations and a rot=0 lump",
                name,
                ('A' as u8 + frame) as char,
            );
        }

        sprite.rotate = 0;
        for r in 0..8 {
            sprite.lump[r] = patch as i32;
            sprite.flip[r] = flipped as u8;
        }
        return;
    }

    // the lump is only used for one rotation
    let mut sprite = &mut tmp[frame as usize];
    // Not effective due to defaults to false
    if sprite.rotate == 0 {
        error!(
            "install_sprite: Sprite {} frame {} has rotations and a rot=0 lump",
            name,
            ('A' as u8 + frame) as char,
        );
    }

    sprite.rotate = 1;
    // make 0 based
    rotation -= 1;
    if sprite.lump[rotation as usize] != -1 {
        error!(
            "install_sprite: Sprite {} : {} : {} has two lumps mapped to it",
            name,
            ('A' as u8 + frame) as char,
            ('1' as u8 + rotation) as char
        );
    }

    sprite.lump[rotation as usize] = patch as i32;
    sprite.flip[rotation as usize] = flipped as u8;
}
