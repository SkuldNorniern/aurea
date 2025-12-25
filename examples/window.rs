use aurea::{Window, AureaResult};
use aurea::elements::{Button, Label, Box, BoxOrientation, Container};

fn main() -> AureaResult<()> {
    let mut window = Window::new("Aurea Example", 800, 600)?;
    
    let mut menu_bar = window.create_menu_bar()?;
    
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
    
    {
        let mut help_menu = menu_bar.add_submenu("Help")?;
        help_menu.add_item("About", || {
            println!("Help -> About clicked");
        })?;
    }
    
    let mut main_box = Box::new(BoxOrientation::Vertical)?;
    
    let welcome_label = Label::new("Welcome to Aurea!")?;
    main_box.add(welcome_label)?;
    
    let mut button_box = Box::new(BoxOrientation::Horizontal)?;
    
    let button1 = Button::new("Button 1")?;
    button_box.add(button1)?;
    
    let button2 = Button::new("Button 2")?;
    button_box.add(button2)?;
    
    let button3 = Button::new("Button 3")?;
    button_box.add(button3)?;
    
    main_box.add(button_box)?;
    
    let info_label = Label::new("This is a native GUI application built with Aurea.")?;
    main_box.add(info_label)?;
    
    let status_label = Label::new("Status: Ready")?;
    main_box.add(status_label)?;
    
    window.set_content(main_box)?;
    
    window.run()?;
    
    Ok(())
}

