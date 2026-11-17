//! Prints game controller events until the process is stopped.

use std::thread::sleep;
use std::time::Duration;

use aurea::input::gamepad::Pads;

fn main() {
    let mut pads = Pads::new();
    let mut events = Vec::new();
    loop {
        pads.poll(&mut events);
        for event in events.drain(..) {
            println!("{event:?}");
        }
        sleep(Duration::from_millis(4));
    }
}
