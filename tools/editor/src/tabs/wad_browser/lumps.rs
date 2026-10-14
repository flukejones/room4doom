//! WAD content classification, filtering, selection and activation.

use crate::assets::EditorAssets;
use crate::gfx::GfxCache;
use editor_core::Name8;
use gpui::{
    Anchor, AnyElement, Context, ScrollStrategy, Window, div, point, prelude::*, px, uniform_list,
};
use ui_common::widgets::{
    Color, ContextMenu, DropdownMenu, IconPosition, Label, ListItem, prelude::*,
};
use wad::WadData;

use super::{LoadedWad, MapSource, WadBrowser, WadBrowserEvent};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum LumpKind {
    Map,
    MapData,
    Flat,
    Patch,
    Sprite,
    Music,
    Sound,
    Textures,
    Palette,
    Colourmap,
    Text,
    Other,
}

const KINDS: [(Option<LumpKind>, &str); 13] = [
    (None, "All"),
    (Some(LumpKind::Map), "Maps"),
    (Some(LumpKind::MapData), "Map lumps"),
    (Some(LumpKind::Flat), "Flats"),
    (Some(LumpKind::Patch), "Patches"),
    (Some(LumpKind::Sprite), "Sprites"),
    (Some(LumpKind::Music), "Music"),
    (Some(LumpKind::Sound), "Sounds"),
    (Some(LumpKind::Textures), "Textures"),
    (Some(LumpKind::Palette), "Palette"),
    (Some(LumpKind::Colourmap), "Colourmap"),
    (Some(LumpKind::Text), "Text"),
    (Some(LumpKind::Other), "Other"),
];

pub(super) struct LumpRow {
    name: String,
    size: usize,
    kind: LumpKind,
}

impl WadBrowser {
    pub(super) fn filter_lumps(&mut self) {
        self.visible_lumps = self
            .lumps
            .iter()
            .enumerate()
            .filter_map(|(index, row)| {
                (self.kind_filter.is_none_or(|kind| kind == row.kind)
                    && row.name.to_uppercase().contains(&self.name_filter))
                .then_some(index)
            })
            .collect();
        self.reveal_lump();
    }

    fn reveal_lump(&self) {
        if let Some(index) = self
            .selected_lump
            .and_then(|selected| self.visible_lumps.iter().position(|&i| i == selected))
        {
            self.lump_scroll
                .scroll_to_item(index, ScrollStrategy::Nearest);
        }
    }

    pub(super) fn select_lump(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(wad_index) = self.selected_wad else {
            return;
        };
        let Some(row) = self.lumps.get(index) else {
            return;
        };
        self.selected_lump = Some(index);
        self.selected_map = None;
        self.preview.clear();
        self.preview.title = row.name.clone();
        let name = row.name.clone();
        let kind = row.kind;
        let size = row.size;
        if kind == LumpKind::Map {
            self.selected_map = Some(MapSource::Wad(wad_index, name.clone()));
            self.preview.wireframe = self.map_wireframe(wad_index, &name);
        } else if matches!(kind, LumpKind::Flat | LumpKind::Patch | LumpKind::Sprite) {
            let loaded = &mut self.wads[wad_index];
            let LoadedWad {
                name: wad_name,
                path,
                wad,
                assets,
                gfx,
                ..
            } = loaded;
            let assets = assets.get_or_insert_with(|| {
                let mut assets = EditorAssets::load(std::slice::from_ref(path), wad, None);
                assets.set_map_wad(wad_name);
                assets
            });
            let gfx = gfx.get_or_insert_with(|| GfxCache::new(assets));
            let image = if kind == LumpKind::Flat {
                Name8::new(&name)
                    .ok()
                    .and_then(|name| assets.iwad_flat_num(&name))
                    .map(|num| gfx.flat_image(assets, num))
            } else {
                gfx.patch_image(assets, wad, &name)
            };
            if let Some(image) = image {
                self.preview.set_image(&image);
            } else {
                self.preview.text = format!("Unable to decode {name} ({size} bytes)");
            }
        } else if kind == LumpKind::Text {
            self.preview.text =
                String::from_utf8_lossy(&self.wads[wad_index].wad.lumps()[index].data).into_owned();
        } else {
            self.preview.text = format!("{size} bytes");
        }
        self.reveal_lump();
        cx.notify();
    }

    pub(super) fn activate_lump(&mut self, index: usize, cx: &mut Context<Self>) {
        self.select_lump(index, cx);
        if self
            .lumps
            .get(index)
            .is_some_and(|row| row.kind == LumpKind::Textures)
        {
            if let Some(wad_index) = self.selected_wad
                && let Ok(lump) = Name8::new(&self.lumps[index].name)
            {
                cx.emit(WadBrowserEvent::OpenTextureSet {
                    wad_name: self.wads[wad_index].name.clone(),
                    lump,
                });
            }
        } else {
            self.open_map(cx);
        }
    }

    pub(super) fn lump_column(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self.kind_filter;
        let label = KINDS
            .iter()
            .find(|(kind, _)| *kind == selected)
            .map_or("All", |(_, label)| *label);
        let menu = self
            .kind_menu
            .get_or_insert_with(|| {
                let weak = cx.weak_entity();
                ContextMenu::build(window, cx, move |mut menu, _, _| {
                    for (kind, label) in KINDS {
                        let weak = weak.clone();
                        menu = menu.toggleable_entry(
                            label,
                            kind == selected,
                            IconPosition::Start,
                            None,
                            move |window, cx| {
                                let _ = weak.update(cx, |this, cx| {
                                    this.kind_filter = kind;
                                    this.kind_menu = None;
                                    this.filter_lumps();
                                    this.focus(window, cx);
                                    cx.notify();
                                });
                            },
                        );
                    }
                    menu
                })
            })
            .clone();
        div()
            .flex()
            .flex_col()
            .h_full()
            .min_h_0()
            .p_3()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(Label::new("Filter"))
                    .child(div().flex_1().min_w_0().child(self.filter_input.clone())),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(Label::new("Kind"))
                    .child(
                        DropdownMenu::new("lump-kind", label, menu)
                            .attach(Anchor::BottomLeft)
                            .offset(point(px(0.), px(0.))),
                    ),
            )
            .child(
                uniform_list(
                    "lump-list",
                    self.visible_lumps.len(),
                    cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                        range
                            .filter_map(|row| this.visible_lumps.get(row))
                            .map(|&index| this.lump_row(index, cx))
                            .collect()
                    }),
                )
                .flex_1()
                .min_h_0()
                .track_scroll(&self.lump_scroll),
            )
            .into_any_element()
    }

    fn lump_row(&self, index: usize, cx: &Context<Self>) -> AnyElement {
        let row = &self.lumps[index];
        ListItem::new(("lump", index))
            .height(px(32.))
            .toggle_state(self.selected_lump == Some(index))
            .aria_label(row.name.clone())
            .child(Label::new(row.name.clone()).truncate())
            .end_slot(Label::new(format!("{} B", row.size)).color(Color::Muted))
            .on_click(
                cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                    this.focus(window, cx);
                    this.lumps_focused = true;
                    if event.click_count() >= 2 {
                        this.activate_lump(index, cx);
                    } else {
                        this.select_lump(index, cx);
                    }
                }),
            )
            .into_any_element()
    }
}

pub(super) fn rows(wad: &WadData) -> Vec<LumpRow> {
    let mut namespace = None;
    wad.lumps()
        .iter()
        .map(|lump| {
            let name = lump.name.as_str();
            let kind = match name {
                "PLAYPAL" => LumpKind::Palette,
                "COLORMAP" => LumpKind::Colourmap,
                "TEXTURE1" | "TEXTURE2" => LumpKind::Textures,
                "F_START" | "FF_START" => {
                    namespace = Some(LumpKind::Flat);
                    LumpKind::Other
                }
                "P_START" | "PP_START" | "P1_START" | "P2_START" | "P3_START" => {
                    namespace = Some(LumpKind::Patch);
                    LumpKind::Other
                }
                "S_START" | "SS_START" => {
                    namespace = Some(LumpKind::Sprite);
                    LumpKind::Other
                }
                "F_END" | "FF_END" | "P_END" | "PP_END" | "P1_END" | "P2_END" | "P3_END"
                | "S_END" | "SS_END" => {
                    namespace = None;
                    LumpKind::Other
                }
                "THINGS" | "LINEDEFS" | "SIDEDEFS" | "VERTEXES" | "SEGS" | "SSECTORS" | "NODES"
                | "SECTORS" | "REJECT" | "BLOCKMAP" | "BEHAVIOR" => LumpKind::MapData,
                _ if is_map_marker(name) => LumpKind::Map,
                _ if namespace.is_some() => namespace.unwrap_or(LumpKind::Other),
                _ if name.starts_with("D_") && lump.data.starts_with(b"MUS\x1a") => LumpKind::Music,
                _ if (name.starts_with("DS") && lump.data.starts_with(&[3, 0]))
                    || name.starts_with("DP") =>
                {
                    LumpKind::Sound
                }
                _ if !lump.data.is_empty()
                    && lump
                        .data
                        .iter()
                        .all(|byte| byte.is_ascii_graphic() || byte.is_ascii_whitespace()) =>
                {
                    LumpKind::Text
                }
                _ => LumpKind::Other,
            };
            LumpRow {
                name: name.to_owned(),
                size: lump.data.len(),
                kind,
            }
        })
        .collect()
}

fn is_map_marker(name: &str) -> bool {
    let bytes = name.as_bytes();
    (bytes.len() == 4
        && bytes[0] == b'E'
        && bytes[1].is_ascii_digit()
        && bytes[2] == b'M'
        && bytes[3].is_ascii_digit())
        || (bytes.len() == 5
            && &bytes[..3] == b"MAP"
            && bytes[3].is_ascii_digit()
            && bytes[4].is_ascii_digit())
}
