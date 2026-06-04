//! TEMP demo-determinism trace. Throwaway debug instrument, gated by env vars.
//! Remove before commit.
//!
//! `DEMO_TRACE=<path>`  -> one text line per tic:
//!     `tic rndindex prndindex n_things sector_hash thing_hash full_hash`
//! `DEMO_TRACE_DUMP=<path>` + `DEMO_TRACE_FROM`/`DEMO_TRACE_TO` (level_time
//!     inclusive range) -> append full per-thing + per-sector state for those
//!     tics, for eyeballing exactly which field diverged.
//!
//! Hashes are FNV-1a over raw integer bits only (FixedT::raw, Angle bam, enum
//! discriminants) — exact and deterministic, never float-rounded.

use std::cell::RefCell;
use std::fmt::Write as _;
use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::ptr::addr_of;

use math::{get_prndindex, get_rndindex};

use crate::LevelState;
use crate::info::STATES;
use crate::thinker::{Thinker, ThinkerData};

const FNV_OFFSET: u64 = 0xCBF2_9CE4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01B3;

struct Fnv(u64);

impl Fnv {
    fn new() -> Self {
        Self(FNV_OFFSET)
    }
    fn w_i64(&mut self, v: i64) {
        for b in v.to_le_bytes() {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(FNV_PRIME);
        }
    }
    fn w_u64(&mut self, v: u64) {
        self.w_i64(v as i64);
    }
    fn done(&self) -> u64 {
        self.0
    }
}

struct TraceState {
    line: Option<File>,
    dump: Option<File>,
    from: u32,
    to: u32,
}

thread_local! {
    static TRACE: RefCell<Option<TraceState>> = const { RefCell::new(None) };
    static INIT: RefCell<bool> = const { RefCell::new(false) };
}

fn open_append(path: &str) -> Option<File> {
    OpenOptions::new().create(true).append(true).open(path).ok()
}

fn state_index(state: &'static crate::info::StateData) -> u64 {
    let base = addr_of!(STATES).cast::<crate::info::StateData>() as usize;
    let ptr = std::ptr::from_ref(state) as usize;
    ((ptr - base) / size_of::<crate::info::StateData>()) as u64
}

/// Feed a thing's determinism-relevant state into both the running hash and
/// (optionally) the human-readable dump string.
fn hash_thing(h: &mut Fnv, idx: usize, m: &crate::thing::MapObject, dump: Option<&mut String>) {
    h.w_i64(m.x.raw() as i64);
    h.w_i64(m.y.raw() as i64);
    h.w_i64(m.z.raw() as i64);
    h.w_u64(m.angle.to_bam() as u64);
    h.w_i64(m.momx.raw() as i64);
    h.w_i64(m.momy.raw() as i64);
    h.w_i64(m.momz.raw() as i64);
    h.w_i64(m.health as i64);
    h.w_i64(m.tics as i64);
    h.w_u64(state_index(m.state));
    h.w_u64(m.flags.bits() as u64);
    h.w_u64(m.kind as u64);
    h.w_u64(m.movedir as u64);
    h.w_i64(m.movecount as i64);
    h.w_i64(m.reactiontime as i64);
    h.w_i64(m.threshold as i64);
    h.w_u64(m.frame as u64);
    if let Some(d) = dump {
        let _ = writeln!(
            d,
            "  T{idx} kind={:?} x={} y={} z={} ang={} mom=({},{},{}) hp={} tics={} st={} dir={:?} mc={} rt={} thr={} flags={:#x} frame={}",
            m.kind,
            m.x.raw(),
            m.y.raw(),
            m.z.raw(),
            m.angle.to_bam(),
            m.momx.raw(),
            m.momy.raw(),
            m.momz.raw(),
            m.health,
            m.tics,
            state_index(m.state),
            m.movedir,
            m.movecount,
            m.reactiontime,
            m.threshold,
            m.flags.bits(),
            m.frame,
        );
    }
}

/// Called once per gameplay tic from `p_ticker`, after specials.
pub fn trace_tic(level: &LevelState) {
    let active = INIT.with(|i| {
        if !*i.borrow() {
            *i.borrow_mut() = true;
            let line = std::env::var("DEMO_TRACE")
                .ok()
                .and_then(|p| open_append(&p));
            let dump = std::env::var("DEMO_TRACE_DUMP")
                .ok()
                .and_then(|p| open_append(&p));
            if line.is_some() || dump.is_some() {
                let from = std::env::var("DEMO_TRACE_FROM")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                let to = std::env::var("DEMO_TRACE_TO")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(u32::MAX);
                TRACE.with(|t| {
                    *t.borrow_mut() = Some(TraceState {
                        line,
                        dump,
                        from,
                        to,
                    });
                });
            }
        }
        TRACE.with(|t| t.borrow().is_some())
    });
    if !active {
        return;
    }

    let tic = level.level_time;
    let rnd = get_rndindex() as u64;
    let prnd = get_prndindex() as u64;

    let mut sector_h = Fnv::new();
    for s in level.level_data.sectors.iter() {
        sector_h.w_i64(s.floorheight.raw() as i64);
        sector_h.w_i64(s.ceilingheight.raw() as i64);
        sector_h.w_u64(s.lightlevel as u64);
        sector_h.w_u64(s.special as u64);
    }

    TRACE.with(|t| {
        let mut guard = t.borrow_mut();
        let st = guard.as_mut().unwrap();
        let want_dump = st.dump.is_some() && tic >= st.from && tic <= st.to;

        let mut thing_h = Fnv::new();
        let mut dump_buf = if want_dump { Some(String::new()) } else { None };
        let mut count: u64 = 0;
        let mut idx = 0usize;
        level.thinkers.for_each(|th: &Thinker| {
            hash_thinker(&mut thing_h, idx, th, dump_buf.as_mut());
            if matches!(th.data(), ThinkerData::MapObject(_)) {
                count += 1;
            }
            idx += 1;
        });

        let mut full = Fnv::new();
        full.w_u64(rnd);
        full.w_u64(prnd);
        full.w_u64(sector_h.done());
        full.w_u64(thing_h.done());

        if let Some(f) = st.line.as_mut() {
            let _ = writeln!(
                f,
                "{tic} {rnd} {prnd} {count} {:016x} {:016x} {:016x}",
                sector_h.done(),
                thing_h.done(),
                full.done(),
            );
        }
        if let (Some(f), Some(buf)) = (st.dump.as_mut(), dump_buf) {
            let _ = writeln!(f, "=== tic {tic} rnd={rnd} prnd={prnd} things={count} ===");
            let _ = f.write_all(buf.as_bytes());
        }
    });
}

/// Hash a thinker's state by variant. Movers feed their height/direction/count
/// fields; map objects defer to `hash_thing`.
fn hash_thinker(h: &mut Fnv, idx: usize, th: &Thinker, mut dump: Option<&mut String>) {
    match th.data() {
        ThinkerData::MapObject(m) => hash_thing(h, idx, m, dump.as_deref_mut()),
        ThinkerData::VerticalDoor(d) => {
            h.w_u64(1);
            h.w_i64(d.topheight.raw() as i64);
            h.w_i64(d.speed.raw() as i64);
            h.w_i64(d.direction as i64);
            h.w_i64(d.topwait as i64);
            h.w_i64(d.topcountdown as i64);
            if let Some(s) = dump {
                let _ = writeln!(
                    s,
                    "  D{idx} door top={} speed={} dir={} wait={} cd={}",
                    d.topheight.raw(),
                    d.speed.raw(),
                    d.direction,
                    d.topwait,
                    d.topcountdown
                );
            }
        }
        ThinkerData::FloorMove(f) => {
            h.w_u64(2);
            h.w_i64(f.speed.raw() as i64);
            h.w_i64(f.direction as i64);
            h.w_i64(f.destheight.raw() as i64);
            if let Some(s) = dump {
                let _ = writeln!(
                    s,
                    "  F{idx} floor speed={} dir={} dest={}",
                    f.speed.raw(),
                    f.direction,
                    f.destheight.raw()
                );
            }
        }
        ThinkerData::CeilingMove(c) => {
            h.w_u64(3);
            h.w_i64(c.speed.raw() as i64);
            h.w_i64(c.direction as i64);
            h.w_i64(c.bottomheight.raw() as i64);
            h.w_i64(c.topheight.raw() as i64);
            if let Some(s) = dump {
                let _ = writeln!(
                    s,
                    "  C{idx} ceil speed={} dir={} bot={} top={}",
                    c.speed.raw(),
                    c.direction,
                    c.bottomheight.raw(),
                    c.topheight.raw()
                );
            }
        }
        ThinkerData::Platform(p) => {
            h.w_u64(4);
            h.w_i64(p.speed.raw() as i64);
            h.w_i64(p.low.raw() as i64);
            h.w_i64(p.high.raw() as i64);
            h.w_i64(p.count as i64);
            h.w_u64(p.status as u64);
            if let Some(s) = dump {
                let _ = writeln!(
                    s,
                    "  P{idx} plat speed={} low={} high={} count={} status={:?}",
                    p.speed.raw(),
                    p.low.raw(),
                    p.high.raw(),
                    p.count,
                    p.status
                );
            }
        }
        ThinkerData::LightFlash(_)
        | ThinkerData::StrobeFlash(_)
        | ThinkerData::FireFlicker(_)
        | ThinkerData::Glow(_)
        | ThinkerData::TestObject(_)
        | ThinkerData::Remove
        | ThinkerData::Free => {}
    }
}
