//! Tool strip boundary: active tool + sector-fill mode.

use std::cell::RefCell;
use std::rc::Rc;

use slint::ComponentHandle as _;

use crate::boundary::Tool;
use crate::generated::{EditorWindow, ToolController};
use crate::render::apply_damage;
use crate::state::{Damage, SectorFill, SharedState};

pub(crate) fn init(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<ToolController>().on_tool_changed(move |tool| {
        let tool = Tool::from(tool);
        s.borrow_mut().app.set_tool(tool);
        if let Some(ui) = weak.upgrade() {
            ui.global::<ToolController>().set_current(tool.into());
            apply_damage(&ui, &s, Damage::Repaint); // thing-radius circles toggled by tool
        }
    });

    let s = shared.clone();
    ui.global::<ToolController>()
        .on_ngon_sides_chosen(move |sides| {
            s.borrow_mut().app.set_ngon_sides(sides.max(3) as u32);
        });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<ToolController>().on_fill_changed(move |mode| {
        let Some(ui) = weak.upgrade() else { return };
        let mode = SectorFill::from(mode);
        s.borrow_mut().app.sector_fill = mode;
        ui.global::<ToolController>().set_sector_fill(mode.into());
        apply_damage(&ui, &s, Damage::Edited);
    });
}
