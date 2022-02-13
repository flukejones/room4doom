use std::ptr::NonNull;

use crate::d_thinker::Thinker;
use crate::level_data::level::Level;
use crate::{d_thinker::Think, p_spec::*};

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
