use menu_traits::{MenuDraw, MenuFunctions, MenuResponder, MenuTicker, PixelBuf, Scancode};
use std::collections::HashMap;
use wad::{
    lumps::{WadPalette, WadPatch},
    WadData,
};

const SAVESTRINGSIZE: i32 = 24;
const SKULLXOFF: i32 = -32;
const LINEHEIGHT: i32 = 16;

enum Status {
    NoCursor, // 0
    Ok,
    ArrowsOk,
}

struct MenuItem {
    status: Status,
    /// The name of the patch in the wad to draw for this item
    patch: &'static str,
    logic: fn(&mut MenuDoom, i32, &mut dyn MenuFunctions),
    hotkey: char,
}

impl MenuItem {
    fn new(
        status: Status,
        patch: &'static str,
        logic: fn(&mut MenuDoom, i32, &mut (dyn MenuFunctions)),
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

struct Title {
    patch: &'static str,
    x: i32,
    y: i32,
}

impl Title {
    fn new(patch: &'static str, x: i32, y: i32) -> Self {
        Self { patch, x, y }
    }
}

struct MenuSet {
    prev: usize,
    titles: Vec<Title>,
    /// Each item is drawn later during menu setup
    items: Vec<MenuItem>,
    /// Sub-item start X coord
    x: i32,
    /// Sub-item start Y coord
    y: i32,
    /// The index of the last item the user was on in this menu
    last_on: usize,
}

impl MenuSet {
    fn new(prev: usize, titles: Vec<Title>, x: i32, y: i32, items: Vec<MenuItem>) -> Self {
        Self {
            titles,
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
}

fn place_holder(menu: &mut MenuDoom, choice: i32, game: &mut dyn MenuFunctions) {}

type Patches = HashMap<&'static str, WadPatch>;

pub struct MenuDoom {
    /// Is the menu active?
    active: bool,
    ///
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
    pallette: WadPalette,
}

impl MenuDoom {
    pub fn new(wad: &WadData) -> Self {
        let menus = vec![
            MenuSet::new(
                0,
                vec![Title::new("M_DOOM", 92, 2)], // Header item and position
                97, // Sub-items starting X
                64, // First item start Y (is incremented by LINEHEIGHT
                vec![
                    MenuItem::new(Status::Ok, "M_NGAME", sel_new_game, 'n'),
                    MenuItem::new(Status::Ok, "M_OPTION", place_holder, 'o'),
                    MenuItem::new(Status::Ok, "M_LOADG", place_holder, 'l'),
                    MenuItem::new(Status::Ok, "M_SAVEG", place_holder, 's'),
                    MenuItem::new(Status::Ok, "M_RDTHIS", place_holder, 'r'),
                    MenuItem::new(Status::Ok, "M_QUITG", sel_quit_game, 'q'),
                ],
            ),
            MenuSet::new(
                0,
                vec![Title::new("M_EPISOD", 54, 38)],
                48,
                63,
                vec![
                    MenuItem::new(Status::Ok, "M_EPI1", sel_episode, 'k'),
                    MenuItem::new(Status::Ok, "M_EPI2", sel_episode, 't'),
                    MenuItem::new(Status::Ok, "M_EPI3", sel_episode, 'i'),
                    MenuItem::new(Status::Ok, "M_EPI4", sel_episode, 's'),
                ],
            ),
            MenuSet::new(
                1,
                vec![Title::new("M_NEWG", 96, 14), Title::new("M_SKILL", 54, 38)],
                48,
                63,
                vec![
                    MenuItem::new(Status::Ok, "M_JKILL", sel_skill, 'i'),
                    MenuItem::new(Status::Ok, "M_ROUGH", sel_skill, 'r'),
                    MenuItem::new(Status::Ok, "M_HURT", sel_skill, 'h'),
                    MenuItem::new(Status::Ok, "M_ULTRA", sel_skill, 'u'),
                    MenuItem::new(Status::Ok, "M_NMARE", sel_skill, 'n'),
                ],
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
        let pallette = wad.playpal_iter().next().unwrap();

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
            pallette,
        }
    }

    fn draw_patch(&self, name: &str, x: i32, y: i32, pixels: &mut PixelBuf) {
        let image = self.patches.get(name).unwrap();

        let mut xtmp = 0;
        for c in image.columns.iter() {
            for (ytmp, p) in c.pixels.iter().enumerate() {
                let colour = self.pallette.0[*p];

                pixels.set_pixel(
                    (x + xtmp as i32) as usize, // - (image.left_offset as i32),
                    (y + ytmp as i32 + c.y_offset as i32) as usize, // - image.top_offset as i32 - 30,
                    colour.r,
                    colour.g,
                    colour.b,
                    255,
                );
            }
            if c.y_offset == 255 {
                xtmp += 1;
            }
        }
    }
}

fn sel_new_game(menu: &mut MenuDoom, choice: i32, game: &mut dyn MenuFunctions) {
    // TODO: game mode
    menu.current_menu = MenuIndex::Episodes;
    println!("sel_new_game not implemented");
}

fn sel_quit_game(menu: &mut MenuDoom, choice: i32, game: &mut dyn MenuFunctions) {
    println!("sel_quit_game not implemented");
    game.quit_game();
}

fn sel_episode(menu: &mut MenuDoom, choice: i32, game: &mut dyn MenuFunctions) {
    menu.current_menu = MenuIndex::Skill;
    println!("Not implemented");
}

fn sel_skill(menu: &mut MenuDoom, choice: i32, game: &mut dyn MenuFunctions) {
    println!("sel_skill not implemented");
}

impl MenuResponder for MenuDoom {
    fn responder(&mut self, sc: Scancode, game: &mut impl MenuFunctions) -> bool {
        let mut res = false;
        if sc == Scancode::Escape {
            res = true;
            game.quit_game();
        }
        if sc == Scancode::P {
            res = true;
            self.active = !self.active;
            game.pause_game(self.active);
        }
        res
    }
}

impl MenuTicker for MenuDoom {
    fn ticker(&mut self, game: &mut impl MenuFunctions) {}
}

impl MenuDraw for MenuDoom {
    fn render_menu(&mut self, buffer: &mut PixelBuf) {
        if self.active {
            let active = &self.menus[self.current_menu as usize];
            // Titles
            for item in active.titles.iter() {
                self.draw_patch(item.patch, item.x, item.y, buffer);
            }
            // sub-items
            let x = active.x;
            let mut y = active.y;
            for item in active.items.iter() {
                self.draw_patch(item.patch, x, y, buffer);
                y += LINEHEIGHT;
            }
        }
    }
}
