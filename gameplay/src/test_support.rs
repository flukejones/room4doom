//! Shared fixtures for gameplay unit tests: build a real, map-loaded
//! [`LevelState`] and spawn things on it so physics/damage/AI functions can be
//! driven and their results (including RNG-consumption order) pinned.
//!
//! `LevelState` stores raw self-pointers (thinker arena) and raw pointers to
//! the player arrays, so all three must stay pinned and alive together —
//! [`TestLevel`] owns them in `Box`es and must not be moved-from after spawning.

use std::sync::mpsc::{Receiver, channel};
use std::sync::{Mutex, MutexGuard};

use game_config::{GameMode, GameOptions};
use math::m_clear_random;
use sound_common::{SndServerTx, SoundAction};

use crate::MapObjKind;
use crate::doom_def::{MAXPLAYERS, ONFLOORZ};
use crate::level::LevelState;
use crate::player::Player;
use crate::thing::MapObject;
use crate::thinker::ThinkerAlloc;
use math::FixedT;
use test_utils::doom1_wad_path;
use wad::WadData;

/// Owns a pinned [`LevelState`] plus the player arrays it points into. Build
/// with [`TestLevel::load`]; spawn things with [`TestLevel::spawn`].
pub struct TestLevel {
    // Ownership anchors: LevelState holds raw pointers into these, so they must
    // outlive it. Read only through those pointers, never directly.
    #[allow(dead_code, reason = "kept alive for the raw pointers in LevelState")]
    players: Box<[Player; MAXPLAYERS]>,
    #[allow(dead_code, reason = "kept alive for the raw pointers in LevelState")]
    players_in_game: Box<[bool; MAXPLAYERS]>,
    level: Box<LevelState>,
    // Holding the receiver keeps the sound channel open (sends fail-soft
    // without it, but this avoids the warn spam).
    _snd_rx: Receiver<SoundAction>,
}

impl TestLevel {
    /// Load `map` from the in-repo shareware WAD into a usable `LevelState`
    /// (no `PicData`/render setup — only the data physics needs).
    ///
    /// Any test that drives engine code (spawn/think/specials all roll the
    /// global RNG) must hold [`rng_guard`] for its whole body so it does not
    /// race other tests on the shared RNG.
    pub fn load(map_name: &str) -> Self {
        let (tx, rx): (SndServerTx, Receiver<SoundAction>) = channel();
        let mut players_in_game = Box::new([false; MAXPLAYERS]);
        players_in_game[0] = true;
        let mut players = Box::new([
            Player::default(),
            Player::default(),
            Player::default(),
            Player::default(),
        ]);

        // SAFETY: the raw pointers new_empty stores stay valid for as long as
        // the Boxes this struct owns; the LevelState is boxed and never moved.
        let mut level = unsafe {
            Box::new(LevelState::new_empty(
                GameOptions::default(),
                GameMode::Shareware,
                tx,
                &players_in_game,
                &mut players,
            ))
        };

        // Load the real map straight into level_data (the PicData-driven
        // LevelState::load path is bypassed; flats aren't needed for physics).
        let wad = WadData::new(&doom1_wad_path());
        level.level_data.load(map_name, |_| None, &wad, None, None);

        // Replicate LevelState::load's tail: blockmap thing-chains + a thinker
        // arena sized to the thing count.
        let bm = level.level_data.blockmap();
        let num_blocks = (bm.columns * bm.rows) as usize;
        level.blocklinks = vec![None; num_blocks];
        let cap = level.level_data.things().len() * 2 + 256;
        // SAFETY: replaces the empty arena from new_empty before any thinker is
        // pushed; nothing references the old (zero-capacity) arena yet.
        level.thinkers = unsafe { ThinkerAlloc::new(cap) };

        Self {
            players,
            players_in_game,
            level,
            _snd_rx: rx,
        }
    }

    /// Spawn a thing of `kind` at world `(x, y)`, auto-placed on the floor.
    /// Returns a mutable reference valid for the lifetime of this `TestLevel`.
    pub fn spawn(&mut self, x: i32, y: i32, kind: MapObjKind) -> &mut MapObject {
        // SAFETY: see `spawn_ptr`; the returned reference is reborrowed from a
        // stable arena slot and bounded by `&mut self`.
        unsafe { &mut *self.spawn_ptr(x, y, kind) }
    }

    /// Spawn a thing and return a raw pointer. The thinker arena is fixed
    /// capacity (never reallocates), so the pointer stays valid across later
    /// spawns — use this when a test needs two live things at once (e.g. an
    /// attacker and a target).
    pub fn spawn_ptr(&mut self, x: i32, y: i32, kind: MapObjKind) -> *mut MapObject {
        let ptr = MapObject::spawn_map_object(
            FixedT::from(x),
            FixedT::from(y),
            ONFLOORZ.into(),
            kind,
            &mut self.level,
        );
        assert!(!ptr.is_null(), "spawn returned null for {kind:?}");
        ptr
    }

    /// Spawn player 1's `MT_PLAYER` mobj and cross-link it with `players[0]`,
    /// mirroring `p_spawn_player`, so monster sight checks (`look_for_players`)
    /// can find it. Returns the mobj pointer.
    pub fn spawn_player(&mut self, x: i32, y: i32) -> *mut MapObject {
        let mobj = self.spawn_ptr(x, y, MapObjKind::MT_PLAYER);
        let player: *mut Player = &mut self.players[0];
        unsafe {
            (*mobj).player = Some(player);
            (*mobj).health = (*player).status.health;
            (*player).set_mobj(mobj);
        }
        mobj
    }

    /// Advance one thing's thinker by `n` tics (state machine + movement),
    /// mirroring the per-tic `run_thinkers` call for a single object.
    pub fn run_tics(&mut self, thing: *mut MapObject, n: u32) {
        for _ in 0..n {
            // SAFETY: `thing` is a live arena slot; its `thinker` pointer drives
            // one tic of the state machine + movement.
            let removed = unsafe { (*(*thing).thinker).think(&mut self.level) };
            if removed {
                break; // thing removed itself
            }
        }
    }

    /// Read access to the loaded map data (sectors, linedefs, …).
    pub fn level_data(&self) -> &level::LevelData {
        &self.level.level_data
    }

    /// Mutable access to the map data, e.g. to build a `MapPtr` to a linedef
    /// for `cross_special_line`.
    pub fn level_data_mut(&mut self) -> &mut level::LevelData {
        &mut self.level.level_data
    }

    /// Spawn the sector-special thinkers (light flicker/strobe/glow, etc.) the
    /// way `P_SpawnSpecials` does at level load. Opt-in so the physics fixtures
    /// keep a clean (empty-of-light-thinkers) baseline.
    pub fn spawn_level_specials(&mut self) {
        crate::env::specials::spawn_specials(&mut self.level);
    }

    /// Advance the whole level by `n` tics: runs every thinker (movers, lights,
    /// monsters). This is the mover/light update path; texture-scroll and
    /// switch animation (`update_specials`) need `PicData` and are skipped.
    pub fn run_level_tics(&mut self, n: u32) {
        for _ in 0..n {
            let level: *mut LevelState = &mut *self.level;
            // SAFETY: run_thinkers takes &mut LevelState; the raw reborrow
            // mirrors the engine's `level.thinkers.run_thinkers(level)` call,
            // where the arena and the level alias deliberately.
            unsafe {
                (*level).thinkers.run_thinkers(&mut *level);
            }
            self.level.level_time += 1;
        }
    }
}

/// The Doom RNG is global mutable state, so tests that assert on
/// `get_prndindex()` must not run concurrently with each other. Acquire this
/// guard for the whole test; it clears the RNG on entry so the index starts
/// from a known zero.
static RNG_LOCK: Mutex<()> = Mutex::new(());

/// Lock the global RNG for the duration of a test and reset it to index 0.
/// Hold the returned guard until all RNG assertions are done.
pub fn rng_guard() -> MutexGuard<'static, ()> {
    let guard = RNG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    m_clear_random();
    guard
}

#[cfg(test)]
mod smoke {
    use super::{TestLevel, rng_guard};
    use crate::MapObjKind;
    use math::{get_prndindex, p_random};

    /// The harness builds a real E1M1 level and spawns a thing that lands in a
    /// valid subsector with floor/ceiling resolved from the map.
    #[test]
    fn spawn_lands_on_floor() {
        let _g = rng_guard();
        let mut level = TestLevel::load("E1M1");
        // Player-1 start (see check_e1m1_things): (1056, -3616).
        let barrel = level.spawn(1056, -3616, MapObjKind::MT_BARREL);
        // E1M1 start sector floor is 0, ceiling 72 (see check_e1m1_sectors).
        assert_eq!(barrel.floorz().to_f32(), 0.0);
        assert_eq!(barrel.ceilingz().to_f32(), 72.0);
        assert_eq!(barrel.z.to_f32(), 0.0, "ONFLOORZ should snap z to floor");
        assert!(barrel.health > 0);
    }

    /// RNG seed/read works so physics tests can pin consumption order. The
    /// guard serialises RNG-sensitive tests against the global RNG state.
    #[test]
    fn rng_index_is_observable() {
        let _g = rng_guard();
        assert_eq!(get_prndindex(), 0);
        let _ = p_random();
        assert_eq!(get_prndindex(), 1);
        let _ = p_random();
        assert_eq!(get_prndindex(), 2);
    }
}
