//! Multi-window example demonstrating different window types
//!
//! This example shows how to create and manage multiple windows with different types:
//! - Normal window (main application window)
//! - Popup window (borderless, stays on top)
//! - Tool window (floating tool palette)
//! - Dialog window (modal dialog)

use aurea::elements::{Box, BoxOrientation, Button, Container, Label};
use aurea::logger;
use aurea::{AureaResult, Window, WindowManager, WindowType};
use log::LevelFilter;
use std::sync::Arc;

fn main() -> AureaResult<()> {
    logger::init(LevelFilter::Debug).unwrap_or_else(|e| {
        eprintln!("Failed to initialize logger: {}", e);
    });

    let manager = WindowManager::new();

    // Create main window
    let mut main_window = Window::new("Main Window", 800, 600)?;
    setup_main_window(&mut main_window)?;
    let main_window_arc = Arc::new(main_window);
    manager.register(main_window_arc.clone());

    // Create popup window
    let mut popup = Window::with_type("Popup", 300, 200, WindowType::Popup)?;
    setup_popup(&mut popup)?;
    let popup_arc = Arc::new(popup);
    manager.register(popup_arc.clone());

    // Create tool window
    let mut tool = Window::with_type("Tool Palette", 200, 400, WindowType::Tool)?;
    setup_tool_window(&mut tool)?;
    let tool_arc = Arc::new(tool);
    manager.register(tool_arc.clone());

    println!("Created {} windows", manager.count());

    // Event loop
    loop {
        let events = manager.poll_all_events();
        
        let mut should_exit = false;
        for event in events {
            match event {
                aurea::WindowEvent::CloseRequested => {
                    println!("Window close requested");
                    should_exit = true;
                }
                _ => {}
            }
        }

        if should_exit {
            break;
        }

        manager.process_all_frames()?;

        // Small delay to prevent busy loop
        std::thread::sleep(std::time::Duration::from_millis(16));
    }

    Ok(())
}

fn setup_main_window(window: &mut Window) -> AureaResult<()> {
    let mut main_box = Box::new(BoxOrientation::Vertical)?;
    
    main_box.add(Label::new("Main Application Window")?)?;
    main_box.add(Label::new("This is a normal window with full decorations.")?)?;
    main_box.add(Label::new("")?)?;
    
    let mut button_box = Box::new(BoxOrientation::Horizontal)?;
    button_box.add(Button::with_callback("Open Popup", || {
        println!("Open popup clicked");
    })?)?;
    button_box.add(Button::with_callback("Open Tool", || {
        println!("Open tool clicked");
    })?)?;
    
    main_box.add(button_box)?;
    main_box.add(Label::new("")?)?;
    main_box.add(Label::new("Status: Ready")?)?;

    window.set_content(main_box)?;
    Ok(())
}

fn setup_popup(window: &mut Window) -> AureaResult<()> {
    let mut popup_box = Box::new(BoxOrientation::Vertical)?;
    
    popup_box.add(Label::new("Popup Window")?)?;
    popup_box.add(Label::new("This is a popup window.")?)?;
    popup_box.add(Label::new("It stays on top and has minimal decorations.")?)?;
    popup_box.add(Button::with_callback("Close", || {
        println!("Close popup clicked");
    })?)?;

    window.set_content(popup_box)?;
    Ok(())
}

fn setup_tool_window(window: &mut Window) -> AureaResult<()> {
    let mut tool_box = Box::new(BoxOrientation::Vertical)?;
    
    tool_box.add(Label::new("Tool Palette")?)?;
    tool_box.add(Button::with_callback("Tool 1", || {
        println!("Tool 1 clicked");
    })?)?;
    tool_box.add(Button::with_callback("Tool 2", || {
        println!("Tool 2 clicked");
    })?)?;
    tool_box.add(Button::with_callback("Tool 3", || {
        println!("Tool 3 clicked");
    })?)?;
    tool_box.add(Label::new("")?)?;
    tool_box.add(Label::new("Floating tool window")?)?;

    window.set_content(tool_box)?;
    Ok(())
}
