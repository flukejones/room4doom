//! Shared native text fields, widget focus and preview-image lifetimes.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    AnyElement, App, AppContext as _, Context, Div, Entity, FocusHandle, Focusable as _,
    FontWeight, Hsla, ObjectFit, RenderImage, SharedString, Window, div, img, prelude::*, px,
};

use super::controls::ChoiceMenus;
use crate::components::types::{FlatSlot, PopupId, TexSlot};
use crate::gfx::image::UiImage;
use editor_core::SectorKey;
use ui_common::ActiveTheme as _;

/// Identity of the editing target; replacing it resets each field's local text history.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum FieldOwner {
    Dialog(PopupId),
    Wall {
        document: u64,
        line: i32,
        side: i32,
        slot: TexSlot,
    },
    Sector {
        document: u64,
        sector: i32,
        slot: FlatSlot,
    },
    Selection {
        document: u64,
        revision: u64,
        sector: Option<SectorKey>,
    },
}

pub(crate) struct FormState {
    pub(crate) background_opacity: f32,
    pub(crate) focus: FocusHandle,
    pub(crate) owner: FieldOwner,
    pub(crate) inputs: Rc<RefCell<HashMap<String, Entity<ui_common::TextInput>>>>,
    bindings: HashMap<String, FieldBinding>,
    pub(crate) choice_menus: ChoiceMenus,
    pub(super) selected_foreground: Option<Hsla>,
    images: HashMap<String, (UiImage, Arc<RenderImage>)>,
    live_images: HashSet<String>,
    previous_images: HashSet<String>,
    retired_images: Vec<Arc<RenderImage>>,
}

type ChangedCallback = Rc<dyn Fn(&String, &mut Window, &mut App)>;
type CommittedCallback = Rc<dyn Fn(&(), &mut Window, &mut App)>;

struct FieldCallbacks {
    changed: ChangedCallback,
    committed: CommittedCallback,
    commit_on_blur: bool,
}

struct FieldBinding {
    owner: FieldOwner,
    callbacks: Rc<RefCell<FieldCallbacks>>,
}

impl FormState {
    pub(crate) fn new<T: 'static>(cx: &Context<T>) -> Self {
        Self {
            background_opacity: 1.0,
            focus: cx.focus_handle(),
            owner: FieldOwner::Selection {
                document: 0,
                revision: 0,
                sector: None,
            },
            inputs: Rc::new(RefCell::new(HashMap::new())),
            bindings: HashMap::new(),
            choice_menus: ChoiceMenus::default(),
            selected_foreground: None,
            images: HashMap::new(),
            live_images: HashSet::new(),
            previous_images: HashSet::new(),
            retired_images: Vec::new(),
        }
    }

    pub(crate) fn text_input_focused(&self, window: &Window, cx: &App) -> bool {
        self.inputs
            .borrow()
            .values()
            .any(|input| input.read(cx).focus_handle(cx).is_focused(window))
    }

    pub(crate) fn panel(&self, cx: &App) -> Div {
        let colors = cx.theme();
        let mut background = colors.colors().background;
        background.a = self.background_opacity.clamp(0., 1.);
        div()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .bg(background)
            .text_color(colors.colors().text)
    }

    /// Retains callbacks for the field ID and owner; listeners must read current
    /// entity state or capture a stable field key, rather than a changing value.
    pub(crate) fn field(
        &mut self,
        id: impl AsRef<str>,
        label: impl Into<SharedString>,
        value: impl AsRef<str>,
        changed: impl Fn(&String, &mut Window, &mut App) + 'static,
        committed: impl Fn(&(), &mut Window, &mut App) + 'static,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        self.input_field(id, label, value, changed, committed, true, window, cx)
    }

    pub(crate) fn action_field(
        &mut self,
        id: impl AsRef<str>,
        label: impl Into<SharedString>,
        value: impl AsRef<str>,
        changed: impl Fn(&String, &mut Window, &mut App) + 'static,
        accepted: impl Fn(&(), &mut Window, &mut App) + 'static,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        self.input_field(id, label, value, changed, accepted, false, window, cx)
    }

    /// Callbacks belong to the field ID and owner. Capture stable field keys and
    /// read changing values from the owning entity when the event is delivered.
    pub(crate) fn input_field(
        &mut self,
        id: impl AsRef<str>,
        label: impl Into<SharedString>,
        value: impl AsRef<str>,
        changed: impl Fn(&String, &mut Window, &mut App) + 'static,
        committed: impl Fn(&(), &mut Window, &mut App) + 'static,
        commit_on_blur: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let id = id.as_ref();
        let make_callbacks = || FieldCallbacks {
            changed: Rc::new(changed),
            committed: Rc::new(committed),
            commit_on_blur,
        };
        let (callbacks, owner_changed) = if let Some(binding) = self.bindings.get_mut(id) {
            let owner_changed = binding.owner != self.owner;
            binding.owner = self.owner;
            if owner_changed
                || binding.callbacks.borrow().commit_on_blur != commit_on_blur
                || !self.inputs.borrow().contains_key(id)
            {
                binding.callbacks.replace(make_callbacks());
            }
            (binding.callbacks.clone(), owner_changed)
        } else {
            let callbacks = Rc::new(RefCell::new(make_callbacks()));
            self.bindings.insert(
                id.to_owned(),
                FieldBinding {
                    owner: self.owner,
                    callbacks: callbacks.clone(),
                },
            );
            (callbacks, false)
        };
        let focus = self.focus.clone();
        let existing = self.inputs.borrow().get(id).cloned();
        let input = existing.unwrap_or_else(|| {
            let input = cx.new(|cx| {
                let changed = callbacks.clone();
                ui_common::TextInput::new(value.as_ref().to_owned(), window, cx)
                    .on_change(move |value, window, cx| {
                        let callback = changed.borrow().changed.clone();
                        callback(&value.to_owned(), window, cx);
                    })
                    .on_commit(move |reason, window, cx| {
                        let callback = {
                            let callbacks = callbacks.borrow();
                            (reason == ui_common::InputCommit::Submit || callbacks.commit_on_blur)
                                .then(|| callbacks.committed.clone())
                        };
                        if let Some(callback) = callback {
                            callback(&(), window, cx);
                        }
                        if reason == ui_common::InputCommit::Submit {
                            window.focus(&focus, cx);
                        }
                    })
            });
            self.inputs
                .borrow_mut()
                .insert(id.to_owned(), input.clone());
            input
        });
        input.update(cx, |input, cx| {
            if owner_changed {
                input.reset_history(cx);
            }
            input.set_text(value.as_ref(), window, cx);
        });
        let label = label.into();
        div()
            .flex()
            .items_center()
            .gap_2()
            .when(!label.is_empty(), |row| {
                row.child(div().w(px(110.)).flex_shrink_0().child(label))
            })
            .child(div().flex_1().min_w_0().child(input))
            .into_any_element()
    }

    pub(crate) fn image(&mut self, id: impl Into<String>, source: UiImage) -> Option<AnyElement> {
        if source.width() == 0 || source.height() == 0 {
            return None;
        }
        let id = id.into();
        self.live_images.insert(id.clone());
        let stale = self.images.get(&id).is_none_or(|(old, _)| old != &source);
        if stale {
            let mut pixels = source.as_bytes().to_vec();
            for pixel in pixels.as_chunks_mut::<4>().0 {
                pixel.swap(0, 2);
            }
            let image = image::RgbaImage::from_raw(source.width(), source.height(), pixels)?;
            if let Some((_, old)) = self.images.insert(
                id.clone(),
                (
                    source,
                    Arc::new(RenderImage::new(vec![image::Frame::new(image)])),
                ),
            ) {
                self.retired_images.push(old);
            }
        }
        Some(
            img(self.images[&id].1.clone())
                .size_full()
                .object_fit(ObjectFit::Contain)
                .into_any_element(),
        )
    }

    pub(crate) fn prepare_frame(&mut self, window: &mut Window, cx: &App) {
        self.selected_foreground = Some(ui_common::accent_foreground(cx));
        self.images.retain(|id, (_, image)| {
            if self.live_images.contains(id) || self.previous_images.contains(id) {
                true
            } else {
                self.retired_images.push(image.clone());
                false
            }
        });
        for image in self.retired_images.drain(..) {
            if let Err(error) = window.drop_image(image) {
                log::error!("Editor preview eviction failed: {error}");
            }
        }
        std::mem::swap(&mut self.live_images, &mut self.previous_images);
        self.live_images.clear();
    }
}

pub(crate) fn flush_input_registry(
    inputs: &Rc<RefCell<HashMap<String, Entity<ui_common::TextInput>>>>,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let focused = inputs
        .borrow()
        .values()
        .find(|input| input.read(cx).focus_handle(cx).is_focused(window))
        .cloned();
    if let Some(input) = focused {
        input.update(cx, |input, cx| input.commit_blur(window, cx));
        true
    } else {
        false
    }
}

pub(crate) fn row_index(value: usize) -> i32 {
    i32::try_from(value).expect("GUI list exceeds i32 entries")
}

pub(crate) fn heading(text: impl Into<SharedString>) -> Div {
    div()
        .font_weight(FontWeight::SEMIBOLD)
        .mt_2()
        .child(text.into())
}

pub(crate) fn labels<const N: usize>(values: &[&'static str; N]) -> [SharedString; N] {
    values.map(SharedString::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::input::{SelItem, Selection};
    use gpui::{Render, TestAppContext, size};

    struct FieldHost {
        forms: FormState,
        selection: Selection,
        value: Rc<RefCell<String>>,
    }

    impl Render for FieldHost {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.forms.owner = FieldOwner::Selection {
                document: 0,
                revision: self.selection.revision(),
                sector: None,
            };
            let value = self.value.borrow().clone();
            let draft = self.value.clone();
            self.forms.field(
                "value",
                "Value",
                value,
                move |value, _, _| {
                    draft.borrow_mut().clone_from(value);
                },
                |_, _, _| {},
                window,
                cx,
            )
        }
    }

    #[gpui::test]
    fn field_history_stays_with_the_edited_selection(cx: &mut TestAppContext) {
        cx.update(|cx| ui_common::init("editor test", cx));
        let wad = wad::WadData::new(&test_utils::doom1_wad_path());
        let map = editor_core::import_wad_map(&wad, "E1M1").unwrap();
        let mut lines = map.lines.keys();
        let first = lines.next().unwrap();
        let second = lines.next().unwrap();
        let window = cx.open_window(size(px(300.), px(100.)), |_, cx| {
            let mut selection = Selection::default();
            selection.push(SelItem::Line(first));
            FieldHost {
                forms: FormState::new(cx),
                selection,
                value: Rc::new(RefCell::new("old".into())),
            }
        });
        let input = window
            .update(cx, |host, window, cx| {
                let input = host.forms.inputs.borrow()["value"].clone();
                window.focus(&input.read(cx).focus_handle(cx), cx);
                input
            })
            .unwrap();
        cx.dispatch_action(window.into(), ui_common::SelectAll);
        cx.simulate_input(window.into(), "shared");
        window
            .update(cx, |host, _, cx| {
                // Both objects have the same displayed value; only target identity changes.
                host.selection.replace(SelItem::Line(second));
                cx.notify();
            })
            .unwrap();
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        cx.dispatch_action(window.into(), ui_common::Undo);
        window
            .update(cx, |_, _, cx| {
                assert_eq!(
                    input.read(cx).text(),
                    "shared",
                    "Undo must not restore the previous object's field text"
                );
            })
            .unwrap();
    }
}
