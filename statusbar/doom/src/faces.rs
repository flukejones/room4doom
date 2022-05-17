use gamestate_traits::PixelBuf;
use wad::{
    lumps::{WadPatch, WAD_PATCH},
    WadData,
};

// TODO: export this from game not here
pub(crate) const TICRATE: usize = 35;

const PAIN_FACES: usize = 5;
const STRAIGHT_FACES: usize = 3;
const TURN_FACES: usize = 2;
const SPECIAL_FACES: usize = 3;
const EXTRA_FACES: usize = 3;

const FACE_STRIDE: usize = STRAIGHT_FACES + TURN_FACES + SPECIAL_FACES;
const FACE_COUNT: usize = FACE_STRIDE * PAIN_FACES + EXTRA_FACES;

const TURNOFFSET: usize = STRAIGHT_FACES;
const OUCHOFFSET: usize = TURNOFFSET + TURN_FACES;
const EVILGRINOFFSET: usize = OUCHOFFSET + 1;
const RAMPAGEOFFSET: usize = EVILGRINOFFSET + 1;
const GODFACE: usize = PAIN_FACES * FACE_STRIDE;
const DEADFACE: usize = GODFACE + 1;

const EVILGRINCOUNT: usize = 2 * TICRATE;
const STRAIGHTFACECOUNT: usize = TICRATE / 2;
const TURNCOUNT: usize = 1 * TICRATE;
const OUCHCOUNT: usize = 1 * TICRATE;
const RAMPAGEDELAY: usize = 2 * TICRATE;

const MUCH_PAIN: usize = 20;

pub(crate) struct DoomguyFace {
    faces: [WadPatch; FACE_COUNT],
}

impl DoomguyFace {
    pub(crate) fn new(wad: &WadData) -> Self {
        let mut face_num = 0;
        let mut faces: [WadPatch; FACE_COUNT] = [WAD_PATCH; FACE_COUNT];
        for p in 0..PAIN_FACES {
            for s in 0..STRAIGHT_FACES {
                let lump = wad.get_lump(&format!("STFST{p}{s}")).unwrap();
                faces[face_num] = WadPatch::from_lump(lump);
                face_num += 1;
            }
            // turn right
            let lump = wad.get_lump(&format!("STFTR{p}0")).unwrap();
            faces[face_num] = WadPatch::from_lump(lump);
            face_num += 1;
            // turn left
            let lump = wad.get_lump(&format!("STFTL{p}0")).unwrap();
            faces[face_num] = WadPatch::from_lump(lump);
            face_num += 1;
            // ouch
            let lump = wad.get_lump(&format!("STFOUCH{p}")).unwrap();
            faces[face_num] = WadPatch::from_lump(lump);
            face_num += 1;
            // evil
            let lump = wad.get_lump(&format!("STFEVL{p}")).unwrap();
            faces[face_num] = WadPatch::from_lump(lump);
            face_num += 1;
            // kill
            let lump = wad.get_lump(&format!("STFKILL{p}")).unwrap();
            faces[face_num] = WadPatch::from_lump(lump);
            face_num += 1;
        }
        // immortal
        let lump = wad.get_lump(&format!("STFGOD0")).unwrap();
        faces[face_num] = WadPatch::from_lump(lump);
        face_num += 1;
        // dead
        let lump = wad.get_lump(&format!("STFDEAD0")).unwrap();
        faces[face_num] = WadPatch::from_lump(lump);
        face_num += 1;

        Self { faces }
    }

    // fn draw_face(&self, mut big: bool, upper: bool, buffer: &mut PixelBuf) {
    //     let screen_width = buffer.width();
    //     let screen_height = buffer.height();
    //     if upper {
    //         big = true;
    //     }

    //     let mut x;
    //     let mut y;
    //     if big && !upper {
    //         let patch = self.get_patch("STFB1");
    //         y = if upper {
    //             0
    //         } else {
    //             screen_height - patch.height as i32
    //         };
    //         x = screen_width / 2 - patch.width as i32 / 2;
    //         self.draw_patch(patch, x, y, buffer);
    //     };

    //     let patch = if self.status.health < 20 {
    //         self.get_patch("STFST41")
    //     } else if self.status.health < 40 {
    //         self.get_patch("STFST31")
    //     } else if self.status.health < 60 {
    //         self.get_patch("STFST21")
    //     } else if self.status.health < 80 {
    //         self.get_patch("STFST11")
    //     } else {
    //         self.get_patch("STFST01")
    //     };

    //     let offset_x = patch.width as i32 / 2;
    //     let offset_y = patch.height as i32;
    //     if upper || big {
    //         x = screen_width / 2 - patch.width as i32 / 2;
    //         y = if upper {
    //             1
    //         } else {
    //             screen_height - patch.height as i32
    //         };
    //     } else {
    //         x = offset_x;
    //         y = screen_height - offset_y
    //     };
    //     self.draw_patch(patch, x, y, buffer);
    // }
}
