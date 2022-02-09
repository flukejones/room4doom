pub const FORWARDMOVE: [i32; 2] = [0x19, 0x32];
pub const SIDEMOVE: [i32; 2] = [0x18, 0x28];
pub const ANGLETURN: [i16; 3] = [640, 1280, 320]; // + slow turn

pub const MAXPLMOVE: i32 = FORWARDMOVE[1];

pub const SLOWTURNTICS: i32 = 6;

pub struct ButtonCode {
    // Press "Fire".
    pub bt_attack: u8,
    // Use button, to open doors, activate switches.
    pub bt_use: u8,

    // Flag: game events, not really buttons.
    pub bt_special: u8,
    pub bt_specialmask: u8,

    // Flag, weapon change pending.
    // If true, the next 3 bits hold weapon num.
    pub bt_change: u8,
    // The 3bit weapon mask and shift, convenience.
    pub bt_weaponmask: u8,
    pub bt_weaponshift: u8,

    // Pause the game.
    pub bts_pause: u8,
    // Save the game at each console.
    pub bts_savegame: u8,

    // Savegame slot numbers
    //  occupy the second byte of buttons.
    pub bts_savemask: u8,
    pub bts_saveshift: u8,
}

pub const TIC_CMD_BUTTONS: ButtonCode = ButtonCode {
    // Press "Fire".
    bt_attack: 1,
    // Use button, to open doors, activate switches.
    bt_use: 2,

    // Flag: game events, not really buttons.
    bt_special: 128,
    bt_specialmask: 3,

    // Flag, weapon change pending.
    // If true, the next 3 bits hold weapon num.
    bt_change: 4,
    // The 3bit weapon mask and shift, convenience.
    bt_weaponmask: (8 + 16 + 32),
    bt_weaponshift: 3,

    // Pause the game.
    bts_pause: 1,
    // Save the game at each console.
    bts_savegame: 2,

    // Savegame slot numbers
    //  occupy the second byte of buttons.
    bts_savemask: (4 + 8 + 16),
    bts_saveshift: 2,
};

/// The data sampled per tick (single player)
/// and transmitted to other peers (multiplayer).
/// Mainly movements/button commands per game tick,
/// plus a checksum for internal state consistency.
// G_BuildTiccmd
#[derive(Default, Copy, Clone)]
pub struct TicCmd {
    /// *2048 for move
    pub forwardmove: i8,
    /// *2048 for move
    pub sidemove: i8,
    /// <<16 for angle delta
    pub angleturn: i16,
    /// checks for net game
    pub consistancy: i16,
    pub chatchar: u8,
    pub buttons: u8,
}

impl TicCmd {
    pub const fn new() -> Self {
        TicCmd {
            forwardmove: 0,
            sidemove: 0,
            angleturn: 0,
            consistancy: 0,
            chatchar: 0,
            buttons: 0,
        }
    }
}
