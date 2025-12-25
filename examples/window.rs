use aurea::{Window, AureaResult};
use aurea::elements::{Button, Label, Box, BoxOrientation, Container};
use aurea::logger;
use log::LevelFilter;

fn main() -> AureaResult<()> {
    // Initialize logger with debug level to see detailed logs
    logger::init(LevelFilter::Debug).unwrap_or_else(|e| {
        eprintln!("Failed to initialize logger: {}", e);
    });
    
    // Create a new window
    let mut window = Window::new("Aurea Example", 800, 600)?;
    
    // Create a menu bar
    let mut menu_bar = window.create_menu_bar()?;
    
    // File menu
    {
        let mut file_menu = menu_bar.add_submenu("File")?;
        file_menu.add_item("New", || {
            println!("File -> New clicked");
        })?;
        file_menu.add_item("Open...", || {
            println!("File -> Open clicked");
        })?;
        file_menu.add_item("Save", || {
            println!("File -> Save clicked");
        })?;
        file_menu.add_item("Exit", || {
            println!("File -> Exit clicked");
            std::process::exit(0);
        })?;
    }
    
    // Edit menu
    {
        let mut edit_menu = menu_bar.add_submenu("Edit")?;
        edit_menu.add_item("Cut", || {
            println!("Edit -> Cut clicked");
        })?;
        edit_menu.add_item("Copy", || {
            println!("Edit -> Copy clicked");
        })?;
        edit_menu.add_item("Paste", || {
            println!("Edit -> Paste clicked");
        })?;
    }
    
    // Help menu
    {
        let mut help_menu = menu_bar.add_submenu("Help")?;
        help_menu.add_item("About", || {
            println!("Help -> About clicked");
        })?;
    }
    
    // Create a vertical layout container
    let mut main_box = Box::new(BoxOrientation::Vertical)?;
    
    // Add a welcome label
    let welcome_label = Label::new("Welcome to Aurea!")?;
    main_box.add(welcome_label)?;
    
    // Create a horizontal box for buttons
    let mut button_box = Box::new(BoxOrientation::Horizontal)?;
    
    // Add some buttons
    let button1 = Button::new("Button 1")?;
    button_box.add(button1)?;
    
    let button2 = Button::new("Button 2")?;
    button_box.add(button2)?;
    
    let button3 = Button::new("Button 3")?;
    button_box.add(button3)?;
    
    // Add the button box to the main box
    main_box.add(button_box)?;
    
    // Add more labels
    let info_label = Label::new("This is a native GUI application built with Aurea.")?;
    main_box.add(info_label)?;
    
    let status_label = Label::new("Status: Ready")?;
    main_box.add(status_label)?;
    
    // Set the main box as the window's content
    window.set_content(main_box)?;
    
    // Run the application event loop
    window.run()?;
    
    Ok(())
}

