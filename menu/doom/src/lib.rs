//! A menu `GameSubsystem` as used by Doom. This loads and uses the Doom assets
//! to display the menu but because it uses `SubsystemTrait` for the actual
//! interaction with the rest of the game it ends up being fairly generic - you
//! could make this fully generic with a little work, or use it as the basis for
//! a different menu.

use gamestate_traits::{
    DrawBuffer, GameMode, GameState, GameTraits, KeyCode, Skill, SubsystemTrait
};
use hud_util::{draw_patch, draw_text_line, fullscreen_scale, hud_scale, measure_text_line};
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

#[derive(Debug, Copy, Clone, PartialOrd, PartialEq)]
enum Status {
    NoCursor, // 0
    Ok,
    ArrowsOk,
}

#[derive(Clone)]
struct MenuItem {
    status: Status,
    /// The name of the patch in the wad to draw for this item
    patch: String,
    /// A function pointer to the 'logic' that drives this menu item
    logic: fn(&mut MenuDoom, usize, &mut dyn GameTraits),
    /// The `char` which activates this item (as a capital letter)
    hotkey: char,
}

impl MenuItem {
    fn new(
        status: Status,
        patch: impl ToString,
        logic: fn(&mut MenuDoom, usize, &mut dyn GameTraits),
        hotkey: char,
    ) -> Self {
        Self {
            status,
            patch: patch.to_string(),
            logic,
            hotkey,
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

    fn on(&self) -> &MenuItem {
        &self.items[self.last_on]
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
}

fn place_holder(_: &mut MenuDoom, _: usize, _: &mut dyn GameTraits) {}

type Patches = HashMap<String, WadPatch>;

pub struct MenuDoom {
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
}

impl MenuDoom {
    pub fn new(mode: GameMode, wad: &WadData, buf_width: i32) -> Self {
        let x_pos = |original_x: i32| -> i32 { original_x };

        let save_slot_items =
            |logic: fn(&mut MenuDoom, usize, &mut dyn GameTraits)| -> Vec<MenuItem> {
                (0..SAVE_SLOT_COUNT)
                    .map(|i| {
                        let hotkey = char::from_digit(i as u32 + 1, 10).unwrap();
                        MenuItem::new(Status::Ok, "", logic, hotkey)
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
                    MenuItem::new(Status::Ok, "M_NGAME", sel_new_game, 'N'),
                    MenuItem::new(Status::Ok, "M_OPTION", place_holder, 'O'),
                    MenuItem::new(Status::Ok, "M_LOADG", sel_load_game, 'L'),
                    MenuItem::new(Status::Ok, "M_SAVEG", sel_save_game, 'S'),
                    MenuItem::new(Status::Ok, "M_RDTHIS", sel_readthis, 'R'),
                    MenuItem::new(Status::Ok, "M_QUITG", sel_quit_game, 'Q'),
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
                            return Some(MenuItem::new(
                                Status::Ok,
                                format!("M_EPI{e}"),
                                sel_episode,
                                char::from_digit(e, 10).unwrap(),
                            ));
                        }
                        None
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
                    MenuItem::new(Status::Ok, "M_JKILL", sel_skill, 'I'),
                    MenuItem::new(Status::Ok, "M_ROUGH", sel_skill, 'R'),
                    MenuItem::new(Status::Ok, "M_HURT", sel_skill, 'H'),
                    MenuItem::new(Status::Ok, "M_ULTRA", sel_skill, 'U'),
                    MenuItem::new(Status::Ok, "M_NMARE", sel_skill, 'N'),
                ],
            ),
            MenuSet::new(
                MenuIndex::ReadThis1,
                MenuIndex::TopLevel,
                vec![],
                buf_width / 2 - 160,
                0,
                match mode {
                    GameMode::Commercial => {
                        vec![MenuItem::new(Status::Ok, "HELP", sel_readthis1, 0 as char)]
                    }
                    GameMode::Retail => {
                        vec![MenuItem::new(Status::Ok, "HELP1", sel_readthis1, 0 as char)]
                    }
                    _ => {
                        vec![MenuItem::new(Status::Ok, "HELP1", sel_readthis1, 0 as char)]
                    }
                },
            ),
            MenuSet::new(
                MenuIndex::ReadThis2,
                MenuIndex::ReadThis1,
                vec![],
                buf_width / 2 - 160,
                0,
                match mode {
                    GameMode::Commercial => {
                        vec![MenuItem::new(
                            Status::Ok,
                            "CREDIT",
                            sel_readthis2,
                            0 as char,
                        )]
                    }
                    GameMode::Retail => {
                        vec![MenuItem::new(
                            Status::Ok,
                            "CREDIT",
                            sel_readthis2,
                            0 as char,
                        )]
                    }
                    _ => {
                        vec![MenuItem::new(Status::Ok, "HELP2", sel_readthis2, 0 as char)]
                    }
                },
            ),
            // Load game menu
            MenuSet::new(
                MenuIndex::LoadGame,
                MenuIndex::TopLevel,
                vec![Title::new("M_LOADG", 72, 28)],
                80,
                54,
                save_slot_items(sel_load_slot),
            ),
            // Save game menu
            MenuSet::new(
                MenuIndex::SaveGame,
                MenuIndex::TopLevel,
                vec![Title::new("M_SAVEG", 72, 28)],
                80,
                54,
                save_slot_items(sel_save_slot),
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

        // Save/load border decoration patches
        for name in ["M_LSLEFT", "M_LSCNTR", "M_LSRGHT"] {
            if let Some(lump) = wad.get_lump(name) {
                patches.insert(name.to_string(), WadPatch::from_lump(lump));
            }
        }

        let palette = wad.playpal_iter().next().unwrap();

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
        }
    }

    /// Sets menu state for entering
    fn enter_menu(&mut self, game: &mut dyn GameTraits) {
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
    fn exit_menu(&mut self, game: &mut dyn GameTraits) {
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

    /// Read save slot descriptions from disk via GameTraits.
    fn read_save_strings(&mut self, game: &dyn GameTraits) {
        let descs = game.read_save_descriptions();
        for (i, desc) in descs.into_iter().enumerate() {
            if i >= SAVE_SLOT_COUNT {
                break;
            }
            self.save_strings[i] = desc.unwrap_or_else(|| EMPTY_STRING.to_string());
        }
    }

    /// Open the load game menu, disabling empty slots.
    fn open_load_menu(&mut self, game: &mut dyn GameTraits) {
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
    fn open_save_menu(&mut self, game: &mut dyn GameTraits) {
        self.read_save_strings(game);
        self.current_menu = MenuIndex::SaveGame;
    }

    /// Commit save: write to slot file and close menu.
    fn do_save(&mut self, slot: usize, game: &mut dyn GameTraits) {
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

    fn draw_pixels(&mut self, pixels: &mut impl DrawBuffer) {
        let (sx, sy) = hud_scale(pixels);

        if self.active || self.in_help {
            let active = &self.menus[self.current_menu as usize];
            let is_fullscreen = active.titles.is_empty() && active.y == 0;
            let is_save_load = self.is_save_load_menu();

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
                // Draw save/load slots with borders and text descriptions
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
                    // Draw typing cursor when editing this slot
                    if self.save_enter && i == self.save_slot {
                        let cursor_x = x + measure_text_line(&self.save_strings[i], draw_sx);
                        let y = y + LINEHEIGHT as f32 * 0.33;
                        draw_text_line("_", cursor_x, y, draw_sx, draw_sy, &self.palette, pixels);
                    }
                    y += LINEHEIGHT as f32 * draw_sy;
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

            if !is_fullscreen {
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

fn sel_new_game(menu: &mut MenuDoom, _: usize, game: &mut dyn GameTraits) {
    if game.get_mode() == GameMode::Commercial {
        menu.current_menu = MenuIndex::Skill;
        return;
    }
    menu.current_menu = MenuIndex::Episodes;
}

fn sel_readthis(menu: &mut MenuDoom, _: usize, _: &mut dyn GameTraits) {
    menu.current_menu = MenuIndex::ReadThis1;
}

fn sel_readthis1(menu: &mut MenuDoom, _: usize, _: &mut dyn GameTraits) {
    menu.current_menu = MenuIndex::ReadThis2;
}

fn sel_readthis2(menu: &mut MenuDoom, _: usize, _: &mut dyn GameTraits) {
    menu.current_menu = MenuIndex::TopLevel;
}

fn sel_quit_game(_menu: &mut MenuDoom, _: usize, game: &mut dyn GameTraits) {
    game.quit_game();
}

// TODO: kind of bad, should make a better method to set episode even if not
// sequential
fn sel_episode(menu: &mut MenuDoom, _choice: usize, _game: &mut dyn GameTraits) {
    menu.episode = menu
        .get_current_menu()
        .on()
        .hotkey
        .to_digit(10)
        .unwrap_or_default() as usize
        - 1;
    menu.current_menu = MenuIndex::Skill;
}

fn sel_skill(menu: &mut MenuDoom, choice: usize, game: &mut dyn GameTraits) {
    menu.exit_menu(game);
    let skill = Skill::from(choice);
    game.defered_init_new(skill, menu.episode + 1, 1);
}

/// Main menu "Load Game" entry — opens load slot picker.
fn sel_load_game(menu: &mut MenuDoom, _: usize, game: &mut dyn GameTraits) {
    menu.open_load_menu(game);
}

/// Main menu "Save Game" entry — opens save slot picker (only in-level).
fn sel_save_game(menu: &mut MenuDoom, _: usize, game: &mut dyn GameTraits) {
    if game.game_state() != GameState::Level {
        return;
    }
    menu.open_save_menu(game);
}

/// Load slot selected — load from slot file.
fn sel_load_slot(menu: &mut MenuDoom, choice: usize, game: &mut dyn GameTraits) {
    game.load_game(format!("slot{choice}"));
    menu.exit_menu(game);
}

/// Save slot selected — enter string editing mode.
fn sel_save_slot(menu: &mut MenuDoom, choice: usize, _game: &mut dyn GameTraits) {
    menu.save_enter = true;
    menu.save_slot = choice;
    menu.save_old = menu.save_strings[choice].clone();
    if menu.save_strings[choice] == EMPTY_STRING {
        menu.save_strings[choice].clear();
    }
    menu.save_char_idx = menu.save_strings[choice].len();
}

impl SubsystemTrait for MenuDoom {
    fn init(&mut self, _game: &impl GameTraits) {
        for menu in self.menus.iter_mut() {
            if menu.this == MenuIndex::Skill {
                menu.last_on = 2;
            }
        }
    }

    fn responder(&mut self, mut sc: KeyCode, game: &mut impl GameTraits) -> bool {
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
                    self.exit_menu(game);
                    return true;
                }
                KeyCode::Down => {
                    let active = self.get_current_menu();
                    active.last_on += 1;
                    if active.last_on > active.items.len() - 1 {
                        active.last_on = 0;
                    }
                    game.start_sound(SfxName::Pstop);
                    return true;
                }
                KeyCode::Up => {
                    let active = self.get_current_menu();
                    if active.last_on == 0 {
                        active.last_on = active.items.len() - 1;
                    } else {
                        active.last_on -= 1;
                    }
                    game.start_sound(SfxName::Pstop);
                    return true;
                }

                KeyCode::Return => {
                    let mut idx = 0;
                    for (i, m) in self.menus.iter().enumerate() {
                        if m.this == self.current_menu {
                            idx = i;
                        }
                    }

                    let last_on = self.menus[idx].last_on;
                    let status = self.menus[idx].items[last_on].status;
                    let logic = self.menus[idx].items[last_on].logic;

                    if status != Status::NoCursor {
                        (logic)(self, last_on, game);
                        game.start_sound(SfxName::Pistol);
                    }
                    return true;
                }

                KeyCode::Backspace => {
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

    fn ticker(&mut self, _: &mut impl GameTraits) -> bool {
        self.skull_anim_counter -= 1;
        if self.skull_anim_counter <= 0 {
            self.which_skull ^= 1;
            self.skull_anim_counter = 8;
        }
        self.active
    }

    fn get_palette(&self) -> &WadPalette {
        &self.palette
    }

    fn draw(&mut self, buffer: &mut impl DrawBuffer) {
        self.draw_pixels(buffer)
    }
}
