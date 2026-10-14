//! Wall elevation, texture bands and offset editing.

pub(crate) mod actions;

use super::*;
use crate::components::types::{BandRect, GfxEntry, TexSlot};
use crate::gfx::image::UiImage;
use gpui::{AvailableSpace, Bounds, canvas, point, size};

#[derive(Clone)]
pub(crate) struct WallDraft {
    pub(crate) line_index: i32,
    pub(crate) side_tab: i32,
    pub(crate) has_back: bool,
    pub(crate) wall_img: UiImage,
    pub(crate) wall_w: f32,
    pub(crate) wall_h: f32,
    pub(crate) bands: Vec<BandRect>,
    pub(crate) selected_slot: TexSlot,
    pub(crate) textures: Vec<GfxEntry>,
    pub(crate) active_index: i32,
    pub(crate) xoff: i32,
    pub(crate) yoff: i32,
    pub(crate) floor_h: i32,
    pub(crate) ceil_h: i32,
}

impl Default for WallDraft {
    fn default() -> Self {
        Self {
            line_index: -1,
            side_tab: 0,
            has_back: false,
            wall_img: Default::default(),
            wall_w: Default::default(),
            wall_h: Default::default(),
            bands: Default::default(),
            selected_slot: Default::default(),
            textures: Default::default(),
            active_index: -1,
            xoff: Default::default(),
            yoff: Default::default(),
            floor_h: Default::default(),
            ceil_h: Default::default(),
        }
    }
}

impl DialogsView {
    pub(super) fn wall_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let draft = &self.drafts.wall;
        let side = cx.listener(|this, index: &usize, _, cx| {
            this.drafts.wall.side_tab = row_index(*index);
            actions::side_changed(&mut this.drafts, &this.shared.borrow());
            cx.notify();
        });
        let tabs = if draft.has_back {
            self.forms.segmented(
                "wall-side",
                ["Front", "Back"],
                usize::try_from(draft.side_tab).unwrap_or(0),
                side,
            )
        } else {
            self.forms.segmented("wall-side", ["Front"], 0, side)
        };
        let mut fields = div().flex().flex_col().flex_none().gap_2();
        macro_rules! field {
            ($id:literal,$label:literal,$field:ident,$command:ident) => {
                fields = fields.child(self.forms.field(
                    $id,
                    $label,
                    draft.$field.to_string(),
                    cx.listener(|this, value: &String, _, cx| {
                        if let Ok(n) = value.parse() {
                            this.drafts.wall.$field = n;
                            let damage =
                                actions::$command(&mut this.drafts, &mut this.shared.borrow_mut());
                            Self::emit_damage(damage, cx);
                        }
                    }),
                    |_, _, _| {},
                    window,
                    cx,
                ));
            };
        }
        field!("wall-xoff", "Offset X", xoff, offset_changed);
        field!("wall-yoff", "Offset Y", yoff, offset_changed);
        field!("wall-floor", "Floor height", floor_h, height_changed);
        field!("wall-ceil", "Ceiling height", ceil_h, height_changed);

        let width = self.popup_geom(PopupId::WallEdit).w - 24.;
        let preview_width = ((width - 12.) * 0.45).min(360.);
        let grid = self.asset_grid(
            "wall",
            |this| &this.drafts.wall.textures,
            self.drafts.wall.active_index,
            width - preview_width - 12.,
            cx,
            Self::pick_wall_texture,
        );
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap_2()
            .child(div().flex_none().child(tabs))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_shrink_0()
                            .w(px(preview_width))
                            .min_h_0()
                            .gap_2()
                            .child(
                                canvas(
                                    cx.processor(|this, bounds: Bounds<Pixels>, window, cx| {
                                        this.wall_preview(bounds, window, cx)
                                    }),
                                    |_, mut preview, window, cx| preview.paint(window, cx),
                                )
                                .flex_1()
                                .min_h_0()
                                .w_full(),
                            )
                            .child(fields),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .child(grid),
                    ),
            )
            .into_any_element()
    }

    fn wall_preview(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let draft = &self.drafts.wall;
        let width = draft.wall_w.max(1.);
        let height = draft.wall_h.max(1.);
        let scale = (f32::from(bounds.size.width) / width)
            .min(f32::from(bounds.size.height) / height)
            .clamp(0., 1.);
        let image = self.forms.image("wall-editor", draft.wall_img.clone());
        let mut preview = div()
            .debug_selector(|| "wall-preview".into())
            .relative()
            .w(px(width * scale))
            .h(px(height * scale))
            .children(image);
        for (i, band) in draft.bands.iter().enumerate() {
            let slot = band.slot;
            let inputs = self.forms.inputs.clone();
            let focus = self.forms.focus.clone();
            let callback = cx.listener(move |this, _, _, cx| {
                actions::select_band(&mut this.drafts, slot);
                cx.notify();
            });
            preview = preview.child(
                div()
                    .absolute()
                    .top(px(band.y * scale))
                    .left_0()
                    .w_full()
                    .h(px(band.h * scale))
                    .child(
                        Button::new(("wall-band", i), "")
                            .tab_index(0_isize)
                            .full_width()
                            .height(px(band.h * scale))
                            .style(ButtonStyle::Transparent)
                            .selected_style(ButtonStyle::Transparent)
                            .toggle_state(slot == draft.selected_slot)
                            .aria_label(format!("{slot:?} wall texture"))
                            .on_click(move |event, window, cx| {
                                if flush_input_registry(&inputs, window, cx) {
                                    window.focus(&focus, cx);
                                }
                                callback(event, window, cx);
                            }),
                    )
                    .when(slot == draft.selected_slot, |band| {
                        band.child(
                            div()
                                .absolute()
                                .inset_0()
                                .border_2()
                                .border_color(cx.theme().colors().border_focused),
                        )
                    }),
            );
        }
        // Paint boundaries last, above both the textures and the band controls.
        for band in draft.bands.iter().skip(1) {
            preview = preview.child(
                div()
                    .absolute()
                    .top(px(band.y * scale - 1.))
                    .left_0()
                    .w_full()
                    .h(px(3.))
                    .bg(cx.theme().colors().text)
                    .child(div().w_full().h(px(1.)).bg(cx.theme().colors().background)),
            );
        }
        let mut preview = preview.into_any_element();
        let extent = preview.layout_as_root(
            size(
                AvailableSpace::Definite(bounds.size.width),
                AvailableSpace::Definite(bounds.size.height),
            ),
            window,
            cx,
        );
        preview.prepaint_at(
            point(
                bounds.left() + (bounds.size.width - extent.width) / 2.,
                bounds.top(),
            ),
            window,
            cx,
        );
        preview
    }

    fn pick_wall_texture(&mut self, name: &str, cx: &mut Context<Self>) {
        let damage = actions::pick_texture(&mut self.drafts, &mut self.shared.borrow_mut(), name);
        Self::emit_damage(damage, cx);
    }
}
