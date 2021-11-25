use crate::level_data::level::Level;
use crate::{d_thinker::Think, p_spec::*};

impl Think for FireFlicker {
    fn think(&mut self, level: &mut Level) -> bool {
        false
    }
}

impl Think for LightFlash {
    fn think(&mut self, level: &mut Level) -> bool {
        false
    }
}

impl Think for Strobe {
    fn think(&mut self, level: &mut Level) -> bool {
        false
    }
}

impl Think for Glow {
    fn think(&mut self, level: &mut Level) -> bool {
        false
    }
}

impl Think for Platform {
    fn think(&mut self, level: &mut Level) -> bool {
        false
    }
}

impl Think for FloorMove {
    fn think(&mut self, level: &mut Level) -> bool {
        false
    }
}

impl Think for CeilingMove {
    fn think(&mut self, level: &mut Level) -> bool {
        false
    }
}
