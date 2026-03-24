//! A menu `GameSubsystem` as used by Doom. This loads and uses the Doom assets
//! to display the menu but because it uses `SubsystemTrait` for the actual
//! interaction with the rest of the game it ends up being fairly generic - you
//! could make this fully generic with a little work, or use it as the basis for
//! a different menu.

use game_config::{GameMode, Skill};
use gameplay::english as lang;
use gamestate_traits::{ConfigKey, ConfigTraits, GameState, GameTraits, KeyCode, SubsystemTrait};
use hud_util::{
    draw_patch, draw_text_line, draw_text_line_tinted, fullscreen_scale, hud_scale, measure_text_line
};
use render_common::DrawBuffer;
use sound_common::SfxName;
use std::collections::HashMap;
use wad::WadData;
use wad::types::{BLACK, WadPalette, WadPatch};

const SAVESTRINGSIZE: usize = 24;
const LINEHEIGHT: i32 = 16;
const SKULLS: [&str; 2] = ["M_SKULL1", "M_SKULL2"];
const SAVE_SLOT_COUNT: usize = 6;
const EMPTY_STRING: &str = "EMPTY SLOT";
/// Save/load border tile count (original Doom uses 24 tiles of 8px each)
const SAVE_BORDER_TILES: i32 = 24;
/// Quicksave slot sentinel: not yet assigned
const QS_UNSET: i32 = -1;
/// Quicksave slot sentinel: user must pick a slot
const QS_PICKING: i32 = -2;

const TINT_SELECTED: u32 = 0xFF8030; // orange
const TINT_NORMAL: u32 = 0xFF0000; // red (doom default)
const TINT_VALUE: u32 = 0x00C000; // 75% green

#[derive(Debug, Copy, Clone, PartialOrd, PartialEq)]
enum Status {
    NoCursor, // 0
    Ok,
}

#[derive(Clone)]
enum ItemKind {
    Patch,
    Label,
    Slider { min: i32, max: i32, step: i32 },
    Toggle,
    Cycle { options: &'static [&'static str] },
}

#[derive(Debug, Clone, Copy)]
enum MenuAction {
    None,
    GoTo(MenuIndex),
    NewGame,
    SelectEpisode,
    OpenLoadGame,
    OpenSaveGame,
    OpenOptSound,
    OpenOptVideo,
    OpenOptGraphics,
    OpenOptHud,
    OpenOptInput,
    StartGame,
    LoadSlot,
    SaveSlot,
    VideoApply,
    EndGame,
    QuitGame,
}

#[derive(Clone)]
struct MenuItem {
    status: Status,
    patch: String,
    action: MenuAction,
    hotkey: char,
    kind: ItemKind,
    config_key: Option<ConfigKey>,
    label: &'static str,
    cached_value: i32,
}

impl MenuItem {
    fn new(status: Status, patch: impl ToString, action: MenuAction, hotkey: char) -> Self {
        Self {
            status,
            patch: patch.to_string(),
            action,
            hotkey,
            kind: ItemKind::Patch,
            config_key: None,
            label: "",
            cached_value: 0,
        }
    }

    fn label(label: &'static str, action: MenuAction, hotkey: char) -> Self {
        Self {
            status: Status::Ok,
            patch: String::new(),
            action,
            hotkey,
            kind: ItemKind::Label,
            config_key: None,
            label,
            cached_value: 0,
        }
    }

    fn slider(label: &'static str, key: ConfigKey, min: i32, max: i32, step: i32) -> Self {
        Self {
            status: Status::Ok,
            patch: String::new(),
            action: MenuAction::None,
            hotkey: '\0',
            kind: ItemKind::Slider {
                min,
                max,
                step,
            },
            config_key: Some(key),
            label,
            cached_value: 0,
        }
    }

    fn toggle(label: &'static str, key: ConfigKey) -> Self {
        Self {
            status: Status::Ok,
            patch: String::new(),
            action: MenuAction::None,
            hotkey: '\0',
            kind: ItemKind::Toggle,
            config_key: Some(key),
            label,
            cached_value: 0,
        }
    }

    fn cycle(label: &'static str, key: ConfigKey, options: &'static [&'static str]) -> Self {
        Self {
            status: Status::Ok,
            patch: String::new(),
            action: MenuAction::None,
            hotkey: '\0',
            kind: ItemKind::Cycle {
                options,
            },
            config_key: Some(key),
            label,
            cached_value: 0,
        }
    }
}

/// A title item, such as the DOOM logo. Typically drawn at the top of the menu
/// but you could draw it anywhere you want really.
#[derive(Clone)]
struct Title {
    /// The name of the patch in the wad to draw for this item
    patch: String,
    x: i32,
    y: i32,
}

impl Title {
    fn new(patch: impl ToString, x: i32, y: i32) -> Self {
        Self {
            patch: patch.to_string(),
            x,
            y,
        }
    }
}

#[derive(Clone)]
struct MenuSet {
    /// Must match this items location in the menu array as determined by the
    /// order or `MenuIndex`
    this: MenuIndex,
    /// The location in the menu array of the `MenuSet` that popping this one
    /// would lead to -- as in, the previous `MenuSet`, for example popping the
    /// Skill selection leads back to the Episode selection.
    prev: MenuIndex,
    /// Titles associated with this menu. Can be empty.
    titles: Vec<Title>,
    /// Each `MenuItem` is a row in this `MenuSet`. The order in the vector is
    /// the order they are drawn in (top to bottom)
    items: Vec<MenuItem>,
    /// Sub-item start X coord
    x: i32,
    /// Sub-item start Y coord
    y: i32,
    /// The index of the last item the user was on in this menu. When the user
    /// selects this `MenuSet` again this item will be pre-selected.
    last_on: usize,
}

impl MenuSet {
    fn new(
        this: MenuIndex,
        prev: MenuIndex,
        titles: Vec<Title>,
        x: i32,
        y: i32,
        items: Vec<MenuItem>,
    ) -> Self {
        Self {
            titles,
            this,
            prev,
            items,
            x,
            y,
            last_on: 0,
        }
    }
}

/// Must match the order of `MenuDoom::menus` declaration
#[derive(Debug, Copy, Clone, PartialOrd, PartialEq)]
enum MenuIndex {
    TopLevel,
    Episodes,
    Skill,
    ReadThis1,
    ReadThis2,
    LoadGame,
    SaveGame,
    Options,
    OptSound,
    OptVideo,
    OptGraphics,
    OptHud,
    OptInput,
}

pub struct MenuState {
    active: bool,
    current_menu: MenuIndex,
    restart_needed: bool,
    video_snapshot: Option<Vec<(ConfigKey, i32)>>,
    last_on: Vec<usize>,
}

const MUSIC_TYPE_OPTIONS: &[&str] = &[lang::OPT_MUS_OPL2, lang::OPT_MUS_OPL3, lang::OPT_MUS_GUS];
const WINDOW_MODE_OPTIONS: &[&str] = &[
    lang::OPT_MODE_WINDOWED,
    lang::OPT_MODE_BORDERLESS,
    lang::OPT_MODE_EXCLUSIVE,
];
const RENDERER_OPTIONS: &[&str] = &[lang::OPT_REND_CLASSIC, lang::OPT_REND_SOFT3D];
const HUD_SIZE_OPTIONS: &[&str] = &[lang::OPT_HUD_SIZE_FULL, lang::OPT_HUD_SIZE_BAR];
const HUD_WIDTH_OPTIONS: &[&str] = &[lang::OPT_HUD_WIDTH_CLASSIC, lang::OPT_HUD_WIDTH_WIDE];
const HUD_MSG_MODE_OPTIONS: &[&str] = &[
    lang::OPT_HUD_MSG_OFF,
    lang::OPT_HUD_MSG_STACK,
    lang::OPT_HUD_MSG_OVER,
];

type Patches = HashMap<String, WadPatch>;

pub struct GameMenu {
    /// Is the menu active?
    active: bool,
    /// A specific helper for pressing F1
    in_help: bool,
    /// True when user is typing a save description
    save_enter: bool,
    /// Which slot the user is editing
    save_slot: usize,
    /// The old description (restored on cancel)
    save_old: String,
    /// Cursor position in save description string
    save_char_idx: usize,
    /// Cached descriptions for each save slot
    save_strings: [String; SAVE_SLOT_COUNT],
    /// Quicksave slot: QS_UNSET (-1), QS_PICKING (-2), or 0..5
    quicksave_slot: i32,
    /// Main menu def
    menus: Vec<MenuSet>,
    current_menu: MenuIndex,

    patches: Patches,
    palette: WadPalette,
    /// Track the episode selected by episode menu
    episode: usize,
    which_skull: usize,
    skull_anim_counter: i32,
    restart_needed: bool,
    dim_background: bool,
    /// Snapshot of video config values taken on entry to OptVideo.
    /// Used to revert on backspace (without Apply).
    video_snapshot: Option<Vec<(ConfigKey, i32)>>,
}

impl GameMenu {
    /// Build the full menu tree from WAD patches and game mode.
    ///
    /// - Creates all menu pages (top-level, episodes, skill, options, etc.)
    /// - Pre-caches title/item/skull patches from the WAD
    /// - Adjusts episode count based on available WAD lumps
    pub fn new(mode: GameMode, wad: &WadData, buf_width: i32) -> Self {
        let x_pos = |original_x: i32| -> i32 { original_x };

        let save_slot_items = |action: MenuAction| -> Vec<MenuItem> {
            (0..SAVE_SLOT_COUNT)
                .map(|i| {
                    let hotkey = char::from_digit(i as u32 + 1, 10).unwrap();
                    MenuItem::new(Status::Ok, "", action, hotkey)
                })
                .collect()
        };

        let menus = vec![
            MenuSet::new(
                MenuIndex::TopLevel,
                MenuIndex::TopLevel,
                vec![Title::new("M_DOOM", x_pos(92), 2)],
                x_pos(97),
                64,
                vec![
                    MenuItem::new(Status::Ok, "M_NGAME", MenuAction::NewGame, 'N'),
                    MenuItem::new(
                        Status::Ok,
                        "M_OPTION",
                        MenuAction::GoTo(MenuIndex::Options),
                        'O',
                    ),
                    MenuItem::new(Status::Ok, "M_LOADG", MenuAction::OpenLoadGame, 'L'),
                    MenuItem::new(Status::Ok, "M_SAVEG", MenuAction::OpenSaveGame, 'S'),
                    MenuItem::new(
                        Status::Ok,
                        "M_RDTHIS",
                        MenuAction::GoTo(MenuIndex::ReadThis1),
                        'R',
                    ),
                    MenuItem::new(Status::Ok, "M_QUITG", MenuAction::QuitGame, 'Q'),
                ],
            ),
            MenuSet::new(
                MenuIndex::Episodes,
                MenuIndex::TopLevel,
                vec![Title::new("M_EPISOD", x_pos(54), 38)],
                x_pos(48),
                63,
                (1..=9)
                    .filter_map(|e| {
                        if wad.lump_exists(&format!("M_EPI{e}")) {
                            let mut item = MenuItem::new(
                                Status::Ok,
                                format!("M_EPI{e}"),
                                MenuAction::SelectEpisode,
                                char::from_digit(e, 10).unwrap(),
                            );
                            item.cached_value = e as i32;
                            Some(item)
                        } else {
                            None
                        }
                    })
                    .collect(),
            ),
            MenuSet::new(
                MenuIndex::Skill,
                if mode == GameMode::Commercial {
                    MenuIndex::TopLevel
                } else {
                    MenuIndex::Episodes
                },
                vec![
                    Title::new("M_NEWG", x_pos(96), 14),
                    Title::new("M_SKILL", x_pos(54), 38),
                ],
                x_pos(48),
                63,
                vec![
                    MenuItem::new(Status::Ok, "M_JKILL", MenuAction::StartGame, 'I'),
                    MenuItem::new(Status::Ok, "M_ROUGH", MenuAction::StartGame, 'R'),
                    MenuItem::new(Status::Ok, "M_HURT", MenuAction::StartGame, 'H'),
                    MenuItem::new(Status::Ok, "M_ULTRA", MenuAction::StartGame, 'U'),
                    MenuItem::new(Status::Ok, "M_NMARE", MenuAction::StartGame, 'N'),
                ],
            ),
            MenuSet::new(
                MenuIndex::ReadThis1,
                MenuIndex::TopLevel,
                vec![],
                buf_width / 2 - 160,
                0,
                match mode {
                    GameMode::Commercial => vec![MenuItem::new(
                        Status::Ok,
                        "HELP",
                        MenuAction::GoTo(MenuIndex::ReadThis2),
                        '\0',
                    )],
                    GameMode::Retail => vec![MenuItem::new(
                        Status::Ok,
                        "HELP1",
                        MenuAction::GoTo(MenuIndex::ReadThis2),
                        '\0',
                    )],
                    _ => vec![MenuItem::new(
                        Status::Ok,
                        "HELP1",
                        MenuAction::GoTo(MenuIndex::ReadThis2),
                        '\0',
                    )],
                },
            ),
            MenuSet::new(
                MenuIndex::ReadThis2,
                MenuIndex::ReadThis1,
                vec![],
                buf_width / 2 - 160,
                0,
                match mode {
                    GameMode::Commercial | GameMode::Retail => vec![MenuItem::new(
                        Status::Ok,
                        "CREDIT",
                        MenuAction::GoTo(MenuIndex::TopLevel),
                        '\0',
                    )],
                    _ => vec![MenuItem::new(
                        Status::Ok,
                        "HELP2",
                        MenuAction::GoTo(MenuIndex::TopLevel),
                        '\0',
                    )],
                },
            ),
            MenuSet::new(
                MenuIndex::LoadGame,
                MenuIndex::TopLevel,
                vec![Title::new("M_LOADG", 72, 28)],
                80,
                54,
                save_slot_items(MenuAction::LoadSlot),
            ),
            MenuSet::new(
                MenuIndex::SaveGame,
                MenuIndex::TopLevel,
                vec![Title::new("M_SAVEG", 72, 28)],
                80,
                54,
                save_slot_items(MenuAction::SaveSlot),
            ),
            MenuSet::new(
                MenuIndex::Options,
                MenuIndex::TopLevel,
                vec![],
                72,
                40,
                vec![
                    MenuItem::label(lang::OPT_END_GAME, MenuAction::EndGame, 'E'),
                    MenuItem::toggle(lang::OPT_MENU_DIM, ConfigKey::MenuDim),
                    MenuItem::label(lang::OPT_SOUND, MenuAction::OpenOptSound, 'S'),
                    MenuItem::label(lang::OPT_GRAPHICS, MenuAction::OpenOptGraphics, 'G'),
                    MenuItem::label(lang::OPT_VIDEO, MenuAction::OpenOptVideo, 'V'),
                    MenuItem::label(lang::OPT_HUD, MenuAction::OpenOptHud, 'H'),
                    MenuItem::label(lang::OPT_INPUT, MenuAction::OpenOptInput, 'I'),
                ],
            ),
            MenuSet::new(
                MenuIndex::OptSound,
                MenuIndex::Options,
                vec![],
                32,
                40,
                vec![
                    MenuItem::slider(lang::OPT_SFX_VOL, ConfigKey::SfxVolume, 0, 100, 5),
                    MenuItem::slider(lang::OPT_MUS_VOL, ConfigKey::MusVolume, 0, 100, 5),
                    MenuItem::cycle(lang::OPT_MUS_TYPE, ConfigKey::MusicType, MUSIC_TYPE_OPTIONS),
                ],
            ),
            MenuSet::new(
                MenuIndex::OptVideo,
                MenuIndex::Options,
                vec![],
                32,
                40,
                vec![
                    MenuItem::cycle(lang::OPT_MODE, ConfigKey::WindowMode, WINDOW_MODE_OPTIONS),
                    MenuItem::toggle(lang::OPT_VSYNC, ConfigKey::VSync),
                    MenuItem::label(lang::OPT_APPLY, MenuAction::VideoApply, 'A'),
                ],
            ),
            MenuSet::new(
                MenuIndex::OptGraphics,
                MenuIndex::Options,
                vec![],
                32,
                40,
                vec![
                    MenuItem::cycle(lang::OPT_RENDERER, ConfigKey::Renderer, RENDERER_OPTIONS),
                    MenuItem::cycle(
                        lang::OPT_DETAIL,
                        ConfigKey::HiRes,
                        &[lang::OPT_DETAIL_LOW, lang::OPT_DETAIL_HIGH],
                    ),
                    MenuItem::toggle(lang::OPT_FRAME_INTERP, ConfigKey::FrameInterpolation),
                    MenuItem::toggle(lang::OPT_VOXELS, ConfigKey::Voxels),
                    MenuItem::toggle(lang::OPT_CRT_GAMMA, ConfigKey::CrtGamma),
                    MenuItem::toggle(lang::OPT_HEALTH_VIG, ConfigKey::HealthVignette),
                    MenuItem::toggle(lang::OPT_SHOW_FPS, ConfigKey::ShowFps),
                ],
            ),
            MenuSet::new(
                MenuIndex::OptHud,
                MenuIndex::Options,
                vec![],
                32,
                40,
                vec![
                    MenuItem::cycle(lang::OPT_HUD_SIZE, ConfigKey::HudSize, HUD_SIZE_OPTIONS),
                    MenuItem::cycle(lang::OPT_HUD_WIDTH, ConfigKey::HudWidth, HUD_WIDTH_OPTIONS),
                    MenuItem::cycle(
                        lang::OPT_HUD_MSG_MODE,
                        ConfigKey::HudMsgMode,
                        HUD_MSG_MODE_OPTIONS,
                    ),
                    MenuItem::slider(lang::OPT_HUD_MSG_TIME, ConfigKey::HudMsgTime, 1, 10, 1),
                ],
            ),
            MenuSet::new(
                MenuIndex::OptInput,
                MenuIndex::Options,
                vec![],
                32,
                40,
                vec![
                    MenuItem::slider(lang::OPT_MOUSE_SENS, ConfigKey::MouseSensitivity, 0, 15, 1),
                    MenuItem::toggle(lang::OPT_INVERT_Y, ConfigKey::InvertY),
                ],
            ),
        ];

        let mut patches = HashMap::new();
        for menu in &menus {
            for item in &menu.titles {
                if let Some(lump) = wad.get_lump(&item.patch) {
                    patches.insert(item.patch.to_string(), WadPatch::from_lump(lump));
                }
            }
            for item in &menu.items {
                if !item.patch.is_empty() {
                    if let Some(lump) = wad.get_lump(&item.patch) {
                        patches.insert(item.patch.to_string(), WadPatch::from_lump(lump));
                    }
                }
            }
        }

        for patch in SKULLS {
            if let Some(lump) = wad.get_lump(patch) {
                patches.insert(patch.to_string(), WadPatch::from_lump(lump));
            }
        }

        // Thermometer (slider) patches
        for name in ["M_THERML", "M_THERMM", "M_THERMR", "M_THERMO"] {
            if let Some(lump) = wad.get_lump(name) {
                patches.insert(name.to_string(), WadPatch::from_lump(lump));
            }
        }

        // Save/load border decoration patches
        for name in ["M_LSLEFT", "M_LSCNTR", "M_LSRGHT"] {
            if let Some(lump) = wad.get_lump(name) {
                patches.insert(name.to_string(), WadPatch::from_lump(lump));
            }
        }

        let palette = wad.lump_iter::<WadPalette>("PLAYPAL").next().unwrap();

        Self {
            active: false,
            in_help: false,
            save_enter: false,
            save_slot: 0,
            save_old: String::new(),
            save_char_idx: 0,
            save_strings: std::array::from_fn(|_| EMPTY_STRING.to_string()),
            quicksave_slot: QS_UNSET,
            menus,
            current_menu: MenuIndex::TopLevel,
            patches,
            palette,
            episode: 0,
            which_skull: 0,
            skull_anim_counter: 10,
            restart_needed: false,
            dim_background: true,
            video_snapshot: None,
        }
    }

    /// Sets menu state for entering
    fn enter_menu<T: GameTraits + ConfigTraits>(&mut self, game: &mut T) {
        if self.in_help {
            self.in_help = false;
            self.current_menu = MenuIndex::TopLevel;
            game.start_sound(SfxName::Swtchx);
        } else {
            self.active = true;
            game.start_sound(SfxName::Swtchn);
        }
    }

    /// Sets menu state on exit
    fn exit_menu<T: GameTraits + ConfigTraits>(&mut self, game: &mut T) {
        if self.current_menu == MenuIndex::OptVideo {
            self.revert_video_snapshot(game);
        }
        self.restart_needed = false;
        self.active = false;
        self.in_help = false;
        self.save_enter = false;
        self.current_menu = MenuIndex::TopLevel;
        game.start_sound(SfxName::Swtchx);
    }

    fn get_current_menu(&mut self) -> &mut MenuSet {
        let mut idx = 0;
        for (i, m) in self.menus.iter().enumerate() {
            if m.this == self.current_menu {
                idx = i;
            }
        }
        &mut self.menus[idx]
    }

    fn get_menu_mut(&mut self, index: MenuIndex) -> &mut MenuSet {
        self.menus.iter_mut().find(|m| m.this == index).unwrap()
    }

    fn get_patch(&self, name: &str) -> &WadPatch {
        self.patches
            .get(name)
            .unwrap_or_else(|| panic!("{name} not in cache"))
    }

    /// Draw an OG Doom thermometer slider: left cap + n middle + right cap +
    /// dot. The slider is vertically centered on the text line, dot offset
    /// 1px down.
    fn draw_thermo(
        &self,
        x: f32,
        y: f32,
        width: usize,
        dot: usize,
        sx: f32,
        sy: f32,
        pixels: &mut impl DrawBuffer,
    ) {
        let seg_w = 8.0;
        let bg_h = 13.0;
        let bg_y = y - ((LINEHEIGHT as f32 - bg_h) / 2.0 + 1.0) * sy;
        let dot_y = bg_y + 1.0 * sy;
        let mut xx = x;
        // Left cap
        if let Some(p) = self.patches.get("M_THERML") {
            draw_patch(p, xx, bg_y, sx, sy, &self.palette, pixels);
            xx += seg_w * sx;
        }
        // Middle segments
        if let Some(p) = self.patches.get("M_THERMM") {
            for _ in 0..width {
                draw_patch(p, xx, bg_y, sx, sy, &self.palette, pixels);
                xx += seg_w * sx;
            }
        }
        // Right cap
        if let Some(p) = self.patches.get("M_THERMR") {
            draw_patch(p, xx, bg_y, sx, sy, &self.palette, pixels);
        }
        // Dot (1px down from background)
        if let Some(p) = self.patches.get("M_THERMO") {
            let dot_x = x + (seg_w + dot as f32 * seg_w) * sx;
            draw_patch(p, dot_x, dot_y, sx, sy, &self.palette, pixels);
        }
    }

    /// Apply a left/right adjustment to a slider, toggle, or cycle menu item.
    /// Returns true if the value changed.
    fn adjust_option_item<T: GameTraits + ConfigTraits>(
        &mut self,
        menu_idx: usize,
        item_idx: usize,
        dir: i32,
        game: &mut T,
    ) -> bool {
        let item = &self.menus[menu_idx].items[item_idx];
        let Some(key) = item.config_key else {
            return false;
        };
        let val = game.config_value(key);
        let new_val = match &item.kind {
            ItemKind::Slider {
                min,
                max,
                step,
            } => (val + dir * step).clamp(*min, *max),
            ItemKind::Toggle => {
                if val == 0 {
                    1
                } else {
                    0
                }
            }
            ItemKind::Cycle {
                options,
            } => {
                let n = options.len() as i32;
                ((val + dir) % n + n) % n
            }
            _ => return false,
        };
        if new_val == val {
            return false;
        }
        game.set_config_value(key, new_val);
        self.menus[menu_idx].items[item_idx].cached_value = new_val;
        if matches!(key, ConfigKey::VSync) {
            self.restart_needed = true;
        }
        // Save immediately for all submenus except Video (which uses Apply)
        if self.current_menu != MenuIndex::OptVideo {
            game.mark_config_changed();
        }
        true
    }

    /// Dispatch a menu action triggered by the user selecting an item.
    fn execute_action<T: GameTraits + ConfigTraits>(
        &mut self,
        action: MenuAction,
        choice: usize,
        game: &mut T,
    ) {
        match action {
            MenuAction::None => {}
            MenuAction::GoTo(target) => self.current_menu = target,
            MenuAction::NewGame => {
                self.current_menu = if game.get_mode() == GameMode::Commercial {
                    MenuIndex::Skill
                } else {
                    MenuIndex::Episodes
                };
            }
            MenuAction::SelectEpisode => {
                self.episode =
                    self.menus[MenuIndex::Episodes as usize].items[choice].cached_value as usize;
                self.current_menu = MenuIndex::Skill;
            }
            MenuAction::OpenLoadGame => self.open_load_menu(game),
            MenuAction::OpenSaveGame => {
                if game.game_state() == GameState::Level {
                    self.open_save_menu(game);
                }
            }
            MenuAction::OpenOptSound => {
                self.refresh_options_cache(MenuIndex::OptSound, game);
                self.current_menu = MenuIndex::OptSound;
            }
            MenuAction::OpenOptVideo => {
                self.refresh_options_cache(MenuIndex::OptVideo, game);
                let idx = MenuIndex::OptVideo as usize;
                self.video_snapshot = Some(
                    self.menus[idx]
                        .items
                        .iter()
                        .filter_map(|item| item.config_key.map(|k| (k, game.config_value(k))))
                        .collect(),
                );
                self.current_menu = MenuIndex::OptVideo;
            }
            MenuAction::OpenOptGraphics => {
                self.refresh_options_cache(MenuIndex::OptGraphics, game);
                self.current_menu = MenuIndex::OptGraphics;
            }
            MenuAction::OpenOptHud => {
                self.refresh_options_cache(MenuIndex::OptHud, game);
                self.current_menu = MenuIndex::OptHud;
            }
            MenuAction::OpenOptInput => {
                self.refresh_options_cache(MenuIndex::OptInput, game);
                self.current_menu = MenuIndex::OptInput;
            }
            MenuAction::StartGame => {
                self.exit_menu(game);
                game.defered_init_new(Skill::from(choice), self.episode, 1);
            }
            MenuAction::LoadSlot => {
                game.load_game(format!("slot{choice}"));
                self.exit_menu(game);
            }
            MenuAction::SaveSlot => {
                self.save_enter = true;
                self.save_slot = choice;
                self.save_old = self.save_strings[choice].clone();
                if self.save_strings[choice] == EMPTY_STRING {
                    self.save_strings[choice].clear();
                }
                self.save_char_idx = self.save_strings[choice].len();
            }
            MenuAction::VideoApply => {
                game.mark_config_changed();
                let idx = MenuIndex::OptVideo as usize;
                self.video_snapshot = Some(
                    self.menus[idx]
                        .items
                        .iter()
                        .filter_map(|item| item.config_key.map(|k| (k, game.config_value(k))))
                        .collect(),
                );
            }
            MenuAction::EndGame => {
                game.start_title();
                self.active = false;
            }
            MenuAction::QuitGame => game.quit_game(),
        }
    }

    /// Restore video config values to the snapshot taken on menu entry,
    /// discarding unapplied changes.
    fn revert_video_snapshot<T: GameTraits + ConfigTraits>(&mut self, game: &mut T) {
        if let Some(snap) = self.video_snapshot.take() {
            let mut changed = false;
            for (key, val) in snap {
                if game.config_value(key) != val {
                    game.set_config_value(key, val);
                    changed = true;
                }
            }
            if changed {
                game.mark_config_changed();
            }
        }
    }

    /// Sync cached display values for all config-bound items in a menu page.
    fn refresh_options_cache<T: GameTraits + ConfigTraits>(&mut self, menu: MenuIndex, game: &T) {
        let idx = menu as usize;
        for item in &mut self.menus[idx].items {
            if let Some(key) = item.config_key {
                item.cached_value = game.config_value(key);
            }
        }
    }

    pub fn save_state(&self) -> MenuState {
        MenuState {
            active: self.active,
            current_menu: self.current_menu,
            restart_needed: self.restart_needed,
            video_snapshot: self.video_snapshot.clone(),
            last_on: self.menus.iter().map(|m| m.last_on).collect(),
        }
    }

    pub fn restore_state(&mut self, state: MenuState) {
        self.active = state.active;
        self.current_menu = state.current_menu;
        self.restart_needed = state.restart_needed;
        self.video_snapshot = state.video_snapshot;
        for (i, &pos) in state.last_on.iter().enumerate() {
            if i < self.menus.len() && pos < self.menus[i].items.len() {
                self.menus[i].last_on = pos;
            }
        }
    }

    fn is_options_menu(&self) -> bool {
        matches!(
            self.current_menu,
            MenuIndex::Options
                | MenuIndex::OptSound
                | MenuIndex::OptVideo
                | MenuIndex::OptGraphics
                | MenuIndex::OptHud
                | MenuIndex::OptInput
        )
    }

    fn in_options_submenu(&self) -> bool {
        matches!(
            self.current_menu,
            MenuIndex::OptSound
                | MenuIndex::OptVideo
                | MenuIndex::OptGraphics
                | MenuIndex::OptHud
                | MenuIndex::OptInput
        )
    }

    /// Read save slot descriptions from disk via GameTraits.
    fn read_save_strings<T: GameTraits + ConfigTraits>(&mut self, game: &T) {
        let descs = game.read_save_descriptions();
        for (i, desc) in descs.into_iter().enumerate() {
            if i >= SAVE_SLOT_COUNT {
                break;
            }
            self.save_strings[i] = desc.unwrap_or_else(|| EMPTY_STRING.to_string());
        }
    }

    /// Open the load game menu, disabling empty slots.
    fn open_load_menu<T: GameTraits + ConfigTraits>(&mut self, game: &mut T) {
        self.read_save_strings(game);
        // Determine slot status before borrowing menus mutably
        let statuses: Vec<Status> = self
            .save_strings
            .iter()
            .map(|s| {
                if s == EMPTY_STRING {
                    Status::NoCursor
                } else {
                    Status::Ok
                }
            })
            .collect();
        let load = self.get_menu_mut(MenuIndex::LoadGame);
        for (i, item) in load.items.iter_mut().enumerate() {
            item.status = statuses[i];
        }
        self.current_menu = MenuIndex::LoadGame;
    }

    /// Open the save game menu (all slots enabled).
    fn open_save_menu<T: GameTraits + ConfigTraits>(&mut self, game: &mut T) {
        self.read_save_strings(game);
        self.current_menu = MenuIndex::SaveGame;
    }

    /// Commit save: write to slot file and close menu.
    fn do_save<T: GameTraits + ConfigTraits>(&mut self, slot: usize, game: &mut T) {
        let desc = self.save_strings[slot].clone();
        game.save_game(format!("slot{slot}"), desc);
        game.start_sound(SfxName::Pistol);
        if self.quicksave_slot == QS_PICKING {
            self.quicksave_slot = slot as i32;
        }
        self.save_enter = false;
        self.exit_menu(game);
    }

    /// Draw save/load border decoration around a slot row.
    fn draw_save_border(&self, x: f32, y: f32, sx: f32, sy: f32, pixels: &mut impl DrawBuffer) {
        let left = self.get_patch("M_LSLEFT");
        let center = self.get_patch("M_LSCNTR");
        let right = self.get_patch("M_LSRGHT");
        let border_y = y - LINEHEIGHT as f32 * 0.25;
        draw_patch(left, x - 8.0 * sx, border_y, sx, sy, &self.palette, pixels);
        let mut bx = x;
        for _ in 0..SAVE_BORDER_TILES {
            draw_patch(center, bx, border_y, sx, sy, &self.palette, pixels);
            bx += 8.0 * sx;
        }
        draw_patch(right, bx, border_y, sx, sy, &self.palette, pixels);
    }

    /// Returns true if the current menu is the save or load game menu.
    fn is_save_load_menu(&self) -> bool {
        matches!(self.current_menu, MenuIndex::LoadGame | MenuIndex::SaveGame)
    }

    /// Render the active menu page to the draw buffer.
    ///
    /// - Dims background if enabled
    /// - Draws titles, item patches/labels, skull cursor
    /// - Handles save/load slot rendering with text cursor
    /// - Draws options submenus with sliders, toggles, and cycle values
    fn draw_pixels(&mut self, pixels: &mut impl DrawBuffer) {
        let (sx, sy) = hud_scale(pixels);

        if self.active || self.in_help {
            if self.dim_background {
                let buf = pixels.buf_mut();
                for pixel in buf.iter_mut() {
                    let r = (*pixel >> 16) & 0xFF;
                    let g = (*pixel >> 8) & 0xFF;
                    let b = *pixel & 0xFF;
                    *pixel = 0xFF000000 | ((r >> 1) << 16) | ((g >> 1) << 8) | (b >> 1);
                }
            }

            let active = &self.menus[self.current_menu as usize];
            let is_fullscreen = active.titles.is_empty() && active.y == 0;
            let is_save_load = self.is_save_load_menu();
            let is_options = self.is_options_menu();

            // Full-screen readthis/help pages: use CRT-correct aspect
            let (draw_sx, draw_sy) = if is_fullscreen {
                pixels.buf_mut().fill(BLACK);
                fullscreen_scale(pixels)
            } else {
                (sx, sy)
            };

            // Horizontal centering offset: menus are designed for 320-wide space.
            let x_ofs = (pixels.size().width_f32() - 320.0 * draw_sx) / 2.0;

            // Titles
            for item in active.titles.iter() {
                draw_patch(
                    self.get_patch(&item.patch),
                    x_ofs + item.x as f32 * draw_sx,
                    item.y as f32 * draw_sy,
                    draw_sx,
                    draw_sy,
                    &self.palette,
                    pixels,
                );
            }

            // Sub-items
            let x = if is_fullscreen {
                x_ofs
            } else {
                x_ofs + active.x as f32 * draw_sx
            };
            let mut y = active.y as f32 * draw_sy;

            if is_save_load {
                for i in 0..active.items.len() {
                    self.draw_save_border(x, y, draw_sx, draw_sy, pixels);
                    draw_text_line(
                        &self.save_strings[i],
                        x,
                        y,
                        draw_sx,
                        draw_sy,
                        &self.palette,
                        pixels,
                    );
                    if self.save_enter && i == self.save_slot {
                        let cursor_x = x + measure_text_line(&self.save_strings[i], draw_sx);
                        let y = y + LINEHEIGHT as f32 * 0.33;
                        draw_text_line("_", cursor_x, y, draw_sx, draw_sy, &self.palette, pixels);
                    }
                    y += LINEHEIGHT as f32 * draw_sy;
                }
            } else if is_options {
                let center_x = x_ofs + 160.0 * draw_sx;
                let gap = 4.0 * draw_sx;
                let sel_idx = active.last_on;
                for (item_i, item) in active.items.iter().enumerate() {
                    let label_tint = if item_i == sel_idx {
                        TINT_SELECTED
                    } else {
                        TINT_NORMAL
                    };
                    let label_w = measure_text_line(item.label, draw_sx);
                    draw_text_line_tinted(
                        item.label,
                        center_x - gap - label_w,
                        y,
                        draw_sx,
                        draw_sy,
                        &self.palette,
                        label_tint,
                        pixels,
                    );
                    match &item.kind {
                        ItemKind::Slider {
                            min,
                            max,
                            ..
                        } => {
                            let therm_x = center_x + gap;
                            let therm_w = 10usize; // number of middle segments
                            let range = (max - min) as f32;
                            let dot = if range > 0.0 {
                                ((item.cached_value - min) as f32 / range * therm_w as f32) as usize
                            } else {
                                0
                            }
                            .min(therm_w);
                            self.draw_thermo(therm_x, y, therm_w, dot, draw_sx, draw_sy, pixels);
                        }
                        ItemKind::Toggle => {
                            let text = if item.cached_value != 0 {
                                lang::OPT_ON
                            } else {
                                lang::OPT_OFF
                            };
                            draw_text_line_tinted(
                                text,
                                center_x + gap,
                                y,
                                draw_sx,
                                draw_sy,
                                &self.palette,
                                TINT_VALUE,
                                pixels,
                            );
                        }
                        ItemKind::Cycle {
                            options,
                        } => {
                            let idx =
                                (item.cached_value as usize).min(options.len().saturating_sub(1));
                            draw_text_line_tinted(
                                options[idx],
                                center_x + gap,
                                y,
                                draw_sx,
                                draw_sy,
                                &self.palette,
                                TINT_VALUE,
                                pixels,
                            );
                        }
                        ItemKind::Label => {}
                        ItemKind::Patch => {}
                    }
                    y += LINEHEIGHT as f32 * draw_sy;
                }
                if self.restart_needed && self.in_options_submenu() {
                    let note_y = pixels.size().height_f32() - 16.0 * draw_sy;
                    draw_text_line(
                        lang::OPT_RESTART,
                        x_ofs + 32.0 * draw_sx,
                        note_y,
                        draw_sx,
                        draw_sy,
                        &self.palette,
                        pixels,
                    );
                }
            } else {
                for item in active.items.iter() {
                    if !item.patch.is_empty() {
                        draw_patch(
                            self.get_patch(&item.patch),
                            x,
                            y,
                            draw_sx,
                            draw_sy,
                            &self.palette,
                            pixels,
                        );
                    }
                    y += LINEHEIGHT as f32 * draw_sy;
                }
            }

            if !is_fullscreen && !is_options {
                // SKULL cursor
                let y = active.y as f32 * sy - 5.0 + active.last_on as f32 * LINEHEIGHT as f32 * sy;
                draw_patch(
                    self.get_patch(SKULLS[self.which_skull]),
                    x - 32.0 * sx,
                    y,
                    sx,
                    sy,
                    &self.palette,
                    pixels,
                );
            }
        }
    }
}

impl SubsystemTrait for GameMenu {
    fn init<T: GameTraits + ConfigTraits>(&mut self, _game: &T) {
        for menu in self.menus.iter_mut() {
            if menu.this == MenuIndex::Skill {
                menu.last_on = 2;
            }
        }
    }

    fn responder<T: GameTraits + ConfigTraits>(&mut self, mut sc: KeyCode, game: &mut T) -> bool {
        // Save description string editing intercepts all input
        if self.save_enter {
            match sc {
                KeyCode::Escape => {
                    // Cancel editing, restore old description
                    self.save_strings[self.save_slot] = self.save_old.clone();
                    self.save_enter = false;
                    game.start_sound(SfxName::Swtchx);
                    return true;
                }
                KeyCode::Return => {
                    // Commit save if description is non-empty
                    if !self.save_strings[self.save_slot].is_empty() {
                        self.do_save(self.save_slot, game);
                    }
                    return true;
                }
                KeyCode::Backspace => {
                    if self.save_char_idx > 0 {
                        self.save_strings[self.save_slot].pop();
                        self.save_char_idx -= 1;
                    }
                    return true;
                }
                _ => {
                    // Accept printable uppercase ASCII chars (font range ! to _)
                    let name = sc.to_string();
                    if name.len() == 1 && self.save_char_idx < SAVESTRINGSIZE {
                        let c = name.chars().next().unwrap();
                        if c.is_ascii_graphic() || c == ' ' {
                            self.save_strings[self.save_slot].push(c);
                            self.save_char_idx += 1;
                        }
                    }
                    return true;
                }
            }
        }

        if !self.active {
            // F-keys
            match sc {
                KeyCode::F1 => {
                    // HELP
                    self.in_help = !self.in_help;
                    if self.in_help {
                        self.current_menu = MenuIndex::ReadThis1;
                        game.start_sound(SfxName::Swtchn);
                    } else {
                        self.current_menu = MenuIndex::TopLevel;
                        game.start_sound(SfxName::Swtchx);
                    }
                    return true;
                }
                KeyCode::F2 => {
                    // SAVE — open save menu directly
                    if game.game_state() != GameState::Level {
                        game.start_sound(SfxName::Oof);
                        return true;
                    }
                    self.active = true;
                    self.open_save_menu(game);
                    game.start_sound(SfxName::Swtchn);
                    return true;
                }
                KeyCode::F3 => {
                    // LOAD — open load menu directly
                    self.active = true;
                    self.open_load_menu(game);
                    game.start_sound(SfxName::Swtchn);
                    return true;
                }
                KeyCode::F6 => {
                    // Quicksave
                    if game.game_state() != GameState::Level {
                        game.start_sound(SfxName::Oof);
                        return true;
                    }
                    if self.quicksave_slot < 0 {
                        // First quicksave: open save menu to pick a slot
                        self.quicksave_slot = QS_PICKING;
                        self.active = true;
                        self.open_save_menu(game);
                        game.start_sound(SfxName::Swtchn);
                    } else {
                        // Re-save to previously chosen slot
                        let slot = self.quicksave_slot as usize;
                        self.read_save_strings(game);
                        let desc = self.save_strings[slot].clone();
                        let desc = if desc == EMPTY_STRING {
                            String::new()
                        } else {
                            desc
                        };
                        game.save_game(format!("slot{slot}"), desc);
                        game.start_sound(SfxName::Swtchn);
                    }
                    return true;
                }
                KeyCode::F9 => {
                    // Quickload
                    if self.quicksave_slot >= 0 {
                        let slot = self.quicksave_slot as usize;
                        game.load_game(format!("slot{slot}"));
                        game.start_sound(SfxName::Swtchn);
                    } else {
                        game.start_sound(SfxName::Oof);
                    }
                    return true;
                }
                KeyCode::Pause => {
                    game.toggle_pause_game();
                    return true;
                }
                KeyCode::Escape => {
                    self.enter_menu(game);
                    return true;
                }
                _ => {}
            }
        } else {
            let hot_key = sc.to_string();
            if hot_key.len() == 1 {
                let hk = hot_key.chars().next().unwrap();
                for (i, item) in self.get_current_menu().items.iter().enumerate() {
                    if item.hotkey == hk {
                        self.get_current_menu().last_on = i;
                        sc = KeyCode::Return;
                        break;
                    }
                }
            }
            match sc {
                KeyCode::Escape => {
                    if self.current_menu == MenuIndex::TopLevel {
                        self.exit_menu(game);
                    } else {
                        if self.current_menu == MenuIndex::OptVideo {
                            self.revert_video_snapshot(game);
                        }
                        if self.current_menu == MenuIndex::Options {
                            self.restart_needed = false;
                        }
                        let active = self.get_current_menu();
                        self.current_menu = active.prev;
                        game.start_sound(SfxName::Swtchn);
                    }
                    return true;
                }
                KeyCode::Down => {
                    let active = self.get_current_menu();
                    if active.items.is_empty() {
                        return true;
                    }
                    active.last_on += 1;
                    if active.last_on >= active.items.len() {
                        active.last_on = 0;
                    }
                    game.start_sound(SfxName::Pstop);
                    return true;
                }
                KeyCode::Up => {
                    let active = self.get_current_menu();
                    if active.items.is_empty() {
                        return true;
                    }
                    if active.last_on == 0 {
                        active.last_on = active.items.len() - 1;
                    } else {
                        active.last_on -= 1;
                    }
                    game.start_sound(SfxName::Pstop);
                    return true;
                }

                KeyCode::Return => {
                    let idx = self.current_menu as usize;
                    let last_on = self.menus[idx].last_on;
                    let status = self.menus[idx].items[last_on].status;
                    let kind = self.menus[idx].items[last_on].kind.clone();
                    let action = self.menus[idx].items[last_on].action;

                    if status != Status::NoCursor {
                        match &kind {
                            ItemKind::Toggle
                            | ItemKind::Cycle {
                                ..
                            } => {
                                self.adjust_option_item(idx, last_on, 1, game);
                            }
                            _ => {
                                self.execute_action(action, last_on, game);
                            }
                        }
                        game.start_sound(SfxName::Pistol);
                    }
                    return true;
                }

                KeyCode::Left | KeyCode::Right => {
                    let idx = self.current_menu as usize;
                    let last_on = self.menus[idx].last_on;
                    let dir = if sc == KeyCode::Right { 1 } else { -1 };
                    if self.adjust_option_item(idx, last_on, dir, game) {
                        game.start_sound(SfxName::Pstop);
                    }
                    return true;
                }

                KeyCode::Backspace => {
                    if self.current_menu == MenuIndex::OptVideo {
                        self.revert_video_snapshot(game);
                    }
                    if self.current_menu == MenuIndex::Options {
                        self.restart_needed = false;
                    }
                    let active = self.get_current_menu();
                    self.current_menu = active.prev;
                    game.start_sound(SfxName::Swtchn);
                    return true;
                }

                _ => {}
            }
        }

        false
    }

    fn ticker<T: GameTraits + ConfigTraits>(&mut self, game: &mut T) -> bool {
        self.skull_anim_counter -= 1;
        if self.skull_anim_counter <= 0 {
            self.which_skull ^= 1;
            self.skull_anim_counter = 8;
        }
        if self.active && self.is_options_menu() {
            let idx = self.current_menu as usize;
            for item in &mut self.menus[idx].items {
                if let Some(key) = item.config_key {
                    item.cached_value = game.config_value(key);
                }
            }
        }
        self.dim_background = game.config_value(ConfigKey::MenuDim) != 0;
        self.active
    }

    fn get_palette(&self) -> &WadPalette {
        &self.palette
    }

    fn draw(&mut self, buffer: &mut impl DrawBuffer) {
        self.draw_pixels(buffer)
    }
}
