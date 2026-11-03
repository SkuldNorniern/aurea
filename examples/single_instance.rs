//! One running copy: run it, then run it again with a file. The second hands the file to the first
//! and leaves.

use std::error::Error;
use std::process::ExitCode;
use std::thread::sleep;
use std::time::Duration;

use aurea::app::{identity, set_identity, single_instance, take_opened};
use aurea::{Window, WindowEvent};

fn main() -> Result<ExitCode, Box<dyn Error>> {
    set_identity(aurea::identity!());
    if !single_instance() {
        println!("handed over to the running copy");
        return Ok(ExitCode::SUCCESS);
    }
    println!(
        "{} is the running copy, opened {:?}",
        identity().id,
        take_opened()
    );
    let window = Window::new("Single instance", 400, 200)?;
    loop {
        for event in window.poll_events() {
            match event {
                WindowEvent::CloseRequested => return Ok(ExitCode::SUCCESS),
                WindowEvent::OpenRequested => {
                    println!("asked to open {:?}", take_opened());
                    window.focus();
                }
                _ => {}
            }
        }
        window.process_frames()?;
        sleep(Duration::from_millis(16));
    }
}
