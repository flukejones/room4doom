//! Bounded integer editing with shared text input, buttons and theme colours.

use std::ops::RangeInclusive;

use crate::widgets::{Button, ButtonStyle, prelude::*};
use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, KeyDownEvent, Render, Role,
    SharedString, Window, div, prelude::*, px,
};

use crate::TextInput;

pub struct NumberInputChanged(pub i32);

pub struct NumberInput {
    label: SharedString,
    input: Entity<TextInput>,
    value: i32,
    value_text: SharedString,
    draft: Option<i32>,
    bounds: RangeInclusive<i32>,
}

impl EventEmitter<NumberInputChanged> for NumberInput {}

impl NumberInput {
    pub fn new(
        label: impl Into<SharedString>,
        value: i32,
        bounds: RangeInclusive<i32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        assert!(!bounds.is_empty(), "number input needs an ordered range");
        let value = value.clamp(*bounds.start(), *bounds.end());
        let value_text: SharedString = value.to_string().into();
        let changed = cx.weak_entity();
        let committed = changed.clone();
        let input = cx.new(|cx| {
            TextInput::new(value_text.to_string(), window, cx)
                .on_change(move |text, _, cx| {
                    changed
                        .update(cx, |this, cx| {
                            this.draft = text.trim().parse().ok();
                            if let Some(value) =
                                this.draft.filter(|value| this.bounds.contains(value))
                            {
                                this.publish(value, cx);
                            }
                            cx.notify();
                        })
                        .ok();
                })
                .on_commit(move |_, window, cx| {
                    let committed = committed.clone();
                    // TextInput still owns its mutable borrow while invoking this callback.
                    window.defer(cx, move |_, cx| {
                        committed
                            .update(cx, |this, cx| {
                                this.set_value(this.draft.unwrap_or(this.value), cx);
                            })
                            .ok();
                    });
                })
        });
        Self {
            label: label.into(),
            input,
            value,
            value_text,
            draft: Some(value),
            bounds,
        }
    }

    fn publish(&mut self, value: i32, cx: &mut Context<Self>) {
        if self.value != value {
            self.value = value;
            self.value_text = value.to_string().into();
            cx.emit(NumberInputChanged(value));
        }
    }

    fn set_value(&mut self, value: i32, cx: &mut Context<Self>) {
        let value = value.clamp(*self.bounds.start(), *self.bounds.end());
        self.publish(value, cx);
        self.draft = Some(value);
        self.input
            .update(cx, |input, cx| input.replace_value(&self.value_text, cx));
        cx.notify();
    }

    fn step(&mut self, delta: i32, cx: &mut Context<Self>) {
        self.set_value(self.draft.unwrap_or(self.value).saturating_add(delta), cx);
    }
}

impl Focusable for NumberInput {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.focus_handle(cx)
    }
}

impl Render for NumberInput {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let value = self.draft.unwrap_or(self.value);
        div()
            .id(("number-input", cx.entity_id()))
            .role(Role::SpinButton)
            .aria_label(self.label.clone())
            .aria_value(self.value_text.clone())
            .flex()
            .items_center()
            .gap_1()
            .w(px(112.))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                let modifiers = event.keystroke.modifiers;
                if modifiers.control || modifiers.platform || modifiers.alt {
                    return;
                }
                match event.keystroke.key.as_str() {
                    "up" => this.step(1, cx),
                    "down" => this.step(-1, cx),
                    _ => return,
                }
                cx.stop_propagation();
            }))
            .child(
                Button::new("decrease", "−")
                    .style(ButtonStyle::Filled)
                    .width(px(24.))
                    .aria_label("Decrease value")
                    .disabled(value <= *self.bounds.start())
                    .on_click(cx.listener(|this, _, _, cx| this.step(-1, cx))),
            )
            .child(div().flex_1().min_w_0().child(self.input.clone()))
            .child(
                Button::new("increase", "+")
                    .style(ButtonStyle::Filled)
                    .width(px(24.))
                    .aria_label("Increase value")
                    .disabled(value >= *self.bounds.end())
                    .on_click(cx.listener(|this, _, _, cx| this.step(1, cx))),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{TestAppContext, size};

    #[gpui::test]
    fn typing_stepping_undo_and_bounds(cx: &mut TestAppContext) {
        cx.update(|cx| crate::init("number input test", cx));
        let window = cx.open_window(size(px(200.), px(80.)), |window, cx| {
            NumberInput::new("Sides", 5, 3..=32, window, cx)
        });
        window
            .update(cx, |number, window, cx| {
                window.focus(&number.focus_handle(cx), cx);
                window.activate_window();
            })
            .unwrap();
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
                .unwrap();
        });

        cx.dispatch_action(window.into(), crate::SelectAll);
        cx.simulate_input(window.into(), "12");
        cx.simulate_keystrokes(window.into(), "up");
        window
            .update(cx, |number, _, cx| {
                assert_eq!(number.value, 13);
                assert_eq!(number.input.read(cx).text(), "13");
            })
            .unwrap();
        cx.dispatch_action(window.into(), crate::Undo);
        window
            .update(cx, |number, _, _| assert_eq!(number.value, 12))
            .unwrap();

        for (typed, key, expected) in [("99", "up", 32), ("0", "down", 3), ("invalid", "up", 4)] {
            cx.dispatch_action(window.into(), crate::SelectAll);
            cx.simulate_input(window.into(), typed);
            cx.simulate_keystrokes(window.into(), "enter");
            cx.simulate_keystrokes(window.into(), key);
            window
                .update(cx, |number, _, cx| {
                    assert_eq!(number.value, expected);
                    assert_eq!(
                        number.input.read(cx).text().parse::<i32>().unwrap(),
                        expected
                    );
                })
                .unwrap();
        }
        cx.dispatch_action(window.into(), crate::SelectAll);
        cx.simulate_input(window.into(), "999");
        cx.update(|cx| {
            cx.update_window(window.into(), |_, window, cx| {
                window.blur(cx);
                window.draw(cx).clear(cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        window
            .update(cx, |number, _, cx| {
                assert_eq!(number.value, 32);
                assert_eq!(number.input.read(cx).text(), "32");
            })
            .unwrap();
    }
}
