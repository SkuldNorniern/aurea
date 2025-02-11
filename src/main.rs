use aurea::{Window, AureaError, AureaResult};
use aurea::elements::{Box, BoxOrientation, Button, Label, Container};

fn main() -> AureaResult<()> {
    let mut window = Window::new("Quill", 800, 600)?;
    
    // Create menu bar
    let mut menu_bar = window.create_menu_bar()?;
    
    // File Menu
    {
        let mut file_menu = menu_bar.add_submenu("File")?;
        file_menu.add_item("New", || println!("New"))?;
        file_menu.add_item("Open...", || println!("Open"))?;
        file_menu.add_item("Save", || println!("Save"))?;
    }
    
    // Edit Menu
    {
        let mut edit_menu = menu_bar.add_submenu("Edit")?;
        edit_menu.add_item("Cut", || println!("Cut"))?;
        edit_menu.add_item("Copy", || println!("Copy"))?;
        edit_menu.add_item("Paste", || println!("Paste"))?;
    }

    // Create main content
    let mut main_box = Box::new(BoxOrientation::Vertical)?;
    
    // Add a welcome label
    let welcome_label = Label::new("Welcome to Quill!")?;
    main_box.add(welcome_label)?;
    
    // Create a horizontal box for buttons
    let mut button_box = Box::new(BoxOrientation::Horizontal)?;
    
    // Add some buttons
    let new_button = Button::new("New Document")?;
    let open_button = Button::new("Open Document")?;
    let save_button = Button::new("Save Document")?;
    
    button_box.add(new_button)?;
    button_box.add(open_button)?;
    button_box.add(save_button)?;
    
    // Add the button box to the main box
    main_box.add(button_box)?;
    
    // Set the main box as the window's content
    window.set_content(main_box)?;

    // Run the application
    window.run()?;
    Ok(())
}