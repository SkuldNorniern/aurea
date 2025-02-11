use aurea::{Window, AureaError, AureaResult};
use aurea::elements::{Box, BoxOrientation, Button, Label, Container};

fn main() -> AureaResult<()> {
    let mut window = Window::new("Quill", 800, 600)?;
    
    let mut menu_bar = window.create_menu_bar()?;
    
    {
        let mut file_menu = menu_bar.add_submenu("File")?;
        file_menu.add_item("New", || println!("New"))?;
        file_menu.add_item("Open...", || println!("Open"))?;
        file_menu.add_item("Save", || println!("Save"))?;
    }
    
    {
        let mut edit_menu = menu_bar.add_submenu("Edit")?;
        edit_menu.add_item("Cut", || println!("Cut"))?;
        edit_menu.add_item("Copy", || println!("Copy"))?;
        edit_menu.add_item("Paste", || println!("Paste"))?;
    }

    let mut main_box = Box::new(BoxOrientation::Vertical)?;
    
    let welcome_label = Label::new("Welcome to Quill!")?;
    main_box.add(welcome_label)?;
    
    let mut button_box = Box::new(BoxOrientation::Horizontal)?;
    
    let new_button = Button::new("New Document")?;
    let open_button = Button::new("Open Document")?;
    let save_button = Button::new("Save Document")?;
    
    button_box.add(new_button)?;
    button_box.add(open_button)?;
    button_box.add(save_button)?;
    
    main_box.add(button_box)?;
    
    window.set_content(main_box)?;

    window.run()?;
    Ok(())
}