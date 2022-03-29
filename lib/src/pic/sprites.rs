use log::{debug, warn};
use wad::lumps::WadPatch;

pub(super) struct SpriteFrame {
    rotate: bool,
    lump: String,
    flip: [u8; 8],
}

pub(super) struct SpriteDef {
    num_frames: i32,
    frames: Vec<SpriteFrame>,
}

/// Initialise the sprite definitions based on the names and appended bits
pub fn init_spritedefs(names: &[&str], patches: &[WadPatch]) {
    // Sure, we can function without sprites
    if names.is_empty() {
        warn!("No sprites used, sprite name list is empty");
        return;
    }

    let mut sprites: Vec<SpriteDef> = Vec::with_capacity(names.len());

    for name in names {
        let mut max_frame = -1;

        // scan the patches. Each patch has the lump name stored.
        for patch in patches {
            if patch.name.contains(name) {
                debug!(
                    "Matched {name}, {}, frame {}, rotate {}",
                    patch.name,
                    patch.name.as_bytes()[4] - 'A' as u8,
                    patch.name.as_bytes()[5] - '0' as u8
                );

                // TODO: check for modified game and fetch new lump from name

                if patch.name.len() >= 7 {
                    debug!(
                        "Matched {name}, {}, frame {}, rotate {}",
                        patch.name,
                        patch.name.as_bytes()[6] - 'A' as u8,
                        patch.name.as_bytes()[7] - '0' as u8
                    );
                }
            }
        }
    }
}
