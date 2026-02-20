//! Window event system for external event loop integration

pub use aurea_core::{EventCallback, KeyCode, Modifiers, MouseButton, WindowEvent};
pub use aurea_runtime::EventQueue;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn event_queue_push_pop_all() {
        let queue = EventQueue::new();
        queue.push(WindowEvent::CloseRequested);
        queue.push(WindowEvent::Focused);
        let out = queue.pop_all();
        assert_eq!(out.len(), 2);
        assert!(matches!(out[0], WindowEvent::CloseRequested));
        assert!(matches!(out[1], WindowEvent::Focused));
        assert!(queue.pop_all().is_empty());
    }

    #[test]
    fn event_queue_process_events_invokes_callbacks() {
        let queue = EventQueue::new();
        queue.push(WindowEvent::CloseRequested);
        let received = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let rec = std::sync::Arc::clone(&received);
        queue.register_callback(Arc::new(move |e| {
            crate::sync::lock(&rec).push(e);
        }));
        let processed = queue.process_events();
        assert_eq!(processed.len(), 1);
        assert_eq!(crate::sync::lock(&received).len(), 1);
    }

    #[test]
    fn modifiers_from_bits_and_is_any() {
        let none = Modifiers::from_bits(0);
        assert!(!none.is_any());
        assert!(!none.shift && !none.ctrl && !none.alt && !none.meta);

        let shift = Modifiers::from_bits(0b0001);
        assert!(shift.is_any());
        assert!(shift.shift && !shift.ctrl);

        let all = Modifiers::from_bits(0b1111);
        assert!(all.is_any());
        assert!(all.shift && all.ctrl && all.alt && all.meta);
    }

    #[test]
    fn modifiers_default() {
        let m = Modifiers::default();
        assert!(!m.is_any());
    }

    #[test]
    fn event_queue_key_input() {
        let queue = EventQueue::new();
        let mods = Modifiers::from_bits(0b0010);
        queue.push(WindowEvent::KeyInput {
            key: KeyCode::A,
            pressed: true,
            modifiers: mods,
        });
        let out = queue.pop_all();
        assert_eq!(out.len(), 1);
        match &out[0] {
            WindowEvent::KeyInput { key, pressed, .. } => {
                assert_eq!(*key, KeyCode::A);
                assert!(*pressed);
            }
            _ => panic!("expected KeyInput"),
        }
    }

    #[test]
    fn event_queue_mouse_button() {
        let queue = EventQueue::new();
        queue.push(WindowEvent::MouseButton {
            button: MouseButton::Left,
            pressed: true,
            modifiers: Modifiers::default(),
        });
        let out = queue.pop_all();
        assert_eq!(out.len(), 1);
        match &out[0] {
            WindowEvent::MouseButton { button, pressed, .. } => {
                assert_eq!(*button, MouseButton::Left);
                assert!(*pressed);
            }
            _ => panic!("expected MouseButton"),
        }
    }

    #[test]
    fn event_queue_mouse_wheel() {
        let queue = EventQueue::new();
        queue.push(WindowEvent::MouseWheel {
            delta_x: 1.0,
            delta_y: -2.0,
            modifiers: Modifiers::default(),
        });
        let out = queue.pop_all();
        assert_eq!(out.len(), 1);
        match &out[0] {
            WindowEvent::MouseWheel { delta_x, delta_y, .. } => {
                assert_eq!(*delta_x, 1.0);
                assert_eq!(*delta_y, -2.0);
            }
            _ => panic!("expected MouseWheel"),
        }
    }

    #[test]
    fn event_queue_text_input() {
        let queue = EventQueue::new();
        queue.push(WindowEvent::TextInput {
            text: "hello".into(),
        });
        let out = queue.pop_all();
        assert_eq!(out.len(), 1);
        match &out[0] {
            WindowEvent::TextInput { text } => assert_eq!(text, "hello"),
            _ => panic!("expected TextInput"),
        }
    }

    #[test]
    fn event_queue_focus() {
        let queue = EventQueue::new();
        queue.push(WindowEvent::Focused);
        queue.push(WindowEvent::Unfocused);
        let out = queue.pop_all();
        assert_eq!(out.len(), 2);
        assert!(matches!(out[0], WindowEvent::Focused));
        assert!(matches!(out[1], WindowEvent::Unfocused));
    }

    #[test]
    fn event_queue_resized() {
        let queue = EventQueue::new();
        queue.push(WindowEvent::Resized {
            width: 800,
            height: 600,
        });
        let out = queue.pop_all();
        assert_eq!(out.len(), 1);
        match &out[0] {
            WindowEvent::Resized { width, height } => {
                assert_eq!(*width, 800);
                assert_eq!(*height, 600);
            }
            _ => panic!("expected Resized"),
        }
    }

    #[test]
    fn event_queue_scale_factor_changed() {
        let queue = EventQueue::new();
        queue.push(WindowEvent::ScaleFactorChanged {
            scale_factor: 2.0,
        });
        let out = queue.pop_all();
        assert_eq!(out.len(), 1);
        match &out[0] {
            WindowEvent::ScaleFactorChanged { scale_factor } => assert_eq!(*scale_factor, 2.0),
            _ => panic!("expected ScaleFactorChanged"),
        }
    }
}
