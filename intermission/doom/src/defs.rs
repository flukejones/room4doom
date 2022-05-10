use wad::lumps::WadPatch;

pub(crate) const TICRATE: i32 = 35;
pub(crate) const SHOW_NEXT_LOC_DELAY: i32 = 4;

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
    kind: AnimType,
    // period in tics between animations
    period: i32,
    // number of animation frames
    num_of: i32,
    // location of animation
    location: (i32, i32),
    // ALWAYS: n/a,
    // RANDOM: period deviation (<256),
    // LEVEL: level
    data1: i32,
    // ALWAYS: n/a,
    // RANDOM: random base period,
    // LEVEL: n/a
    data2: i32,
    // actual graphics for frames of animations
    patches: Vec<WadPatch>,
    // following must be initialized to zero before use!
    // next value of bcnt (used in conjunction with period)
    next_tic: i32,
    // last drawn animation frame
    last_drawn: i32,
    // next frame number to animate
    counter: i32,
    // used by RANDOM and LEVEL when animating
    state: i32,
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

pub(crate) static EPISODE0_ANIMS: [Animation; 10] = [
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
];

pub(crate) static EPISODE1_ANIMS: [Animation; 9] = [
    Animation::new(AnimType::Level, TICRATE / 3, 1, (128, 136), 1),
    Animation::new(AnimType::Level, TICRATE / 3, 1, (128, 136), 2),
    Animation::new(AnimType::Level, TICRATE / 3, 1, (128, 136), 3),
    Animation::new(AnimType::Level, TICRATE / 3, 1, (128, 136), 4),
    Animation::new(AnimType::Level, TICRATE / 3, 1, (128, 136), 5),
    Animation::new(AnimType::Level, TICRATE / 3, 1, (128, 136), 6),
    Animation::new(AnimType::Level, TICRATE / 3, 1, (128, 136), 7),
    Animation::new(AnimType::Level, TICRATE / 3, 3, (192, 144), 8),
    Animation::new(AnimType::Level, TICRATE / 3, 1, (128, 136), 8),
];

pub(crate) static EPISODE2_ANIMS: [Animation; 6] = [
    Animation::new(AnimType::Always, TICRATE / 3, 3, (104, 168), 0),
    Animation::new(AnimType::Always, TICRATE / 3, 3, (40, 136), 0),
    Animation::new(AnimType::Always, TICRATE / 3, 3, (160, 96), 0),
    Animation::new(AnimType::Always, TICRATE / 3, 3, (104, 80), 0),
    Animation::new(AnimType::Always, TICRATE / 3, 3, (120, 32), 0),
    Animation::new(AnimType::Always, TICRATE / 4, 3, (40, 0), 0),
];
