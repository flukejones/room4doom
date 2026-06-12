//! Map-list popup: pick which map of a multi-map WAD to open.

use std::cell::RefCell;
use std::rc::Rc;

use slint::ComponentHandle as _;

use crate::generated::{EditorWindow, MapsController};
use crate::project::open_wad_map;
use crate::state::SharedState;

pub(crate) fn init(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<MapsController>().on_picked(move |index| {
        let Some(ui) = weak.upgrade() else { return };
        let Some(name) = s.borrow().wad_maps.get(index as usize).cloned() else {
            return;
        };
        open_wad_map(&ui, &s, &name);
    });
}
