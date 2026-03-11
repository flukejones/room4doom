//! Binary save/load for game state (quicksave).
//!
//! Format: little-endian, manual serialization. Cross-references between
//! map objects (target/tracer) are dropped on load, matching original Doom
//! behaviour.

use std::fmt;
use std::ptr::null_mut;

use glam::Vec2;
use log::{debug, warn};
use map_data::map_defs::Sector;
use map_data::{LineDefFlags, MapData, MapPtr};
use math::{Angle, get_prndindex, get_rndindex, set_prndindex, set_rndindex};
use wad::types::WadThing;

use crate::Skill;
use crate::doom_def::{MAXPLAYERS, WeaponType};
use crate::env::ceiling::CeilingMove;
use crate::env::doors::VerticalDoor;
use crate::env::floor::FloorMove;
use crate::env::lights::{FireFlicker, Glow, LightFlash, StrobeFlash};
use crate::env::platforms::Platform;
use crate::info::{MOBJINFO, MapObjKind, STATES, SpriteNum, StateNum};
use crate::level::Level;
use crate::pic::Button;
use crate::player::{Player, PlayerState};
use crate::thing::MapObject;
use crate::thinker::{Think, Thinker, ThinkerData};

// ── Constants ──────────────────────────────────────────────────────────

const SAVE_MAGIC: &[u8; 4] = b"R4DS";
const SAVE_VERSION: u32 = 1;
const HEADER_SIZE: usize = 64;

// Thinker tags
const TAG_MOBJ: u8 = 1;
const TAG_VDOOR: u8 = 2;
const TAG_FLOOR: u8 = 3;
const TAG_CEILING: u8 = 4;
const TAG_PLATFORM: u8 = 5;
const TAG_LIGHT_FLASH: u8 = 6;
const TAG_STROBE: u8 = 7;
const TAG_FIRE_FLICKER: u8 = 8;
const TAG_GLOW: u8 = 9;

// ── Error ──────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum SaveError {
    Io(std::io::Error),
    BadMagic,
    VersionMismatch(u32),
    Truncated,
    InvalidThinkerTag(u8),
    InvalidStateNum(u16),
    InvalidSectorNum(u32),
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SaveError::Io(e) => write!(f, "IO: {e}"),
            SaveError::BadMagic => write!(f, "bad save magic"),
            SaveError::VersionMismatch(v) => write!(f, "version mismatch: {v}"),
            SaveError::Truncated => write!(f, "truncated save data"),
            SaveError::InvalidThinkerTag(t) => write!(f, "invalid thinker tag: {t}"),
            SaveError::InvalidStateNum(s) => write!(f, "invalid state num: {s}"),
            SaveError::InvalidSectorNum(s) => write!(f, "invalid sector num: {s}"),
        }
    }
}

impl std::error::Error for SaveError {}

impl From<std::io::Error> for SaveError {
    fn from(e: std::io::Error) -> Self {
        SaveError::Io(e)
    }
}

// ── State ↔ index ──────────────────────────────────────────────────────

/// Convert a state reference to its index in the global STATES array.
fn state_to_index(state: &'static crate::info::State) -> u16 {
    let base = std::ptr::addr_of!(STATES) as *const crate::info::State as usize;
    let ptr = state as *const _ as usize;
    let idx = (ptr - base) / std::mem::size_of::<crate::info::State>();
    idx as u16
}

/// Convert a state index back to a reference.
fn index_to_state(idx: u16) -> Result<&'static crate::info::State, SaveError> {
    let max = StateNum::Count as u16;
    if idx >= max {
        return Err(SaveError::InvalidStateNum(idx));
    }
    Ok(unsafe { &STATES[idx as usize] })
}

// ── SaveWriter ─────────────────────────────────────────────────────────

pub struct SaveWriter {
    buf: Vec<u8>,
}

impl SaveWriter {
    pub fn new() -> Self {
        Self {
            buf: Vec::with_capacity(64 * 1024),
        }
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }

    fn write_u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    fn write_bool(&mut self, v: bool) {
        self.buf.push(v as u8);
    }

    fn write_u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn write_i16(&mut self, v: i16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn write_u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn write_i32(&mut self, v: i32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn write_f32(&mut self, v: f32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn write_bytes(&mut self, v: &[u8]) {
        self.buf.extend_from_slice(v);
    }
}

// ── SaveReader ─────────────────────────────────────────────────────────

pub struct SaveReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> SaveReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
        }
    }

    fn read_u8(&mut self) -> Result<u8, SaveError> {
        if self.pos >= self.data.len() {
            return Err(SaveError::Truncated);
        }
        let v = self.data[self.pos];
        self.pos += 1;
        Ok(v)
    }

    fn read_bool(&mut self) -> Result<bool, SaveError> {
        Ok(self.read_u8()? != 0)
    }

    fn read_u16(&mut self) -> Result<u16, SaveError> {
        if self.pos + 2 > self.data.len() {
            return Err(SaveError::Truncated);
        }
        let v = u16::from_le_bytes([self.data[self.pos], self.data[self.pos + 1]]);
        self.pos += 2;
        Ok(v)
    }

    fn read_i16(&mut self) -> Result<i16, SaveError> {
        Ok(self.read_u16()? as i16)
    }

    fn read_u32(&mut self) -> Result<u32, SaveError> {
        if self.pos + 4 > self.data.len() {
            return Err(SaveError::Truncated);
        }
        let v = u32::from_le_bytes([
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ]);
        self.pos += 4;
        Ok(v)
    }

    fn read_i32(&mut self) -> Result<i32, SaveError> {
        Ok(self.read_u32()? as i32)
    }

    fn read_f32(&mut self) -> Result<f32, SaveError> {
        Ok(f32::from_le_bytes(self.read_u32()?.to_le_bytes()))
    }

    fn read_bytes(&mut self, len: usize) -> Result<&'a [u8], SaveError> {
        if self.pos + len > self.data.len() {
            return Err(SaveError::Truncated);
        }
        let s = &self.data[self.pos..self.pos + len];
        self.pos += len;
        Ok(s)
    }

    fn skip(&mut self, n: usize) -> Result<(), SaveError> {
        if self.pos + n > self.data.len() {
            return Err(SaveError::Truncated);
        }
        self.pos += n;
        Ok(())
    }
}

// ── Header ─────────────────────────────────────────────────────────────

struct SaveHeader {
    map_name: [u8; 8],
    skill: Skill,
    episode: usize,
    map: usize,
    level_time: u32,
    game_tic: u32,
    prndindex: u8,
    rndindex: u8,
}

fn write_header(w: &mut SaveWriter, h: &SaveHeader) {
    w.write_bytes(SAVE_MAGIC);
    w.write_u32(SAVE_VERSION);
    w.write_bytes(&h.map_name);
    w.write_u8(h.skill as u8);
    w.write_u8(h.episode as u8);
    w.write_u8(h.map as u8);
    w.write_u8(0); // pad
    w.write_u32(h.level_time);
    w.write_u32(h.game_tic);
    w.write_u8(h.prndindex);
    w.write_u8(h.rndindex);
    // reserved (34 bytes to fill to 64)
    let used = 4 + 4 + 8 + 1 + 1 + 1 + 1 + 4 + 4 + 1 + 1; // = 30
    let pad = HEADER_SIZE - used;
    for _ in 0..pad {
        w.write_u8(0);
    }
}

fn read_header(r: &mut SaveReader) -> Result<SaveHeader, SaveError> {
    let magic = r.read_bytes(4)?;
    if magic != SAVE_MAGIC {
        return Err(SaveError::BadMagic);
    }
    let version = r.read_u32()?;
    if version != SAVE_VERSION {
        return Err(SaveError::VersionMismatch(version));
    }
    let map_name_bytes = r.read_bytes(8)?;
    let mut map_name = [0u8; 8];
    map_name.copy_from_slice(map_name_bytes);
    let skill = Skill::from(r.read_u8()? as i32);
    let episode = r.read_u8()? as usize;
    let map = r.read_u8()? as usize;
    r.skip(1)?; // pad
    let level_time = r.read_u32()?;
    let game_tic = r.read_u32()?;
    let prndindex = r.read_u8()?;
    let rndindex = r.read_u8()?;
    let used = 4 + 4 + 8 + 1 + 1 + 1 + 1 + 4 + 4 + 1 + 1;
    r.skip(HEADER_SIZE - used)?;
    Ok(SaveHeader {
        map_name,
        skill,
        episode,
        map,
        level_time,
        game_tic,
        prndindex,
        rndindex,
    })
}

// ── World (sectors, linedefs, sidedefs) ────────────────────────────────

fn save_world(w: &mut SaveWriter, map_data: &MapData) {
    let sectors = map_data.sectors();
    w.write_u32(sectors.len() as u32);
    for s in sectors {
        w.write_f32(s.floorheight);
        w.write_f32(s.ceilingheight);
        w.write_u32(s.floorpic as u32);
        w.write_u32(s.ceilingpic as u32);
        w.write_u32(s.lightlevel as u32);
        w.write_i16(s.special);
        w.write_i16(s.tag);
    }

    let linedefs = map_data.linedefs();
    w.write_u32(linedefs.len() as u32);
    for l in linedefs {
        w.write_u32(l.flags.bits());
        w.write_i16(l.special);
        w.write_i16(l.tag);
    }

    let sidedefs = map_data.sidedefs();
    w.write_u32(sidedefs.len() as u32);
    for sd in sidedefs {
        w.write_f32(sd.textureoffset);
        w.write_f32(sd.rowoffset);
        w.write_i32(sd.toptexture.map_or(-1, |v| v as i32));
        w.write_i32(sd.bottomtexture.map_or(-1, |v| v as i32));
        w.write_i32(sd.midtexture.map_or(-1, |v| v as i32));
    }
}

fn load_world(r: &mut SaveReader, map_data: &mut MapData) -> Result<(), SaveError> {
    let n_sectors = r.read_u32()? as usize;
    let sectors = map_data.sectors_mut();
    if n_sectors != sectors.len() {
        warn!(
            "Sector count mismatch: save={}, map={}",
            n_sectors,
            sectors.len()
        );
    }
    let count = n_sectors.min(sectors.len());
    for i in 0..count {
        sectors[i].floorheight = r.read_f32()?;
        sectors[i].ceilingheight = r.read_f32()?;
        sectors[i].floorpic = r.read_u32()? as usize;
        sectors[i].ceilingpic = r.read_u32()? as usize;
        sectors[i].lightlevel = r.read_u32()? as usize;
        sectors[i].special = r.read_i16()?;
        sectors[i].tag = r.read_i16()?;
    }
    // skip extra sectors in save if map has fewer
    for _ in count..n_sectors {
        r.skip(4 + 4 + 4 + 4 + 4 + 2 + 2)?;
    }

    let n_linedefs = r.read_u32()? as usize;
    let linedefs = map_data.linedefs_mut();
    let count = n_linedefs.min(linedefs.len());
    for i in 0..count {
        linedefs[i].flags = LineDefFlags::from_bits_truncate(r.read_u32()?);
        linedefs[i].special = r.read_i16()?;
        linedefs[i].tag = r.read_i16()?;
    }
    for _ in count..n_linedefs {
        r.skip(4 + 2 + 2)?;
    }

    let n_sidedefs = r.read_u32()? as usize;
    let sidedefs = map_data.sidedefs_mut();
    let count = n_sidedefs.min(sidedefs.len());
    for i in 0..count {
        sidedefs[i].textureoffset = r.read_f32()?;
        sidedefs[i].rowoffset = r.read_f32()?;
        let top = r.read_i32()?;
        sidedefs[i].toptexture = if top < 0 { None } else { Some(top as usize) };
        let bot = r.read_i32()?;
        sidedefs[i].bottomtexture = if bot < 0 { None } else { Some(bot as usize) };
        let mid = r.read_i32()?;
        sidedefs[i].midtexture = if mid < 0 { None } else { Some(mid as usize) };
    }
    for _ in count..n_sidedefs {
        r.skip(4 + 4 + 4 + 4 + 4)?;
    }

    Ok(())
}

// ── Thinker save ───────────────────────────────────────────────────────

/// Get the sector index from a `MapPtr<Sector>` by matching `.num`.
fn sector_index(s: &MapPtr<Sector>) -> u32 {
    s.num as u32
}

fn save_mobj_with_player_index(w: &mut SaveWriter, m: &MapObject, player_idx: i8) {
    w.write_f32(m.xy.x);
    w.write_f32(m.xy.y);
    w.write_f32(m.z);
    w.write_f32(m.angle.rad());
    w.write_u16(m.sprite as u16);
    w.write_u32(m.frame);
    w.write_f32(m.floorz);
    w.write_f32(m.ceilingz);
    w.write_f32(m.radius);
    w.write_f32(m.height);
    w.write_f32(m.momxy.x);
    w.write_f32(m.momxy.y);
    w.write_f32(m.momz);
    w.write_u16(m.kind as u16);
    w.write_i32(m.tics);
    w.write_u16(state_to_index(m.state));
    w.write_u32(m.flags.bits());
    w.write_i32(m.health);
    w.write_u8(m.movedir as u8);
    w.write_i32(m.movecount);
    w.write_i32(m.reactiontime);
    w.write_i32(m.threshold);
    w.write_u8(m.lastlook as u8);
    w.write_i16(m.spawnpoint.x);
    w.write_i16(m.spawnpoint.y);
    w.write_i16(m.spawnpoint.angle);
    w.write_i16(m.spawnpoint.kind);
    w.write_i16(m.spawnpoint.flags);
    w.write_u8(player_idx as u8);
}

// ── Thinker load ───────────────────────────────────────────────────────

/// Resolve a sector index to a `MapPtr<Sector>`.
fn resolve_sector(sector_num: u32, map_data: &mut MapData) -> Result<MapPtr<Sector>, SaveError> {
    let sectors = map_data.sectors_mut();
    let idx = sector_num as usize;
    if idx >= sectors.len() {
        return Err(SaveError::InvalidSectorNum(sector_num));
    }
    Ok(MapPtr::new(&mut sectors[idx]))
}

fn load_thinkers(
    r: &mut SaveReader,
    level: &mut Level,
    players: &mut [Player],
    players_in_game: &[bool; MAXPLAYERS],
) -> Result<(), SaveError> {
    let count = r.read_u32()?;
    debug!("Loading {} thinkers", count);

    for _ in 0..count {
        let tag = r.read_u8()?;
        match tag {
            TAG_MOBJ => {
                load_mobj(r, level, players, players_in_game)?;
            }
            TAG_VDOOR => {
                load_vdoor(r, level)?;
            }
            TAG_FLOOR => {
                load_floor(r, level)?;
            }
            TAG_CEILING => {
                load_ceiling(r, level)?;
            }
            TAG_PLATFORM => {
                load_platform(r, level)?;
            }
            TAG_LIGHT_FLASH => {
                load_light_flash(r, level)?;
            }
            TAG_STROBE => {
                load_strobe(r, level)?;
            }
            TAG_FIRE_FLICKER => {
                load_fire_flicker(r, level)?;
            }
            TAG_GLOW => {
                load_glow(r, level)?;
            }
            other => return Err(SaveError::InvalidThinkerTag(other)),
        }
    }
    Ok(())
}

fn load_mobj(
    r: &mut SaveReader,
    level: &mut Level,
    players: &mut [Player],
    players_in_game: &[bool; MAXPLAYERS],
) -> Result<(), SaveError> {
    let x = r.read_f32()?;
    let y = r.read_f32()?;
    let z = r.read_f32()?;
    let angle_rad = r.read_f32()?;
    let sprite = r.read_u16()?;
    let frame = r.read_u32()?;
    let floorz = r.read_f32()?;
    let ceilingz = r.read_f32()?;
    let radius = r.read_f32()?;
    let height = r.read_f32()?;
    let momx = r.read_f32()?;
    let momy = r.read_f32()?;
    let momz = r.read_f32()?;
    let kind_raw = r.read_u16()?;
    let tics = r.read_i32()?;
    let state_idx = r.read_u16()?;
    let flags_raw = r.read_u32()?;
    let health = r.read_i32()?;
    let movedir = r.read_u8()?;
    let movecount = r.read_i32()?;
    let reactiontime = r.read_i32()?;
    let threshold = r.read_i32()?;
    let lastlook = r.read_u8()?;
    let sp_x = r.read_i16()?;
    let sp_y = r.read_i16()?;
    let sp_angle = r.read_i16()?;
    let sp_kind = r.read_i16()?;
    let sp_flags = r.read_i16()?;
    let player_idx = r.read_u8()? as i8;

    let kind = MapObjKind::from(kind_raw);
    let info = MOBJINFO[kind as usize];
    let state = index_to_state(state_idx)?;

    let level_ptr = level as *mut Level;

    let mobj = MapObject::from_save_data(
        Vec2::new(x, y),
        z,
        Angle::new(angle_rad),
        unsafe { std::mem::transmute::<u16, SpriteNum>(sprite) },
        frame,
        floorz,
        ceilingz,
        radius,
        height,
        Vec2::new(momx, momy),
        momz,
        kind,
        info,
        tics,
        state,
        crate::thing::MapObjFlag::from_bits_truncate(flags_raw),
        health,
        crate::thing::MoveDir::from(movedir as usize),
        movecount,
        reactiontime,
        threshold,
        lastlook as usize,
        WadThing {
            x: sp_x,
            y: sp_y,
            angle: sp_angle,
            kind: sp_kind,
            flags: sp_flags,
        },
        level_ptr,
    );

    let thinker = MapObject::create_thinker(ThinkerData::MapObject(mobj), MapObject::think);

    if let Some(t) = level.thinkers.push_raw(thinker) {
        let mobj = t.mobj_mut();
        unsafe {
            mobj.set_thing_position();
        }

        // Link player ↔ mobj
        if player_idx >= 0 && (player_idx as usize) < MAXPLAYERS {
            let pi = player_idx as usize;
            if players_in_game[pi] {
                mobj.player = Some(&mut players[pi] as *mut Player);
                players[pi].set_mobj(mobj as *mut MapObject);
            }
        }
    }

    Ok(())
}

fn load_vdoor(r: &mut SaveReader, level: &mut Level) -> Result<(), SaveError> {
    let sector_num = r.read_u32()?;
    let kind = r.read_u8()?;
    let topheight = r.read_f32()?;
    let speed = r.read_f32()?;
    let direction = r.read_i32()?;
    let topwait = r.read_i32()?;
    let topcountdown = r.read_i32()?;

    let sector = resolve_sector(sector_num, &mut level.map_data)?;

    let door = VerticalDoor {
        thinker: null_mut(),
        sector: sector.clone(),
        kind: unsafe { std::mem::transmute::<u8, _>(kind) },
        topheight,
        speed,
        direction,
        topwait,
        topcountdown,
    };

    let thinker =
        VerticalDoor::create_thinker(ThinkerData::VerticalDoor(door), VerticalDoor::think);
    if let Some(t) = level.thinkers.push_raw(thinker) {
        let ptr = t as *mut Thinker as *mut ();
        let sec = resolve_sector(sector_num, &mut level.map_data)?;
        sec.as_ptr().cast::<Sector>();
        unsafe { (*sec.as_ptr()).specialdata = Some(ptr) };
    }

    Ok(())
}

fn load_floor(r: &mut SaveReader, level: &mut Level) -> Result<(), SaveError> {
    let sector_num = r.read_u32()?;
    let kind = r.read_u8()?;
    let speed = r.read_f32()?;
    let crush = r.read_bool()?;
    let direction = r.read_i32()?;
    let newspecial = r.read_i16()?;
    let texture = r.read_u32()? as usize;
    let destheight = r.read_f32()?;

    let sector = resolve_sector(sector_num, &mut level.map_data)?;

    let floor = FloorMove {
        thinker: null_mut(),
        sector: sector.clone(),
        kind: unsafe { std::mem::transmute::<u8, _>(kind) },
        speed,
        crush,
        direction,
        newspecial,
        texture,
        destheight,
    };

    let thinker = FloorMove::create_thinker(ThinkerData::FloorMove(floor), FloorMove::think);
    if let Some(t) = level.thinkers.push_raw(thinker) {
        let ptr = t as *mut Thinker as *mut ();
        unsafe {
            (*resolve_sector(sector_num, &mut level.map_data)?.as_ptr()).specialdata = Some(ptr)
        };
    }

    Ok(())
}

fn load_ceiling(r: &mut SaveReader, level: &mut Level) -> Result<(), SaveError> {
    let sector_num = r.read_u32()?;
    let kind = r.read_u8()?;
    let bottomheight = r.read_f32()?;
    let topheight = r.read_f32()?;
    let speed = r.read_f32()?;
    let crush = r.read_bool()?;
    let direction = r.read_i32()?;
    let tag = r.read_i16()?;
    let olddirection = r.read_i32()?;

    let sector = resolve_sector(sector_num, &mut level.map_data)?;

    let ceil = CeilingMove {
        thinker: null_mut(),
        sector: sector.clone(),
        kind: unsafe { std::mem::transmute::<u8, _>(kind) },
        bottomheight,
        topheight,
        speed,
        crush,
        direction,
        tag,
        olddirection,
    };

    let thinker = CeilingMove::create_thinker(ThinkerData::CeilingMove(ceil), CeilingMove::think);
    if let Some(t) = level.thinkers.push_raw(thinker) {
        let ptr = t as *mut Thinker as *mut ();
        unsafe {
            (*resolve_sector(sector_num, &mut level.map_data)?.as_ptr()).specialdata = Some(ptr)
        };
    }

    Ok(())
}

fn load_platform(r: &mut SaveReader, level: &mut Level) -> Result<(), SaveError> {
    use crate::env::platforms::{PlatKind, PlatStatus};

    let sector_num = r.read_u32()?;
    let speed = r.read_f32()?;
    let low = r.read_f32()?;
    let high = r.read_f32()?;
    let wait = r.read_i32()?;
    let count = r.read_i32()?;
    let status = r.read_u8()?;
    let old_status = r.read_u8()?;
    let crush = r.read_bool()?;
    let tag = r.read_i16()?;
    let kind = r.read_u8()?;

    let sector = resolve_sector(sector_num, &mut level.map_data)?;

    let plat = Platform {
        thinker: null_mut(),
        sector: sector.clone(),
        speed,
        low,
        high,
        wait,
        count,
        status: unsafe { std::mem::transmute::<u8, PlatStatus>(status) },
        old_status: unsafe { std::mem::transmute::<u8, PlatStatus>(old_status) },
        crush,
        tag,
        kind: unsafe { std::mem::transmute::<u8, PlatKind>(kind) },
    };

    let thinker = Platform::create_thinker(ThinkerData::Platform(plat), Platform::think);
    if let Some(t) = level.thinkers.push_raw(thinker) {
        let ptr = t as *mut Thinker as *mut ();
        unsafe {
            (*resolve_sector(sector_num, &mut level.map_data)?.as_ptr()).specialdata = Some(ptr)
        };
        // Add to active platforms
        let plat_ptr = t.platform_mut() as *mut Platform;
        level.add_active_platform(plat_ptr);
    }

    Ok(())
}

fn load_light_flash(r: &mut SaveReader, level: &mut Level) -> Result<(), SaveError> {
    let sector_num = r.read_u32()?;
    let count = r.read_i32()?;
    let max_light = r.read_u32()? as usize;
    let min_light = r.read_u32()? as usize;
    let max_time = r.read_i32()?;
    let min_time = r.read_i32()?;

    let sector = resolve_sector(sector_num, &mut level.map_data)?;

    let lf = LightFlash {
        thinker: null_mut(),
        sector,
        count,
        max_light,
        min_light,
        max_time,
        min_time,
    };

    let thinker = LightFlash::create_thinker(ThinkerData::LightFlash(lf), LightFlash::think);
    level.thinkers.push_raw(thinker);

    Ok(())
}

fn load_strobe(r: &mut SaveReader, level: &mut Level) -> Result<(), SaveError> {
    let sector_num = r.read_u32()?;
    let count = r.read_i32()?;
    let min_light = r.read_u32()? as usize;
    let max_light = r.read_u32()? as usize;
    let dark_time = r.read_i32()?;
    let bright_time = r.read_i32()?;

    let sector = resolve_sector(sector_num, &mut level.map_data)?;

    let sf = StrobeFlash {
        thinker: null_mut(),
        sector,
        count,
        min_light,
        max_light,
        dark_time,
        bright_time,
    };

    let thinker = StrobeFlash::create_thinker(ThinkerData::StrobeFlash(sf), StrobeFlash::think);
    level.thinkers.push_raw(thinker);

    Ok(())
}

fn load_fire_flicker(r: &mut SaveReader, level: &mut Level) -> Result<(), SaveError> {
    let sector_num = r.read_u32()?;
    let count = r.read_i32()?;
    let max_light = r.read_u32()? as usize;
    let min_light = r.read_u32()? as usize;

    let sector = resolve_sector(sector_num, &mut level.map_data)?;

    let ff = FireFlicker {
        thinker: null_mut(),
        sector,
        count,
        max_light,
        min_light,
    };

    let thinker = FireFlicker::create_thinker(ThinkerData::FireFlicker(ff), FireFlicker::think);
    level.thinkers.push_raw(thinker);

    Ok(())
}

fn load_glow(r: &mut SaveReader, level: &mut Level) -> Result<(), SaveError> {
    let sector_num = r.read_u32()?;
    let min_light = r.read_u32()? as usize;
    let max_light = r.read_u32()? as usize;
    let direction = r.read_i32()?;

    let sector = resolve_sector(sector_num, &mut level.map_data)?;

    let g = Glow {
        thinker: null_mut(),
        sector,
        min_light,
        max_light,
        direction,
    };

    let thinker = Glow::create_thinker(ThinkerData::Glow(g), Glow::think);
    level.thinkers.push_raw(thinker);

    Ok(())
}

// ── Players ────────────────────────────────────────────────────────────

fn save_players(w: &mut SaveWriter, players: &[Player], players_in_game: &[bool; MAXPLAYERS]) {
    for i in 0..MAXPLAYERS {
        w.write_bool(players_in_game[i]);
        if !players_in_game[i] {
            continue;
        }
        let p = &players[i];
        // PlayerState
        w.write_u8(match p.player_state {
            PlayerState::Live => 0,
            PlayerState::Dead => 1,
            PlayerState::Reborn => 2,
        });
        w.write_f32(p.viewz);
        w.write_f32(p.viewheight);
        w.write_f32(p.deltaviewheight);
        w.write_f32(p.bob);
        w.write_bool(p.onground);

        // PlayerStatus
        let s = &p.status;
        w.write_bool(s.attackdown);
        w.write_bool(s.usedown);
        w.write_u8(s.readyweapon as u8);
        w.write_i32(s.health);
        w.write_i32(s.armorpoints);
        w.write_i32(s.armortype);
        for c in &s.cards {
            w.write_bool(*c);
        }
        for wo in &s.weaponowned {
            w.write_bool(*wo);
        }
        for a in &s.ammo {
            w.write_u32(*a);
        }
        for ma in &s.maxammo {
            w.write_u32(*ma);
        }
        w.write_bool(s.backpack);
        for pw in &s.powers {
            w.write_i32(*pw);
        }
        w.write_i32(s.damagecount);
        w.write_i32(s.bonuscount);
        w.write_u32(s.cheats.bits());

        // Frags
        for f in &p.frags {
            w.write_i32(*f);
        }

        // Weapon state
        w.write_u8(p.pendingweapon as u8);
        w.write_i32(p.refire);
        w.write_i32(p.total_kills);
        w.write_i32(p.items_collected);
        w.write_i32(p.secrets_found);
        w.write_u32(p.extralight as u32);
        w.write_i32(p.fixedcolormap);
        w.write_bool(p.didsecret);
        w.write_bool(p.head_bob);
        w.write_i16(p.lookdir);

        // PspDef
        for psp in &p.psprites {
            match psp.state {
                Some(st) => w.write_u16(state_to_index(st)),
                None => w.write_u16(0xFFFF),
            }
            w.write_i32(psp.tics);
            w.write_f32(psp.sx);
            w.write_f32(psp.sy);
        }
    }
}

fn load_players(
    r: &mut SaveReader,
    players: &mut [Player],
    players_in_game: &mut [bool; MAXPLAYERS],
) -> Result<(), SaveError> {
    for i in 0..MAXPLAYERS {
        let in_game = r.read_bool()?;
        players_in_game[i] = in_game;
        if !in_game {
            continue;
        }
        let p = &mut players[i];
        let ps = r.read_u8()?;
        p.player_state = match ps {
            0 => PlayerState::Live,
            1 => PlayerState::Dead,
            _ => PlayerState::Reborn,
        };
        p.viewz = r.read_f32()?;
        p.viewheight = r.read_f32()?;
        p.deltaviewheight = r.read_f32()?;
        p.bob = r.read_f32()?;
        p.onground = r.read_bool()?;

        let s = &mut p.status;
        s.attackdown = r.read_bool()?;
        s.usedown = r.read_bool()?;
        s.readyweapon = unsafe { std::mem::transmute::<u8, WeaponType>(r.read_u8()?) };
        s.health = r.read_i32()?;
        s.armorpoints = r.read_i32()?;
        s.armortype = r.read_i32()?;
        for c in s.cards.iter_mut() {
            *c = r.read_bool()?;
        }
        for wo in s.weaponowned.iter_mut() {
            *wo = r.read_bool()?;
        }
        for a in s.ammo.iter_mut() {
            *a = r.read_u32()?;
        }
        for ma in s.maxammo.iter_mut() {
            *ma = r.read_u32()?;
        }
        s.backpack = r.read_bool()?;
        for pw in s.powers.iter_mut() {
            *pw = r.read_i32()?;
        }
        s.damagecount = r.read_i32()?;
        s.bonuscount = r.read_i32()?;
        s.cheats = crate::player::PlayerCheat::from_bits_truncate(r.read_u32()?);

        for f in p.frags.iter_mut() {
            *f = r.read_i32()?;
        }

        p.pendingweapon = unsafe { std::mem::transmute::<u8, WeaponType>(r.read_u8()?) };
        p.refire = r.read_i32()?;
        p.total_kills = r.read_i32()?;
        p.items_collected = r.read_i32()?;
        p.secrets_found = r.read_i32()?;
        p.extralight = r.read_u32()? as usize;
        p.fixedcolormap = r.read_i32()?;
        p.didsecret = r.read_bool()?;
        p.head_bob = r.read_bool()?;
        p.lookdir = r.read_i16()?;

        for psp in p.psprites.iter_mut() {
            let si = r.read_u16()?;
            psp.state = if si == 0xFFFF {
                None
            } else {
                Some(index_to_state(si)?)
            };
            psp.tics = r.read_i32()?;
            psp.sx = r.read_f32()?;
            psp.sy = r.read_f32()?;
        }
    }
    Ok(())
}

// ── Buttons ────────────────────────────────────────────────────────────

fn save_buttons(w: &mut SaveWriter, level: &Level) {
    w.write_u32(level.button_list.len() as u32);
    for b in &level.button_list {
        w.write_u32(b.line.num as u32);
        w.write_u8(match b.bwhere {
            crate::pic::ButtonWhere::Top => 0,
            crate::pic::ButtonWhere::Middle => 1,
            crate::pic::ButtonWhere::Bottom => 2,
        });
        w.write_u32(b.texture as u32);
        w.write_u32(b.timer);
    }
}

fn load_buttons(r: &mut SaveReader, level: &mut Level) -> Result<(), SaveError> {
    let count = r.read_u32()? as usize;
    level.button_list.clear();
    for _ in 0..count {
        let line_num = r.read_u32()? as usize;
        let bwhere = r.read_u8()?;
        let texture = r.read_u32()? as usize;
        let timer = r.read_u32()?;

        let linedefs = level.map_data.linedefs_mut();
        if line_num < linedefs.len() {
            let line = MapPtr::new(&mut linedefs[line_num]);
            level.button_list.push(Button {
                line,
                bwhere: match bwhere {
                    0 => crate::pic::ButtonWhere::Top,
                    1 => crate::pic::ButtonWhere::Middle,
                    _ => crate::pic::ButtonWhere::Bottom,
                },
                texture,
                timer,
            });
        }
    }
    Ok(())
}

// ── Respawn queue ──────────────────────────────────────────────────────

fn save_respawn_queue(w: &mut SaveWriter, level: &Level) {
    w.write_u32(level.respawn_queue.len() as u32);
    for (time, thing) in &level.respawn_queue {
        w.write_u32(*time);
        w.write_i16(thing.x);
        w.write_i16(thing.y);
        w.write_i16(thing.angle);
        w.write_i16(thing.kind);
        w.write_i16(thing.flags);
    }
}

fn load_respawn_queue(r: &mut SaveReader, level: &mut Level) -> Result<(), SaveError> {
    let count = r.read_u32()? as usize;
    level.respawn_queue.clear();
    for _ in 0..count {
        let time = r.read_u32()?;
        let x = r.read_i16()?;
        let y = r.read_i16()?;
        let angle = r.read_i16()?;
        let kind = r.read_i16()?;
        let flags = r.read_i16()?;
        level.respawn_queue.push_back((
            time,
            WadThing {
                x,
                y,
                angle,
                kind,
                flags,
            },
        ));
    }
    Ok(())
}

// ── Level stats ────────────────────────────────────────────────────────

fn save_level_stats(w: &mut SaveWriter, level: &Level) {
    w.write_i32(level.total_level_kills);
    w.write_i32(level.total_level_items);
    w.write_i32(level.total_level_secrets);
}

fn load_level_stats(r: &mut SaveReader, level: &mut Level) -> Result<(), SaveError> {
    level.total_level_kills = r.read_i32()?;
    level.total_level_items = r.read_i32()?;
    level.total_level_secrets = r.read_i32()?;
    Ok(())
}

// ── Public API ─────────────────────────────────────────────────────────

/// Serialize the current game state to bytes.
pub fn save_game_to_bytes(
    level: &Level,
    players: &[Player],
    players_in_game: &[bool; MAXPLAYERS],
    game_tic: u32,
) -> Vec<u8> {
    let mut w = SaveWriter::new();

    // Build map name padded to 8 bytes
    let mut map_name = [0u8; 8];
    let name_bytes = level.map_name.as_bytes();
    let len = name_bytes.len().min(8);
    map_name[..len].copy_from_slice(&name_bytes[..len]);

    write_header(
        &mut w,
        &SaveHeader {
            map_name,
            skill: level.options.skill,
            episode: level.options.episode,
            map: level.options.map,
            level_time: level.level_time,
            game_tic,
            prndindex: get_prndindex() as u8,
            rndindex: get_rndindex() as u8,
        },
    );

    save_world(&mut w, &level.map_data);

    // Save thinkers — we need to determine player_index for each mobj
    // First count thinkers
    let mut thinker_count = 0u32;
    level.thinkers.for_each(|t| {
        if !matches!(t.data(), ThinkerData::Free | ThinkerData::Remove) {
            thinker_count += 1;
        }
    });
    w.write_u32(thinker_count);

    level.thinkers.for_each(|t| match t.data() {
        ThinkerData::MapObject(mobj) => {
            w.write_u8(TAG_MOBJ);
            // Determine player index
            let player_idx: i8 = if let Some(player_ptr) = mobj.player() {
                // Find which player this is by comparing pointers
                let mut idx = -1i8;
                for i in 0..MAXPLAYERS {
                    if std::ptr::eq(player_ptr, &players[i]) {
                        idx = i as i8;
                        break;
                    }
                }
                idx
            } else {
                -1
            };
            save_mobj_with_player_index(&mut w, mobj, player_idx);
        }
        ThinkerData::VerticalDoor(d) => {
            w.write_u8(TAG_VDOOR);
            w.write_u32(sector_index(&d.sector));
            w.write_u8(d.kind as u8);
            w.write_f32(d.topheight);
            w.write_f32(d.speed);
            w.write_i32(d.direction);
            w.write_i32(d.topwait);
            w.write_i32(d.topcountdown);
        }
        ThinkerData::FloorMove(f) => {
            w.write_u8(TAG_FLOOR);
            w.write_u32(sector_index(&f.sector));
            w.write_u8(f.kind as u8);
            w.write_f32(f.speed);
            w.write_bool(f.crush);
            w.write_i32(f.direction);
            w.write_i16(f.newspecial);
            w.write_u32(f.texture as u32);
            w.write_f32(f.destheight);
        }
        ThinkerData::CeilingMove(c) => {
            w.write_u8(TAG_CEILING);
            w.write_u32(sector_index(&c.sector));
            w.write_u8(c.kind as u8);
            w.write_f32(c.bottomheight);
            w.write_f32(c.topheight);
            w.write_f32(c.speed);
            w.write_bool(c.crush);
            w.write_i32(c.direction);
            w.write_i16(c.tag);
            w.write_i32(c.olddirection);
        }
        ThinkerData::Platform(p) => {
            w.write_u8(TAG_PLATFORM);
            w.write_u32(sector_index(&p.sector));
            w.write_f32(p.speed);
            w.write_f32(p.low);
            w.write_f32(p.high);
            w.write_i32(p.wait);
            w.write_i32(p.count);
            w.write_u8(p.status as u8);
            w.write_u8(p.old_status as u8);
            w.write_bool(p.crush);
            w.write_i16(p.tag);
            w.write_u8(p.kind as u8);
        }
        ThinkerData::LightFlash(l) => {
            w.write_u8(TAG_LIGHT_FLASH);
            w.write_u32(sector_index(&l.sector));
            w.write_i32(l.count);
            w.write_u32(l.max_light as u32);
            w.write_u32(l.min_light as u32);
            w.write_i32(l.max_time);
            w.write_i32(l.min_time);
        }
        ThinkerData::StrobeFlash(s) => {
            w.write_u8(TAG_STROBE);
            w.write_u32(sector_index(&s.sector));
            w.write_i32(s.count);
            w.write_u32(s.min_light as u32);
            w.write_u32(s.max_light as u32);
            w.write_i32(s.dark_time);
            w.write_i32(s.bright_time);
        }
        ThinkerData::FireFlicker(ff) => {
            w.write_u8(TAG_FIRE_FLICKER);
            w.write_u32(sector_index(&ff.sector));
            w.write_i32(ff.count);
            w.write_u32(ff.max_light as u32);
            w.write_u32(ff.min_light as u32);
        }
        ThinkerData::Glow(g) => {
            w.write_u8(TAG_GLOW);
            w.write_u32(sector_index(&g.sector));
            w.write_u32(g.min_light as u32);
            w.write_u32(g.max_light as u32);
            w.write_i32(g.direction);
        }
        _ => {}
    });

    save_players(&mut w, players, players_in_game);
    save_buttons(&mut w, level);
    save_respawn_queue(&mut w, level);
    save_level_stats(&mut w, level);

    w.into_bytes()
}

/// Parsed save header — returned to the caller for level init.
pub struct SaveGameHeader {
    pub map_name: String,
    pub skill: Skill,
    pub episode: usize,
    pub map: usize,
    pub level_time: u32,
    pub game_tic: u32,
    pub prndindex: u8,
    pub rndindex: u8,
}

/// Parse header only (used to determine which level to load).
pub fn parse_save_header(data: &[u8]) -> Result<SaveGameHeader, SaveError> {
    let mut r = SaveReader::new(data);
    let h = read_header(&mut r)?;
    let name_end = h.map_name.iter().position(|&b| b == 0).unwrap_or(8);
    let map_name = String::from_utf8_lossy(&h.map_name[..name_end]).to_string();
    Ok(SaveGameHeader {
        map_name,
        skill: h.skill,
        episode: h.episode,
        map: h.map,
        level_time: h.level_time,
        game_tic: h.game_tic,
        prndindex: h.prndindex,
        rndindex: h.rndindex,
    })
}

/// Apply saved state to an already-loaded level. The level must have been
/// loaded with the correct map, and then had its thinkers cleared.
pub fn load_game_from_bytes(
    data: &[u8],
    level: &mut Level,
    players: &mut [Player],
    players_in_game: &mut [bool; MAXPLAYERS],
) -> Result<SaveGameHeader, SaveError> {
    let mut r = SaveReader::new(data);
    let h = read_header(&mut r)?;

    let name_end = h.map_name.iter().position(|&b| b == 0).unwrap_or(8);
    let map_name = String::from_utf8_lossy(&h.map_name[..name_end]).to_string();

    // Restore world geometry state
    load_world(&mut r, &mut level.map_data)?;

    // Restore thinkers
    load_thinkers(&mut r, level, players, players_in_game)?;

    // Restore players
    load_players(&mut r, players, players_in_game)?;

    // Restore buttons
    load_buttons(&mut r, level)?;

    // Restore respawn queue
    load_respawn_queue(&mut r, level)?;

    // Restore level stats
    load_level_stats(&mut r, level)?;

    // Restore level time
    level.level_time = h.level_time;

    // Restore RNG
    set_prndindex(h.prndindex as usize);
    set_rndindex(h.rndindex as usize);

    let header = SaveGameHeader {
        map_name,
        skill: h.skill,
        episode: h.episode,
        map: h.map,
        level_time: h.level_time,
        game_tic: h.game_tic,
        prndindex: h.prndindex,
        rndindex: h.rndindex,
    };

    Ok(header)
}
