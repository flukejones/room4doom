//! A menu `Machination` as used by Doom. This loads and uses the Doom assets to
//! display the menu but because it uses `MachinationTrait` for the actual interaction
//! with the rest of the game it ends up being fairly generic - you could make this
//! fully generic with a little work, or use it as the basis for a different menu.

use gamestate_traits::{GameMode, GameTraits, MachinationTrait, PixelBuf, Scancode, Skill};
use sound_traits::SfxName;
use std::collections::HashMap;
use wad::{
    lumps::{WadPalette, WadPatch},
    WadData,
};

const SAVESTRINGSIZE: i32 = 24;
const SKULLXOFF: i32 = -32;
const LINEHEIGHT: i32 = 16;
const SKULLS: [&str; 2] = ["M_SKULL1", "M_SKULL2"];

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
    patch: &'static str,
    /// A function pointer to the 'logic' that drives this menu item
    logic: fn(&mut MenuDoom, i32, &mut dyn GameTraits),
    /// The `char` which activates this item (as a capital letter)
    hotkey: char,
}

impl MenuItem {
    fn new(
        status: Status,
        patch: &'static str,
        logic: fn(&mut MenuDoom, i32, &mut (dyn GameTraits)),
        hotkey: char,
    ) -> Self {
        Self {
            status,
            patch,
            logic,
            hotkey,
        }
    }
}

/// A title item, such as the DOOM logo. Typically drawn at the top of the menu but
/// you could draw it anywhere you want really.
#[derive(Clone)]
struct Title {
    /// The name of the patch in the wad to draw for this item
    patch: &'static str,
    x: i32,
    y: i32,
}

impl Title {
    fn new(patch: &'static str, x: i32, y: i32) -> Self {
        Self { patch, x, y }
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
}

fn place_holder(menu: &mut MenuDoom, choice: i32, game: &mut dyn GameTraits) {}

type Patches = HashMap<&'static str, WadPatch>;

pub struct MenuDoom {
    /// Is the menu active?
    active: bool,
    /// A specific helper for pressing F1
    in_help: bool,
    save_enter: bool,
    save_slot: usize,
    /// The old description (for overwrites)
    save_old: String,
    /// Which char of the buffer to edit
    save_char_idx: usize,
    //
    /// Main menu def
    menus: Vec<MenuSet>,
    current_menu: MenuIndex,

    patches: Patches,
    palette: WadPalette,
    /// Track the episode selected by episode menu
    episode: i32,
    which_skull: usize,
    skull_anim_counter: i32,
}

impl MenuDoom {
    pub fn new(mode: GameMode, wad: &WadData) -> Self {
        let menus = vec![
            MenuSet::new(
                MenuIndex::TopLevel,
                MenuIndex::TopLevel,
                vec![Title::new("M_DOOM", 92, 2)], // Header item and position
                97,                                // Sub-items starting X
                64, // First item start Y (is incremented by LINEHEIGHT
                vec![
                    MenuItem::new(Status::Ok, "M_NGAME", sel_new_game, 'N'),
                    MenuItem::new(Status::Ok, "M_OPTION", place_holder, 'O'),
                    MenuItem::new(Status::Ok, "M_LOADG", place_holder, 'L'),
                    MenuItem::new(Status::Ok, "M_SAVEG", place_holder, 'S'),
                    MenuItem::new(Status::Ok, "M_RDTHIS", sel_readthis, 'R'),
                    MenuItem::new(Status::Ok, "M_QUITG", sel_quit_game, 'Q'),
                ],
            ),
            MenuSet::new(
                MenuIndex::Episodes,
                MenuIndex::TopLevel,
                vec![Title::new("M_EPISOD", 54, 38)],
                48,
                63,
                if mode == GameMode::Retail {
                    vec![
                        MenuItem::new(Status::Ok, "M_EPI1", sel_episode, 'K'),
                        MenuItem::new(Status::Ok, "M_EPI2", sel_episode, 'T'),
                        MenuItem::new(Status::Ok, "M_EPI3", sel_episode, 'I'),
                        MenuItem::new(Status::Ok, "M_EPI4", sel_episode, 'S'),
                    ]
                } else {
                    vec![
                        MenuItem::new(Status::Ok, "M_EPI1", sel_episode, 'K'),
                        MenuItem::new(Status::Ok, "M_EPI2", sel_episode, 'T'),
                        MenuItem::new(Status::Ok, "M_EPI3", sel_episode, 'I'),
                    ]
                },
            ),
            MenuSet::new(
                MenuIndex::Skill,
                if mode == GameMode::Commercial {
                    MenuIndex::TopLevel
                } else {
                    MenuIndex::Episodes
                },
                vec![Title::new("M_NEWG", 96, 14), Title::new("M_SKILL", 54, 38)],
                48,
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
                0,
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
                0,
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
        ];

        let mut patches = HashMap::new();
        for menu in &menus {
            for item in &menu.titles {
                if let Some(lump) = wad.get_lump(item.patch) {
                    patches.insert(item.patch, WadPatch::from_lump(lump));
                }
            }
            for item in &menu.items {
                if let Some(lump) = wad.get_lump(item.patch) {
                    patches.insert(item.patch, WadPatch::from_lump(lump));
                }
            }
        }

        for patch in SKULLS {
            if let Some(lump) = wad.get_lump(patch) {
                patches.insert(patch, WadPatch::from_lump(lump));
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
            //
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

    fn get_patch(&self, name: &str) -> &WadPatch {
        self.patches
            .get(name)
            .expect(&format!("{name} not in cache"))
    }
}

fn sel_new_game(menu: &mut MenuDoom, _: i32, game: &mut dyn GameTraits) {
    if game.get_mode() == GameMode::Commercial {
        menu.current_menu = MenuIndex::Skill;
        return;
    }
    menu.current_menu = MenuIndex::Episodes;
}

fn sel_readthis(menu: &mut MenuDoom, _: i32, _: &mut dyn GameTraits) {
    menu.current_menu = MenuIndex::ReadThis1;
}

fn sel_readthis1(menu: &mut MenuDoom, _: i32, _: &mut dyn GameTraits) {
    menu.current_menu = MenuIndex::ReadThis2;
}

fn sel_readthis2(menu: &mut MenuDoom, _: i32, _: &mut dyn GameTraits) {
    menu.current_menu = MenuIndex::TopLevel;
}

fn sel_quit_game(menu: &mut MenuDoom, _: i32, game: &mut dyn GameTraits) {
    game.quit_game();
}

fn sel_episode(menu: &mut MenuDoom, choice: i32, game: &mut dyn GameTraits) {
    menu.episode = choice;
    menu.current_menu = MenuIndex::Skill;
}

fn sel_skill(menu: &mut MenuDoom, choice: i32, game: &mut dyn GameTraits) {
    menu.exit_menu(game);
    let skill = Skill::from(choice);
    game.defered_init_new(skill, menu.episode + 1, 1);
}

impl MachinationTrait for MenuDoom {
    fn init(&mut self, _game: &impl GameTraits) {
        for menu in self.menus.iter_mut() {
            if menu.this == MenuIndex::Skill {
                menu.last_on = 2;
            }
        }
    }

    fn responder(&mut self, mut sc: Scancode, game: &mut impl GameTraits) -> bool {
        if !self.active {
            // F-keys
            match sc {
                Scancode::F1 => {
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
                Scancode::F2 => {
                    // SAVE
                }
                Scancode::F3 => {
                    // LOAD
                }
                Scancode::F6 => {
                    // QUICKSAVE
                }
                Scancode::F9 => {
                    // QUICKLOAD
                }
                Scancode::Pause => {
                    game.toggle_pause_game();
                    return true;
                }
                Scancode::Escape => {
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
                        sc = Scancode::Return;
                        break;
                    }
                }
            }
            match sc {
                Scancode::Escape => {
                    self.exit_menu(game);
                    return true;
                }
                Scancode::Down => {
                    let active = self.get_current_menu(); //&mut self.menus[self.current_menu as usize];
                    active.last_on += 1;
                    if active.last_on > active.items.len() - 1 {
                        active.last_on = 0;
                    }
                    game.start_sound(SfxName::Pstop);
                    return true;
                }
                Scancode::Up => {
                    let active = self.get_current_menu();
                    if active.last_on == 0 {
                        active.last_on = active.items.len() - 1;
                    } else {
                        active.last_on -= 1;
                    }
                    game.start_sound(SfxName::Pstop);
                    return true;
                }

                Scancode::Return => {
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
                        (logic)(self, last_on as i32, game);
                        game.start_sound(SfxName::Pistol);
                    }
                    return true;
                }

                Scancode::Backspace => {
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

    fn draw(&mut self, buffer: &mut PixelBuf) {
        if self.active || self.in_help {
            let active = &self.menus[self.current_menu as usize];
            // Titles
            for item in active.titles.iter() {
                self.draw_patch(self.get_patch(item.patch), item.x, item.y, buffer);
            }
            // sub-items
            let x = active.x;
            let mut y = active.y;
            for item in active.items.iter() {
                self.draw_patch(self.get_patch(item.patch), x, y, buffer);
                y += LINEHEIGHT;
            }

            // SKULL
            let y = active.y - 5 + active.last_on as i32 * LINEHEIGHT;
            self.draw_patch(self.get_patch(SKULLS[self.which_skull]), x + -32, y, buffer);
        }
    }
}
