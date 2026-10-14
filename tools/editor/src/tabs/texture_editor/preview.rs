//! Composite preview and patch hit testing in texture coordinates.

use std::sync::Arc;

use gpui::{
    AnyElement, Context, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit,
    Pixels, Point, RenderImage, canvas, div, img, prelude::*, px,
};

use super::{PatchDrag, TextureEditor};
use crate::{assets::patch_dims, gfx::compose_texture_highlight};

impl TextureEditor {
    #[expect(
        clippy::cast_precision_loss,
        reason = "Texture dimensions fit the preview's logical pixel coordinates."
    )]
    pub(super) fn fit_preview(&mut self) {
        let state = self.shared.borrow();
        let Some(def) = self
            .current
            .and_then(|i| state.assets.as_ref()?.textures().get(i))
        else {
            self.preview_size = [0.0; 2];
            return;
        };
        let width = def.width.max(1) as f32;
        let height = def.height.max(1) as f32;
        let available = self.preview_bounds.size;
        self.zoom = ((f32::from(available.width) - 16.0).max(1.0) / width)
            .min((f32::from(available.height) - 16.0).max(1.0) / height)
            .clamp(1.0, 8.0)
            .floor();
        self.preview_size = [width * self.zoom, height * self.zoom];
    }

    pub(super) fn refresh_preview(&mut self, cx: &mut Context<Self>) {
        let image = {
            let state = self.shared.borrow();
            self.current.and_then(|index| {
                let assets = state.assets.as_ref()?;
                let wad = state.wad_data.as_ref()?;
                let def = assets.textures().get(index)?;
                let colormap = self.colormap.and_then(|level| assets.colormap(level));
                let image = compose_texture_highlight(
                    def,
                    assets.imported_patches(),
                    wad,
                    assets.palette(),
                    self.selected_patch,
                    colormap.map(|c| c.as_slice()),
                );
                let mut bytes = image.into_bytes();
                for pixel in bytes.as_chunks_mut::<4>().0 {
                    pixel.swap(0, 2);
                }
                let width = u32::try_from(def.width.max(1)).ok()?;
                let height = u32::try_from(def.height.max(1)).ok()?;
                let pixels = image::RgbaImage::from_raw(width, height, bytes)?;
                Some(Arc::new(RenderImage::new(vec![image::Frame::new(pixels)])))
            })
        };
        if let Some(previous) = std::mem::replace(&mut self.preview, image) {
            self.retired_images.push(previous);
        }
        cx.notify();
    }

    pub(super) fn retire_previews(&mut self, window: &mut gpui::Window) {
        for image in self.retired_images.drain(..) {
            if let Err(error) = window.drop_image(image) {
                log::warn!("Texture preview eviction: {error}");
            }
        }
    }

    pub(super) fn preview_view(&self, cx: &Context<Self>) -> AnyElement {
        let weak = cx.weak_entity();
        let measure = canvas(
            move |bounds, _, cx| {
                weak.update(cx, |this, cx| {
                    if this.preview_bounds != bounds {
                        this.preview_bounds = bounds;
                        this.fit_preview();
                        cx.notify();
                    }
                })
                .ok();
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        div()
            .id("texture-preview")
            .relative()
            .w_full()
            .h(px(320.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .overflow_hidden()
            .child(measure)
            .children(self.preview.as_ref().map(|image| {
                img(image.clone())
                    .w(px(self.preview_size[0]))
                    .h(px(self.preview_size[1]))
                    .object_fit(ObjectFit::Contain)
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    this.flush_inputs(window, cx);
                    this.focus(window, cx);
                    this.preview_press(event.position, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if this.drag.is_some() {
                    this.preview_move(event.position, cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| {
                    this.cancel_drag(cx);
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| {
                    this.cancel_drag(cx);
                }),
            )
            .into_any_element()
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "A mouse position is rounded down to a texture texel."
    )]
    fn preview_texel(&self, position: Point<Pixels>) -> [i32; 2] {
        let local = [
            f32::from(position.x - self.preview_bounds.origin.x)
                - (f32::from(self.preview_bounds.size.width) - self.preview_size[0]) * 0.5,
            f32::from(position.y - self.preview_bounds.origin.y)
                - (f32::from(self.preview_bounds.size.height) - self.preview_size[1]) * 0.5,
        ];
        [
            (local[0] / self.zoom).floor() as i32,
            (local[1] / self.zoom).floor() as i32,
        ]
    }

    fn preview_press(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let texel = self.preview_texel(position);
        let hit = {
            let state = self.shared.borrow();
            self.current.and_then(|index| {
                let assets = state.assets.as_ref()?;
                let wad = state.wad_data.as_ref()?;
                let def = assets.textures().get(index)?;
                def.patches
                    .iter()
                    .enumerate()
                    .rev()
                    .find_map(|(index, patch)| {
                        let (width, height) =
                            patch_dims(assets.imported_patches(), wad, patch.patch.as_str())?;
                        let width = i32::try_from(width).ok()?;
                        let height = i32::try_from(height).ok()?;
                        (texel[0] >= patch.origin_x
                            && texel[0] < patch.origin_x + width
                            && texel[1] >= patch.origin_y
                            && texel[1] < patch.origin_y + height)
                            .then(|| {
                                (
                                    index,
                                    [texel[0] - patch.origin_x, texel[1] - patch.origin_y],
                                    patch.patch.as_str().to_owned(),
                                )
                            })
                    })
            })
        };
        if let Some((patch, grab, name)) = hit {
            self.selected_patch = Some(patch);
            self.new_patch = name;
            self.drag = Some(PatchDrag {
                patch,
                grab,
                recorded: false,
            });
        } else {
            self.selected_patch = None;
            self.drag = None;
        }
        self.refresh_preview(cx);
    }

    fn preview_move(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let texel = self.preview_texel(position);
        let Some(drag) = &mut self.drag else { return };
        let (patch, grab) = (drag.patch, drag.grab);
        {
            let mut state = self.shared.borrow_mut();
            let Some(index) = self.current else { return };
            let Some(assets) = state.assets.as_mut() else {
                return;
            };
            let Some(placement) = assets
                .textures()
                .get(index)
                .and_then(|def| def.patches.get(patch))
            else {
                return;
            };
            let origin = [texel[0] - grab[0], texel[1] - grab[1]];
            if [placement.origin_x, placement.origin_y] == origin {
                return;
            }
            if !drag.recorded {
                self.history.record(assets);
                drag.recorded = true;
            }
            let placement = &mut assets
                .texture_mut(index)
                .expect("texture checked above")
                .patches[patch];
            placement.origin_x = origin[0];
            placement.origin_y = origin[1];
            state.assets_dirty = true;
        }
        self.refresh_preview(cx);
    }
}
