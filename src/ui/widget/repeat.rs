//! Firing again and again while a part is held, as a key does.

use crate::ui::frame::Ui;
use crate::ui::state::{Channel, WidgetId};

/// Seconds held before it starts repeating.
const AFTER: f32 = 0.42;
/// Seconds between repeats.
const EVERY: f32 = 0.07;
const HELD_SINCE: Channel = Channel::Custom(1);
const REPEATS: Channel = Channel::Custom(2);

/// How many times that part fires this frame: once as it is pressed, then again on a beat while
/// `held`.
pub(crate) fn fires<Id: WidgetId>(ui: &mut Ui<'_, Id>, id: Id, part: u16, held: bool) -> f32 {
    let now = ui.time();
    if ui.press(id, part).is_some() {
        ui.set_animated(id, part, HELD_SINCE, now);
        ui.set_animated(id, part, REPEATS, 0.0);
        return 1.0;
    }
    if !held {
        return 0.0;
    }
    ui.keep_animating();
    let since = ui.animate(id, part, HELD_SINCE, now, 0.0);
    let done = ui.animate(id, part, REPEATS, 0.0, 0.0);
    let due = ((now - since - AFTER) / EVERY).floor() + 1.0;
    if due > done {
        ui.set_animated(id, part, REPEATS, due);
        due - done
    } else {
        0.0
    }
}
