//! Display the end-of-level statistics for the player and the next level's name

use defs::{AnimType, Animation, MAP_POINTS, Patches, SHOW_NEXT_LOC_DELAY, State, animations};
use game_config::GameMode;
use gameplay::{TICRATE, WorldEndPlayerInfo};
use gamestate_traits::{ConfigTraits, GameTraits, KeyCode, SubsystemTrait, WorldInfo};
use hud_util::{
    HUD_STRING, HUDString, draw_patch, draw_text_line, fullscreen_scale, measure_text_line
};
use log::warn;
use math::m_random;
use render_common::DrawBuffer;
use sound_common::MusTrack;
use std::collections::HashMap;
use wad::WadData;
use wad::types::{BLACK, WadFlat, WadPalette, WadPatch};
use wad::umapinfo::UMapInfo;

mod defs;
mod loc_state;
mod no_state;
mod stat_state;
mod text_state;

const EP4_BG: &str = "INTERPIC";
const COMMERCIAL_BG: &str = "INTERPIC";
const TITLE_Y: f32 = 2.0;

pub(crate) enum LevelDisplay<'a> {
    Patch(&'a WadPatch),
    Text(&'a str),
}

impl LevelDisplay<'_> {
    pub(crate) fn draw_centered(
        &self,
        center_x: f32,
        y: f32,
        sx: f32,
        sy: f32,
        palette: &WadPalette,
        buffer: &mut impl DrawBuffer,
    ) {
        match self {
            LevelDisplay::Patch(patch) => {
                draw_patch(
                    patch,
                    center_x - patch.width as f32 * sx / 2.0,
                    y,
                    sx,
                    sy,
                    palette,
                    buffer,
                );
            }
            LevelDisplay::Text(text) => {
                let w = measure_text_line(text, sx);
                draw_text_line(text, center_x - w / 2.0, y, sx, sy, palette, buffer);
            }
        }
    }
}

pub struct Intermission {
    palette: WadPalette,
    bg_patches: Vec<WadPatch>,
    yah_patches: Vec<WadPatch>,
    /// 0 or 1 (left/right). Splat is 2
    yah_idx: usize,
    level_names: Vec<Vec<WadPatch>>,
    animations: Vec<Vec<Animation>>,
    current_bg: usize,
    /// General counter for animated BG
    bg_count: i32,
    mode: GameMode,
    // info updated by ticker
    player_info: WorldEndPlayerInfo,
    level_info: WorldInfo,

    pointer_on: bool,
    count: i32,
    state: State,
    /// General patches not specific to retail/commercial/registered
    patches: Patches,
    /// UMAPINFO level name patches keyed by map name (e.g. "WILV50")
    umapinfo_patches: HashMap<String, WadPatch>,
    /// UMAPINFO level names keyed by map name (e.g. "E6M1" → "Cursed Darkness")
    umapinfo_names: HashMap<String, String>,
    umapinfo: Option<UMapInfo>,
    /// Intertext typewriter text (set during init from UMAPINFO)
    inter_text: HUDString,
    /// Background flat for intertext screen
    inter_text_bg: Option<WadFlat>,
}

impl Intermission {
    pub fn new(mode: GameMode, wad: &WadData, umapinfo: &Option<UMapInfo>) -> Self {
        let palette = wad.lump_iter::<WadPalette>("PLAYPAL").next().unwrap();

        let mut level_names = Vec::new();
        let mut bg_patches = Vec::new();
        let mut yah_patches = Vec::new();
        if mode == GameMode::Commercial {
            let lump = wad.get_lump(COMMERCIAL_BG).unwrap();
            bg_patches.push(WadPatch::from_lump(lump));

            let mut names_patches = Vec::new();
            for m in 0..32 {
                let name = format!("CWILV{m:0>2}");
                let lump = wad.get_lump(&name).unwrap();
                names_patches.push(WadPatch::from_lump(lump));
            }
            level_names.push(names_patches);
        } else {
            for e in 0..3 {
                if mode == GameMode::Shareware && e > 0 {
                    break;
                }
                let lump = wad.get_lump(&format!("WIMAP{e}")).unwrap();
                bg_patches.push(WadPatch::from_lump(lump));

                let mut names_patches = Vec::new();
                for m in 0..9 {
                    let name = format!("WILV{e}{m}");
                    let lump = wad.get_lump(&name).unwrap();
                    names_patches.push(WadPatch::from_lump(lump));
                }
                level_names.push(names_patches);
            }

            let lump = wad.get_lump("WIURH0").unwrap();
            yah_patches.push(WadPatch::from_lump(lump));
            let lump = wad.get_lump("WIURH1").unwrap();
            yah_patches.push(WadPatch::from_lump(lump));
            let lump = wad.get_lump("WISPLAT").unwrap();
            yah_patches.push(WadPatch::from_lump(lump));
        }

        if mode == GameMode::Retail {
            let lump = wad.get_lump(EP4_BG).unwrap();
            bg_patches.push(WadPatch::from_lump(lump));

            let mut names_patches = Vec::new();
            for m in 0..9 {
                let name = format!("WILV3{m}");
                let lump = wad.get_lump(&name).unwrap();
                names_patches.push(WadPatch::from_lump(lump));
            }
            level_names.push(names_patches);
        }

        // load all the BG animations
        let mut anims = animations();
        for (e, anims) in anims.iter_mut().enumerate() {
            if mode == GameMode::Shareware && e > 0 {
                break;
            }
            for (l, anim) in anims.iter_mut().enumerate() {
                for i in 0..anim.num_of {
                    // episode, level, anim_num
                    if let Some(lump) = wad.get_lump(&format!("WIA{e}{l:0>2}{i:0>2}")) {
                        anim.patches.push(WadPatch::from_lump(lump));
                    } else if mode != GameMode::Commercial {
                        warn!("Missing WIA{e}{l:0>2}{i:0>2}");
                    }
                }
            }
        }

        // TODO: TMP TESTING STUFF HERE
        let font_start = b'!';
        let font_end = b'_';
        let font_count = font_end - font_start + 1;
        for i in 0..font_count {
            let i = i + font_start;
            if let Some(lump) = wad.get_lump(&format!("STCFN{i:0>3}")) {
                WadPatch::from_lump(lump);
            } else if mode != GameMode::Commercial {
                warn!("Missing STCFN{i:0>3}");
            }
        }

        let mut umapinfo_patches = HashMap::new();
        let mut umapinfo_names = HashMap::new();
        if let Some(info) = umapinfo {
            for entry in info.entries() {
                if let Some(ref pic) = entry.level_pic {
                    if let Some(lump) = wad.get_lump(pic) {
                        umapinfo_patches.insert(entry.map_name.clone(), WadPatch::from_lump(lump));
                    }
                }
                if let Some(ref name) = entry.level_name {
                    umapinfo_names.insert(entry.map_name.clone(), name.clone());
                }
                if let Some(ref pic) = entry.exit_pic {
                    let key = format!("__exitpic_{}", entry.map_name);
                    if let Some(lump) = wad.get_lump(pic) {
                        umapinfo_patches.insert(key, WadPatch::from_lump(lump));
                    }
                }
                if let Some(ref pic) = entry.enter_pic {
                    let key = format!("__enterpic_{}", entry.map_name);
                    if let Some(lump) = wad.get_lump(pic) {
                        umapinfo_patches.insert(key, WadPatch::from_lump(lump));
                    }
                }
            }
        }

        Self {
            palette,
            bg_patches,
            level_names,
            animations: anims,
            yah_patches,
            yah_idx: 0,
            current_bg: 0,
            bg_count: 0,
            mode,
            player_info: WorldEndPlayerInfo::default(),
            level_info: WorldInfo::default(),
            pointer_on: true,
            count: SHOW_NEXT_LOC_DELAY * TICRATE,
            state: State::None,
            patches: Patches::new(wad),
            umapinfo_patches,
            umapinfo_names,
            umapinfo: umapinfo.clone(),
            inter_text: HUD_STRING,
            inter_text_bg: None,
        }
    }

    /// Draw the fullscreen background patch, clearing the buffer first and
    /// centering.
    pub(crate) fn draw_bg(&self, x_offset: f32, sx: f32, sy: f32, buffer: &mut impl DrawBuffer) {
        buffer.buf_mut().fill(BLACK);
        draw_patch(self.get_bg(), x_offset, 0.0, sx, sy, &self.palette, buffer);
    }

    pub(crate) fn get_bg(&self) -> &WadPatch {
        let completed_map = self.map_name_for(self.level_info.episode, self.level_info.last);
        let exit_key = format!("__exitpic_{}", completed_map);
        if let Some(patch) = self.umapinfo_patches.get(&exit_key) {
            return patch;
        }
        self.bg_patches
            .get(self.current_bg)
            .unwrap_or(&self.bg_patches[self.bg_patches.len() - 1])
    }

    fn map_name_for(&self, episode: usize, map: usize) -> String {
        if self.mode == GameMode::Commercial {
            if map < 9 {
                format!("MAP0{}", map + 1)
            } else {
                format!("MAP{}", map + 1)
            }
        } else {
            format!("E{}M{}", episode + 1, map)
        }
    }

    pub(crate) fn get_this_level_name(&self) -> LevelDisplay<'_> {
        let name = self.map_name_for(self.level_info.episode, self.level_info.last);
        if let Some(patch) = self.umapinfo_patches.get(&name) {
            return LevelDisplay::Patch(patch);
        }
        if let Some(text) = self.umapinfo_names.get(&name) {
            return LevelDisplay::Text(text);
        }
        let ep = self.level_info.episode.min(self.level_names.len() - 1);
        let idx = self.level_info.last - 1;
        if let Some(names) = self.level_names.get(ep) {
            if let Some(patch) = names.get(idx) {
                return LevelDisplay::Patch(patch);
            }
        }
        LevelDisplay::Patch(&self.level_names[0][0])
    }

    pub(crate) fn get_enter_level_name(&self) -> LevelDisplay<'_> {
        let name = self.map_name_for(self.level_info.episode, self.level_info.next + 1);
        if let Some(patch) = self.umapinfo_patches.get(&name) {
            return LevelDisplay::Patch(patch);
        }
        if let Some(text) = self.umapinfo_names.get(&name) {
            return LevelDisplay::Text(text);
        }
        let ep = self.level_info.episode.min(self.level_names.len() - 1);
        if let Some(names) = self.level_names.get(ep) {
            if let Some(patch) = names.get(self.level_info.next) {
                return LevelDisplay::Patch(patch);
            }
        }
        LevelDisplay::Patch(&self.level_names[0][0])
    }

    fn init_animated_bg(&mut self) {
        if self.mode == GameMode::Commercial || self.level_info.episode > 2 {
            return;
        }

        for anim in self.animations[self.level_info.episode].iter_mut() {
            anim.counter = -1;
            // Next time to draw?
            match anim.kind {
                AnimType::Always => {
                    anim.next_tic = self.bg_count + 1 + (m_random() % anim.period);
                }
                AnimType::Level => {
                    anim.next_tic = self.bg_count + 1;
                }
            }
        }
    }

    fn update_animated_bg(&mut self) {
        if self.mode == GameMode::Commercial || self.level_info.episode > 2 {
            return;
        }

        for (i, anim) in self.animations[self.level_info.episode]
            .iter_mut()
            .enumerate()
        {
            if self.bg_count == anim.next_tic {
                match anim.kind {
                    AnimType::Always => {
                        anim.counter += 1;
                        if anim.counter >= anim.num_of {
                            anim.counter = 0;
                        }
                        anim.next_tic = self.bg_count + anim.period;
                    }
                    AnimType::Level => {
                        if !(self.state == State::StatCount && i == 7)
                            && self.level_info.next == anim.data1 as usize
                        {
                            anim.counter += 1;
                            if anim.counter == anim.num_of {
                                anim.counter -= 1;
                            }
                            anim.next_tic = self.bg_count + anim.period;
                        }
                    }
                }
            }
        }
    }

    fn draw_animated_bg_pixels(
        &self,
        x_offset: f32,
        sx: f32,
        sy: f32,
        pixels: &mut impl DrawBuffer,
    ) {
        if self.mode == GameMode::Commercial || self.level_info.episode > 2 {
            return;
        }

        for anim in self.animations[self.level_info.episode].iter() {
            if anim.counter >= 0 {
                draw_patch(
                    &anim.patches[anim.counter as usize],
                    x_offset + anim.location.0 as f32 * sx,
                    anim.location.1 as f32 * sy,
                    sx,
                    sy,
                    &self.palette,
                    pixels,
                );
            }
        }
    }

    // fn draw_animated_bg(&self, scale: i32, buffer: &mut RenderTarget) {
    //     if self.mode == GameMode::Commercial || self.level_info.epsd > 2 {
    //         return;
    //     }

    //     match buffer.render_type() {
    //         gamestate_traits::RenderType::Software => {
    //             let pixels = unsafe { buffer.software_unchecked() };
    //             self.draw_animated_bg_pixels(scale, pixels);
    //         }
    //         gamestate_traits::RenderType::SoftOpenGL => {
    //             let pixels = unsafe { buffer.soft_opengl_unchecked() };
    //             self.draw_animated_bg_pixels(scale, pixels);
    //         }
    //         gamestate_traits::RenderType::OpenGL => todo!(),
    //         gamestate_traits::RenderType::Vulkan => todo!(),
    //     }
    // }
}

impl SubsystemTrait for Intermission {
    fn init<T: GameTraits + ConfigTraits>(&mut self, game: &T) {
        self.bg_count = 0;
        self.yah_idx = 0;
        self.current_bg = 0;
        self.pointer_on = true;
        self.state = State::None;
        self.inter_text_bg = None;

        self.player_info = game.player_end_info().clone();
        self.level_info = game.level_end_info().clone();
        self.current_bg = self.level_info.episode;

        // Pre-load intertext backdrop from UMAPINFO
        let map_name = self.map_name_for(self.level_info.episode, self.level_info.last);
        if let Some(entry) = self.umapinfo.as_ref().and_then(|u| u.get(&map_name)) {
            if entry.inter_text.is_some() {
                let backdrop = entry.inter_backdrop.as_deref().unwrap_or("FLOOR4_8");
                let wad = game.get_wad_data();
                if let Some(lump) = wad.get_lump(backdrop) {
                    self.inter_text_bg = Some(WadFlat {
                        name: backdrop.to_string(),
                        data: lump.data.clone(),
                    });
                }
            }
        }

        self.init_stats();
    }

    fn responder<T: GameTraits + ConfigTraits>(&mut self, sc: KeyCode, _game: &mut T) -> bool {
        if sc == KeyCode::Return || sc == KeyCode::Space {
            if self.state == State::InterText {
                self.skip_inter_text();
                return true;
            }
            self.count = 0;
            return true;
        }
        false
    }

    fn ticker<T: GameTraits + ConfigTraits>(&mut self, game: &mut T) -> bool {
        self.bg_count += 1;

        if self.bg_count == 1 {
            if self.mode == GameMode::Commercial {
                game.change_music(MusTrack::Dm2int);
            } else {
                game.change_music(MusTrack::Inter);
            }

            self.player_info.total_kills = if self.level_info.maxkills > 0 {
                (self.player_info.total_kills * 100) / self.level_info.maxkills
            } else {
                0
            };
            self.player_info.items_collected = if self.level_info.maxitems > 0 {
                (self.player_info.items_collected * 100) / self.level_info.maxitems
            } else {
                0
            };
            self.player_info.secrets_found = if self.level_info.maxsecret > 0 {
                (self.player_info.secrets_found * 100) / self.level_info.maxsecret
            } else {
                0
            };
        }

        match self.state {
            State::StatCount => {
                self.update_stats();
            }
            State::InterText => {
                self.update_inter_text();
            }
            State::NextLoc => {
                self.update_show_next_loc();
            }
            State::None => {
                self.update_no_state(game);
            }
        }

        false
    }

    fn get_palette(&self) -> &WadPalette {
        &self.palette
    }

    fn draw(&mut self, buffer: &mut impl DrawBuffer) {
        let (sx, sy) = fullscreen_scale(buffer);
        let x_ofs = ((buffer.size().width_f32() - 320.0 * sx) / 2.0).floor();

        match self.state {
            State::StatCount => {
                self.draw_stats_pixels(x_ofs, sx, sy, buffer);
            }
            State::InterText => {
                self.draw_inter_text(buffer);
            }
            State::NextLoc => {
                self.draw_next_loc_pixels(x_ofs, sx, sy, buffer);
            }
            State::None => {
                self.draw_no_state(x_ofs, sx, sy, buffer);
            }
        }
    }
}
