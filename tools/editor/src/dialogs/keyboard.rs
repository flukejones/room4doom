//! Modal focus traversal and direct feature keyboard commands.

use super::{App, Context, DialogsView, KeyDownEvent, PopupId, Window, flush_input_registry};

impl DialogsView {
    pub(crate) fn handle_key(
        &mut self,
        event: &KeyDownEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(popup) = self.active_popup() else {
            return false;
        };
        match event.keystroke.key.as_str() {
            "tab" => {
                self.tab_control(event.keystroke.modifiers.shift, window, cx);
                true
            }
            "escape" => {
                let inputs = self.input_registry();
                let entity = cx.entity();
                window.defer(cx, move |window, cx| {
                    flush_input_registry(&inputs, window, cx);
                    entity.update(cx, |this, cx| this.close_popup(cx));
                });
                true
            }
            _ if self.text_input_focused(window, cx) => false,
            key if popup == PopupId::WallEdit => {
                let key = match key {
                    "up" => "\u{f700}",
                    "down" => "\u{f701}",
                    "left" => "\u{f702}",
                    "right" => "\u{f703}",
                    _ => return false,
                };
                let damage = super::wall::actions::key(
                    &mut self.drafts,
                    &mut self.shared.borrow_mut(),
                    key,
                    event.keystroke.modifiers.shift,
                );
                Self::emit_damage(damage, cx);
                true
            }
            _ => false,
        }
    }

    pub(crate) fn tab_control(&self, backwards: bool, window: &Window, cx: &mut App) {
        let inputs = self.input_registry();
        let focus = self.popup_focus.clone();
        window.defer(cx, move |window, cx| {
            flush_input_registry(&inputs, window, cx);
            let mut first = None;
            loop {
                if backwards {
                    window.focus_prev(cx);
                } else {
                    window.focus_next(cx);
                }
                let Some(current) = window.focused(cx) else {
                    return;
                };
                if focus.contains_focused(window, cx) {
                    return;
                }
                if first.as_ref() == Some(&current) {
                    return;
                }
                first.get_or_insert(current);
            }
        });
    }
}
