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

    // Create popup window first, setup, then Arc it
    let mut popup = Window::with_type("Popup", 300, 200, WindowType::Popup)?;
    setup_popup(&mut popup)?;
    popup.hide(); // Hide initially
    let popup_arc = Arc::new(popup);

    // Create tool window, setup, then Arc it
    let mut tool = Window::with_type("Tool Palette", 200, 400, WindowType::Tool)?;
    setup_tool_window(&mut tool)?;
    tool.hide(); // Hide initially
    let tool_arc = Arc::new(tool);

    // Create main window, setup with references to other Arcs, then Arc it
    let mut main_window = Window::new("Main Window", 800, 600)?;
    setup_main_window(&mut main_window, popup_arc.clone(), tool_arc.clone())?;
    let main_window_arc = Arc::new(main_window);

    // Register all windows
    manager.register(main_window_arc.clone());
    manager.register(popup_arc.clone());
    manager.register(tool_arc.clone());

    println!("Created {} windows", manager.count());

    // Event loop
    loop {
        // Poll main window events
        let main_events = main_window_arc.poll_events();
        for event in main_events {
             if let aurea::WindowEvent::CloseRequested = event {
                 println!("Main window close requested - exiting");
                 return Ok(());
             }
        }

        // Poll popup events
        let popup_events = popup_arc.poll_events();
        for event in popup_events {
             if let aurea::WindowEvent::CloseRequested = event {
                 println!("Popup close requested - hiding");
                 popup_arc.hide();
             }
        }

        // Poll tool events
        let tool_events = tool_arc.poll_events();
        for event in tool_events {
             if let aurea::WindowEvent::CloseRequested = event {
                 println!("Tool window close requested - hiding");
                 tool_arc.hide();
             }
        }
        
        // Pump OS events globally
        unsafe {
            unsafe extern "C" {
                fn ng_platform_poll_events() -> i32;
            }
            ng_platform_poll_events();
        }

        manager.process_all_frames()?;

        // Small delay to prevent busy loop
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
}

fn setup_main_window(
    window: &mut Window,
    popup_arc: Arc<Window>,
    tool_arc: Arc<Window>,
) -> AureaResult<()> {
    let mut main_box = Box::new(BoxOrientation::Vertical)?;
    
    main_box.add(Label::new("Main Application Window")?)?;
    main_box.add(Label::new("This is a normal window with full decorations.")?)?;
    main_box.add(Label::new("")?)?;
    
    let mut button_box = Box::new(BoxOrientation::Horizontal)?;
    
    let p_clone = popup_arc.clone();
    button_box.add(Button::with_callback("Open Popup", move || {
        if p_clone.is_visible() {
            println!("Open button clicked but it's already opened");
        } else {
            println!("Opening popup");
            p_clone.show();
        }
    })?)?;
    
    let t_clone = tool_arc.clone();
    button_box.add(Button::with_callback("Open Tool", move || {
        if t_clone.is_visible() {
            println!("Open button clicked but it's already opened");
        } else {
            println!("Opening tool");
            t_clone.show();
        }
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
    
    // We use the raw handle to request a close from the callback
    // This avoids circular Arc dependencies while still allowing the button to work
    let handle = window.handle() as usize; 
    
    popup_box.add(Button::with_callback("Close", move || {
        println!("Close popup clicked");
        unsafe extern "C" {
            fn ng_platform_window_request_close(handle: *mut std::ffi::c_void);
        }
        unsafe {
            // Re-construct the handle and request close
            let h = handle as *mut std::ffi::c_void;
            ng_platform_window_request_close(h);
        }
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
