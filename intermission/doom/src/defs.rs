use game_traits::util::get_num_sprites;
use gameplay::MAXPLAYERS;
use std::mem::MaybeUninit;
use wad::{lumps::WadPatch, WadData};

pub(crate) const TICRATE: i32 = 35;
pub(crate) const SHOW_NEXT_LOC_DELAY: i32 = 4;

pub(crate) struct Patches {
    pub nums: [WadPatch; 10],
    pub minus: WadPatch,
    pub percent: WadPatch,
    pub kills: WadPatch,
    pub secret: WadPatch,
    pub sp_secret: WadPatch,
    pub items: WadPatch,
    pub frags: WadPatch,
    pub colon: WadPatch,
    pub time: WadPatch,
    pub sucks: WadPatch,
    pub par: WadPatch,
    pub killers: WadPatch,
    pub victims: WadPatch,
    pub total: WadPatch,
    pub star: WadPatch,
    pub bstar: WadPatch,
    pub enter: WadPatch,
    pub finish: WadPatch,
    pub players: [WadPatch; MAXPLAYERS],
    pub bplayers: [WadPatch; MAXPLAYERS],
}

impl Patches {
    pub(super) fn new(wad: &WadData) -> Self {
        let mut players: [MaybeUninit<WadPatch>; MAXPLAYERS] = [
            MaybeUninit::uninit(),
            MaybeUninit::uninit(),
            MaybeUninit::uninit(),
            MaybeUninit::uninit(),
        ];
        for n in 0..MAXPLAYERS {
            let lump = wad.get_lump(&format!("STPB{n}")).unwrap();
            players[n] = MaybeUninit::new(WadPatch::from_lump(lump));
        }

        let mut bplayers: [MaybeUninit<WadPatch>; MAXPLAYERS] = [
            MaybeUninit::uninit(),
            MaybeUninit::uninit(),
            MaybeUninit::uninit(),
            MaybeUninit::uninit(),
        ];
        for n in 1..MAXPLAYERS + 1 {
            let lump = wad.get_lump(&format!("WIBP{n}")).unwrap();
            bplayers[n - 1] = MaybeUninit::new(WadPatch::from_lump(lump));
        }

        Self {
            nums: get_num_sprites("WINUM", 0, wad),
            minus: WadPatch::from_lump(wad.get_lump("WIMINUS").unwrap()),
            percent: WadPatch::from_lump(wad.get_lump("WIPCNT").unwrap()),
            kills: WadPatch::from_lump(wad.get_lump("WIOSTK").unwrap()),
            secret: WadPatch::from_lump(wad.get_lump("WIOSTS").unwrap()),
            sp_secret: WadPatch::from_lump(wad.get_lump("WISCRT2").unwrap()),
            items: WadPatch::from_lump(wad.get_lump("WIOSTI").unwrap()),
            frags: WadPatch::from_lump(wad.get_lump("WIFRGS").unwrap()),
            colon: WadPatch::from_lump(wad.get_lump("WICOLON").unwrap()),
            time: WadPatch::from_lump(wad.get_lump("WITIME").unwrap()),
            sucks: WadPatch::from_lump(wad.get_lump("WISUCKS").unwrap()),
            par: WadPatch::from_lump(wad.get_lump("WIPAR").unwrap()),
            killers: WadPatch::from_lump(wad.get_lump("WIKILRS").unwrap()),
            victims: WadPatch::from_lump(wad.get_lump("WIVCTMS").unwrap()),
            total: WadPatch::from_lump(wad.get_lump("WIMSTT").unwrap()),
            star: WadPatch::from_lump(wad.get_lump("STFST01").unwrap()),
            bstar: WadPatch::from_lump(wad.get_lump("STFDEAD0").unwrap()),
            enter: WadPatch::from_lump(wad.get_lump("WIENTER").unwrap()),
            finish: WadPatch::from_lump(wad.get_lump("WIF").unwrap()),
            players: unsafe { players.map(|n| n.assume_init()) },
            bplayers: unsafe { bplayers.map(|n| n.assume_init()) },
        }
    }
}

#[derive(Debug, PartialOrd, PartialEq)]
pub(crate) enum State {
    StatCount,
    NextLoc,
    None,
}

pub(crate) enum AnimType {
    Always,
    Random,
    Level,
}

pub(crate) const MAP_POINTS: [[(i32, i32); 9]; 3] = [
    [
        (185, 164), // location of level 0 (CJ)
        (148, 143), // location of level 1 (CJ)
        (69, 122),  // location of level 2 (CJ)
        (209, 102), // location of level 3 (CJ)
        (116, 89),  // location of level 4 (CJ)
        (166, 55),  // location of level 5 (CJ)
        (71, 56),   // location of level 6 (CJ)
        (135, 29),  // location of level 7 (CJ)
        (71, 24),   // location of level 8 (CJ)
    ],
    [
        (254, 25),  // location of level 0 (CJ)
        (97, 50),   // location of level 1 (CJ)
        (188, 64),  // location of level 2 (CJ)
        (128, 78),  // location of level 3 (CJ)
        (214, 92),  // location of level 4 (CJ)
        (133, 130), // location of level 5 (CJ)
        (208, 136), // location of level 6 (CJ)
        (148, 140), // location of level 7 (CJ)
        (235, 158), // location of level 8 (CJ)
    ],
    [
        (156, 168), // location of level 0 (CJ)
        (48, 154),  // location of level 1 (CJ)
        (174, 95),  // location of level 2 (CJ)
        (265, 75),  // location of level 3 (CJ)
        (130, 48),  // location of level 4 (CJ)
        (279, 23),  // location of level 5 (CJ)
        (198, 48),  // location of level 6 (CJ)
        (140, 25),  // location of level 7 (CJ)
        (281, 136), // location of level 8 (CJ)
    ],
];

pub(crate) struct Animation {
    pub kind: AnimType,
    // period in tics between animations
    pub period: i32,
    // number of animation frames
    pub num_of: i32,
    // location of animation
    pub location: (i32, i32),
    // ALWAYS: n/a,
    // RANDOM: period deviation (<256),
    // LEVEL: level
    pub data1: i32,
    // ALWAYS: n/a,
    // RANDOM: random base period,
    // LEVEL: n/a
    pub data2: i32,
    // actual graphics for frames of animations
    pub patches: Vec<WadPatch>,
    // following must be initialized to zero before use!
    // next value of bcnt (used in conjunction with period)
    pub next_tic: i32,
    // last drawn animation frame
    pub last_drawn: i32,
    // next frame number to animate
    pub counter: i32,
    // used by RANDOM and LEVEL when animating
    pub state: i32,
}

impl Animation {
    pub(crate) const fn new(
        kind: AnimType,
        period: i32,
        num_of: i32,
        location: (i32, i32),
        data1: i32,
    ) -> Self {
        Self {
            kind,
            period,
            num_of,
            location,
            data1,
            data2: 0,
            patches: Vec::new(),
            next_tic: 0,
            last_drawn: 0,
            counter: 0,
            state: 0,
        }
    }
}

pub(super) fn animations() -> Vec<Vec<Animation>> {
    vec![
        vec![
            Animation::new(AnimType::Always, TICRATE / 3, 3, (224, 104), 0),
            Animation::new(AnimType::Always, TICRATE / 3, 3, (184, 160), 0),
            Animation::new(AnimType::Always, TICRATE / 3, 3, (112, 136), 0),
            Animation::new(AnimType::Always, TICRATE / 3, 3, (72, 112), 0),
            Animation::new(AnimType::Always, TICRATE / 3, 3, (88, 96), 0),
            Animation::new(AnimType::Always, TICRATE / 3, 3, (64, 48), 0),
            Animation::new(AnimType::Always, TICRATE / 3, 3, (192, 40), 0),
            Animation::new(AnimType::Always, TICRATE / 3, 3, (136, 16), 0),
            Animation::new(AnimType::Always, TICRATE / 3, 3, (80, 16), 0),
            Animation::new(AnimType::Always, TICRATE / 3, 3, (64, 24), 0),
        ],
        vec![
            // These don't seem right
            Animation::new(AnimType::Level, TICRATE / 3, 1, (128, 136), 1),
            Animation::new(AnimType::Level, TICRATE / 3, 1, (128, 136), 2),
            Animation::new(AnimType::Level, TICRATE / 3, 1, (128, 136), 3),
            Animation::new(AnimType::Level, TICRATE / 3, 1, (128, 136), 4),
            Animation::new(AnimType::Level, TICRATE / 3, 1, (128, 136), 5),
            Animation::new(AnimType::Level, TICRATE / 3, 1, (128, 136), 6),
            Animation::new(AnimType::Level, TICRATE / 3, 1, (128, 136), 7),
            Animation::new(AnimType::Level, TICRATE / 3, 3, (192, 144), 8),
            Animation::new(AnimType::Level, TICRATE / 3, 1, (128, 136), 8),
        ],
        vec![
            Animation::new(AnimType::Always, TICRATE / 3, 3, (104, 168), 0),
            Animation::new(AnimType::Always, TICRATE / 3, 3, (40, 136), 0),
            Animation::new(AnimType::Always, TICRATE / 3, 3, (160, 96), 0),
            Animation::new(AnimType::Always, TICRATE / 3, 3, (104, 80), 0),
            Animation::new(AnimType::Always, TICRATE / 3, 3, (120, 32), 0),
            Animation::new(AnimType::Always, TICRATE / 4, 3, (40, 0), 0),
        ],
    ]
}
