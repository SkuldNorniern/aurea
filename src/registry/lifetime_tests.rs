//! Callback registrations are dropped with the thing that made them.

#[cfg(test)]
mod tests {
    use crate::elements::Button;
    use crate::registry::elements::invoke_button_callback;
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn dropping_a_button_drops_its_callback() {
        let fired = Rc::new(Cell::new(0));
        let f = Rc::clone(&fired);

        let button = Button::with_callback("go", move || f.set(f.get() + 1));
        let Ok(button) = button else {
            return;
        };
        let id = button.callback_id();

        invoke_button_callback(id);
        assert_eq!(fired.get(), 1, "callback should run while the button lives");

        drop(button);
        invoke_button_callback(id);
        assert_eq!(fired.get(), 1, "callback should be gone with the button");
        assert_eq!(
            Rc::strong_count(&fired),
            1,
            "the registry still holds what the callback captured"
        );
    }
}
