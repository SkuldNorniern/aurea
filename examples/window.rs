use aurea::elements::{Box, BoxOrientation, Button, Container, Label};
use aurea::logger;
use aurea::{AureaResult, Window};
use aurea::MenuBar;
use log::LevelFilter;

fn main() -> AureaResult<()> {
    logger::init(LevelFilter::Debug).unwrap_or_else(|e| {
        eprintln!("Failed to initialize logger: {}", e);
    });

    let mut window = Window::new("Aurea Example", 800, 600)?;
    setup_menu_bar(&mut window)?;
    setup_ui(&mut window)?;
    window.run()?;

    Ok(())
}

fn setup_menu_bar(window: &mut Window) -> AureaResult<()> {
    let mut menu_bar = window.create_menu_bar()?;
    
    setup_file_menu(&mut menu_bar)?;
    setup_edit_menu(&mut menu_bar)?;
    setup_help_menu(&mut menu_bar)?;
    
    Ok(())
}

fn setup_file_menu(menu_bar: &mut MenuBar) -> AureaResult<()> {
    let mut file_menu = menu_bar.add_submenu("File")?;
    file_menu.add_item("New", || println!("File -> New clicked"))?;
    file_menu.add_item("Open...", || println!("File -> Open clicked"))?;
    file_menu.add_item("Save", || println!("File -> Save clicked"))?;
    file_menu.add_item("Exit", || {
        println!("File -> Exit clicked");
        std::process::exit(0);
    })?;
    Ok(())
}

fn setup_edit_menu(menu_bar: &mut MenuBar) -> AureaResult<()> {
    let mut edit_menu = menu_bar.add_submenu("Edit")?;
    edit_menu.add_item("Cut", || println!("Edit -> Cut clicked"))?;
    edit_menu.add_item("Copy", || println!("Edit -> Copy clicked"))?;
    edit_menu.add_item("Paste", || println!("Edit -> Paste clicked"))?;
    Ok(())
}

fn setup_help_menu(menu_bar: &mut MenuBar) -> AureaResult<()> {
    let mut help_menu = menu_bar.add_submenu("Help")?;
    help_menu.add_item("About", || println!("Help -> About clicked"))?;
    Ok(())
}

fn setup_ui(window: &mut Window) -> AureaResult<()> {
    let mut main_box = Box::new(BoxOrientation::Vertical)?;
    
    main_box.add(Label::new("Welcome to Aurea!")?)?;
    main_box.add(create_button_row()?)?;
    main_box.add(Label::new("This is a native GUI application built with Aurea.")?)?;
    main_box.add(Label::new("Status: Ready")?)?;
    
    window.set_content(main_box)?;
    Ok(())
}

fn create_button_row() -> AureaResult<Box> {
    let mut button_box = Box::new(BoxOrientation::Horizontal)?;
    button_box.add(Button::with_callback("Button 1", || {
        println!("Button 1 clicked!");
    })?)?;
    button_box.add(Button::with_callback("Button 2", || {
        println!("Button 2 clicked!");
    })?)?;
    button_box.add(Button::with_callback("Button 3", || {
        println!("Button 3 clicked!");
    })?)?;
    Ok(button_box)
}
