//! Inspector panel glue: selection → controller properties, apply callbacks → mutations.
//! Invalid field values fall back to the existing value.

use std::cell::RefCell;
use std::rc::Rc;

use editor_core::{
    LineDef, LineFlags as LineBits, Name8, Sector, SideDef, Thing, ThingFlags as ThingBits,
};
use slint::{ComponentHandle as _, Model as _, VecModel};

use crate::boundary::Tool;
use crate::generated::{
    EditorWindow, LineFlags, LineSide, LineTouched, LinedefInspectorController,
    SectorInspectorController, ThingInspectorController, ThingOptions, ThingTouched,
};
use crate::level_editor::{ThingTemplate, default_sector};
use crate::render::apply_damage;
use crate::state::{Damage, SelKey, SharedState, SyncKey};
use crate::views::view_draw_settings;
use crate::views::view_draw_settings::{name_or, parse_or};

/// Row index for `value` in the preset combo, or -1 when absent.
fn special_index(values: &slint::ModelRc<i32>, value: i32) -> i32 {
    values
        .iter()
        .position(|v| v == value)
        .map_or(-1, |i| i as i32)
}

fn push_specials(
    labels: Vec<slint::SharedString>,
    values: Vec<i32>,
) -> (slint::ModelRc<slint::SharedString>, slint::ModelRc<i32>) {
    (
        slint::ModelRc::new(VecModel::from(labels)),
        slint::ModelRc::new(VecModel::from(values)),
    )
}

/// Rebuild special-combo models from the open project; clear when none.
/// Parallel label + value lists so the pick callback returns the value directly.
pub(crate) fn set_special_models(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let (line_labels, line_values, sec_labels, sec_values) = {
        let state = &mut *shared.borrow_mut();
        // Force re-emit of special indices on next sync.
        state.map_render.panels_key = None;
        match &state.project {
            Some(p) => {
                let label = |v: i32, d: &str| slint::format!("{v} {d}");
                (
                    p.line_specials
                        .iter()
                        .map(|s| label(s.value, &s.desc))
                        .collect(),
                    p.line_specials.iter().map(|s| s.value).collect(),
                    p.sector_specials
                        .iter()
                        .map(|s| label(s.value, &s.desc))
                        .collect(),
                    p.sector_specials.iter().map(|s| s.value).collect(),
                )
            }
            None => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
        }
    };
    let line = ui.global::<LinedefInspectorController>();
    let (labels, values) = push_specials(line_labels, line_values);
    line.set_specials(labels);
    line.set_special_values(values);
    let sector = ui.global::<SectorInspectorController>();
    let (labels, values) = push_specials(sec_labels, sec_values);
    sector.set_specials(labels);
    sector.set_special_values(values);
    push_thing_types(ui, shared);
}

fn push_thing_types(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let types = shared.borrow().thing_types();
    let names: Vec<slint::SharedString> = types
        .iter()
        .map(|t| slint::SharedString::from(t.name))
        .collect();
    let kinds: Vec<i32> = types.iter().map(|t| t.kind).collect();
    let ctl = ui.global::<ThingInspectorController>();
    ctl.set_types(slint::ModelRc::new(VecModel::from(names)));
    ctl.set_type_num(slint::ModelRc::new(VecModel::from(kinds)));
}

/// Row index for `kind` in the controller's `type-num` list, or -1.
fn type_row(ctl: &ThingInspectorController, kind: i32) -> i32 {
    ctl.get_type_num()
        .iter()
        .position(|k| k == kind)
        .map_or(-1, |i| i as i32)
}

fn type_kind(ctl: &ThingInspectorController, row: i32) -> Option<i32> {
    ctl.get_type_num().row_data(usize::try_from(row).ok()?)
}

pub(crate) fn init(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    wire_line_apply(ui, shared);
    wire_thing_apply(ui, shared);
    wire_sector_apply(ui, shared);

    let weak = ui.as_weak();
    ui.global::<LinedefInspectorController>()
        .on_special_picked(move |value| {
            if let Some(ui) = weak.upgrade() {
                ui.global::<LinedefInspectorController>()
                    .set_special(slint::format!("{value}"));
            }
        });

    let weak = ui.as_weak();
    ui.global::<SectorInspectorController>()
        .on_special_picked(move |value| {
            if let Some(ui) = weak.upgrade() {
                ui.global::<SectorInspectorController>()
                    .set_special(slint::format!("{value}"));
            }
        });
}

/// Order-independent fingerprint of a selected-index set (XOR with bias).
fn sel_key(indices: &[u32]) -> SelKey {
    let xor = indices.iter().fold(0u32, |a, &i| a ^ i.wrapping_add(1));
    (
        indices.len(),
        indices.first().copied().unwrap_or(u32::MAX),
        indices.last().copied().unwrap_or(u32::MAX),
        xor,
    )
}

/// Common value of `field` across `items`, or `None` when they differ or empty.
fn fold_common<T: Copy + PartialEq, I>(items: &[I], field: impl Fn(&I) -> T) -> Option<T> {
    let first = field(items.first()?);
    items.iter().all(|i| field(i) == first).then_some(first)
}

pub(crate) fn sync(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    {
        let state = &mut *shared.borrow_mut();
        let key: SyncKey = (
            sel_key(&state.app.selected_lines()),
            sel_key(&state.app.selected_things()),
            state.app.current_sector,
            state.app.tool,
            state.app.thing_template,
        );
        // Geometry edits clear `panels_key` to force re-sync.
        if state.map_render.panels_key == Some(key) {
            return;
        }
        state.map_render.panels_key = Some(key);
    }
    let state = shared.borrow();
    let app = &state.app;
    let map = app.map.as_ref();

    let lines: Vec<LineDef> = map
        .map(|m| {
            app.selected_lines()
                .into_iter()
                .filter_map(|i| m.lines.get(i as usize).map(line_clone))
                .collect()
        })
        .unwrap_or_default();
    if lines.is_empty() {
        ui.global::<LinedefInspectorController>().set_active(false);
    } else {
        sync_lines(ui, &lines);
    }

    let things: Vec<Thing> = map
        .map(|m| {
            app.selected_things()
                .into_iter()
                .filter_map(|i| m.things.get(i as usize).copied())
                .collect()
        })
        .unwrap_or_default();
    if !things.is_empty() {
        sync_things(ui, &things);
    } else if app.tool == Tool::Thing {
        sync_thing_template(ui, app.thing_template);
    } else {
        ui.global::<ThingInspectorController>().set_active(false);
    }

    let sector = app
        .current_sector
        .and_then(|i| Some((i, *map?.sectors.get(i as usize)?)));
    match sector {
        Some((index, sector)) => sync_sector(ui, index, sector),
        None => ui.global::<SectorInspectorController>().set_active(false),
    }
    drop(state);
    view_draw_settings::sync(ui, shared);
}

fn line_clone(l: &LineDef) -> LineDef {
    LineDef {
        v1: l.v1,
        v2: l.v2,
        flags: l.flags,
        special: l.special,
        tag: l.tag,
        front: l.front,
        back: l.back,
    }
}

/// Shared flag bit across lines; mixed → false.
fn common_flag(lines: &[LineDef], bit: LineBits) -> bool {
    fold_common(lines, |l| l.flags.contains(bit)).unwrap_or(false)
}

/// Shared string field across lines, or "" when they differ.
fn common_str(lines: &[LineDef], field: impl Fn(&LineDef) -> String) -> slint::SharedString {
    let first = match lines.first() {
        Some(l) => field(l),
        None => return "".into(),
    };
    if lines.iter().all(|l| field(l) == first) {
        first.into()
    } else {
        "".into()
    }
}

/// Push selected linedef(s) into the panel; shared values shown, differing shown blank.
fn sync_lines(ui: &EditorWindow, lines: &[LineDef]) {
    let ctl = ui.global::<LinedefInspectorController>();
    ctl.set_active(true);
    ctl.set_line_index(if lines.len() == 1 { 0 } else { -1 });
    ctl.set_flags(LineFlags {
        blocks: common_flag(lines, LineBits::BLOCKING),
        block_monsters: common_flag(lines, LineBits::BLOCK_MONSTERS),
        two_sided: common_flag(lines, LineBits::TWO_SIDED),
        upper_unpeg: common_flag(lines, LineBits::UNPEG_TOP),
        lower_unpeg: common_flag(lines, LineBits::UNPEG_BOTTOM),
        secret: common_flag(lines, LineBits::SECRET),
        block_sound: common_flag(lines, LineBits::BLOCK_SOUND),
        hidden: common_flag(lines, LineBits::UNMAPPED),
    });
    let special = fold_common(lines, |l| l.special);
    ctl.set_special(special.map_or_else(|| "".into(), |s| slint::format!("{s}")));
    ctl.set_special_index(special.map_or(-1, |s| special_index(&ctl.get_special_values(), s)));
    ctl.set_tag(fold_common(lines, |l| l.tag).map_or_else(|| "".into(), |t| slint::format!("{t}")));
    let off = |v: Option<i32>| v.map_or_else(|| "".into(), |v| slint::format!("{v}"));
    ctl.set_front(LineSide {
        top: common_str(lines, |l| l.front.top_tex.to_dwd_field().to_owned()),
        mid: common_str(lines, |l| l.front.middle_tex.to_dwd_field().to_owned()),
        bottom: common_str(lines, |l| l.front.bottom_tex.to_dwd_field().to_owned()),
        xoff: off(fold_common(lines, |l| l.front.x_offset)),
        yoff: off(fold_common(lines, |l| l.front.y_offset)),
    });
    ctl.set_has_back(lines.iter().any(|l| l.back.is_some()));
    ctl.set_back(LineSide {
        top: common_str(lines, |l| back_tex(l, |b| b.top_tex)),
        mid: common_str(lines, |l| back_tex(l, |b| b.middle_tex)),
        bottom: common_str(lines, |l| back_tex(l, |b| b.bottom_tex)),
        xoff: off(fold_common(lines, |l| l.back.map(|b| b.x_offset)).flatten()),
        yoff: off(fold_common(lines, |l| l.back.map(|b| b.y_offset)).flatten()),
    });
    clear_line_touched(&ctl);
}

/// Back-side texture as a dwd field, or "" when one-sided.
fn back_tex(line: &LineDef, pick: impl Fn(&SideDef) -> Name8) -> String {
    line.back
        .as_ref()
        .map(|b| pick(b).to_dwd_field().to_owned())
        .unwrap_or_default()
}

fn clear_line_touched(ctl: &LinedefInspectorController) {
    ctl.set_touched(LineTouched::default());
}

fn clear_thing_touched(ctl: &ThingInspectorController) {
    ctl.set_touched(ThingTouched::default());
}

/// Push selected thing(s) into the panel; shared fields shown, differing shown blank.
fn sync_things(ui: &EditorWindow, things: &[Thing]) {
    let ctl = ui.global::<ThingInspectorController>();
    ctl.set_active(true);
    ctl.set_thing_index(if things.len() == 1 { 0 } else { -1 });
    let kind = fold_common(things, |t| t.kind);
    ctl.set_type_multiple(kind.is_none());
    ctl.set_type_index(kind.map_or(-1, |k| type_row(&ctl, k)));
    ctl.set_angle(
        fold_common(things, |t| t.angle).map_or_else(|| "".into(), |a| slint::format!("{a}")),
    );
    let common_opt =
        |bit: ThingBits| fold_common(things, |t| t.options.contains(bit)).unwrap_or(false);
    ctl.set_options(ThingOptions {
        easy: common_opt(ThingBits::EASY),
        normal: common_opt(ThingBits::NORMAL),
        hard: common_opt(ThingBits::HARD),
        ambush: common_opt(ThingBits::AMBUSH),
        multi: common_opt(ThingBits::MULTIPLAYER),
    });
    clear_thing_touched(&ctl);
}

/// Push the placement template (THING tool, no selection).
fn sync_thing_template(ui: &EditorWindow, template: ThingTemplate) {
    let ctl = ui.global::<ThingInspectorController>();
    ctl.set_active(true);
    ctl.set_thing_index(-1);
    ctl.set_type_multiple(false);
    ctl.set_type_index(type_row(&ctl, template.kind));
    ctl.set_angle(slint::format!("{}", template.angle));
    ctl.set_options(ThingOptions {
        easy: template.options.contains(ThingBits::EASY),
        normal: template.options.contains(ThingBits::NORMAL),
        hard: template.options.contains(ThingBits::HARD),
        ambush: template.options.contains(ThingBits::AMBUSH),
        multi: template.options.contains(ThingBits::MULTIPLAYER),
    });
    clear_thing_touched(&ctl);
}

fn sync_sector(ui: &EditorWindow, index: u32, sector: Sector) {
    let ctl = ui.global::<SectorInspectorController>();
    ctl.set_active(true);
    ctl.set_sector_index(index as i32);
    ctl.set_floor_h(slint::format!("{}", sector.floor_height));
    ctl.set_ceil_h(slint::format!("{}", sector.ceil_height));
    ctl.set_light(slint::format!("{}", sector.light_level));
    ctl.set_special(slint::format!("{}", sector.special));
    ctl.set_special_index(special_index(&ctl.get_special_values(), sector.special));
    ctl.set_tag(slint::format!("{}", sector.tag));
    ctl.set_floor_flat(sector.floor_flat.to_dwd_field().into());
    ctl.set_ceil_flat(sector.ceil_flat.to_dwd_field().into());
}

/// Read panel fields into a `Sector`, falling back to `fallback` per field.
pub fn sector_from_panel(ui: &EditorWindow, fallback: Sector) -> Sector {
    let ctl = ui.global::<SectorInspectorController>();
    Sector {
        floor_height: parse_or(&ctl.get_floor_h(), fallback.floor_height),
        floor_flat: name_or(&ctl.get_floor_flat(), fallback.floor_flat),
        ceil_height: parse_or(&ctl.get_ceil_h(), fallback.ceil_height),
        ceil_flat: name_or(&ctl.get_ceil_flat(), fallback.ceil_flat),
        light_level: parse_or(&ctl.get_light(), fallback.light_level),
        special: parse_or(&ctl.get_special(), fallback.special),
        tag: parse_or(&ctl.get_tag(), fallback.tag),
    }
}

/// Touched line-panel fields; `apply_to` overrides only touched fields.
struct LineEdit {
    flag_set: LineBits, // bits touched
    flag_on: LineBits,  // of those, bits turned on
    special: Option<i32>,
    tag: Option<i32>,
    front_top: Option<Name8>,
    front_mid: Option<Name8>,
    front_bottom: Option<Name8>,
    front_xoff: Option<i32>,
    front_yoff: Option<i32>,
    back_top: Option<Name8>,
    back_mid: Option<Name8>,
    back_bottom: Option<Name8>,
    back_xoff: Option<i32>,
    back_yoff: Option<i32>,
}

impl LineEdit {
    fn read(ctl: &LinedefInspectorController) -> Self {
        let f = ctl.get_flags();
        let t = ctl.get_touched();
        let front = ctl.get_front();
        let back = ctl.get_back();
        let mut flag_set = LineBits::empty();
        let mut flag_on = LineBits::empty();
        for (touched, on, bit) in [
            (t.blocks, f.blocks, LineBits::BLOCKING),
            (t.block_monsters, f.block_monsters, LineBits::BLOCK_MONSTERS),
            (t.two_sided, f.two_sided, LineBits::TWO_SIDED),
            (t.upper_unpeg, f.upper_unpeg, LineBits::UNPEG_TOP),
            (t.lower_unpeg, f.lower_unpeg, LineBits::UNPEG_BOTTOM),
            (t.secret, f.secret, LineBits::SECRET),
            (t.block_sound, f.block_sound, LineBits::BLOCK_SOUND),
            (t.hidden, f.hidden, LineBits::UNMAPPED),
        ] {
            if touched {
                flag_set.insert(bit);
                if on {
                    flag_on.insert(bit);
                }
            }
        }
        let name = |on: bool, s: slint::SharedString| {
            on.then(|| Name8::from_dwd_field(s.trim()).unwrap_or(Name8::EMPTY))
        };
        let num = |on: bool, s: slint::SharedString| on.then(|| s.trim().parse().ok()).flatten();
        Self {
            flag_set,
            flag_on,
            special: num(t.special, ctl.get_special()),
            tag: num(t.tag, ctl.get_tag()),
            front_top: name(t.front_top, front.top),
            front_mid: name(t.front_mid, front.mid),
            front_bottom: name(t.front_bottom, front.bottom),
            front_xoff: num(t.front_xoff, front.xoff),
            front_yoff: num(t.front_yoff, front.yoff),
            back_top: name(t.back_top, back.top),
            back_mid: name(t.back_mid, back.mid),
            back_bottom: name(t.back_bottom, back.bottom),
            back_xoff: num(t.back_xoff, back.xoff),
            back_yoff: num(t.back_yoff, back.yoff),
        }
    }

    fn apply_to(&self, old: &LineDef) -> LineDef {
        // bitflags `!` masks to known flags; raw masks preserve pass-through bits.
        let flags = LineBits::from_bits_retain(
            (old.flags.bits() & !self.flag_set.bits()) | self.flag_on.bits(),
        );
        let front = SideDef {
            x_offset: self.front_xoff.unwrap_or(old.front.x_offset),
            y_offset: self.front_yoff.unwrap_or(old.front.y_offset),
            top_tex: self.front_top.unwrap_or(old.front.top_tex),
            bottom_tex: self.front_bottom.unwrap_or(old.front.bottom_tex),
            middle_tex: self.front_mid.unwrap_or(old.front.middle_tex),
            sector: old.front.sector,
        };
        let back = old.back.map(|b| SideDef {
            x_offset: self.back_xoff.unwrap_or(b.x_offset),
            y_offset: self.back_yoff.unwrap_or(b.y_offset),
            top_tex: self.back_top.unwrap_or(b.top_tex),
            bottom_tex: self.back_bottom.unwrap_or(b.bottom_tex),
            middle_tex: self.back_mid.unwrap_or(b.middle_tex),
            sector: b.sector,
        });
        LineDef {
            v1: old.v1,
            v2: old.v2,
            flags,
            special: self.special.unwrap_or(old.special),
            tag: self.tag.unwrap_or(old.tag),
            front,
            back,
        }
    }
}

fn wire_line_apply(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<LinedefInspectorController>().on_apply(move || {
        let Some(ui) = weak.upgrade() else { return };
        let edit = LineEdit::read(&ui.global::<LinedefInspectorController>());
        let damage = {
            let state = &mut *s.borrow_mut();
            let indices = state.app.selected_lines();
            if indices.is_empty() {
                return;
            }
            state.app.apply_lines(&indices, |old| edit.apply_to(old))
        };
        apply_damage(&ui, &s, damage);
        sync(&ui, &s);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<LinedefInspectorController>().on_flip(move || {
        let Some(ui) = weak.upgrade() else { return };
        let damage = {
            let state = &mut *s.borrow_mut();
            let indices = state.app.selected_lines();
            if indices.is_empty() {
                return;
            }
            state.app.flip_selected_lines(&indices)
        };
        apply_damage(&ui, &s, damage);
        sync(&ui, &s);
    });
}

/// Touched thing-panel fields; `apply_to` overrides only touched fields.
struct ThingEdit {
    kind: Option<i32>,
    angle: Option<i32>,
    opt_set: ThingBits,
    opt_on: ThingBits,
}

impl ThingEdit {
    fn read(ctl: &ThingInspectorController) -> Self {
        let t = ctl.get_touched();
        let o = ctl.get_options();
        let kind = t.r#type.then(|| type_kind(ctl, ctl.get_type_index()));
        let mut opt_set = ThingBits::empty();
        let mut opt_on = ThingBits::empty();
        for (touched, on, bit) in [
            (t.easy, o.easy, ThingBits::EASY),
            (t.normal, o.normal, ThingBits::NORMAL),
            (t.hard, o.hard, ThingBits::HARD),
            (t.ambush, o.ambush, ThingBits::AMBUSH),
            (t.multi, o.multi, ThingBits::MULTIPLAYER),
        ] {
            if touched {
                opt_set.insert(bit);
                if on {
                    opt_on.insert(bit);
                }
            }
        }
        Self {
            kind: kind.flatten(),
            angle: t
                .angle
                .then(|| ctl.get_angle().trim().parse().ok())
                .flatten(),
            opt_set,
            opt_on,
        }
    }

    fn apply_to(&self, old: &Thing) -> Thing {
        Thing {
            x: old.x,
            y: old.y,
            z: old.z,
            angle: self.angle.unwrap_or(old.angle),
            kind: self.kind.unwrap_or(old.kind),
            // Raw-bit mask preserves pass-through bits.
            options: ThingBits::from_bits_retain(
                (old.options.bits() & !self.opt_set.bits()) | self.opt_on.bits(),
            ),
        }
    }
}

fn wire_thing_apply(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<ThingInspectorController>().on_apply(move || {
        let Some(ui) = weak.upgrade() else { return };
        let edit = ThingEdit::read(&ui.global::<ThingInspectorController>());
        let damage = {
            let state = &mut *s.borrow_mut();
            let indices = state.app.selected_things();
            if indices.is_empty() {
                // THING tool + no selection → edit the placement template.
                let template = &mut state.app.thing_template;
                if let Some(a) = edit.angle {
                    template.angle = a;
                }
                if let Some(k) = edit.kind {
                    template.kind = k;
                }
                template.options = ThingBits::from_bits_retain(
                    (template.options.bits() & !edit.opt_set.bits()) | edit.opt_on.bits(),
                );
                Damage::None
            } else {
                state.app.apply_things(&indices, |old| edit.apply_to(old))
            }
        };
        apply_damage(&ui, &s, damage);
        sync(&ui, &s);
    });
}

fn wire_sector_apply(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<SectorInspectorController>().on_apply(move || {
        let Some(ui) = weak.upgrade() else { return };
        let damage = {
            let state = &mut *s.borrow_mut();
            let Some(index) = state.app.current_sector else {
                return;
            };
            let Some(old) = state
                .app
                .map
                .as_ref()
                .and_then(|m| m.sectors.get(index as usize))
                .copied()
            else {
                return;
            };
            let new = sector_from_panel(&ui, old);
            state.app.apply_sector(index, new)
        };
        apply_damage(&ui, &s, damage);
        sync(&ui, &s);
    });

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<SectorInspectorController>()
        .on_new_sector(move || {
            let Some(ui) = weak.upgrade() else { return };
            {
                let state = &mut *s.borrow_mut();
                let base = default_sector();
                let new = sector_from_panel(&ui, base);
                state.app.new_sector(new);
            }
            sync(&ui, &s);
        });
}
