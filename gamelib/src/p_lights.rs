use std::ptr::NonNull;

use crate::angle::Angle;
use crate::d_thinker::Think;
use crate::d_thinker::Thinker;
use crate::level_data::level::Level;
use crate::level_data::map_defs::Sector;
use crate::DPtr;

pub struct FireFlicker {
    pub thinker: NonNull<Thinker>,
    pub sector: DPtr<Sector>,
    pub count: i32,
    pub max_light: i32,
    pub min_light: i32,
}

impl Think for FireFlicker {
    fn think(object: &mut crate::d_thinker::ThinkerType, level: &mut Level) -> bool {
        todo!()
    }

    fn set_thinker_ptr(&mut self, ptr: std::ptr::NonNull<Thinker>) {
        self.thinker = ptr;
    }

    fn thinker(&self) -> NonNull<Thinker> {
        self.thinker
    }
}

pub struct LightFlash {
    pub thinker: NonNull<Thinker>,
    pub sector: DPtr<Sector>,
    pub count: i32,
    pub max_light: i32,
    pub min_light: i32,
    pub max_time: i32,
    pub min_time: i32,
}

impl Think for LightFlash {
    fn think(object: &mut crate::d_thinker::ThinkerType, level: &mut Level) -> bool {
        todo!()
    }

    fn set_thinker_ptr(&mut self, ptr: std::ptr::NonNull<Thinker>) {
        self.thinker = ptr;
    }

    fn thinker(&self) -> NonNull<Thinker> {
        self.thinker
    }
}

pub struct Strobe {
    pub thinker: NonNull<Thinker>,
    pub sector: DPtr<Sector>,
    pub count: i32,
    pub min_light: i32,
    pub max_light: i32,
    pub dark_time: i32,
    pub bright_time: i32,
}

impl Think for Strobe {
    fn think(object: &mut crate::d_thinker::ThinkerType, level: &mut Level) -> bool {
        todo!()
    }

    fn set_thinker_ptr(&mut self, ptr: std::ptr::NonNull<Thinker>) {
        self.thinker = ptr;
    }

    fn thinker(&self) -> NonNull<Thinker> {
        self.thinker
    }
}

pub struct Glow {
    pub thinker: NonNull<Thinker>,
    pub sector: DPtr<Sector>,
    pub min_light: i32,
    pub max_light: i32,
    pub direction: Angle,
}

impl Think for Glow {
    fn think(object: &mut crate::d_thinker::ThinkerType, level: &mut Level) -> bool {
        todo!()
    }

    fn set_thinker_ptr(&mut self, ptr: std::ptr::NonNull<Thinker>) {
        self.thinker = ptr;
    }

    fn thinker(&self) -> NonNull<Thinker> {
        self.thinker
    }
}
