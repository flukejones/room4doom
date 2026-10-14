//! Persisted native dialog sizes.

use super::{DialogsView, PopupGeom, PopupId};
use crate::app::preferences::{self as preferences, PopupWindow, WindowGeom};

impl DialogsView {
    pub(super) fn popup_geom(&self, id: PopupId) -> PopupGeom {
        self.drafts
            .geometry
            .iter()
            .find(|(key, _)| *key == id)
            .map(|(_, geom)| geom.clone())
            .unwrap_or_default()
    }
    pub(super) fn set_popup_geom(&mut self, id: PopupId, geom: PopupGeom) {
        if let Some((_, current)) = self.drafts.geometry.iter_mut().find(|(key, _)| *key == id) {
            *current = geom;
        } else {
            self.drafts.geometry.push((id, geom));
        }
    }
    pub(super) fn restore_geometry(&mut self, id: PopupId) {
        let geom = self
            .shared
            .borrow()
            .prefs
            .popup_windows
            .get(popup_window(id));
        self.set_popup_geom(
            id,
            PopupGeom {
                x: 0.,
                y: 0.,
                w: geom.w,
                h: geom.h,
            },
        );
    }
    pub(super) fn persist_geometry(&self, id: PopupId) {
        let geom = self.popup_geom(id);
        let state = &mut *self.shared.borrow_mut();
        state.prefs.popup_windows.set(
            popup_window(id),
            WindowGeom {
                off_x: geom.x,
                off_y: geom.y,
                w: geom.w,
                h: geom.h,
            },
        );
        let mut saved = state.prefs.clone();
        if self.active == Some(PopupId::Prefs) {
            self.drafts.prefs.snapshot.restore(&mut saved);
        }
        if let Err(error) = preferences::save_prefs(&saved) {
            log::warn!("saving popup geometry: {error}");
        }
    }
}
fn popup_window(id: PopupId) -> PopupWindow {
    match id {
        PopupId::MapList => PopupWindow::MapList,
        PopupId::Browser => PopupWindow::Browser,
        PopupId::Remap => PopupWindow::Remap,
        PopupId::Prefs => PopupWindow::Prefs,
        PopupId::WallEdit => PopupWindow::WallEdit,
        PopupId::SectorEdit => PopupWindow::SectorEdit,
        PopupId::ProjectSettings => PopupWindow::ProjectSettings,
        PopupId::BuildBsp => PopupWindow::BuildBsp,
        PopupId::NewMap => PopupWindow::NewMap,
        PopupId::Audit => PopupWindow::Audit,
        PopupId::Animations => PopupWindow::Animations,
    }
}
