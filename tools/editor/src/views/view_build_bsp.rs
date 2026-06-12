//! Build-BSP popup: pick partition interval; run traced BSP, animate at that interval.

use std::cell::RefCell;
use std::rc::Rc;

use slint::ComponentHandle as _;

use crate::bsp_anim::{MAX_INTERVAL_MS, MIN_INTERVAL_MS};
use crate::generated::{BuildBspController, EditorWindow};
use crate::jobs;
use crate::state::SharedState;

pub(crate) fn init(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<BuildBspController>().on_run(move || {
        let Some(ui) = weak.upgrade() else { return };
        let ctl = ui.global::<BuildBspController>();
        let ms = ctl.get_interval_ms();
        {
            let state = &mut *s.borrow_mut();
            state.anim_interval_ms = (ms.max(0) as u64).clamp(MIN_INTERVAL_MS, MAX_INTERVAL_MS);
            state.anim_keep_all = ctl.get_keep_all();
        }
        jobs::start_build(&ui, &s);
    });
}
