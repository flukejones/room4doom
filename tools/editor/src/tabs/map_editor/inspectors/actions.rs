//! Selection inspector presentation and typed editing operations. Invalid fields retain existing values.

use super::{
    draw::{self, name_or, parse_or},
    line::LineInspectorData,
    thing::ThingInspectorData,
};
use crate::app::session::{Damage, EditorSession};
use crate::boundary::Tool;
use crate::components::types::{LineFlags, LineSide, LineTouched, ThingOptions, ThingTouched};
use crate::level_editor::{ThingTemplate, default_sector};
use crate::tabs::map_editor::presentation::{MapPresentation, SyncKey};
use editor_core::{
    ArenaKey as _, LineDef, LineFlags as LineBits, Name8, Sector, SectorKey, SideDef, SpecialDef,
    Thing, ThingFlags as ThingBits,
};

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
    fn read(ctl: &LineInspectorData) -> Self {
        let f = &ctl.flags;
        let t = &ctl.touched;
        let front = &ctl.front;
        let back = &ctl.back;
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
        let name =
            |on: bool, s: &str| on.then(|| Name8::from_dwd_field(s.trim()).unwrap_or(Name8::EMPTY));
        let num = |on: bool, s: &str| on.then(|| s.trim().parse().ok()).flatten();
        Self {
            flag_set,
            flag_on,
            special: num(t.special, &ctl.special),
            tag: num(t.tag, &ctl.tag),
            front_top: name(t.front_top, &front.top),
            front_mid: name(t.front_mid, &front.mid),
            front_bottom: name(t.front_bottom, &front.bottom),
            front_xoff: num(t.front_xoff, &front.xoff),
            front_yoff: num(t.front_yoff, &front.yoff),
            back_top: name(t.back_top, &back.top),
            back_mid: name(t.back_mid, &back.mid),
            back_bottom: name(t.back_bottom, &back.bottom),
            back_xoff: num(t.back_xoff, &back.xoff),
            back_yoff: num(t.back_yoff, &back.yoff),
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

/// Touched thing-panel fields; `apply_to` overrides only touched fields.
struct ThingEdit {
    kind: Option<i32>,
    angle: Option<i32>,
    opt_set: ThingBits,
    opt_on: ThingBits,
}

impl ThingEdit {
    fn read(ctl: &ThingInspectorData) -> Self {
        let t = &ctl.touched;
        let o = &ctl.options;
        let kind = t.r#type.then_some(ctl.kind).flatten();
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
            kind,
            angle: t.angle.then(|| ctl.angle.trim().parse().ok()).flatten(),
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

/// Rebuild typed presets from the open project's definitions.
pub(crate) fn set_special_models(ui: &mut MapPresentation, state: &mut EditorSession) {
    ui.panels_key = None;
    if let Some(project) = &state.project {
        let choices = |specials: &[SpecialDef]| {
            specials
                .iter()
                .map(|special| {
                    (
                        special.value,
                        format!("{} {}", special.value, special.desc).into(),
                    )
                })
                .collect()
        };
        ui.line.specials = choices(&project.line_specials);
        ui.sector.specials = choices(&project.sector_specials);
    } else {
        ui.line.specials = Default::default();
        ui.sector.specials = Default::default();
    }
    ui.thing.types = state
        .thing_types()
        .into_iter()
        .map(|thing| (thing.kind, thing.name.into()))
        .collect();
}

/// Common value of `field` across `items`, or `None` when they differ or empty.
fn fold_common<T: Copy + PartialEq, I>(items: &[I], field: impl Fn(&I) -> T) -> Option<T> {
    let first = field(items.first()?);
    items.iter().all(|i| field(i) == first).then_some(first)
}

pub(crate) fn sync(ui: &mut MapPresentation, state: &mut EditorSession) {
    {
        let state = &mut *state;
        let key: SyncKey = (
            state.map_editor.selection.revision(),
            state.map_editor.current_sector,
            state.map_editor.tool,
            state.map_editor.thing_template,
        );
        // Geometry edits clear `panels_key` to force re-sync.
        if ui.panels_key == Some(key) {
            return;
        }
        ui.panels_key = Some(key);
        ui.panels_epoch = ui.panels_epoch.wrapping_add(1);
    }
    let state = &*state;
    let app = &state.map_editor;
    let map = app.map.as_ref();

    let lines: Vec<LineDef> = map
        .map(|m| {
            app.selected_lines()
                .into_iter()
                .filter_map(|k| m.lines.get(k).copied())
                .collect()
        })
        .unwrap_or_default();
    if lines.is_empty() {
        ui.line.active = false;
    } else {
        sync_lines(ui, &lines);
    }

    let things: Vec<Thing> = map
        .map(|m| {
            app.selected_things()
                .into_iter()
                .filter_map(|k| m.things.get(k).copied())
                .collect()
        })
        .unwrap_or_default();
    if !things.is_empty() {
        sync_things(ui, &things);
    } else if app.tool == Tool::Thing {
        sync_thing_template(ui, app.thing_template);
    } else {
        ui.thing.active = false;
    }

    let sector = app
        .current_sector
        .and_then(|k| Some((k, *map?.sectors.get(k)?)));
    match sector {
        Some((key, sector)) => sync_sector(ui, key, sector),
        None => ui.sector.active = false,
    }

    draw::sync(ui, state);
}

/// Shared flag bit across lines; mixed → false.
fn common_flag(lines: &[LineDef], bit: LineBits) -> bool {
    fold_common(lines, |l| l.flags.contains(bit)).unwrap_or(false)
}

/// Number as text, or blank when `None` (mixed selection).
fn opt_num(v: Option<i32>) -> String {
    v.map_or_else(String::new, |v| format!("{v}"))
}

/// Shared texture as text, or blank for a mixed selection.
fn opt_name(name: Option<Name8>) -> String {
    name.map(|name| name.to_dwd_field().to_owned())
        .unwrap_or_default()
}

/// Push selected linedef(s) into the panel; shared values shown, differing shown blank.
fn sync_lines(ui: &mut MapPresentation, lines: &[LineDef]) {
    let ctl = &mut ui.line;
    ctl.active = true;
    ctl.line_index = if lines.len() == 1 { 0 } else { -1 };
    ctl.flags = LineFlags {
        blocks: common_flag(lines, LineBits::BLOCKING),
        block_monsters: common_flag(lines, LineBits::BLOCK_MONSTERS),
        two_sided: common_flag(lines, LineBits::TWO_SIDED),
        upper_unpeg: common_flag(lines, LineBits::UNPEG_TOP),
        lower_unpeg: common_flag(lines, LineBits::UNPEG_BOTTOM),
        secret: common_flag(lines, LineBits::SECRET),
        block_sound: common_flag(lines, LineBits::BLOCK_SOUND),
        hidden: common_flag(lines, LineBits::UNMAPPED),
    };
    let special = fold_common(lines, |l| l.special);
    ctl.special = opt_num(special);
    ctl.tag = opt_num(fold_common(lines, |l| l.tag));
    ctl.front = LineSide {
        top: opt_name(fold_common(lines, |l| l.front.top_tex)),
        mid: opt_name(fold_common(lines, |l| l.front.middle_tex)),
        bottom: opt_name(fold_common(lines, |l| l.front.bottom_tex)),
        xoff: opt_num(fold_common(lines, |l| l.front.x_offset)),
        yoff: opt_num(fold_common(lines, |l| l.front.y_offset)),
    };
    ctl.has_back = lines.iter().any(|l| l.back.is_some());
    ctl.back = LineSide {
        top: opt_name(fold_common(lines, |l| l.back.map(|b| b.top_tex)).flatten()),
        mid: opt_name(fold_common(lines, |l| l.back.map(|b| b.middle_tex)).flatten()),
        bottom: opt_name(fold_common(lines, |l| l.back.map(|b| b.bottom_tex)).flatten()),
        xoff: opt_num(fold_common(lines, |l| l.back.map(|b| b.x_offset)).flatten()),
        yoff: opt_num(fold_common(lines, |l| l.back.map(|b| b.y_offset)).flatten()),
    };
    clear_line_touched(ctl);
}

fn clear_line_touched(ctl: &mut LineInspectorData) {
    ctl.touched = LineTouched::default();
}

fn clear_thing_touched(ctl: &mut ThingInspectorData) {
    ctl.touched = ThingTouched::default();
}

/// Push selected thing(s) into the panel; shared fields shown, differing shown blank.
fn sync_things(ui: &mut MapPresentation, things: &[Thing]) {
    let ctl = &mut ui.thing;
    ctl.active = true;
    let kind = fold_common(things, |t| t.kind);
    ctl.kind = kind;
    ctl.angle = opt_num(fold_common(things, |t| t.angle));
    let common_opt =
        |bit: ThingBits| fold_common(things, |t| t.options.contains(bit)).unwrap_or(false);
    ctl.options = ThingOptions {
        easy: common_opt(ThingBits::EASY),
        normal: common_opt(ThingBits::NORMAL),
        hard: common_opt(ThingBits::HARD),
        ambush: common_opt(ThingBits::AMBUSH),
        multi: common_opt(ThingBits::MULTIPLAYER),
    };
    clear_thing_touched(ctl);
}

/// Push the placement template (THING tool, no selection).
fn sync_thing_template(ui: &mut MapPresentation, template: ThingTemplate) {
    let ctl = &mut ui.thing;
    ctl.active = true;
    ctl.kind = Some(template.kind);
    ctl.angle = format!("{}", template.angle);
    ctl.options = ThingOptions {
        easy: template.options.contains(ThingBits::EASY),
        normal: template.options.contains(ThingBits::NORMAL),
        hard: template.options.contains(ThingBits::HARD),
        ambush: template.options.contains(ThingBits::AMBUSH),
        multi: template.options.contains(ThingBits::MULTIPLAYER),
    };
    clear_thing_touched(ctl);
}

fn sync_sector(ui: &mut MapPresentation, key: SectorKey, sector: Sector) {
    let ctl = &mut ui.sector;
    ctl.active = true;
    ctl.sector_index = key.slot() as i32;
    ctl.floor_h = format!("{}", sector.floor_height);
    ctl.ceil_h = format!("{}", sector.ceil_height);
    ctl.light = format!("{}", sector.light_level);
    ctl.special = format!("{}", sector.special);
    ctl.tag = format!("{}", sector.tag);
    ctl.floor_flat = sector.floor_flat.to_dwd_field().to_owned();
    ctl.ceil_flat = sector.ceil_flat.to_dwd_field().to_owned();
}

/// Read panel fields into a `Sector`, falling back to `fallback` per field.
pub fn sector_from_panel(ui: &MapPresentation, fallback: Sector) -> Sector {
    let ctl = &ui.sector;
    Sector {
        floor_height: parse_or(&ctl.floor_h, fallback.floor_height),
        floor_flat: name_or(&ctl.floor_flat, fallback.floor_flat),
        ceil_height: parse_or(&ctl.ceil_h, fallback.ceil_height),
        ceil_flat: name_or(&ctl.ceil_flat, fallback.ceil_flat),
        light_level: parse_or(&ctl.light, fallback.light_level),
        special: parse_or(&ctl.special, fallback.special),
        tag: parse_or(&ctl.tag, fallback.tag),
    }
}

pub(crate) fn line_apply(ui: &MapPresentation, state: &mut EditorSession) -> Damage {
    let edit = LineEdit::read(&ui.line);
    let indices = state.map_editor.selected_lines();
    if indices.is_empty() {
        return Damage::None;
    }
    state
        .map_editor
        .apply_lines(&indices, |old| edit.apply_to(old))
}

pub(crate) fn line_flip(state: &mut EditorSession) -> Damage {
    let indices = state.map_editor.selected_lines();
    if indices.is_empty() {
        return Damage::None;
    }
    state.map_editor.flip_selected_lines(&indices)
}

pub(crate) fn thing_apply(ui: &MapPresentation, state: &mut EditorSession) -> Damage {
    let edit = ThingEdit::read(&ui.thing);
    let indices = state.map_editor.selected_things();
    if indices.is_empty() {
        // THING tool + no selection → edit the placement template.
        let template = &mut state.map_editor.thing_template;
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
        state
            .map_editor
            .apply_things(&indices, |old| edit.apply_to(old))
    }
}

pub(crate) fn sector_apply(ui: &MapPresentation, state: &mut EditorSession) -> Damage {
    let Some(index) = state.map_editor.current_sector else {
        return Damage::None;
    };
    let Some(old) = state
        .map_editor
        .map
        .as_ref()
        .and_then(|m| m.sectors.get(index))
        .copied()
    else {
        return Damage::None;
    };
    let new = sector_from_panel(ui, old);
    state.map_editor.apply_sector(index, new)
}

pub(crate) fn sector_new_sector(ui: &mut MapPresentation, state: &mut EditorSession) {
    {
        let state = &mut *state;
        let base = default_sector();
        let new = sector_from_panel(ui, base);
        state.map_editor.new_sector(new);
    }
    sync(ui, state);
}
